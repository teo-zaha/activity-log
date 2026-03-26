use std::{
    fs,
    path::{Path, PathBuf},
};

use activity_log_core::ts::activity_typescript;
use anyhow::Result;
use clap::Parser;
use dotenvy::dotenv;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Generate TypeScript DTOs for the Angular app"
)]
struct GenerateTsCli {
    #[arg(long, default_value = "dashboard/src/app/generated/activity.ts")]
    out: PathBuf,
}

pub fn generate_ts(out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }

    // Keep the frontend contract generation explicit so backend builds do not mutate repo files.
    fs::write(out, activity_typescript())?;

    let index_path = generated_index_path(out);
    fs::write(index_path, "export * from './activity'\n")?;
    println!("Generated TypeScript DTOs at {}", out.display());
    Ok(())
}

fn generated_index_path(out: &Path) -> PathBuf {
    out.parent()
        .unwrap_or_else(|| Path::new("."))
        .join("index.ts")
}

fn main() -> Result<()> {
    dotenv().ok();
    let cli = GenerateTsCli::parse();
    generate_ts(&cli.out)
}
