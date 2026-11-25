use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use activity_log_core::model::{
    BackendError, BackendErrorKind, CollectOptions, CollectionFailedResponse, ConnectionId,
    ExportFile, LogsResponse, ProviderInfo, ProviderStatus, Source, SupportedOs,
};
use activity_log_core::provider::connection::{Connection, ConnectionLocation, ConnectionValues};
use activity_log_core::runtime::{
    Collector, FixSurface, Host, Outcome, Plan, RunReport, Slot, Stored, Superseded,
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
            recheckable: false,
            fix_action: None,
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
            recheckable: false,
            fix_action: None,
            checked_at: "2026-03-25T10:00:00Z".to_string(),
        },
    ]
}

fn collector_ok() -> Collector {
    Arc::new(|_options, plans, _progress| Box::pin(async move { settle(plans).await }))
}

/// Finishes once `release` is notified.
fn collector_held(release: &Arc<Notify>) -> Collector {
    let release = release.clone();
    Arc::new(move |_options, plans, _progress| {
        let release = release.clone();
        Box::pin(async move {
            release.notified().await;
            settle(plans).await
        })
    })
}

const MARCH_FIRST_WEEK: &str = r#"{"start":"2026-03-01T00:00:00Z","end":"2026-03-06T00:00:00Z"}"#;

async fn client(collector: Collector) -> Client {
    client_for(
        collector,
        FixSurface::Web,
        activity_log_core::provider::current_os(),
    )
    .await
}

