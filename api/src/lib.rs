use activity_log_core::{
    model::{CollectRequest, CollectingResponse, ErrorResponse},
    runtime::{ActivitySnapshot, CollectionResult, CollectionRuntime, resolve_collect_options},
};
use rocket::{
    Build, Rocket, State, fairing::AdHoc, get, http::Status, post, response::status,
    serde::json::Json,
};

#[get("/api/activity")]
async fn get_activity(
    runtime: &State<CollectionRuntime>,
) -> status::Custom<Json<serde_json::Value>> {
    match runtime.activity_snapshot().await {
        ActivitySnapshot::Collecting => status::Custom(
            Status::Accepted,
            Json(serde_json::to_value(CollectingResponse { collecting: true }).unwrap()),
        ),
        ActivitySnapshot::Ready(log) => {
            status::Custom(Status::Ok, Json(serde_json::to_value(log).unwrap()))
        }
        ActivitySnapshot::Empty => status::Custom(
            Status::ServiceUnavailable,
            Json(
                serde_json::to_value(ErrorResponse {
                    error: "No data yet".to_string(),
                })
                .unwrap(),
            ),
        ),
    }
}

#[get("/api/providers")]
async fn get_providers(runtime: &State<CollectionRuntime>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(runtime.providers_response()).unwrap())
}

#[post("/api/collect", format = "json", data = "<request>")]
async fn post_collect(
    runtime: &State<CollectionRuntime>,
    request: Json<CollectRequest>,
) -> status::Custom<Json<serde_json::Value>> {
    let options = match resolve_collect_options(&request) {
        Ok(options) => options,
        Err(message) => return bad_request(message),
    };

    match runtime.collect_with_options(options).await {
        Ok(CollectionResult::Ready(log)) => {
            status::Custom(Status::Ok, Json(serde_json::to_value(log).unwrap()))
        }
        Ok(CollectionResult::Collecting) => status::Custom(
            Status::Accepted,
            Json(serde_json::to_value(CollectingResponse { collecting: true }).unwrap()),
        ),
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

pub async fn build_rocket() -> anyhow::Result<Rocket<Build>> {
    let runtime = CollectionRuntime::build_default().await?;
    Ok(build_rocket_with(runtime, true))
}

pub fn build_rocket_with(
    runtime: CollectionRuntime,
    start_initial_collection: bool,
) -> Rocket<Build> {
    let rocket = rocket::build().manage(runtime).mount(
        "/",
        rocket::routes![get_activity, get_providers, post_collect],
    );

    if start_initial_collection {
        rocket.attach(AdHoc::on_liftoff("initial-collection", |rocket| {
            Box::pin(async move {
                if let Some(task) = rocket
                    .state::<CollectionRuntime>()
                    .expect("collection runtime")
                    .initial_collection_task()
                {
                    tokio::spawn(task);
                }
            })
        }))
    } else {
        rocket
    }
}

#[cfg(test)]
mod tests;
