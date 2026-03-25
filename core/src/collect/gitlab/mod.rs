use anyhow::{Result, anyhow};
use chrono::{Days, NaiveDate};
use reqwest::Client;
use serde_json::{Map, Value};

use crate::{
    cache::{get_cached, set_cached},
    collect::load_required_envs,
    model::{CollectOptions, GitLabEntry, GitLabType},
    utils::ensure_valid_date,
};

pub async fn collect_gitlab(options: &CollectOptions) -> Result<Vec<GitLabEntry>> {
    ensure_valid_date(&options.since, "since")?;
    ensure_valid_date(&options.until, "until")?;

    let cache_key = format!("gitlab-{}-{}", options.since, options.until);
    if let Some(cached) = get_cached::<Vec<GitLabEntry>>(&cache_key)? {
        eprintln!("  [gitlab] cache hit ({} events)", cached.len());
        return Ok(cached);
    }

    let Some(env_values) = load_required_envs("gitlab", &["GITLAB_URL", "GITLAB_TOKEN", "GITLAB_USER"]) else {
        return Ok(Vec::new());
    };
    let gitlab_url = &env_values[0];
    let gitlab_token = &env_values[1];
    let gitlab_user = &env_values[2];

    let after = NaiveDate::parse_from_str(&options.since, "%Y-%m-%d")?
        .checked_sub_days(Days::new(1))
        .unwrap();
    let before = NaiveDate::parse_from_str(&options.until, "%Y-%m-%d")?
        .checked_add_days(Days::new(1))
        .unwrap();

    let client = Client::builder().build()?;
    let mut entries = Vec::new();
    let mut page = 1;

    loop {
        let url = format!(
            "{}/api/v4/users/{}/events?after={}&before={}&per_page=100&page={page}",
            gitlab_url.trim_end_matches('/'),
            gitlab_user,
            after.format("%Y-%m-%d"),
            before.format("%Y-%m-%d"),
        );

        let response = client
            .get(&url)
            .header("PRIVATE-TOKEN", gitlab_token)
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

        for event in &events {
            if let Some(entry) = map_gitlab_event(event) {
                entries.push(entry);
            }
        }

        if events.len() < 100 {
            break;
        }
        page += 1;
    }

    set_cached(&cache_key, &entries)?;
    Ok(entries)
}

pub fn map_gitlab_event(event: &Value) -> Option<GitLabEntry> {
    let object = event.as_object()?.clone();
    let push_data = object.get("push_data").and_then(Value::as_object);
    let note = object.get("note").and_then(Value::as_object);
    let note_position = note.and_then(|note| note.get("position")).and_then(Value::as_object);
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
        mr_iid: object.get("target_iid").and_then(Value::as_i64).map(|value| value as i32),
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
        raw: Value::Object(Map::<String, Value>::from_iter(object.into_iter())),
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
