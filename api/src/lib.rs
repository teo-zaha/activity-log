use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use activity_log_core::{
    collect_activity_with_provider_runtimes,
    model::{
        ActivityLog, CollectOptions, CollectRequest, CollectingResponse, ErrorResponse,
        ProviderInfo, ProvidersResponse,
    },
    provider::{ProviderRuntime, current_os, inspect_provider_runtimes},
    utils::{days_ago, ensure_valid_date, resolve_range, today},
};
use rocket::{
    Build, Rocket, State, fairing::AdHoc, get, http::Status, post, response::status,
    serde::json::Json,
};
use tokio::sync::RwLock;

type CollectorFuture = Pin<Box<dyn Future<Output = anyhow::Result<ActivityLog>> + Send + 'static>>;
type Collector = Arc<dyn Fn(CollectOptions) -> CollectorFuture + Send + Sync>;

pub struct AppState {
    cached_log: RwLock<Option<ActivityLog>>,
    provider_info: Vec<ProviderInfo>,
    is_collecting: AtomicBool,
    // Injecting the collector keeps the HTTP layer testable without touching live integrations.
    collector: Collector,
}

impl AppState {
    pub fn new(collector: Collector, provider_info: Vec<ProviderInfo>) -> Self {
        Self {
            cached_log: RwLock::new(None),
            provider_info,
            is_collecting: AtomicBool::new(false),
            collector,
        }
    }

    async fn cached_log(&self) -> Option<ActivityLog> {
        self.cached_log.read().await.clone()
    }

    async fn store(&self, log: ActivityLog) {
        *self.cached_log.write().await = Some(log);
    }

    fn provider_info(&self) -> Vec<ProviderInfo> {
        self.provider_info.clone()
    }

    fn is_collecting(&self) -> bool {
        self.is_collecting.load(Ordering::SeqCst)
    }
}

#[get("/api/activity")]
async fn get_activity(state: &State<AppState>) -> status::Custom<Json<serde_json::Value>> {
    // The frontend polls this endpoint on startup and expects 202 until the first collection ends.
    if state.is_collecting() {
        return status::Custom(
            Status::Accepted,
            Json(serde_json::to_value(CollectingResponse { collecting: true }).unwrap()),
        );
    }

    if let Some(log) = state.cached_log().await {
        return status::Custom(Status::Ok, Json(serde_json::to_value(log).unwrap()));
    }

    status::Custom(
        Status::ServiceUnavailable,
        Json(
            serde_json::to_value(ErrorResponse {
                error: "No data yet".to_string(),
            })
            .unwrap(),
        ),
    )
}

#[get("/api/providers")]
async fn get_providers(state: &State<AppState>) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        current_os: current_os(),
        providers: state.provider_info(),
    })
}

#[post("/api/collect", format = "json", data = "<request>")]
async fn post_collect(
    state: &State<AppState>,
    request: Json<CollectRequest>,
) -> status::Custom<Json<serde_json::Value>> {
    let options = match resolve_collect_options(&request) {
        Ok(options) => options,
        Err(message) => return bad_request(message),
    };

    if state
        .is_collecting
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        // Reuse the same 202 contract instead of queueing overlapping full collections.
        return status::Custom(
            Status::Accepted,
            Json(serde_json::to_value(CollectingResponse { collecting: true }).unwrap()),
        );
    }

    let result = (state.collector)(options).await;
    state.is_collecting.store(false, Ordering::SeqCst);

    match result {
        Ok(log) => {
            state.store(log.clone()).await;
            status::Custom(Status::Ok, Json(serde_json::to_value(log).unwrap()))
        }
        Err(err) => status::Custom(
            Status::InternalServerError,
            Json(
                serde_json::to_value(ErrorResponse {
                    error: format!("Collection failed: {err}"),
                })
                .unwrap(),
            ),
        ),
    }
}

fn bad_request(message: &str) -> status::Custom<Json<serde_json::Value>> {
    status::Custom(
        Status::BadRequest,
        Json(
            serde_json::to_value(ErrorResponse {
                error: message.to_string(),
            })
            .unwrap(),
        ),
    )
}

fn resolve_collect_options(request: &CollectRequest) -> Result<CollectOptions, &'static str> {
    match (&request.since, &request.until, &request.preset) {
        (Some(_), Some(_), Some(_)) | (Some(_), None, Some(_)) | (None, Some(_), Some(_)) => {
            Err("Provide either since/until or preset, not both")
        }
        (Some(since), Some(until), None) => {
            if ensure_valid_date(since, "since").is_err()
                || ensure_valid_date(until, "until").is_err()
            {
                return Err("since and until must be valid dates in YYYY-MM-DD format");
            }

            Ok(CollectOptions {
                since: since.clone(),
                until: until.clone(),
                output: None,
                git_dirs: None,
            })
        }
        (None, None, Some(preset)) => {
            let (since, until) = resolve_range(preset.clone());
            Ok(CollectOptions {
                since,
                until,
                output: None,
                git_dirs: None,
            })
        }
        _ => Err("Provide either since/until or preset"),
    }
}

fn default_collector(provider_runtimes: Vec<ProviderRuntime>) -> Collector {
    let provider_runtimes = Arc::new(provider_runtimes);
    Arc::new(move |options| {
        let provider_runtimes = Arc::clone(&provider_runtimes);
        Box::pin(async move {
            collect_activity_with_provider_runtimes(options, provider_runtimes.as_ref()).await
        })
    })
}

pub async fn build_rocket() -> anyhow::Result<Rocket<Build>> {
    let provider_runtimes = inspect_provider_runtimes().await;
    let provider_info = provider_runtimes
        .iter()
        .map(ProviderRuntime::info_owned)
        .collect();
    Ok(build_rocket_with(
        default_collector(provider_runtimes),
        provider_info,
        true,
    ))
}

pub fn build_rocket_with(
    collector: Collector,
    provider_info: Vec<ProviderInfo>,
    start_initial_collection: bool,
) -> Rocket<Build> {
    let rocket = rocket::build()
        .manage(AppState::new(collector, provider_info))
        .mount(
            "/",
            rocket::routes![get_activity, get_providers, post_collect],
        );

    if start_initial_collection {
        rocket.attach(AdHoc::on_liftoff("initial-collection", |rocket| {
            Box::pin(async move {
                let state = rocket.state::<AppState>().expect("app state");
                if state
                    .is_collecting
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    return;
                }

                let options = CollectOptions {
                    since: days_ago(30),
                    until: today(),
                    output: None,
                    git_dirs: None,
                };

                match (state.collector)(options).await {
                    Ok(log) => state.store(log).await,
                    Err(err) => eprintln!("Initial collection failed: {err}"),
                }

                state.is_collecting.store(false, Ordering::SeqCst);
            })
        }))
    } else {
        rocket
    }
}

#[cfg(test)]
mod tests;
