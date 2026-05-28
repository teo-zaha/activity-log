use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};

use crate::{
    model::{CollectOptions, SystemEntry, system::raw_system_event::RawSystemEvent},
    platform::query_windows_event_log,
};

use super::{WINDOWS_SYSTEM_EVENTS, map_windows_events, parse_raw_system_events};

pub(super) async fn collect_system(options: &CollectOptions) -> Result<Vec<SystemEntry>> {
    let ids = WINDOWS_SYSTEM_EVENTS
        .iter()
        .map(|(id, _, _)| *id)
        .collect::<Vec<_>>();
    let options = options.clone();
    let system_raw = tokio::task::spawn_blocking(move || query_log("System", &ids, &options))
        .await
        .map_err(|err| anyhow!("System collection task failed: {err}"))??;

    Ok(map_windows_events(&system_raw, WINDOWS_SYSTEM_EVENTS))
}

fn query_log(log_name: &str, ids: &[i32], options: &CollectOptions) -> Result<Vec<RawSystemEvent>> {
    let (start, end) = super::utc_bounds(options.since, options.until)?;
    let query = build_event_query(ids, start, end);
    let xml_events = query_windows_event_log(log_name, &query)?;
    let mut payload = Vec::with_capacity(xml_events.len());

    for xml in xml_events {
        if let Some(event) = parse_event_xml(&xml)? {
            payload.push(event);
        }
    }

    let json = serde_json::to_string(&payload)?;
    parse_raw_system_events(&json)
}

fn build_event_query(ids: &[i32], start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    let ids = ids
        .iter()
        .map(|id| format!("EventID={id}"))
        .collect::<Vec<_>>()
        .join(" or ");

    format!(
        "*[System[({ids}) and TimeCreated[@SystemTime >= '{start}' and @SystemTime <= '{end}']]]",
        start = start.format("%Y-%m-%dT%H:%M:%SZ"),
        end = end.format("%Y-%m-%dT%H:%M:%SZ"),
    )
}

pub(crate) fn parse_event_xml(xml: &str) -> Result<Option<RawSystemEvent>> {
    let Some(id) = extract_xml_value(xml, "<EventID", "</EventID>") else {
        return Ok(None);
    };
    let Some(timestamp) = extract_xml_attribute(xml, "SystemTime") else {
        return Ok(None);
    };

    let id = id.trim().parse::<i32>()?;
    let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp)?
        .with_timezone(&Utc)
        .to_rfc3339();

    Ok(Some(RawSystemEvent { id, ts: timestamp }))
}

fn extract_xml_value(xml: &str, open_tag: &str, close_tag: &str) -> Option<String> {
    let start = xml.find(open_tag)?;
    let content_start = xml[start..].find('>')? + start + 1;
    let content_end = xml[content_start..].find(close_tag)? + content_start;
    Some(xml[content_start..content_end].to_string())
}

fn extract_xml_attribute(xml: &str, attribute: &str) -> Option<String> {
    let single = format!("{attribute}='");
    if let Some(start) = xml.find(&single) {
        let start = start + single.len();
        let end = xml[start..].find('\'')? + start;
        return Some(xml[start..end].to_string());
    }

    let double = format!(r#"{attribute}=""#);
    let start = xml.find(&double)? + double.len();
    let end = xml[start..].find('"')? + start;
    Some(xml[start..end].to_string())
}
