//! 0.0.7's values as today's connections, before anything is written: one GitLab and one
//! Jira connection where any of their keys is set, the local Git connection 0.0.7 always ran,
//! on Windows one WSL Git connection per distribution it searched or held repositories in, and
//! Outlook and System where today's adapters run. Each value goes through today's
//! `settings::view::normalize`; one it refuses is left out, as is a token with no address to
//! bind it to.

use std::collections::BTreeMap;

use super::v0_0_7::{self, LegacyValues, parse_wsl_repo};
use crate::{
    model::{LegacySkipReason, Source, SupportedOs},
    provider::{
        adapter,
        connection::{ConnectionLocation, ConnectionValues},
        git::{GIT_DIRS, WSL_HOME},
        gitlab::{GITLAB_TOKEN, GITLAB_URL, GITLAB_USER},
        jira::{JIRA_INSECURE, JIRA_TOKEN, JIRA_URL},
        provider_config::ProviderConfigKey,
    },
    secrets::sealed::Binding,
    settings::view::{Submitted, normalize},
};

#[cfg(test)]
mod tests;

/// One connection to add, its values in their stored form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Planned {
    pub(super) source: Source,
    pub(super) location: ConnectionLocation,
    pub(super) values: Vec<(ProviderConfigKey, String)>,
    /// What was left out of it, by 0.0.7's key name.
    pub(super) skipped: Vec<(&'static str, LegacySkipReason)>,
}

impl Planned {
    fn new(source: Source, location: ConnectionLocation) -> Self {
        Self {
            source,
            location,
            values: Vec::new(),
            skipped: Vec::new(),
        }
    }

    /// `raw` normalized for `key`, or left out with why.
    fn take(&mut self, legacy: &'static str, key: ProviderConfigKey, raw: Submitted<'_>) {
        match normalize(key, raw) {
            Ok(Some(stored)) => self.values.push((key, stored)),
            Ok(None) => {}
            Err(_) => self.skipped.push((legacy, LegacySkipReason::Invalid)),
        }
    }

    /// Leaves a secret out unless its connection names what it is bound to.
    fn bind_secret(&mut self, legacy: &'static str, secret: ProviderConfigKey) {
        let values = ConnectionValues::new(
            self.values
                .iter()
                .map(|(key, stored)| (key.name.to_string(), Some(stored.clone())))
                .collect(),
        );
        if self.values.iter().any(|(key, _)| *key == secret)
            && Binding::of(&self.source, &values).is_err()
        {
            self.values.retain(|(key, _)| *key != secret);
            self.skipped.push((legacy, LegacySkipReason::NoAddress));
        }
    }

    pub(super) fn has_secret(&self) -> bool {
        self.values.iter().any(|(key, _)| key.is_secret())
    }
}

/// The connections to add, in file order, and the `WSL_GIT_DIRS` items no connection was made
/// from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Plan {
    pub(super) connections: Vec<Planned>,
    pub(super) invalid_wsl_repos: usize,
}

/// The connections `values` stand for on `os`; `wsl_distros` is asked only on Windows with
/// `WSL_DISTROS` unset, as 0.0.7 asked the registry then.
pub(super) fn plan(
    values: &LegacyValues,
    os: SupportedOs,
    wsl_distros: impl FnOnce() -> Vec<String>,
) -> Plan {
    let mut connections = Vec::new();
    let text = |key| values.get(key).map(Submitted::Text);

    let gitlab = [
        (v0_0_7::GITLAB_URL, GITLAB_URL),
        (v0_0_7::GITLAB_TOKEN, GITLAB_TOKEN),
        (v0_0_7::GITLAB_USER, GITLAB_USER),
    ];
    if gitlab
        .iter()
        .any(|(legacy, _)| values.get(legacy).is_some())
    {
        let mut planned = Planned::new(Source::Gitlab, ConnectionLocation::Local);
        for (legacy, key) in gitlab {
            if let Some(raw) = text(legacy) {
                planned.take(legacy, key, raw);
            }
        }
        planned.bind_secret(v0_0_7::GITLAB_TOKEN, GITLAB_TOKEN);
        connections.push(planned);
    }

    let jira_keys = [v0_0_7::JIRA_URL, v0_0_7::JIRA_TOKEN, v0_0_7::JIRA_INSECURE];
    if jira_keys.iter().any(|legacy| values.get(legacy).is_some()) {
        let mut planned = Planned::new(Source::Jira, ConnectionLocation::Local);
        for (legacy, key) in [
            (v0_0_7::JIRA_URL, JIRA_URL),
            (v0_0_7::JIRA_TOKEN, JIRA_TOKEN),
        ] {
            if let Some(raw) = text(legacy) {
                planned.take(legacy, key, raw);
            }
        }
        // 0.0.7 read anything but `true` as false; today's default is false.
        if values.bool(v0_0_7::JIRA_INSECURE) {
            planned.take(
                v0_0_7::JIRA_INSECURE,
                JIRA_INSECURE,
                Submitted::Text("true"),
            );
        }
        planned.bind_secret(v0_0_7::JIRA_TOKEN, JIRA_TOKEN);
        connections.push(planned);
    }

    let mut git = Planned::new(Source::Git, ConnectionLocation::Local);
    if let Some(dirs) = values.list(v0_0_7::GIT_DIRS) {
        git.take(v0_0_7::GIT_DIRS, GIT_DIRS, Submitted::List(&dirs));
    }
    connections.push(git);

    let mut invalid_wsl_repos = 0;
    if os == SupportedOs::Windows {
        let searched = values.list(v0_0_7::WSL_DISTROS).unwrap_or_else(wsl_distros);
        let mut repos = BTreeMap::<String, Vec<String>>::new();
        let mut repo_only = Vec::new();
        for spec in values.list(v0_0_7::WSL_GIT_DIRS).unwrap_or_default() {
            let Some(repo) = parse_wsl_repo(&spec) else {
                invalid_wsl_repos += 1;
                continue;
            };
            if !searched.contains(&repo.distro) && !repo_only.contains(&repo.distro) {
                repo_only.push(repo.distro.clone());
            }
            repos.entry(repo.distro).or_default().push(repo.path);
        }
        let mut seen = Vec::new();
        for distro in searched.iter().chain(&repo_only) {
            if seen.contains(distro) {
                continue;
            }
            seen.push(distro.clone());
            let mut planned = Planned::new(Source::Git, ConnectionLocation::Wsl(distro.clone()));
            if let Some(paths) = repos.get(distro) {
                // A searched distribution was searched from its home as well.
                let dirs = searched
                    .contains(distro)
                    .then(|| WSL_HOME.to_string())
                    .into_iter()
                    .chain(paths.iter().cloned())
                    .collect::<Vec<_>>();
                planned.take(v0_0_7::WSL_GIT_DIRS, GIT_DIRS, Submitted::List(&dirs));
            }
            connections.push(planned);
        }
    }

    for source in [Source::Outlook, Source::System] {
        if adapter(&source).supported_os().contains(&os) {
            connections.push(Planned::new(source, ConnectionLocation::Local));
        }
    }

    Plan {
        connections,
        invalid_wsl_repos,
    }
}
