use std::path::PathBuf;

use activity_log_api::build_rocket;

#[rocket::main]
async fn main() -> anyhow::Result<()> {
    let config = config_arg(std::env::args().skip(1))?;
    // First, so every path below is the profile's.
    activity_log_core::profile::init(config.as_deref())?;

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);

    println!("\nRust API listening at http://localhost:{port}\n");

    build_rocket(config.as_deref())
        .await?
        .configure(rocket::Config {
            port,
            ..rocket::Config::default()
        })
        .launch()
        .await?;

    Ok(())
}

/// `--config <path>`, the only argument the server takes.
fn config_arg(mut args: impl Iterator<Item = String>) -> anyhow::Result<Option<PathBuf>> {
    let mut config = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                let path = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--config needs a path"))?;
                config = Some(PathBuf::from(path));
            }
            other => anyhow::bail!("Unknown argument {other}; the server takes --config <path>"),
        }
    }
    Ok(config)
}
