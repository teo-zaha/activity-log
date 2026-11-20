use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use activity_log_core::model::{
    CollectOptions, CollectionFailedResponse, ConnectionId, ErrorResponse, ExportFile,
    LogsResponse, ProviderInfo, ProviderStatus, Source, SupportedOs,
};
use activity_log_core::provider::connection::{Connection, ConnectionLocation, ConnectionValues};
use activity_log_core::runtime::{
    Collector, Host, Outcome, Plan, RunReport, Slot, Stored, Superseded,
};
use rocket::{http::Status, local::asynchronous::Client};
use tokio::sync::Notify;

use crate::{build_rocket_with, spawner};

/// Settles every planned connection with no entries.
async fn settle(plans: Vec<(ConnectionId, Plan)>) -> RunReport {
    let mut slots = Vec::new();
    for (id, plan) in plans {
        let slot = match plan {
            Plan::Collect { check, .. } => match check.await {
                Ok(checked) => Slot::Settled(Outcome::Collected(Stored {
                    key: checked.key,
                    entries: Vec::new().into(),
                })),
                Err(Superseded) => Slot::Skipped,
            },
            Plan::Keep(outcome) => Slot::Kept(outcome),
        };
        slots.push((id, slot));
    }
    RunReport { slots }
}

fn fixture_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            connection: ConnectionId::new(Source::Git, "default"),
            label: "Git".to_string(),
            wsl_distro: None,
            status: ProviderStatus::Available,
            usable: true,
            supported_os: vec![SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
            reason: None,
            problem: None,
            checked_at: "2026-03-25T10:00:00Z".to_string(),
        },
        ProviderInfo {
            connection: ConnectionId::new(Source::Outlook, "default"),
            label: "Outlook".to_string(),
            wsl_distro: None,
            status: ProviderStatus::Unavailable,
            usable: false,
            supported_os: vec![SupportedOs::Windows],
            reason: Some("OS not supported".to_string()),
            problem: None,
            checked_at: "2026-03-25T10:00:00Z".to_string(),
        },
    ]
}

fn collector_ok() -> Collector {
    Arc::new(|_options, plans, _progress| Box::pin(async move { Ok(settle(plans).await) }))
}

/// Finishes once `release` is notified.
fn collector_held(release: &Arc<Notify>) -> Collector {
    let release = release.clone();
    Arc::new(move |_options, plans, _progress| {
        let release = release.clone();
        Box::pin(async move {
            release.notified().await;
            Ok(settle(plans).await)
        })
    })
}

const MARCH_FIRST_WEEK: &str = r#"{"start":"2026-03-01T00:00:00Z","end":"2026-03-06T00:00:00Z"}"#;

async fn client(collector: Collector) -> Client {
    let host = Host::with_collector(collector, fixture_providers(), spawner());
    Client::tracked(build_rocket_with(host, false))
        .await
        .unwrap()
}

async fn post_collect<'a>(
    client: &'a Client,
    body: &'a str,
) -> rocket::local::asynchronous::LocalResponse<'a> {
    client
        .post("/api/collect")
        .header(rocket::http::ContentType::JSON)
        .body(body)
        .dispatch()
        .await
}

