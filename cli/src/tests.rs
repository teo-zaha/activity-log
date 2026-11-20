use activity_log_core::model::{
    ActivityLog, ActivityLogMeta, CollectedActivity, CollectedActivitySummary, ConnectionId,
    OutputFormat, ProviderInfo, ProviderStatus, Source, SupportedOs,
};
use clap::Parser;
use tempfile::tempdir;

#[path = "generate_ts_support.rs"]
mod generate_ts_support;

use crate::{Cli, build_collect_request, parse_format, provider_summary_lines};
use generate_ts_support::{generate_ts, stale_files};

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
fn the_barrel_exports_the_file_written() {
    let tmp = tempdir().unwrap();
    generate_ts(&tmp.path().join("custom.ts")).unwrap();
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("index.ts")).unwrap(),
        "export * from './custom'\n"
    );
}

#[test]
fn an_index_out_file_is_refused_before_anything_is_written() {
    let tmp = tempdir().unwrap();
    assert!(generate_ts(&tmp.path().join("index.ts")).is_err());
    assert!(!tmp.path().join("index.ts").exists());
}

/// The barrel would overwrite the contract, or name it with a specifier TypeScript cannot read.
#[test]
fn an_out_file_the_barrel_cannot_name_is_refused_before_anything_is_written() {
    let tmp = tempdir().unwrap();
    for name in ["Index.ts", "INDEX.TS", "team's.ts", "a b.ts", "activity.js"] {
        assert!(generate_ts(&tmp.path().join(name)).is_err(), "{name}");
    }
    assert!(std::fs::read_dir(tmp.path()).unwrap().next().is_none());
}

/// A file that cannot be read is an error, not drift.
#[cfg(unix)]
#[test]
fn a_file_the_check_cannot_read_is_an_error() {
    let tmp = tempdir().unwrap();
    let out = tmp.path().join("activity.ts");
    std::fs::create_dir(&out).unwrap();
    assert!(stale_files(&out).is_err());
}

#[test]
fn the_drift_check_names_changed_files_and_writes_nothing() {
    let tmp = tempdir().unwrap();
    let out = tmp.path().join("activity.ts");
    assert_eq!(stale_files(&out).unwrap().len(), 2);
    assert!(!out.exists());

    generate_ts(&out).unwrap();
    assert!(stale_files(&out).unwrap().is_empty());

    std::fs::write(&out, "export type Stale = 1\n").unwrap();
    assert_eq!(stale_files(&out).unwrap(), vec![out.clone()]);
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "export type Stale = 1\n"
    );
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

/// The terminal gets the message with the connection's label; the typed cause stays reachable.
#[test]
fn an_edit_failure_keeps_its_typed_cause() {
    use activity_log_core::settings::{Conflict, ConflictKind, EditError};

    let id = ConnectionId::new(Source::Gitlab, "work");
    let conflict = EditError::Conflict(Conflict {
        connection: id.clone(),
        kind: ConflictKind::Removed,
        fields: Vec::new(),
    });

    let failure = super::edit_failure(conflict, |_| Some("Work GitLab".to_string()));

    assert!(failure.to_string().starts_with("Work GitLab "), "{failure}");
    assert!(matches!(
        failure.downcast_ref::<EditError>(),
        Some(EditError::Conflict(conflict)) if conflict.connection == id
    ));
}
