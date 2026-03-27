use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use activity_log_core::model::{
    ActivityLog, ActivityLogMeta, CollectOptions, CollectedActivitySummary, ProviderInfo,
    ProviderStatus, Source, SupportedOs,
};
use activity_log_core::runtime::{CollectionRuntime, Collector, CollectorFuture};
use rocket::{http::Status, local::asynchronous::Client};

use crate::build_rocket_with;

fn fixture_log() -> ActivityLog {
    ActivityLog {
        meta: ActivityLogMeta {
            generated: "2026-03-25T10:00:00Z".to_string(),
            since: "2026-03-01".to_string(),
            until: "2026-03-25".to_string(),
        },
        summary: CollectedActivitySummary::default(),
        entries: Vec::new(),
    }
}

fn fixture_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: Source::Git,
            status: ProviderStatus::Available,
            usable: true,
            supported_os: vec![SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
            reason: None,
            checked_at: "2026-03-25T10:00:00Z".to_string(),
        },
        ProviderInfo {
            id: Source::Outlook,
            status: ProviderStatus::Unavailable,
            usable: false,
            supported_os: vec![SupportedOs::Windows],
            reason: Some("OS not supported".to_string()),
            checked_at: "2026-03-25T10:00:00Z".to_string(),
        },
    ]
}

fn collector_ok() -> Collector {
    Arc::new(|_options: CollectOptions| -> CollectorFuture {
        Box::pin(async { Ok(fixture_log()) })
    })
}

#[rocket::async_test]
async fn returns_503_without_cache() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());
    let client = Client::tracked(build_rocket_with(runtime, false))
        .await
        .unwrap();

    let response = client.get("/api/activity").dispatch().await;
    assert_eq!(response.status(), Status::ServiceUnavailable);
}

#[rocket::async_test]
async fn returns_202_while_collecting() {
    let collector: Collector = Arc::new(|_options: CollectOptions| -> CollectorFuture {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(fixture_log())
        })
    });
    let runtime = CollectionRuntime::new(collector, fixture_providers());
    let task = runtime
        .initial_collection_task()
        .expect("initial collection task");
    tokio::spawn(task);

    for _ in 0..5 {
        if runtime.is_collecting() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let client = Client::tracked(build_rocket_with(runtime, false))
        .await
        .unwrap();
    let response = client.get("/api/activity").dispatch().await;
    assert_eq!(response.status(), Status::Accepted);
}

#[rocket::async_test]
async fn post_collect_returns_log() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());
    let client = Client::tracked(build_rocket_with(runtime, false))
        .await
        .unwrap();

    let response = client
        .post("/api/collect")
        .header(rocket::http::ContentType::JSON)
        .body(r#"{"since":"2026-03-01","until":"2026-03-05"}"#)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Ok);
}

#[rocket::async_test]
async fn post_collect_validates_dates() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());
    let client = Client::tracked(build_rocket_with(runtime, false))
        .await
        .unwrap();

    let response = client
        .post("/api/collect")
        .header(rocket::http::ContentType::JSON)
        .body(r#"{"since":"bad","until":"2026-03-05"}"#)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::BadRequest);
}

#[rocket::async_test]
async fn post_collect_accepts_preset() {
    let captured = Arc::new(Mutex::new(None));
    let collector_capture = Arc::clone(&captured);
    let collector: Collector = Arc::new(move |options: CollectOptions| -> CollectorFuture {
        let collector_capture = Arc::clone(&collector_capture);
        Box::pin(async move {
            *collector_capture.lock().unwrap() = Some(options);
            Ok(fixture_log())
        })
    });
    let runtime = CollectionRuntime::new(collector, fixture_providers());
    let client = Client::tracked(build_rocket_with(runtime, false))
        .await
        .unwrap();

    let response = client
        .post("/api/collect")
        .header(rocket::http::ContentType::JSON)
        .body(r#"{"preset":"today"}"#)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Ok);

    let options = captured.lock().unwrap().clone().unwrap();
    assert_eq!(options.since.to_string().len(), 10);
    assert_eq!(options.until.to_string().len(), 10);
}

#[rocket::async_test]
async fn post_collect_rejects_mixed_request_modes() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());
    let client = Client::tracked(build_rocket_with(runtime, false))
        .await
        .unwrap();

    let response = client
        .post("/api/collect")
        .header(rocket::http::ContentType::JSON)
        .body(r#"{"since":"2026-03-01","until":"2026-03-05","preset":"today"}"#)
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::BadRequest);
}

#[rocket::async_test]
async fn returns_provider_statuses() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());
    let client = Client::tracked(build_rocket_with(runtime, false))
        .await
        .unwrap();

    let response = client.get("/api/providers").dispatch().await;
    assert_eq!(response.status(), Status::Ok);

    let body = response.into_string().await.unwrap();
    assert!(body.contains("\"id\":\"git\""));
    assert!(body.contains("\"id\":\"outlook\""));
    assert!(body.contains("\"usable\":false"));
}
