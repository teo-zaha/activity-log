use std::path::Path;

use activity_log_core::{
    diagnostics,
    export::render_export_request,
    model::{
        CollectRequest, CollectionFailedResponse, ConnectionId, ErrorResponse, ExportRequest,
        LogsResponse,
    },
    runtime::{ActivitySnapshot, resolve_collect_options},
    settings::ConfigFile,
};

mod reload;

pub use reload::ApiRuntime;
use rocket::{
    Build, Data, Rocket, State, data::ByteUnit, delete, fairing::AdHoc, get, http::Status, post,
    response::status, serde::json::Json,
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

/// Runs a connection's startup check again; its kind is collected again for the range on screen.
/// Answers with the sources, this one checking, or 404 for a connection the server does not
/// have.
#[post("/api/connections/recheck", format = "json", data = "<request>")]
async fn post_recheck(api: &State<ApiRuntime>, request: Json<ConnectionId>) -> JsonResponse {
    match api.recheck(&request).await {
        Some(providers) => json(Status::Ok, providers),
        None => json(
            Status::NotFound,
            ErrorResponse {
                error: format!("No source {}", *request),
            },
        ),
    }
}

/// The entries the dashboard sends can be many meetings with long descriptions, far more than
/// Rocket's 1 MiB JSON limit; this route alone reads up to `EXPORT_LIMIT`.
const EXPORT_LIMIT: ByteUnit = ByteUnit::Mebibyte(64);

/// The entries the dashboard shows as a file for the browser to download.
#[post("/api/export", format = "json", data = "<body>")]
async fn post_export(body: Data<'_>) -> JsonResponse {
    let error = |status, error: String| json(status, ErrorResponse { error });
    let body = match body.open(EXPORT_LIMIT).into_bytes().await {
        Ok(body) if body.is_complete() => body.into_inner(),
        Ok(_) => {
            return error(
                Status::PayloadTooLarge,
                format!("An export can hold at most {EXPORT_LIMIT} of entries"),
            );
        }
        Err(err) => {
            return error(
                Status::BadRequest,
                format!("Reading the export failed: {err}"),
            );
        }
    };
    let request = match serde_json::from_slice::<ExportRequest>(&body) {
        Ok(request) => request,
        Err(err) => return error(Status::BadRequest, format!("Not an export request: {err}")),
    };
    match render_export_request(&request) {
        Ok(file) => json(Status::Ok, file),
        Err(err) => error(Status::BadRequest, format!("{err:#}")),
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
            post_recheck,
            post_export,
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
