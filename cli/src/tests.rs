use activity_log_core::model::{
    ActivityLog, ActivityLogMeta, CollectedActivity, CollectedActivitySummary, OutputFormat,
    ProviderInfo, ProviderStatus, Source, SupportedOs,
};
use clap::Parser;
use tempfile::tempdir;

#[path = "generate_ts_support.rs"]
mod generate_ts_support;

use crate::{
    Cli, build_collect_request, parse_format, parse_provider_config_overrides,
    provider_summary_lines,
};
use generate_ts_support::generate_ts;

#[test]
fn parses_cli_defaults() {
    let cli = Cli::try_parse_from(["activity-log-cli"]).unwrap();
    let request = build_collect_request(&cli, &OutputFormat::Json).unwrap();
    assert_eq!(
        request.output_path.as_deref(),
        Some("logs/activity_log.json")
    );
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

    let generated = std::fs::read_to_string(out).unwrap();
    assert!(generated.contains("export type ProviderConfigValueTypeDto"));
    assert!(generated.contains("export type ProviderConfigSnapshotDto"));
    assert!(generated.contains("export type ProviderConfigSaveRequestDto"));
    assert!(generated.contains("export type ProviderConfigSaveResponseDto"));
}

#[test]
fn invalid_range_lists_values_from_enum() {
    let cli = Cli::try_parse_from(["activity-log-cli", "--range", "nope"]).unwrap();
    let err = build_collect_request(&cli, &OutputFormat::Json).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Unknown range: \"nope\". Use today|yesterday|week|lastweek|month|lastmonth"
    );
}

#[test]
fn rejects_set_without_separator() {
    let err = parse_provider_config_overrides(&["JIRA_URL".to_string()]).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid --set \"JIRA_URL\". Use KEY=VALUE."
    );
}

#[test]
fn rejects_unknown_set_key() {
    let err = parse_provider_config_overrides(&["NOPE=value".to_string()]).unwrap_err();
    assert!(
        err.to_string()
            .contains("Unknown provider config key \"NOPE\"")
    );
}

#[test]
fn rejects_invalid_typed_set_value() {
    let err = parse_provider_config_overrides(&["JIRA_INSECURE=1".to_string()]).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid value for \"JIRA_INSECURE\": expected bool"
    );
}

#[test]
fn duplicate_set_key_uses_last_value() {
    let values = parse_provider_config_overrides(&[
        "GIT_DIRS=/tmp/one".to_string(),
        "GIT_DIRS=/tmp/two".to_string(),
    ])
    .unwrap();

    assert_eq!(values.get("GIT_DIRS"), Some(&"/tmp/two".to_string()));
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
                start: "2026-01-01T00:00:00Z".to_string(),
                end: "2026-01-02T00:00:00Z".to_string(),
            },
            summary: CollectedActivitySummary::default(),
            entries: Vec::new(),
        },
    };

    let lines = provider_summary_lines(&provider_info, &collected);

    assert_eq!(lines[0], "  ✓ git (0 entries, 0 merged with gitlab)");
    assert_eq!(lines[1], "  - outlook — skipped (OS not supported)");
}
