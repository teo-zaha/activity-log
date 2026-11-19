use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use activity_log_core::model::{
    ActivityLog, ActivityLogMeta, CollectOptions, CollectedActivitySummary,
    CollectionFailedResponse, ConnectionId, LogsResponse, ProviderInfo, ProviderStatus, Source,
    SupportedOs,
};
use activity_log_core::runtime::{CollectionRuntime, Collector, CollectorFuture, Progress};
use rocket::{http::Status, local::asynchronous::Client};
use tokio::sync::Notify;

use crate::{ApiRuntime, build_rocket_with};

fn fixture_log() -> ActivityLog {
    ActivityLog {
        meta: ActivityLogMeta {
            generated: "2026-03-25T10:00:00Z".to_string(),
            start: "2026-03-01T00:00:00Z".to_string(),
            end: "2026-03-26T00:00:00Z".to_string(),
            pending: Vec::new(),
        },
        summary: CollectedActivitySummary::default(),
        entries: Vec::new(),
    }
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
    Arc::new(
        |_options: CollectOptions, _progress: Progress| -> CollectorFuture {
            Box::pin(async { Ok(fixture_log()) })
        },
    )
}

/// Finishes once `release` is notified.
fn collector_held(release: &Arc<Notify>) -> Collector {
    let release = release.clone();
    Arc::new(move |_options, _progress| -> CollectorFuture {
        let release = release.clone();
        Box::pin(async move {
            release.notified().await;
            Ok(fixture_log())
        })
    })
}

const MARCH_FIRST_WEEK: &str = r#"{"start":"2026-03-01T00:00:00Z","end":"2026-03-06T00:00:00Z"}"#;

async fn client(collector: Collector) -> Client {
    let runtime = CollectionRuntime::new(collector, fixture_providers());
    Client::tracked(build_rocket_with(ApiRuntime::fixed(runtime), false))
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
async fn a_second_collect_answers_202_with_the_running_range() {
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
    assert_eq!(second["meta"]["start"], "2026-03-01T00:00:00+00:00");
}

#[rocket::async_test]
async fn reports_a_failed_collection_with_its_range() {
    let collector: Collector = Arc::new(|_options, _progress| -> CollectorFuture {
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
    let collector: Collector = Arc::new(
        move |options: CollectOptions, _progress| -> CollectorFuture {
            let collector_capture = Arc::clone(&collector_capture);
            Box::pin(async move {
                *collector_capture.lock().unwrap() = Some(options);
                Ok(fixture_log())
            })
        },
    );
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
