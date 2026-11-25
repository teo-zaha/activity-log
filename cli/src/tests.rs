use std::path::PathBuf;

use activity_log_core::model::{ConnectionId, OutputFormat, ProviderProblem, Source};
use chrono::NaiveDate;
use clap::{CommandFactory, Parser, error::ErrorKind};
use strum::IntoEnumIterator;
use tempfile::tempdir;

#[path = "generate_ts_support.rs"]
mod generate_ts_support;

use crate::results::{Effect, problem_terms};
use crate::{Cli, Output, collect_options, range::RangePreset};
use generate_ts_support::{generate_ts, stale_files};

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("activity-log").chain(args.iter().copied()))
}

#[test]
fn exit_status_help_names_every_failing_provider_problem() {
    let help = Cli::command().render_long_help().to_string();
    let (failed, _) = help
        .split_once("1 when a source failed (")
        .and_then(|(_, after)| after.split_once(')'))
        .expect("the help lists why a provider can fail the run");
    let listed: Vec<_> = failed.split(", ").collect();
    for problem in ProviderProblem::iter() {
        let (words, effect) = problem_terms(problem);
        if effect == Effect::Fails {
            assert!(listed.contains(&words), "{problem:?}: {failed}");
        }
    }
}

#[test]
fn collects_json_into_the_default_file() {
    let cli = parse(&[]).unwrap();
    assert_eq!(cli.collect.format, OutputFormat::Json);
    assert_eq!(cli.collect.output, None);
    assert_eq!(
        Output::default_for(OutputFormat::Json),
        Output::File(PathBuf::from("logs/activity_log.json"))
    );
    assert_eq!(
        Output::default_for(OutputFormat::Markdown),
        Output::File(PathBuf::from("logs/activity_log.md"))
    );
}

#[test]
fn a_dash_writes_to_standard_output_and_anything_else_is_a_file() {
    let output = |value| parse(&["--output", value]).unwrap().collect.output;
    assert_eq!(output("-"), Some(Output::Stdout));
    assert_eq!(output("./-"), Some(Output::File(PathBuf::from("./-"))));
    assert_eq!(
        output("reports/week.md"),
        Some(Output::File(PathBuf::from("reports/week.md")))
    );
}

/// The spellings that worked before the flags were typed still do, in any case.
#[test]
fn ranges_and_formats_keep_their_spellings_in_any_case() {
    for (spelling, preset) in [
        ("today", RangePreset::Today),
        ("yesterday", RangePreset::Yesterday),
        ("week", RangePreset::Week),
        ("lastweek", RangePreset::LastWeek),
        ("LastWeek", RangePreset::LastWeek),
        ("month", RangePreset::Month),
        ("lastmonth", RangePreset::LastMonth),
        ("LASTMONTH", RangePreset::LastMonth),
    ] {
        assert_eq!(
            parse(&["--range", spelling]).unwrap().collect.range,
            Some(preset),
            "{spelling}"
        );
    }
    for (spelling, format) in [
        ("json", OutputFormat::Json),
        ("JSON", OutputFormat::Json),
        ("markdown", OutputFormat::Markdown),
        ("Markdown", OutputFormat::Markdown),
    ] {
        assert_eq!(
            parse(&["--format", spelling]).unwrap().collect.format,
            format,
            "{spelling}"
        );
    }
}

#[test]
fn unknown_values_are_refused_with_what_to_enter() {
    for args in [
        ["--range", "fortnight"],
        ["--range", "last-week"],
        ["--format", "xml"],
        ["--since", "2026-13-01"],
        ["--until", "yesterday"],
    ] {
        let err = parse(&args).err().unwrap_or_else(|| panic!("{args:?}"));
        assert!(
            matches!(
                err.kind(),
                ErrorKind::InvalidValue | ErrorKind::ValueValidation
            ),
            "{args:?}: {:?}",
            err.kind()
        );
        assert!(err.use_stderr(), "{args:?}");
    }
    let help = Cli::command().render_long_help().to_string();
    assert!(help.contains("lastweek"), "{help}");
}

#[test]
fn a_range_wins_over_since_and_until() {
    let today = NaiveDate::from_ymd_opt(2026, 3, 25).unwrap();
    let cli = parse(&["--range", "today", "--since", "2026-01-01"]).unwrap();
    let options = collect_options(&cli.collect, today).unwrap();
    let explicit = crate::range::day_range(&chrono::Local, today, today).unwrap();
    assert_eq!(options, explicit);
}

#[test]
fn without_a_range_it_collects_the_last_thirty_days() {
    let today = NaiveDate::from_ymd_opt(2026, 3, 25).unwrap();
    let cli = parse(&[]).unwrap();
    let options = collect_options(&cli.collect, today).unwrap();
    let expected = crate::range::day_range(
        &chrono::Local,
        NaiveDate::from_ymd_opt(2026, 2, 23).unwrap(),
        today,
    )
    .unwrap();
    assert_eq!(options, expected);
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

/// A kind is taken in any case, as `--range` and `--format` are.
#[test]
fn sources_add_takes_a_kind_in_any_case() {
    let cli = Cli::try_parse_from(["activity-log-cli", "sources", "add", "GitLab"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(crate::Command::Sources {
            action: Some(crate::SourcesCommand::Add {
                kind: Source::Gitlab,
                ..
            })
        })
    ));
}

#[test]
fn sources_add_offers_every_kind_of_source() {
    let mut command = Cli::command();
    let add = command
        .find_subcommand_mut("sources")
        .and_then(|sources| sources.find_subcommand_mut("add"))
        .expect("sources add");
    let kind = add
        .get_arguments()
        .find(|argument| argument.get_id() == "kind")
        .expect("the kind argument");
    let offered: Vec<String> = kind
        .get_possible_values()
        .iter()
        .map(|value| value.get_name().to_owned())
        .collect();
    let kinds: Vec<String> = Source::iter().map(|source| source.to_string()).collect();
    assert_eq!(offered, kinds);
    for name in &kinds {
        let cli = Cli::try_parse_from(["activity-log-cli", "sources", "add", name]).unwrap();
        assert!(matches!(
            cli.command,
            Some(crate::Command::Sources {
                action: Some(crate::SourcesCommand::Add { kind, .. })
            }) if kind.to_string() == *name
        ));
    }
}

/// The terminal gets the message with the connection's label; the typed cause stays reachable.
#[test]
fn an_edit_failure_keeps_its_typed_cause() {
    use activity_log_core::settings::{Conflict, ConflictKind, EditError};

    let id = ConnectionId::new(Source::Gitlab, "work");
    let conflict = EditError::Conflict(Conflict {
        connection: id.clone(),
        label: None,
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
