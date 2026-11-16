use anyhow::{Result, anyhow};
use chrono::{DateTime, Days};
use serde_json::{Map, Value};

use crate::{
    model::{CollectOptions, GitLabEntry, GitLabType},
    provider::{gitlab::GitLabConfig, provider_context::ProviderContext},
};

pub async fn collect_gitlab(
    options: &CollectOptions,
    context: &ProviderContext,
) -> Result<Vec<GitLabEntry>> {
    let config = context.config::<GitLabConfig>()?;

    let client = crate::provider::http_client_builder().build()?;

    // `after` and `before` take dates and exclude them; widen by a day on each side and
    // keep only the events inside the exact range below.
    let (first, last) = options.utc_days();
    let after = first - Days::new(1);
    let before = last + Days::new(1);

    let mut entries = Vec::new();
    let mut page = 1;

    loop {
        let url = format!(
            "{}/api/v4/users/{}/events?after={}&before={}&per_page=100&page={page}",
            config.url.trim_end_matches('/'),
            config.user,
            after.format("%Y-%m-%d"),
            before.format("%Y-%m-%d"),
        );

        let response = client
            .get(&url)
            .header("PRIVATE-TOKEN", config.token.as_str())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("GitLab API {status}: {body}"));
        }

        let events = response.json::<Vec<Value>>().await?;
        if events.is_empty() {
            break;
        }

        entries.extend(events.iter().filter_map(map_gitlab_event).filter(|entry| {
            DateTime::parse_from_rfc3339(&entry.timestamp)
                .is_ok_and(|at| options.contains(at.to_utc()))
        }));

        if events.len() < 100 {
            break;
        }
        page += 1;
    }

    Ok(entries)
}

pub fn map_gitlab_event(event: &Value) -> Option<GitLabEntry> {
    let object = event.as_object()?.clone();
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

    Some(GitLabEntry {
        source: "gitlab".to_string(),
        entry_type: GitLabType::from_gitlab_action_name(
            object
                .get("action_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
        ),
        timestamp: object
            .get("created_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        title: object
            .get("target_title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
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
        raw: Value::Object(Map::<String, Value>::from_iter(object)),
    })
}

pub fn commit_to_sha(entry: &GitLabEntry) -> Option<&str> {
    entry
        .raw
        .as_object()
        .and_then(|raw| raw.get("push_data"))
        .and_then(Value::as_object)
        .and_then(|push_data| push_data.get("commit_to"))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests;
