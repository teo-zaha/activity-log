mod config;
#[cfg(any(target_os = "windows", test))]
pub mod windows;

pub use config::*;

use std::{collections::HashSet, ops::ControlFlow, sync::Arc};

use crate::{
    collect::git::{
        GitCollection, collect_git, denied_root, keep_own, open_repo_strict, report_identities,
        visit_git_repos,
    },
    model::{ActivityEntry, IntoActivityEntries, Source, SupportedOs},
    provider::{
        connection::{ConnectionLocation, Instances},
        provider_adapter::{CollectionState, ConnectionEntries},
        provider_check_result::ProviderCheckResult,
        source_failure::{SourceFailure, SourceProblem},
    },
};

/// The label of the Git connection for this computer's own repositories.
pub const LOCAL_LABEL: &str = "This computer";

#[cfg(target_os = "windows")]
use self::windows as windows_impl;
#[cfg(not(target_os = "windows"))]
mod windows_impl {
    use crate::provider::{git::GitConfig, provider_check_result::ProviderCheckResult};

    pub(super) fn check_wsl_ready(
        distro: &str,
        _config: &GitConfig,
        _cancelled: &std::sync::atomic::AtomicBool,
    ) -> ProviderCheckResult {
        ProviderCheckResult::unavailable(format!("WSL {distro} needs Windows"))
    }
}

crate::provider! {
    GitProvider(Source::Git, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], GitConfig) {
        async fn startup_check(&self, _current_os, context) {
            let config = match context.config::<GitConfig>() {
                Ok(c) => c,
                Err(err) => {
                    return ProviderCheckResult::unavailable(format!("Git config is invalid: {err:#}"));
                }
            };

            let config = config.clone();
            let location = context.location();
            // Its timeout drops the wait, which stops a `wsl.exe` that hangs.
            crate::platform::blocking::blocking_until_dropped("Git startup check", move |cancelled| {
                match location {
                    ConnectionLocation::Local => local_check_result(&config),
                    ConnectionLocation::Wsl(distro) => {
                        windows_impl::check_wsl_ready(&distro, &config, cancelled)
                    }
                }
            })
            .await
            .unwrap_or_else(|err| ProviderCheckResult::unavailable(format!("{err:#}")))
        }

        async fn collect(&self, _current_os, options, context) {
            let config = context.config::<GitConfig>()?.clone();
            let commits = collect_git(&options, &config, context.location()).await?;
            if let Some(collection) = context.collection_state::<GitCollection>() {
                collection.record(&commits);
            }
            Ok(commits
                .repos
                .into_iter()
                .flat_map(|repo| repo.commits)
                .into_activity_entries())
        }

        fn description(&self) -> &'static str {
            "Commits from your repositories"
        }

        fn instances(&self) -> Instances {
            Instances::Multiple
        }

        fn collection_state(&self) -> Option<CollectionState> {
            Some(Arc::new(GitCollection::default()))
        }

        /// Keeps the commits authored with any email found in any Git connection. A provisional
        /// pass knows only the emails found so far, so it keeps a subset of the complete one,
        /// and a commit two connections hold may move to the earlier one later.
        fn finish_kind(
            &self,
            state: Option<&CollectionState>,
            results: Vec<ConnectionEntries>,
            complete: bool,
        ) -> Vec<ConnectionEntries> {
            let Some(collection) = state.and_then(|state| state.downcast_ref::<GitCollection>())
            else {
                return results;
            };
            let found = collection.found();
            let mut ignored = 0;
            // A commit is one piece of work, even when two clones or locations hold it.
            let mut seen = HashSet::new();
            let mut copies = 0;
            let results = results
                .into_iter()
                .map(|(id, entries)| {
                    let commits = entries
                        .into_iter()
                        .filter_map(|entry| match entry {
                            ActivityEntry::Git(commit) => Some(commit),
                            _ => None,
                        })
                        .collect();
                    let (mut own, skipped) = keep_own(&found.identities, commits);
                    ignored += skipped;
                    let before = own.len();
                    own.retain(|commit| seen.insert(commit.hash.clone()));
                    copies += before - own.len();
                    (id, own.into_activity_entries())
                })
                .collect();
            if !complete {
                return results;
            }
            report_identities(&found, ignored);
            if copies > 0 {
                crate::diagnostics::info(
                    "git",
                    format!("Showed {copies} commits once that more than one clone holds"),
                );
            }
            results
        }
    }
}

/// The basic check: one local repository opens.
fn local_check_result(config: &GitConfig) -> ProviderCheckResult {
    match check_local_repos(config) {
        Ok(()) => ProviderCheckResult::available(),
        Err(err) => ProviderCheckResult::unavailable_from(&err),
    }
}

/// Stops at the first repository that opens, so the check does not walk the whole disk. None
/// found is `SourceProblem::NoData`.
pub(crate) fn check_local_repos(config: &GitConfig) -> anyhow::Result<()> {
    let mut found = false;
    let mut last_error = None;
    visit_git_repos(config, &mut |repo| {
        found = true;
        match open_repo_strict(&repo) {
            Ok(_) => {
                last_error = None;
                ControlFlow::Break(())
            }
            Err(err) => {
                last_error = Some(format!("{}: {err}", repo.display()));
                ControlFlow::Continue(())
            }
        }
    })?;

    match (found, last_error) {
        (false, _) => Err(match denied_root(config) {
            Some(root) => SourceFailure::new(
                SourceProblem::PermissionDenied,
                format!("Not allowed to read {}", root.display()),
            ),
            None => SourceFailure::new(SourceProblem::NoData, "No repositories found"),
        }
        .into()),
        (true, None) => Ok(()),
        (true, Some(reason)) => Err(anyhow::anyhow!("No readable repositories found: {reason}")),
    }
}
