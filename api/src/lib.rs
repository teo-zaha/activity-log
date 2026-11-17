use std::path::Path;

use activity_log_core::{
    diagnostics,
    model::{CollectRequest, CollectionFailedResponse, ErrorResponse, LogsResponse},
    runtime::{ActivitySnapshot, resolve_collect_options},
    settings::ConfigFile,
};

mod reload;

pub use reload::ApiRuntime;
use rocket::{
    Build, Rocket, State, delete, fairing::AdHoc, get, http::Status, post, response::status,
    serde::json::Json,
};

type JsonResponse = status::Custom<Json<serde_json::Value>>;

fn json(status: Status, body: impl serde::Serialize) -> JsonResponse {
    status::Custom(status, Json(serde_json::to_value(body).unwrap()))
}

/// A running collection answers 202 with the log of what is in so far.
fn activity_response(snapshot: ActivitySnapshot) -> JsonResponse {
    match snapshot {
        ActivitySnapshot::Running(log) => json(Status::Accepted, &*log),
        ActivitySnapshot::Ready(log) => json(Status::Ok, &*log),
        // The state of the last collection, not a failed request: its range stays readable.
        ActivitySnapshot::Failed { options, message } => {
            json(Status::Ok, CollectionFailedResponse::new(&options, message))
        }
        ActivitySnapshot::Empty => json(
            Status::ServiceUnavailable,
            ErrorResponse {
                error: "No data yet".to_string(),
            },
        ),
    }
}

#[get("/api/activity")]
async fn get_activity(api: &State<ApiRuntime>) -> JsonResponse {
    activity_response(api.runtime().await.activity_snapshot())
}

#[get("/api/providers")]
async fn get_providers(api: &State<ApiRuntime>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(api.runtime().await.providers_response()).unwrap())
}

/// Starts the collection and answers at once; `GET /api/activity` shows it filling up.
#[post("/api/collect", format = "json", data = "<request>")]
async fn post_collect(api: &State<ApiRuntime>, request: Json<CollectRequest>) -> JsonResponse {
    match resolve_collect_options(&request) {
        Ok(options) => activity_response(api.start_collection(options).await),
        Err(message) => json(
            Status::BadRequest,
            ErrorResponse {
                error: message.to_string(),
            },
        ),
    }
}

#[get("/api/logs")]
fn get_logs() -> Json<LogsResponse> {
    Json(LogsResponse {
        entries: diagnostics::entries(),
    })
}

#[delete("/api/logs")]
fn delete_logs() -> Status {
    diagnostics::clear();
    Status::NoContent
}

/// The server for the connections in `config` (or the default config file).
pub async fn build_rocket(config: Option<&Path>) -> anyhow::Result<Rocket<Build>> {
    let path = ConfigFile::resolve_path(config);
    println!("Reading connections from {}", path.display());
    let api = ApiRuntime::from_config(path).await?;
    Ok(build_rocket_with(api, true))
}

pub fn build_rocket_with(api: ApiRuntime, start_initial_collection: bool) -> Rocket<Build> {
    let rocket = rocket::build().manage(api).mount(
        "/",
        rocket::routes![
            get_activity,
            get_providers,
            post_collect,
            get_logs,
            delete_logs
        ],
    );

    if start_initial_collection {
        rocket.attach(AdHoc::on_liftoff("initial-collection", |rocket| {
            Box::pin(async move {
                let api = rocket.state::<ApiRuntime>().expect("API runtime");
                api.start_initial_collection().await;
            })
        }))
    } else {
        rocket
    }
}

#[cfg(test)]
mod tests;
