use anyhow::{Result, bail};
use clap::Parser;

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
    /// Write nothing; fail if the files on disk differ from what would be generated.
    #[arg(long)]
    check: bool,
}

fn main() -> Result<()> {
    let cli = GenerateTsCli::parse();
    if !cli.check {
        return generate_ts_support::generate_ts(&cli.out);
    }
    let stale = generate_ts_support::stale_files(&cli.out)?;
    if !stale.is_empty() {
        let files = stale
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        bail!("Generated TypeScript is out of date ({files}); run `pnpm types:generate`");
    }
    Ok(())
}
