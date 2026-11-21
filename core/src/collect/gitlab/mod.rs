use std::collections::HashSet;

use anyhow::{Result, anyhow, bail};
use chrono::Days;
use serde::Deserialize;

use crate::{
    diagnostics,
    model::{CollectOptions, GitLabEntry, GitLabTag, GitLabType, Timestamp},
    provider::{
        gitlab::{GitLabConfig, api::GitLabApi},
        provider_context::ProviderContext,
        remote_api::is_path_segment,
        remote_error::RemoteFailure,
    },
};

const PAGE_SIZE: usize = 100;

/// GitLab named a page it had already sent, or sent the same events again: read on, the list
/// would never end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PagesRepeated;

impl std::fmt::Display for PagesRepeated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitLab sent a page of events it had sent before; stopped reading")
    }
}

impl std::error::Error for PagesRepeated {}

pub async fn collect_gitlab(
    options: &CollectOptions,
    context: &ProviderContext,
) -> Result<Vec<GitLabEntry>> {
    let config = context.config::<GitLabConfig>()?;
    if !is_path_segment(&config.user) {
        bail!("GITLAB_USER must be a GitLab user name");
    }

    let api = GitLabApi::new(config).map_err(|err| anyhow!(err.reason(GitLabApi::SERVICE)))?;

    // `after` and `before` take dates and exclude them; widen by a day on each side and
    // keep only the events inside the exact range below.
    let (first, last) = options.utc_days();
    let after = (first - Days::new(1)).format("%Y-%m-%d").to_string();
    let before = (last + Days::new(1)).format("%Y-%m-%d").to_string();

    let mut entries = Vec::new();
    let mut skipped = 0;
    let mut url = api.endpoint(&["users", &config.user, "events"]);
    url.query_pairs_mut()
        .append_pair("after", &after)
        .append_pair("before", &before)
        .append_pair("per_page", &PAGE_SIZE.to_string())
        .append_pair("page", "1");
    let mut next = Some(url);
    let mut asked = HashSet::new();
    let mut collected = HashSet::new();

    // Pages follow GitLab's pagination headers, not the number of rows a page holds.
    while let Some(url) = next.take() {
        if !asked.insert(url.to_string()) {
            bail!(PagesRepeated);
        }
        // A page that is no JSON list fails the collection; a row that is no event is skipped.
        let (rows, following) =
            api.get_page::<Vec<serde_json::Value>>(url)
                .await
                .map_err(|error| RemoteFailure {
                    service: GitLabApi::SERVICE.into(),
                    error,
                })?;
        // An event already collected is not read twice (a new event can shift GitLab's offsets
        // by one); a page that brings nothing new would repeat without end.
        let (mut events, mut new) = (0, 0);
        for row in rows {
            let Ok(event) = serde_json::from_value::<GitLabEvent>(row) else {
                skipped += 1;
                continue;
            };
            events += 1;
            if !collected.insert(event.id) {
                continue;
            }
            new += 1;
            let entry = map_gitlab_event(event);
            if options.contains(entry.timestamp.utc()) {
                entries.push(entry);
            }
        }
        if events > 0 && new == 0 {
            bail!(PagesRepeated);
        }
        next = following;
    }

    if skipped > 0 {
        diagnostics::warn(
            "gitlab",
            format!("Skipped {skipped} GitLab events that could not be read"),
        );
    }
    Ok(entries)
}

/// One event of `GET /users/:user/events`, with the fields an entry takes from it. GitLab sends
/// `null` for what an event does not have (a push has no target); every event has an id and a
/// creation time.
#[derive(Debug, Deserialize)]
pub(crate) struct GitLabEvent {
    id: u64,
    action_name: String,
    created_at: Timestamp,
    project_id: Option<u64>,
    target_title: Option<String>,
    target_type: Option<String>,
    target_iid: Option<u64>,
    push_data: Option<PushData>,
    note: Option<Note>,
}

#[derive(Debug, Deserialize)]
struct PushData {
    #[serde(rename = "ref")]
    branch: Option<String>,
    commit_count: Option<u64>,
    commit_to: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Note {
    body: Option<String>,
    resolved: Option<bool>,
    position: Option<NotePosition>,
    /// What the comment is on: `MergeRequest`, `Issue`, `Commit`, …
    noteable_type: Option<String>,
    noteable_iid: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct NotePosition {
    new_path: Option<String>,
}

pub(crate) fn map_gitlab_event(event: GitLabEvent) -> GitLabEntry {
    let entry_type = GitLabType::from_gitlab_action_name(&event.action_name);
    let (branch, commit_count, commit_to) = match event.push_data {
        Some(push) => (push.branch, push.commit_count, push.commit_to),
        None => (None, None, None),
    };
    // The merge request or issue the event is about: its own target, or what a comment is on.
    let (about, iid) = match &event.note {
        Some(note) => (note.noteable_type.as_deref(), note.noteable_iid),
        None => (event.target_type.as_deref(), event.target_iid),
    };
    let iid_of = |kind: &str| iid.filter(|_| about == Some(kind));
    let (mr_iid, issue_iid) = (iid_of("MergeRequest"), iid_of("Issue"));
    let (note_body, note_resolved, note_file) = match event.note {
        Some(note) => (
            note.body,
            note.resolved,
            note.position.and_then(|position| position.new_path),
        ),
        None => (None, None, None),
    };
    GitLabEntry {
        source: GitLabTag,
        connection: crate::model::ConnectionName::unstamped(),
        id: event.id.to_string(),
        pushed_commit: commit_to.filter(|_| entry_type.is_push()),
        entry_type,
        timestamp: event.created_at,
        title: event.target_title,
        target_type: event.target_type,
        project_id: event.project_id,
        mr_iid,
        issue_iid,
        branch,
        commit_count,
        note_body,
        note_file,
        note_resolved,
    }
}

#[cfg(test)]
mod tests;