async fn client_for(collector: Collector, surface: FixSurface, os: SupportedOs) -> Client {
    let host = Host::with_collector(collector, fixture_providers(), spawner(), surface, os);
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

/// The error a response carries, after checking its status.
async fn error_with_status(
    response: rocket::local::asynchronous::LocalResponse<'_>,
    status: Status,
) -> BackendError {
    assert_eq!(response.status(), status);
    response.into_json().await.expect("an error body")
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
    let error = error_with_status(response, Status::ServiceUnavailable).await;
    assert_eq!(error.kind, BackendErrorKind::NoData);
}

/// A host that shows nothing (it is quitting before its first collection) has nothing to
/// answer a collect with; the desktop's `collect` agrees.
#[rocket::async_test]
async fn a_collect_with_nothing_to_show_has_no_data() {
    let host = Host::with_collector(
        collector_ok(),
        fixture_providers(),
        spawner(),
        FixSurface::Web,
        activity_log_core::provider::current_os(),
    );
    host.shutdown();
    let client = Client::tracked(build_rocket_with(host, false))
        .await
        .unwrap();

    let response = post_collect(&client, MARCH_FIRST_WEEK).await;

    let error = error_with_status(response, Status::ServiceUnavailable).await;
    assert_eq!(error.kind, BackendErrorKind::NoData);
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

/// A run fails as a whole only when it stops unexpectedly (a panic); each connection's own
/// failure is listed with that connection.
#[rocket::async_test]
async fn reports_a_failed_collection_with_its_range() {
    let collector: Collector =
        Arc::new(|_options, _plans, _progress| Box::pin(async { panic!("the collector broke") }));
    let client = client(collector).await;

    let collect = post_collect(&client, MARCH_FIRST_WEEK).await;
    assert_eq!(collect.status(), Status::Accepted);

    // A failure answers 200 like a log; only a running collection answers 202.
    let mut body = activity_with_status(&client, Status::Ok).await;
    let body: CollectionFailedResponse = serde_json::from_value(body.take()).unwrap();
    assert_eq!(body.start, "2026-03-01T00:00:00+00:00");
    assert_eq!(body.end, "2026-03-06T00:00:00+00:00");
    assert!(body.error.contains("the collector broke"), "{}", body.error);
}

#[rocket::async_test]
async fn post_collect_rejects_plain_dates() {
    let client = client(collector_ok()).await;

    let response = post_collect(&client, r#"{"start":"2026-03-01","end":"2026-03-05"}"#).await;
    let error = error_with_status(response, Status::BadRequest).await;
    assert_eq!(error.kind, BackendErrorKind::InvalidRequest);
}

/// Rocket's own answers keep the contract: a body that is no JSON, a body that is not the
/// route's request and an unknown route all answer a `BackendError`.
#[rocket::async_test]
async fn answers_rocket_makes_itself_are_backend_errors() {
    let client = client(collector_ok()).await;

    let broken = post_collect(&client, r#"{"start":"#).await;
    let error = error_with_status(broken, Status::BadRequest).await;
    assert_eq!(error.kind, BackendErrorKind::InvalidRequest);

    let other_shape = post_collect(&client, r#"{"from":"2026-03-01"}"#).await;
    let error = error_with_status(other_shape, Status::UnprocessableEntity).await;
    assert_eq!(error.kind, BackendErrorKind::InvalidRequest);

    let unknown = client.get("/api/nothing-here").dispatch().await;
    let error = error_with_status(unknown, Status::NotFound).await;
    assert_eq!(error.kind, BackendErrorKind::NotFound);
}

#[rocket::async_test]
async fn post_collect_passes_the_exact_range_to_the_collector() {
    let captured = Arc::new(Mutex::new(None));
    let collector_capture = Arc::clone(&captured);
    let collector: Collector = Arc::new(move |options: CollectOptions, plans, _progress| {
        let collector_capture = Arc::clone(&collector_capture);
        Box::pin(async move {
            *collector_capture.lock().unwrap() = Some(options);
            settle(plans).await
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

    let error = error_with_status(response, Status::BadRequest).await;
    assert_eq!(error.kind, BackendErrorKind::InvalidRequest);
}

/// B38: an entry is exported as the kind its `source` names, or the export is refused; a Git
/// commit sent as `jira` is no Jira entry.
#[rocket::async_test]
async fn rejects_an_export_whose_entry_contradicts_its_source() {
    let client = client(collector_ok()).await;

    let response = post_export(
        &client,
        r#"{"format":"json","start":"2026-03-01T23:00:00Z","end":"2026-03-02T23:00:00Z","timeZone":"UTC","entries":[{"source":"jira","connection":"work","id":"1f3a","timestamp":"2026-03-02T09:30:00Z","repo":"api","hash":"1f3a","message":"Fix","author":"dev@example.org"}]}"#,
    )
    .await;

    let error = error_with_status(response, Status::BadRequest).await;
    assert_eq!(error.kind, BackendErrorKind::InvalidRequest);
}

/// An entry that names an empty connection is refused like one that names none.
#[rocket::async_test]
async fn rejects_an_export_whose_entry_names_an_empty_connection() {
    let client = client(collector_ok()).await;

    let response = post_export(
        &client,
        r#"{"format":"json","start":"2026-03-01T23:00:00Z","end":"2026-03-02T23:00:00Z","timeZone":"UTC","entries":[{"source":"system","connection":"","id":"boot-1","type":"boot","timestamp":"2026-03-02T07:58:00Z","label":"Boot"}]}"#,
    )
    .await;

    let error = error_with_status(response, Status::BadRequest).await;
    assert_eq!(error.kind, BackendErrorKind::InvalidRequest);
}

/// A week of meetings with long descriptions, as the dashboard sends it: past Rocket's 1 MiB JSON
/// default, with Outlook and Calendar entries, each read as the kind its `source` names.
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

    let error = error_with_status(response, Status::BadRequest).await;
    assert_eq!(error.kind, BackendErrorKind::InvalidRequest);
}

#[rocket::async_test]
async fn post_collect_rejects_a_range_of_centuries() {
    let client = client(collector_ok()).await;

    let response = post_collect(
        &client,
        r#"{"start":"1000-01-01T00:00:00Z","end":"9999-12-31T00:00:00Z"}"#,
    )
    .await;

    let error = error_with_status(response, Status::BadRequest).await;
    assert_eq!(error.kind, BackendErrorKind::InvalidRequest);
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
    assert!(body["providers"][0].get("fixAction").is_none());
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
    let error = error_with_status(response, Status::NotFound).await;
    assert_eq!(error.kind, BackendErrorKind::NotFound);
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
        activity_log_core::secrets::Secrets::none(),
        FixSurface::Web,
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

/// A host that shut down still lists its connections; a recheck of one is refused as the
/// host's failure, never as a connection it does not have. The desktop's command agrees.
#[rocket::async_test]
async fn a_recheck_after_shutdown_is_internal_not_not_found() {
    let calendar = Connection::new(
        ConnectionId::new(Source::Calendar, "team"),
        "Team",
        ConnectionLocation::Local,
        ConnectionValues::default(),
    );
    let host = Host::fixed(&[calendar], spawner());
    host.shutdown();
    let client = Client::tracked(build_rocket_with(host, false))
        .await
        .unwrap();

    let response = client
        .post("/api/connections/recheck")
        .header(rocket::http::ContentType::JSON)
        .body(r#"{"source":"calendar","name":"team"}"#)
        .dispatch()
        .await;

    let error = error_with_status(response, Status::InternalServerError).await;
    assert_eq!(error.kind, BackendErrorKind::Internal);
}

/// A body of another media type matches no route, so it answers a route Rocket does not have
/// (`404`, `notFound`), not a refused request.
#[rocket::async_test]
async fn a_collect_sent_as_plain_text_matches_no_route() {
    let client = client(collector_ok()).await;

    let response = client
        .post("/api/collect")
        .header(rocket::http::ContentType::Plain)
        .body(MARCH_FIRST_WEEK)
        .dispatch()
        .await;

    let error = error_with_status(response, Status::NotFound).await;
    assert_eq!(error.kind, BackendErrorKind::NotFound);
}

// GET /api/events (§5): read incrementally, never to the end of an open stream.

/// An open event stream, read one event at a time.
struct Events<'a> {
    response: rocket::local::asynchronous::LocalResponse<'a>,
    buffer: String,
}

/// One server-sent event: its name and its data, parsed.
#[derive(Debug)]
struct Sent {
    event: String,
    data: serde_json::Value,
}

impl<'a> Events<'a> {
    async fn open(client: &'a Client) -> Events<'a> {
        let response = client.get("/api/events").dispatch().await;
        assert_eq!(response.status(), Status::Ok);
        assert_eq!(
            response.content_type(),
            Some(rocket::http::ContentType::EventStream)
        );
        Events {
            response,
            buffer: String::new(),
        }
    }

    /// The next event, skipping heartbeats; `None` once the stream ended.
    async fn next(&mut self) -> Option<Sent> {
        use rocket::tokio::io::AsyncReadExt;
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let block = self.buffer[..end].to_string();
                self.buffer.drain(..end + 2);
                let mut event = None;
                let mut data = None;
                for line in block.lines() {
                    if let Some(name) = line.strip_prefix("event:") {
                        event = Some(name.trim().to_string());
                    } else if let Some(json) = line.strip_prefix("data:") {
                        assert!(data.is_none(), "one data line per event");
                        data = Some(serde_json::from_str(json.trim()).unwrap());
                    }
                }
                match (event, data) {
                    (Some(event), Some(data)) => return Some(Sent { event, data }),
                    // A heartbeat (a comment) or a field without an event.
                    _ => continue,
                }
            }
            let mut chunk = [0u8; 8192];
            // Longer than a ping's interval: a stream is never quiet for that long.
            let wait = activity_log_core::runtime::PING_INTERVAL + Duration::from_secs(5);
            let read = tokio::time::timeout(wait, self.response.read(&mut chunk))
                .await
                .expect("an event within the ping interval")
                .unwrap();
            if read == 0 {
                return None;
            }
            self.buffer
                .push_str(std::str::from_utf8(&chunk[..read]).unwrap());
        }
    }

    async fn until(&mut self, done: impl Fn(&Sent) -> bool) -> Vec<Sent> {
        let mut seen = Vec::new();
        loop {
            let sent = self.next().await.expect("the stream is open");
            let finished = done(&sent);
            seen.push(sent);
            if finished {
                return seen;
            }
        }
    }
}

fn host_of(client: &Client) -> &Host {
    client.rocket().state::<Host>().expect("the host")
}

#[rocket::async_test]
async fn providers_and_events_only_publish_native_actions_for_a_desktop_host() {
    use activity_log_core::{
        provider::source_failure::{FixCause, SourceFailure},
        runtime::CollectFailure,
    };

    let collector: Collector = Arc::new(|_, plans, _| {
        Box::pin(async move {
            let mut report = settle(plans).await;
            let (id, slot) = &mut report.slots[0];
            assert_eq!(id.source, Source::Git);
            let Slot::Settled(Outcome::Collected(stored)) = slot else {
                panic!("expected a collected Git slot")
            };
            let error =
                SourceFailure::caused(FixCause::FolderDenied, "Example folder permission failure");
            *slot = Slot::Settled(Outcome::Failed {
                key: stored.key,
                failure: CollectFailure::of(&error.into()),
                carried: None,
            });
            report
        })
    });
    for surface in [FixSurface::Web, FixSurface::Desktop] {
        let client = client_for(collector.clone(), surface, SupportedOs::Macos).await;
        let expected = (surface == FixSurface::Desktop).then(|| {
            serde_json::json!({
                "kind": "local", "target": {"kind": "systemSettings", "pane": "filesAndFolders"}
            })
        });
        let mut events = Events::open(&client).await;
        assert_eq!(events.next().await.unwrap().event, "state");

        post_collect(&client, MARCH_FIRST_WEEK).await;
        let sent = tokio::time::timeout(
            Duration::from_secs(5),
            events.until(|sent| sent.event == "providers"),
        )
        .await
        .expect("the collection failure publishes providers");
        let incremental = &sent.last().unwrap().data["providers"]["providers"][0];
        assert_eq!(incremental["problem"], "permissionDenied");
        assert_eq!(incremental.get("fixAction"), expected.as_ref());

        let mut fresh = Events::open(&client).await;
        let full = fresh.next().await.unwrap();
        assert_eq!(full.event, "state");
        let base: serde_json::Value = client
            .get("/api/providers")
            .dispatch()
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
        for provider in [
            &base["providers"][0],
            &state["providers"]["providers"]["providers"][0],
            &full.data["providers"]["providers"]["providers"][0],
        ] {
            assert_eq!(provider["problem"], "permissionDenied");
            assert_eq!(provider.get("fixAction"), expected.as_ref());
        }
    }
}

#[rocket::async_test]
async fn build_rocket_installs_a_web_surface_host() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "").unwrap();
    let rocket = crate::build_rocket(Some(&path)).await.unwrap();
    assert_eq!(
        rocket.state::<Host>().unwrap().fix_surface(),
        FixSurface::Web
    );
}

#[rocket::async_test]
async fn the_event_stream_starts_with_both_parts_of_one_host_also_before_any_collection() {
    let client = client(collector_ok()).await;
    let mut events = Events::open(&client).await;

    let first = events.next().await.unwrap();

    assert_eq!(first.event, "state");
    let state = &first.data;
    assert_eq!(
        state["activity"]["state"],
        serde_json::json!({ "kind": "empty" })
    );
    assert_eq!(state["activity"]["epoch"], state["providers"]["epoch"]);
    assert_eq!(
        state["activity"]["authority"],
        state["providers"]["authority"]
    );
    let read: serde_json::Value = client
        .get("/api/runtime-state")
        .dispatch()
        .await
        .into_json()
        .await
        .unwrap();
    assert_eq!(*state, read, "the pair a plain read answers");
}

#[rocket::async_test]
async fn a_collection_reaches_the_stream_as_activity_parts_of_rising_revisions() {
    let release = Arc::new(Notify::new());
    let client = client(collector_held(&release)).await;
    let mut events = Events::open(&client).await;
    let first = events.next().await.unwrap();

    post_collect(&client, MARCH_FIRST_WEEK).await;
    let running = events.next().await.unwrap();
    release.notify_one();
    let seen = events
        .until(|sent| sent.event == "activity" && sent.data["state"]["kind"] == "ready")
        .await;

    // The collect was installed before it ran on: the run is held, so it is seen running.
    assert_eq!(running.event, "activity");
    assert_eq!(running.data["state"]["kind"], "running");
    let mut revisions = vec![first.data["activity"]["revision"].as_u64().unwrap()];
    for sent in std::iter::once(&running).chain(&seen) {
        assert_eq!(sent.data["authority"], first.data["activity"]["authority"]);
        assert_eq!(sent.data["epoch"], first.data["activity"]["epoch"]);
        if sent.event == "activity" {
            revisions.push(sent.data["revision"].as_u64().unwrap());
        }
    }
    assert!(
        revisions.windows(2).all(|pair| pair[0] < pair[1]),
        "{revisions:?}"
    );
}

/// A host of the config file at `path` and a client of it.
async fn client_at(path: &std::path::Path) -> Client {
    let host = Host::start(
        Some(path.to_path_buf()),
        spawner(),
        activity_log_core::runtime::BadConfig::Fail,
        activity_log_core::secrets::Secrets::none(),
        FixSurface::Web,
    )
    .unwrap();
    Client::tracked(build_rocket_with(host, false))
        .await
        .unwrap()
}

/// A config reload changes the authority: an open stream gets one `state` with it, keeps
/// going, and its next messages are parts under it.
#[rocket::async_test]
async fn a_reload_reaches_an_open_stream_as_one_state_and_the_stream_stays() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "").unwrap();
    let client = client_at(&path).await;
    let mut events = Events::open(&client).await;
    let first = events.next().await.unwrap();
    let authority = first.data["activity"]["authority"].as_str().unwrap();

    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    host_of(&client).reload_if_changed().await;
    let installed = events.next().await.unwrap();

    assert_eq!(installed.event, "state");
    assert_ne!(installed.data["activity"]["authority"], authority);
    assert_eq!(
        installed.data["providers"]["providers"]["providers"][0]["connection"]["source"],
        "calendar"
    );
    // The calendar's check (no URL) ends at once: a providers part under the new authority.
    let checked = events.until(|sent| sent.event == "providers").await;
    assert!(checked.iter().all(|sent| sent.event != "state"));
    assert_eq!(
        checked.last().unwrap().data["authority"],
        installed.data["activity"]["authority"]
    );
}

/// A reader that read nothing while the host installed a config and went on gets the latest
/// pair, never the parts of the authority it missed.
#[rocket::async_test]
async fn a_reader_that_lagged_over_an_install_gets_one_pair_of_the_latest_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "").unwrap();
    let client = client_at(&path).await;
    let mut events = Events::open(&client).await;
    events.next().await.unwrap();

    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    host_of(&client).reload_if_changed().await;
    let mut published = host_of(&client).subscribe();
    published
        .wait_for(|value| {
            value
                .providers
                .value
                .providers
                .iter()
                .all(|info| info.status != activity_log_core::model::ProviderStatus::Checking)
        })
        .await
        .unwrap();
    let latest = host_of(&client).runtime_state();

    let next = events.next().await.unwrap();

    assert_eq!(next.event, "state");
    assert_eq!(next.data, serde_json::to_value(&latest).unwrap());
}

#[rocket::async_test]
async fn two_readers_each_get_a_first_pair_and_the_same_parts() {
    let release = Arc::new(Notify::new());
    let client = client(collector_held(&release)).await;
    let mut first = Events::open(&client).await;
    let mut second = Events::open(&client).await;
    let a = first.next().await.unwrap();
    let b = second.next().await.unwrap();
    assert_eq!((a.event.as_str(), b.event.as_str()), ("state", "state"));
    assert_eq!(a.data, b.data);

    post_collect(&client, MARCH_FIRST_WEEK).await;

    let a = first.next().await.unwrap();
    let b = second.next().await.unwrap();
    assert_eq!(a.event, "activity");
    assert_eq!(a.data, b.data);
}

#[rocket::async_test]
async fn the_host_shutting_down_ends_the_stream() {
    let client = client(collector_ok()).await;
    let mut events = Events::open(&client).await;
    events.next().await.unwrap();

    host_of(&client).shutdown();

    assert!(events.next().await.is_none());
}

#[rocket::async_test]
async fn the_server_shutting_down_ends_the_stream() {
    let client = client(collector_ok()).await;
    let mut events = Events::open(&client).await;
    events.next().await.unwrap();

    client.rocket().shutdown().notify();

    assert!(events.next().await.is_none());
}

/// A quiet stream still says it is alive, as an event a browser sees, every `PING_INTERVAL`.
#[tokio::test(start_paused = true)]
async fn a_quiet_stream_sends_a_ping_of_its_host() {
    let client = client(collector_ok()).await;
    let mut events = Events::open(&client).await;
    let first = events.next().await.unwrap();
    let started = tokio::time::Instant::now();

    let ping = events.next().await.unwrap();

    assert_eq!(ping.event, "ping");
    assert_eq!(
        ping.data,
        serde_json::json!({ "epoch": first.data["activity"]["epoch"] })
    );
    assert!(started.elapsed() >= activity_log_core::runtime::PING_INTERVAL);
}

/// With `test-support`, which every crate's tests turn on, the front ends' own `Secrets` have
/// no OS store, so no test (nor a binary a test runs) can reach a keychain: the desktop's has
/// no store at all, the CLI's and the server's only a key file `ACTIVITY_LOG_SECRET_KEY_FILE`
/// would name.
#[test]
fn tests_never_reach_an_os_store() {
    use activity_log_core::{
        model::{KeyFileReason, NoStoreReason, SecretStoreInfo},
        secrets::Secrets,
    };
    assert_eq!(
        Secrets::desktop(None).store_info(),
        SecretStoreInfo::Unavailable {
            why: NoStoreReason::NoOsStore
        }
    );
    for secrets in [Secrets::cli(None), Secrets::api(None)] {
        assert_eq!(
            secrets.store_info(),
            SecretStoreInfo::File {
                why: KeyFileReason::NoOsStore
            }
        );
    }
}
