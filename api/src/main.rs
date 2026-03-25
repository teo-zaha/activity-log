use activity_log_api::build_rocket;
use dotenvy::dotenv;

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    dotenv().ok();

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);

    println!("\nRust API listening at http://localhost:{port}\n");

    build_rocket()
        .configure(rocket::Config {
            port,
            ..rocket::Config::default()
        })
        .launch()
        .await?;

    Ok(())
}
