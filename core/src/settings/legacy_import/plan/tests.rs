use super::{Plan, Planned, plan as plan_of};
use crate::{
    model::{LegacySkipReason, Source, SupportedOs},
    provider::{
        adapter,
        connection::ConnectionLocation,
        git::GIT_DIRS,
        gitlab::{GITLAB_TOKEN, GITLAB_URL, GITLAB_USER},
        jira::{JIRA_INSECURE, JIRA_TOKEN, JIRA_URL},
        provider_config::ProviderConfigKey,
    },
    settings::legacy_import::v0_0_7::{LegacyValues, encode_config_list, parse_config_list},
};

fn values(pairs: &[(&str, Option<&str>)]) -> LegacyValues {
    LegacyValues::new(
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.map(str::to_string)))
            .collect(),
    )
}

/// A list as today's settings store it, from 0.0.7's own reading of `raw`.
fn stored_list(raw: &str) -> String {
    encode_config_list(&parse_config_list(raw))
}

fn of(plan: &Plan, source: Source) -> Vec<&Planned> {
    plan.connections
        .iter()
        .filter(|planned| planned.source == source)
        .collect()
}

fn value(planned: &Planned, key: ProviderConfigKey) -> Option<&str> {
    planned
        .values
        .iter()
        .find(|(stored, _)| *stored == key)
        .map(|(_, value)| value.as_str())
}

fn no_distros() -> Vec<String> {
    panic!("only Windows with WSL_DISTROS unset asks")
}

#[test]
fn gitlab_and_jira_come_with_their_values_and_git_always() {
    let plan = plan_of(
        &values(&[
            ("GITLAB_URL", Some("https://gitlab.example.test")),
            ("GITLAB_TOKEN", Some("glpat-test-0000")),
            ("GITLAB_USER", Some("me")),
            ("JIRA_URL", Some("https://jira.example.test")),
            ("JIRA_TOKEN", Some("jira-test-0000")),
            ("JIRA_INSECURE", Some("TRUE")),
        ]),
        SupportedOs::Macos,
        no_distros,
    );
    let gitlab = of(&plan, Source::Gitlab)[0];
    assert_eq!(
        value(gitlab, GITLAB_URL),
        Some("https://gitlab.example.test")
    );
    assert_eq!(value(gitlab, GITLAB_TOKEN), Some("glpat-test-0000"));
    assert_eq!(value(gitlab, GITLAB_USER), Some("me"));
    let jira = of(&plan, Source::Jira)[0];
    assert_eq!(value(jira, JIRA_TOKEN), Some("jira-test-0000"));
    assert_eq!(value(jira, JIRA_INSECURE), Some("true"));
    let git = of(&plan, Source::Git);
    assert_eq!(git.len(), 1);
    assert_eq!(git[0].location, ConnectionLocation::Local);
    assert_eq!(value(git[0], GIT_DIRS), None);
    assert!(
        plan.connections
            .iter()
            .all(|planned| planned.skipped.is_empty())
    );
}

/// Only where any of its keys is set; `JIRA_INSECURE` anything but true is today's default.
#[test]
fn a_kind_without_a_set_key_is_not_added() {
    let plan = plan_of(
        &values(&[
            ("GITLAB_URL", Some(" ")),
            ("GITLAB_USER", None),
            ("JIRA_INSECURE", Some("yes")),
        ]),
        SupportedOs::Linux,
        no_distros,
    );
    assert!(of(&plan, Source::Gitlab).is_empty());
    let jira = of(&plan, Source::Jira)[0];
    assert_eq!(value(jira, JIRA_INSECURE), None);
    assert_eq!(value(jira, JIRA_URL), None);
}

