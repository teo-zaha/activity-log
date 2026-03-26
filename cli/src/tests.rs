use activity_log_core::model::{
    ActivityLog, ActivityLogMeta, CollectedActivity, CollectedActivitySummary, OutputFormat,
    ProviderInfo, ProviderStatus, Source, SupportedOs,
};
use clap::Parser;
use tempfile::tempdir;

use crate::{
    Cli, build_collect_options, generate_ts::generate_ts, parse_format, provider_summary_lines,
};

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

#[test]
fn invalid_range_lists_values_from_enum() {
    let cli = Cli::try_parse_from(["activity-log-cli", "--range", "nope"]).unwrap();
    let err = build_collect_options(&cli, &OutputFormat::Json).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Unknown range: \"nope\". Use today|yesterday|week|lastweek|month|lastmonth"
    );
}

#[test]
fn provider_summary_includes_skipped_and_usable_providers() {
    let provider_info = vec![
        ProviderInfo {
            id: Source::Git,
            status: ProviderStatus::Available,
            usable: true,
            supported_os: vec![SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
            reason: None,
            checked_at: "2026-01-01T00:00:00Z".to_string(),
        },
        ProviderInfo {
            id: Source::Outlook,
            status: ProviderStatus::Unavailable,
            usable: false,
            supported_os: vec![SupportedOs::Windows],
            reason: Some("OS not supported".to_string()),
            checked_at: "2026-01-01T00:00:00Z".to_string(),
        },
    ];

    let collected = CollectedActivity {
        summary: CollectedActivitySummary::default(),
        log: ActivityLog {
            meta: ActivityLogMeta {
                generated: "2026-01-01T00:00:00Z".to_string(),
                since: "2026-01-01".to_string(),
                until: "2026-01-01".to_string(),
            },
            summary: CollectedActivitySummary::default(),
            entries: Vec::new(),
        },
    };

    let lines = provider_summary_lines(&provider_info, &collected);

    assert_eq!(lines[0], "  ✓ git (0 entries, 0 merged with gitlab)");
    assert_eq!(lines[1], "  - outlook — skipped (OS not supported)");
}