/// Asks for the activity until it answers `status`.
async fn activity_with_status(client: &Client, status: Status) -> serde_json::Value {
    for _ in 0..50 {
        let response = client.get("/api/activity").dispatch().await;
        if response.status() == status {
            return response.into_json().await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the activity never answered {status}");
}

#[rocket::async_test]
async fn returns_503_without_cache() {
    let client = client(collector_ok()).await;

    let response = client.get("/api/activity").dispatch().await;
    assert_eq!(response.status(), Status::ServiceUnavailable);
}

#[rocket::async_test]
async fn a_running_collection_answers_202_with_its_pending_connections() {
    let release = Arc::new(Notify::new());
    let client = client(collector_held(&release)).await;

    let started = post_collect(&client, MARCH_FIRST_WEEK).await;
    assert_eq!(started.status(), Status::Accepted, "answers before it ends");
    let started: serde_json::Value = started.into_json().await.unwrap();
    assert_eq!(started["meta"]["start"], "2026-03-01T00:00:00+00:00");
    assert_eq!(started["entries"], serde_json::json!([]));

    let running = activity_with_status(&client, Status::Accepted).await;
    assert_eq!(
        running["meta"]["pending"],
        serde_json::json!([{ "source": "git", "name": "default" }]),
        "only the usable connection is waited for"
    );

    release.notify_one();
    let done = activity_with_status(&client, Status::Ok).await;
    assert!(done["meta"].get("pending").is_none());
}

#[rocket::async_test]
async fn a_second_collect_of_the_same_range_answers_202_with_the_running_one() {
    let release = Arc::new(Notify::new());
    let client = client(collector_held(&release)).await;
    post_collect(&client, MARCH_FIRST_WEEK).await;

    let second = post_collect(&client, MARCH_FIRST_WEEK).await;

    assert_eq!(second.status(), Status::Accepted);
    let second: serde_json::Value = second.into_json().await.unwrap();
    assert_eq!(second["meta"]["start"], "2026-03-01T00:00:00+00:00");
}

#[rocket::async_test]
async fn a_collect_of_another_range_replaces_the_running_one() {
    let release = Arc::new(Notify::new());
    let client = client(collector_held(&release)).await;
    post_collect(&client, MARCH_FIRST_WEEK).await;

    let second = post_collect(
        &client,
        r#"{"start":"2026-02-01T00:00:00Z","end":"2026-02-02T00:00:00Z"}"#,
    )
    .await;

    assert_eq!(second.status(), Status::Accepted);
    let second: serde_json::Value = second.into_json().await.unwrap();
    assert_eq!(second["meta"]["start"], "2026-02-01T00:00:00+00:00");
    let running = activity_with_status(&client, Status::Accepted).await;
    assert_eq!(running["meta"]["start"], "2026-02-01T00:00:00+00:00");
}

#[rocket::async_test]
async fn reports_a_failed_collection_with_its_range() {
    let collector: Collector = Arc::new(|_options, _plans, _progress| {
        Box::pin(async { Err(anyhow::anyhow!("Jira credentials invalid")) })
    });
    let client = client(collector).await;

    let collect = post_collect(&client, MARCH_FIRST_WEEK).await;
    assert_eq!(collect.status(), Status::Accepted);

    // A failure answers 200 like a log; only a running collection answers 202.
    let mut body = activity_with_status(&client, Status::Ok).await;
    let body: CollectionFailedResponse = serde_json::from_value(body.take()).unwrap();
    assert_eq!(body.start, "2026-03-01T00:00:00+00:00");
    assert_eq!(body.end, "2026-03-06T00:00:00+00:00");
    assert_eq!(body.error, "Jira credentials invalid");
}

#[rocket::async_test]
async fn post_collect_rejects_plain_dates() {
    let client = client(collector_ok()).await;

    let response = post_collect(&client, r#"{"start":"2026-03-01","end":"2026-03-05"}"#).await;
    assert_eq!(response.status(), Status::BadRequest);
}

#[rocket::async_test]
async fn post_collect_passes_the_exact_range_to_the_collector() {
    let captured = Arc::new(Mutex::new(None));
    let collector_capture = Arc::clone(&captured);
    let collector: Collector = Arc::new(move |options: CollectOptions, plans, _progress| {
        let collector_capture = Arc::clone(&collector_capture);
        Box::pin(async move {
            *collector_capture.lock().unwrap() = Some(options);
            Ok(settle(plans).await)
        })
    });
    let client = client(collector).await;

    post_collect(
        &client,
        r#"{"start":"2026-03-01T00:00:00+01:00","end":"2026-03-02T00:00:00+01:00"}"#,
    )
    .await;
    activity_with_status(&client, Status::Ok).await;

    let options = captured.lock().unwrap().unwrap();
    assert_eq!(options.start.to_rfc3339(), "2026-02-28T23:00:00+00:00");
    assert_eq!(options.end.to_rfc3339(), "2026-03-01T23:00:00+00:00");
}

#[rocket::async_test]
async fn returns_provider_statuses() {
    let client = client(collector_ok()).await;

    let response = client.get("/api/providers").dispatch().await;
    assert_eq!(response.status(), Status::Ok);

    let body: serde_json::Value =
        serde_json::from_str(&response.into_string().await.unwrap()).unwrap();
    let providers = body["providers"].as_array().unwrap();
    assert_eq!(
        providers[0]["connection"],
        serde_json::json!({ "source": "git", "name": "default" })
    );
    assert_eq!(providers[1]["connection"]["source"], "outlook");
    assert_eq!(providers[1]["usable"], false);
}

#[rocket::async_test]
async fn logs_can_be_read_and_cleared() {
    let client = client(collector_ok()).await;
    activity_log_core::diagnostics::warn("tests", "logs_can_be_read_and_cleared");
    // The log is shared with the tests running alongside, so only this test's line counts.
    let logged = |body: LogsResponse| {
        body.entries
            .iter()
            .any(|entry| entry.message == "logs_can_be_read_and_cleared")
    };

    let response = client.get("/api/logs").dispatch().await;
    assert_eq!(response.status(), Status::Ok);
    assert!(logged(response.into_json().await.expect("logs json")));

    let response = client.delete("/api/logs").dispatch().await;
    assert_eq!(response.status(), Status::NoContent);
    let body: LogsResponse = client
        .get("/api/logs")
        .dispatch()
        .await
        .into_json()
        .await
        .expect("logs json");
    assert!(!logged(body));
}

async fn post_export<'a>(
    client: &'a Client,
    body: &'a str,
) -> rocket::local::asynchronous::LocalResponse<'a> {
    client
        .post("/api/export")
        .header(rocket::http::ContentType::JSON)
        .body(body)
        .dispatch()
        .await
}

#[rocket::async_test]
async fn exports_the_entries_it_is_sent_as_a_file() {
    let client = client(collector_ok()).await;

    let response = post_export(
        &client,
        r#"{"format":"markdown","start":"2026-03-01T23:00:00Z","end":"2026-03-02T23:00:00Z","timeZone":"Europe/Berlin","entries":[]}"#,
    )
    .await;

    assert_eq!(response.status(), Status::Ok);
    let file: ExportFile = response.into_json().await.expect("export json");
    assert_eq!(file.file_name, "activity-2026-03-02-2026-03-02.md");
    assert!(file.content.contains("Total: 0 events"));
}

#[rocket::async_test]
async fn rejects_an_export_in_an_unknown_zone() {
    let client = client(collector_ok()).await;

    let response = post_export(
        &client,
        r#"{"format":"json","start":"2026-03-01T23:00:00Z","end":"2026-03-02T23:00:00Z","timeZone":"Local","entries":[]}"#,
    )
    .await;

    assert_eq!(response.status(), Status::BadRequest);
    let body: ErrorResponse = response.into_json().await.expect("error json");
    assert_eq!(body.error, r#"Unknown time zone "Local""#);
}

/// A week of meetings with long descriptions, as the dashboard sends it: past Rocket's 1 MiB JSON
/// default, with Outlook and Calendar entries the untagged `ActivityEntry` must tell apart.
fn many_meetings() -> String {
    let description = "Agenda and notes. ".repeat(220);
    let meeting = |source: &str, index: usize| {
        let mut entry = serde_json::json!({
            "source": source,
            "connection": "default",
            "id": format!("{source}-{index}"),
            "timestamp": format!("2026-03-02T{:02}:{:02}:00Z", 8 + index % 10, index % 60),
            "title": format!("{source} meeting {index}"),
            "durationMinutes": 30,
            "meetingWeight": "busy",
            "description": description,
            "descriptionSegments": [{ "kind": "text", "text": description }],
        });
        if source == "outlook" {
            entry["isOrganizer"] = serde_json::Value::Bool(false);
        }
        entry
    };
    let entries = (0..150)
        .flat_map(|index| [meeting("outlook", index), meeting("calendar", index)])
        .collect::<Vec<_>>();
    serde_json::json!({
        "format": "markdown",
        "start": "2026-03-01T23:00:00Z",
        "end": "2026-03-08T23:00:00Z",
        "timeZone": "Europe/Berlin",
        "entries": entries,
    })
    .to_string()
}

#[rocket::async_test]
async fn exports_more_entries_than_the_default_json_limit_allows() {
    let client = client(collector_ok()).await;
    let body = many_meetings();
    assert!(body.len() > 1024 * 1024, "the body is {} bytes", body.len());

    let response = post_export(&client, &body).await;

    assert_eq!(response.status(), Status::Ok);
    let file: ExportFile = response.into_json().await.expect("export json");
    assert!(file.content.contains("Total: 300 events"));
    assert_eq!(
        file.content
            .matches("**[outlook]** outlook meeting")
            .count(),
        150
    );
    assert_eq!(
        file.content
            .matches("**[calendar]** calendar meeting")
            .count(),
        150
    );
}

#[rocket::async_test]
async fn rejects_a_body_that_is_not_an_export_request() {
    let client = client(collector_ok()).await;

    let response = post_export(&client, r#"{"format":"pdf"}"#).await;

    assert_eq!(response.status(), Status::BadRequest);
    let body: ErrorResponse = response.into_json().await.expect("error json");
    assert!(
        body.error.starts_with("Not an export request"),
        "{}",
        body.error
    );
}

#[rocket::async_test]
async fn post_collect_rejects_a_range_of_centuries() {
    let client = client(collector_ok()).await;

    let response = post_collect(
        &client,
        r#"{"start":"1000-01-01T00:00:00Z","end":"9999-12-31T00:00:00Z"}"#,
    )
    .await;

    assert_eq!(response.status(), Status::BadRequest);
    let body: ErrorResponse = response.into_json().await.expect("error json");
    assert_eq!(body.error, "A range can span at most 10 years (3653 days)");
}

#[rocket::async_test]
async fn recheck_answers_with_the_connection_checking_and_404_for_an_unknown_one() {
    let calendar = Connection::new(
        ConnectionId::new(Source::Calendar, "team"),
        "Team",
        ConnectionLocation::Local,
        ConnectionValues::default(),
    );
    let client = Client::tracked(build_rocket_with(
        Host::fixed(&[calendar], spawner()),
        false,
    ))
    .await
    .unwrap();

    let response = client
        .post("/api/connections/recheck")
        .header(rocket::http::ContentType::JSON)
        .body(r#"{"source":"calendar","name":"team"}"#)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Ok);
    let body: serde_json::Value = response.into_json().await.unwrap();
    assert_eq!(body["providers"][0]["status"], "checking");
    // The answer as before, plus the stamp of the providers it installed.
    let state: serde_json::Value = client
        .get("/api/runtime-state")
        .dispatch()
        .await
        .into_json()
        .await
        .unwrap();
    assert_eq!(body["stamp"]["epoch"], state["providers"]["epoch"]);
    assert_eq!(body["stamp"]["authority"], state["providers"]["authority"]);
    assert!(body["stamp"]["revision"].as_u64() <= state["providers"]["revision"].as_u64());

    let response = client
        .post("/api/connections/recheck")
        .header(rocket::http::ContentType::JSON)
        .body(r#"{"source":"calendar","name":"other"}"#)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::NotFound);
}

/// A file that does not parse stops the server with the place, never the text on the bad line.
#[rocket::async_test]
async fn a_config_file_that_does_not_parse_stops_the_server_without_its_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[gitlab]]\ntoken = \"glpat-STARTUP-SECRET\n").unwrap();

    let message = format!(
        "{:#}",
        crate::build_rocket(Some(&path)).await.err().unwrap()
    );

    assert!(message.contains("line 2, column"), "{message}");
    assert!(!message.contains("STARTUP-SECRET"), "{message}");
}

/// The CLI or the desktop app changed the file: the next request already sees it.
#[rocket::async_test]
async fn a_request_sees_a_config_file_changed_since_the_last_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "").unwrap();
    let host = Host::start(
        Some(path.clone()),
        spawner(),
        activity_log_core::runtime::BadConfig::Fail,
    )
    .unwrap();
    let client = Client::tracked(build_rocket_with(host, false))
        .await
        .unwrap();

    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    let body: serde_json::Value = client
        .get("/api/providers")
        .dispatch()
        .await
        .into_json()
        .await
        .unwrap();

    assert_eq!(body["providers"][0]["connection"]["source"], "calendar");
}

