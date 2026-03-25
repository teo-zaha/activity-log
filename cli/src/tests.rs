use activity_log_core::model::OutputFormat;
use clap::Parser;
use tempfile::tempdir;

use crate::{Cli, build_collect_options, generate_ts, parse_format};

#[test]
fn parses_cli_defaults() {
    let cli = Cli::try_parse_from(["activity-log-cli"]).unwrap();
    let options = build_collect_options(&cli, &OutputFormat::Json).unwrap();
    assert_eq!(options.output.as_deref(), Some("logs/activity_log.json"));
}

#[test]
fn rejects_invalid_format() {
    assert!(parse_format("xml").is_err());
}

#[test]
fn generates_ts_files() {
    let tmp = tempdir().unwrap();
    let out = tmp.path().join("generated/activity.ts");
    generate_ts(&out).unwrap();
    assert!(out.exists());
    assert!(tmp.path().join("generated/index.ts").exists());
}