/// A token without an address is left out (it could not be sealed for one); a value today's
/// settings refuse is left out with its reason, never written.
#[test]
fn what_cannot_be_written_is_left_out_with_why() {
    let plan = plan_of(
        &values(&[
            ("GITLAB_TOKEN", Some("glpat-test-0000")),
            ("JIRA_URL", Some("ftp://jira.example.test")),
            ("JIRA_TOKEN", Some("jira-test-0000")),
        ]),
        SupportedOs::Linux,
        no_distros,
    );
    let gitlab = of(&plan, Source::Gitlab)[0];
    assert_eq!(value(gitlab, GITLAB_TOKEN), None);
    assert_eq!(
        gitlab.skipped,
        [("GITLAB_TOKEN", LegacySkipReason::NoAddress)]
    );
    let jira = of(&plan, Source::Jira)[0];
    assert_eq!(value(jira, JIRA_URL), None);
    assert_eq!(
        jira.skipped,
        [
            ("JIRA_URL", LegacySkipReason::Invalid),
            ("JIRA_TOKEN", LegacySkipReason::NoAddress)
        ]
    );
    assert!(!gitlab.has_secret() && !jira.has_secret());
}

/// `GIT_DIRS` unset is the home default, none is `[]`, items are the items.
#[test]
fn git_dirs_keep_unset_none_and_items_apart() {
    let dirs = |raw: Option<&str>| {
        let plan = plan_of(
            &values(&[("GIT_DIRS", raw)]),
            SupportedOs::Linux,
            no_distros,
        );
        value(of(&plan, Source::Git)[0], GIT_DIRS).map(str::to_string)
    };
    assert_eq!(dirs(None), None);
    assert_eq!(dirs(Some("")), None);
    assert_eq!(dirs(Some("[]")), Some(stored_list("[]")));
    assert_eq!(
        dirs(Some(r#"["a"," b ",""]"#)),
        Some(stored_list(r#"["a"," b ",""]"#))
    );
    assert_eq!(dirs(Some("x, y")), Some(stored_list("x, y")));
}

/// On Windows each searched distribution is a WSL Git connection: the registry's while
/// `WSL_DISTROS` is unset; a searched one with repositories searches its home too, one with
/// repositories only holds them alone; an item that is no `Distro:/path` is counted.
#[test]
fn wsl_distributions_are_grouped_on_windows() {
    let plan = plan_of(
        &values(&[(
            "WSL_GIT_DIRS",
            Some("Ubuntu:/home/u/repo, Debian:/srv/r, C:/x, Ubuntu:/home/u/other"),
        )]),
        SupportedOs::Windows,
        || {
            vec![
                "Ubuntu".to_string(),
                "Ubuntu".to_string(),
                "Arch".to_string(),
            ]
        },
    );
    let wsl = of(&plan, Source::Git)
        .into_iter()
        .filter_map(|planned| match &planned.location {
            ConnectionLocation::Wsl(distro) => Some((
                distro.as_str(),
                value(planned, GIT_DIRS).map(str::to_string),
            )),
            ConnectionLocation::Local => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        wsl,
        [
            (
                "Ubuntu",
                Some(stored_list(r#"["~","/home/u/repo","/home/u/other"]"#))
            ),
            ("Arch", None),
            ("Debian", Some(stored_list(r#"["/srv/r"]"#))),
        ]
    );
    assert_eq!(plan.invalid_wsl_repos, 1);

    let none = plan_of(
        &values(&[("WSL_DISTROS", Some("[]"))]),
        SupportedOs::Windows,
        no_distros,
    );
    assert_eq!(of(&none, Source::Git).len(), 1);
}

/// Off Windows no WSL key is read.
#[test]
fn wsl_keys_are_read_only_on_windows() {
    let plan = plan_of(
        &values(&[
            ("WSL_DISTROS", Some("Ubuntu")),
            ("WSL_GIT_DIRS", Some("Ubuntu:/r")),
        ]),
        SupportedOs::Macos,
        no_distros,
    );
    assert_eq!(of(&plan, Source::Git).len(), 1);
    assert_eq!(plan.invalid_wsl_repos, 0);
}

/// Outlook and System only where today's adapters run.
#[test]
fn outlook_and_system_come_where_their_adapters_run() {
    for os in [SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux] {
        let plan = plan_of(
            &values(&[("WSL_DISTROS", Some("[]"))]),
            os.clone(),
            no_distros,
        );
        for source in [Source::Outlook, Source::System] {
            assert_eq!(
                !of(&plan, source.clone()).is_empty(),
                adapter(&source).supported_os().contains(&os),
                "{source:?} on {os:?}"
            );
        }
    }
}
