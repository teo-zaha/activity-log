use anyhow::Result;
use clap::Parser;
use dotenvy::dotenv;

mod generate_ts_support;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Generate TypeScript DTOs for the Angular app"
)]
struct GenerateTsCli {
    #[arg(long, default_value = "dashboard/src/app/generated/activity.ts")]
    out: std::path::PathBuf,
}

fn main() -> Result<()> {
    dotenv().ok();
    let cli = GenerateTsCli::parse();
    generate_ts_support::generate_ts(&cli.out)
}
