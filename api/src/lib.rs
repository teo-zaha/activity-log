use std::{path::Path, sync::Arc};

use activity_log_core::{
    diagnostics,
    export::render_export_request,
    model::{
        ActivityState, BackendError, BackendErrorKind, CollectRequest, ConnectionId, ExportRequest,
        LogsResponse, RuntimeState, ShownActivity,
    },
    runtime::{BadConfig, Host, RunSpawner, answer_collect, answer_recheck},
    settings::ConfigFile,
};
use rocket::{
    Build, Data, Request, Rocket, State, catch, data::ByteUnit, delete, fairing::AdHoc, get,
    http::Status, post, response::status, serde::json::Json,
};

type JsonResponse = status::Custom<Json<serde_json::Value>>;

fn json(status: Status, body: impl serde::Serialize) -> JsonResponse {
    status::Custom(status, Json(serde_json::to_value(body).unwrap()))
}

/// The status an error answers with; its body is the error itself.
fn status_of(kind: BackendErrorKind) -> Status {
    match kind {
        BackendErrorKind::NoData | BackendErrorKind::Busy => Status::ServiceUnavailable,
        BackendErrorKind::InvalidRequest => Status::BadRequest,
        BackendErrorKind::NotFound => Status::NotFound,
        BackendErrorKind::AlreadyExists | BackendErrorKind::Conflict => Status::Conflict,
        BackendErrorKind::TooLarge => Status::PayloadTooLarge,
        BackendErrorKind::NoLocation
        | BackendErrorKind::Unprotectable
        | BackendErrorKind::BadConfig
        | BackendErrorKind::Internal => Status::InternalServerError,
    }
}

fn error(err: BackendError) -> JsonResponse {
    json(status_of(err.kind), err)
}

/// A route's answer, or its error with the error's status.
fn answer(status: Status, result: Result<impl serde::Serialize, BackendError>) -> JsonResponse {
    match result {
        Ok(body) => json(status, body),
        Err(err) => error(err),
    }
}

/// A running collection answers 202 with the log of what is in so far.
fn activity_response(state: ActivityState) -> JsonResponse {
    match state {
        ActivityState::Running { log } => json(Status::Accepted, &*log),
        ActivityState::Ready { log } => json(Status::Ok, &*log),
        // The state of the last collection, not a failed request: its range stays readable.
        ActivityState::Failed { failure } => json(Status::Ok, &failure),
        ActivityState::Empty => error(BackendError::no_data()),
    }
}

// Each request first installs a config file changed since the last look, as the timer does
// between requests.

#[get("/api/activity")]
async fn get_activity(host: &State<Host>) -> JsonResponse {
    host.reload_if_changed().await;
    activity_response(host.activity())
}

/// Both parts of the host's state as one coherent pair, with their stamps.
#[get("/api/runtime-state")]
async fn get_runtime_state(host: &State<Host>) -> Json<RuntimeState> {
    host.reload_if_changed().await;
    Json(host.runtime_state())
}

#[get("/api/providers")]
async fn get_providers(host: &State<Host>) -> Json<serde_json::Value> {
    host.reload_if_changed().await;
    Json(serde_json::to_value(host.providers_response()).unwrap())
}

/// Starts the collection and answers at once; `GET /api/activity` shows it filling up.
#[post("/api/collect", format = "json", data = "<request>")]
async fn post_collect(host: &State<Host>, request: Json<CollectRequest>) -> JsonResponse {
    host.reload_if_changed().await;
    match answer_collect(host, &request) {
        Ok(answer) => {
            let status = match answer.activity {
                ShownActivity::Running(_) => Status::Accepted,
                ShownActivity::Ready(_) | ShownActivity::Failed(_) => Status::Ok,
            };
            json(status, answer)
        }
        Err(err) => error(err),
    }
}

