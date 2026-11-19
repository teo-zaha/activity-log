use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Days};
use serde_json::Value;

use crate::{
    model::{CollectOptions, GitLabEntry, GitLabType},
    provider::{
        gitlab::{GitLabConfig, api::GitLabApi},
        provider_context::ProviderContext,
        remote_api::is_path_segment,
    },
};

const PAGE_SIZE: usize = 100;

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
    let mut page = 1;

    loop {
        let mut url = api.endpoint(&["users", &config.user, "events"]);
        url.query_pairs_mut()
            .append_pair("after", &after)
            .append_pair("before", &before)
            .append_pair("per_page", &PAGE_SIZE.to_string())
            .append_pair("page", &page.to_string());
        let events = api
            .get_json::<Vec<Value>>(url)
            .await
            .map_err(|err| anyhow!(err.message(GitLabApi::SERVICE)))?;
        if events.is_empty() {
            break;
        }

        entries.extend(events.iter().filter_map(map_gitlab_event).filter(|entry| {
            DateTime::parse_from_rfc3339(&entry.timestamp)
                .is_ok_and(|at| options.contains(at.to_utc()))
        }));

        if events.len() < PAGE_SIZE {
            break;
        }
        page += 1;
    }

    Ok(entries)
}

pub fn map_gitlab_event(event: &Value) -> Option<GitLabEntry> {
    let object = event.as_object()?;
    let push_data = object.get("push_data").and_then(Value::as_object);
    let note = object.get("note").and_then(Value::as_object);
    let note_position = note
        .and_then(|note| note.get("position"))
        .and_then(Value::as_object);
    let target_type = object
        .get("target_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    let entry_type = GitLabType::from_gitlab_action_name(
        object
            .get("action_name")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
    );

    let timestamp = object
        .get("created_at")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let title = object
        .get("target_title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let id = match object.get("id") {
        Some(Value::Number(id)) => id.to_string(),
        Some(Value::String(id)) => id.clone(),
        _ => crate::utils::stable_id(&[entry_type.as_ref(), &timestamp, &title]),
    };

    Some(GitLabEntry {
        source: "gitlab".to_string(),
        connection: String::new(),
        id,
        pushed_commit: push_data
            .filter(|_| entry_type == GitLabType::PushedTo)
            .and_then(|value| value.get("commit_to"))
            .and_then(Value::as_str)
            .map(str::to_string),
        entry_type,
        timestamp,
        title,
        target_type,
        mr_iid: object
            .get("target_iid")
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        branch: push_data
            .and_then(|value| value.get("ref"))
            .and_then(Value::as_str)
            .map(str::to_string),
        commit_count: push_data
            .and_then(|value| value.get("commit_count"))
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        note_body: note
            .and_then(|value| value.get("body"))
            .and_then(Value::as_str)
            .map(str::to_string),
        note_file: note_position
            .and_then(|value| value.get("new_path"))
            .and_then(Value::as_str)
            .map(str::to_string),
        note_resolved: note
            .and_then(|value| value.get("resolved"))
            .and_then(Value::as_bool),
    })
}

#[cfg(test)]
mod tests;
