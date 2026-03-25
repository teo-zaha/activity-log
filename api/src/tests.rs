use std::{sync::{Arc, Mutex}, time::Duration};

use activity_log_core::model::{ActivityLog, ActivityLogMeta, CollectOptions};
use rocket::{http::Status, local::asynchronous::Client};

use crate::{AppState, build_rocket_with};

fn fixture_log() -> ActivityLog {
    ActivityLog {
        meta: ActivityLogMeta {
            generated: "2026-03-25T10:00:00Z".to_string(),
            since: "2026-03-01".to_string(),
            until: "2026-03-25".to_string(),
        },
        entries: Vec::new(),
    }
}

#[rocket::async_test]
async fn returns_503_without_cache() {
    let collector = Arc::new(|_options: CollectOptions| Box::pin(async { Ok(fixture_log()) }));
    let client = Client::tracked(build_rocket_with(collector, false))
        .await
        .unwrap();

    let response = client.get("/api/activity").dispatch().await;
    assert_eq!(response.status(), Status::ServiceUnavailable);
}

#[rocket::async_test]
async fn returns_202_while_collecting() {
    let collector = Arc::new(|_options: CollectOptions| {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(fixture_log())
        })
    });
    let rocket = build_rocket_with(collector, false);
    let state = rocket.state::<AppState>().unwrap();
    state
        .is_collecting
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let client = Client::tracked(rocket).await.unwrap();
    let response = client.get("/api/activity").dispatch().await;
    assert_eq!(response.status(), Status::Accepted);
}

#[rocket::async_test]
async fn post_collect_returns_log() {
    let collector = Arc::new(|_options: CollectOptions| Box::pin(async { Ok(fixture_log()) }));
    let client = Client::tracked(build_rocket_with(collector, false))
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
    let collector = Arc::new(|_options: CollectOptions| Box::pin(async { Ok(fixture_log()) }));
    let client = Client::tracked(build_rocket_with(collector, false))
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
    let collector = Arc::new(move |options: CollectOptions| {
        let collector_capture = Arc::clone(&collector_capture);
        Box::pin(async move {
            *collector_capture.lock().unwrap() = Some(options);
            Ok(fixture_log())
        })
    });
    let client = Client::tracked(build_rocket_with(collector, false))
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
    assert_eq!(options.since.len(), 10);
    assert_eq!(options.until.len(), 10);
}

#[rocket::async_test]
async fn post_collect_rejects_mixed_request_modes() {
    let collector = Arc::new(|_options: CollectOptions| Box::pin(async { Ok(fixture_log()) }));
    let client = Client::tracked(build_rocket_with(collector, false))
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