/// Runs a connection's startup check again; its kind is collected again for the range on screen.
/// Answers with the sources, this one checking, or 404 for a connection the server does not
/// have.
#[post("/api/connections/recheck", format = "json", data = "<request>")]
async fn post_recheck(host: &State<Host>, request: Json<ConnectionId>) -> JsonResponse {
    host.reload_if_changed().await;
    answer(Status::Ok, answer_recheck(host, &request))
}

/// The entries the dashboard sends can be many meetings with long descriptions, far more than
/// Rocket's 1 MiB JSON limit; this route alone reads up to `EXPORT_LIMIT`.
const EXPORT_LIMIT: ByteUnit = ByteUnit::Mebibyte(64);

/// The entries the dashboard shows as a file for the browser to download.
#[post("/api/export", format = "json", data = "<body>")]
async fn post_export(body: Data<'_>) -> JsonResponse {
    let body = match body.open(EXPORT_LIMIT).into_bytes().await {
        Ok(body) if body.is_complete() => body.into_inner(),
        Ok(_) => {
            return error(BackendError::new(
                BackendErrorKind::TooLarge,
                format!("An export can hold at most {EXPORT_LIMIT} of entries"),
            ));
        }
        Err(err) => {
            return error(BackendError::invalid_request(format!(
                "Reading the export failed: {err}"
            )));
        }
    };
    let request = match serde_json::from_slice::<ExportRequest>(&body) {
        Ok(request) => request,
        Err(err) => {
            return error(BackendError::invalid_request(format!(
                "Not an export request: {err}"
            )));
        }
    };
    answer(
        Status::Ok,
        render_export_request(&request).map_err(BackendError::from),
    )
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

/// Every answer Rocket makes itself (an unknown route, a body that is no JSON or not the
/// route's request, a panic) is a `BackendError` too, with Rocket's status.
#[catch(default)]
fn catch_default(status: Status, _request: &Request) -> status::Custom<Json<BackendError>> {
    let kind = match status.code {
        404 => BackendErrorKind::NotFound,
        413 => BackendErrorKind::TooLarge,
        400 | 415 | 422 => BackendErrorKind::InvalidRequest,
        _ => BackendErrorKind::Internal,
    };
    let message = format!("{} {}", status.code, status.reason_lossy());
    status::Custom(status, Json(BackendError::new(kind, message)))
}

/// The server for the connections in `config` (or the default config file), which it watches
/// for changes. Without a settings location it serves no sources and watches no file. A file
/// that cannot be read or parsed stops it with the error, which names the place and never the
/// file's text.
pub async fn build_rocket(config: Option<&Path>) -> anyhow::Result<Rocket<Build>> {
    let path = ConfigFile::resolve_path(config);
    if let Some(path) = &path {
        println!("Reading connections from {}", path.display());
    }
    let host = Host::start(path, spawner(), BadConfig::Fail)?;
    Ok(build_rocket_with(host, true))
}

/// Runs host work on Rocket's runtime.
pub fn spawner() -> RunSpawner {
    Arc::new(|work| {
        tokio::spawn(work);
    })
}

pub fn build_rocket_with(host: Host, start_initial_collection: bool) -> Rocket<Build> {
    rocket::build()
        .manage(host)
        .mount(
            "/",
            rocket::routes![
                get_activity,
                get_runtime_state,
                get_providers,
                post_collect,
                post_recheck,
                post_export,
                get_logs,
                delete_logs
            ],
        )
        .register("/", rocket::catchers![catch_default])
        .attach(AdHoc::on_liftoff("host", move |rocket| {
            Box::pin(async move {
                let host = rocket.state::<Host>().expect("host");
                tokio::spawn(host.watch_config());
                if start_initial_collection {
                    host.start_initial_collection();
                }
            })
        }))
        .attach(AdHoc::on_shutdown("host", |rocket| {
            Box::pin(async move {
                if let Some(host) = rocket.state::<Host>() {
                    host.shutdown();
                }
            })
        }))
}

#[cfg(test)]
mod tests;
