use activity_log_core::model::{
    ActivityLog, ActivityLogMeta, CollectedActivity, CollectedActivitySummary, ConnectionId,
    OutputFormat, ProviderInfo, ProviderStatus, Source, SupportedOs,
};
use clap::Parser;
use tempfile::tempdir;

#[path = "generate_ts_support.rs"]
mod generate_ts_support;

use crate::{Cli, build_collect_request, parse_format, provider_summary_lines};
use generate_ts_support::generate_ts;

#[test]
fn parses_cli_defaults() {
    let cli = Cli::try_parse_from(["activity-log-cli"]).unwrap();
    let request = build_collect_request(&cli.collect, &OutputFormat::Json).unwrap();
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
    let err = build_collect_request(&cli.collect, &OutputFormat::Json).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Unknown range: \"nope\". Use today|yesterday|week|lastweek|month|lastmonth"
    );
}

#[test]
fn parses_source_commands() {
    let cli = Cli::try_parse_from([
        "activity-log-cli",
        "--config",
        "/tmp/config.toml",
        "sources",
        "set",
        "gitlab/work",
        "url",
        "https://gitlab.com",
    ])
    .unwrap();
    assert_eq!(
        cli.config.as_deref(),
        Some(std::path::Path::new("/tmp/config.toml"))
    );
    assert!(matches!(
        cli.command,
        Some(crate::Command::Sources {
            action: Some(crate::SourcesCommand::Set { .. })
        })
    ));
    assert!(Cli::try_parse_from(["activity-log-cli", "--set", "JIRA_URL=x"]).is_err());
    assert!(Cli::try_parse_from(["activity-log-cli", "sources", "add", "nope"]).is_err());
}

#[test]
fn provider_summary_includes_skipped_and_usable_providers() {
    let provider_info = vec![
        ProviderInfo {
            connection: ConnectionId::new(Source::Git, "this-computer"),
            label: "Git".to_string(),
            wsl_distro: None,
            status: ProviderStatus::Available,
            usable: true,
            supported_os: vec![SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
            reason: None,
            problem: None,
            checked_at: "2026-01-01T00:00:00Z".to_string(),
        },
        ProviderInfo {
            connection: ConnectionId::new(Source::Outlook, "outlook"),
            label: "Outlook".to_string(),
            wsl_distro: None,
            status: ProviderStatus::Unavailable,
            usable: false,
            supported_os: vec![SupportedOs::Windows],
            reason: Some("OS not supported".to_string()),
            problem: None,
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
                pending: Vec::new(),
            },
            summary: CollectedActivitySummary::default(),
            entries: Vec::new(),
        },
    };

    let lines = provider_summary_lines(&provider_info, &collected);

    assert_eq!(
        lines[0],
        "  ✓ git/this-computer (Git) (0 entries, 0 merged with gitlab)"
    );
    assert_eq!(
        lines[1],
        "  - outlook/outlook (Outlook) — skipped (OS not supported)"
    );
}