#[rocket::async_test]
async fn the_runtime_state_answers_both_parts_of_one_host() {
    let client = client(collector_ok()).await;

    let response = client.get("/api/runtime-state").dispatch().await;
    assert_eq!(
        response.status(),
        Status::Ok,
        "also before the first collection"
    );
    let state: serde_json::Value = response.into_json().await.unwrap();
    assert_eq!(
        state["activity"]["state"],
        serde_json::json!({ "kind": "empty" })
    );
    assert_eq!(state["activity"]["epoch"], state["providers"]["epoch"]);
    assert_eq!(
        state["activity"]["authority"],
        state["providers"]["authority"]
    );
    assert!(state["providers"]["providers"]["providers"].is_array());
}

#[rocket::async_test]
async fn a_collect_answers_its_log_as_before_with_the_stamp_of_the_state_it_installed() {
    let release = Arc::new(Notify::new());
    let client = client(collector_held(&release)).await;

    let started: serde_json::Value = post_collect(&client, MARCH_FIRST_WEEK)
        .await
        .into_json()
        .await
        .unwrap();
    let state: serde_json::Value = client
        .get("/api/runtime-state")
        .dispatch()
        .await
        .into_json()
        .await
        .unwrap();

    assert_eq!(started["meta"]["start"], "2026-03-01T00:00:00+00:00");
    // The host's own kind for it, never left to be read off the pending list.
    assert_eq!(started["kind"], "running");
    assert_eq!(started["stamp"]["epoch"], state["activity"]["epoch"]);
    assert_eq!(started["stamp"]["revision"], state["activity"]["revision"]);
    assert_eq!(state["activity"]["state"]["kind"], "running");
    assert_eq!(state["activity"]["state"]["log"]["meta"], started["meta"]);
    release.notify_one();
}
