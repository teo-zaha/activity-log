use anyhow::{Result, anyhow};
use tokio::process::Command;

use crate::model::{CollectOptions, OutlookEntry};

use super::{normalize_outlook_events, parse_raw_outlook_events};

pub(super) async fn collect_outlook(options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
    let script = format!(
        r#"
function isoDate(value) {{
  if (!value) {{
    return null;
  }}

  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) {{
    return null;
  }}

  return date.toISOString();
}}

function normalizeOrganizer(value) {{
  if (value === undefined || value === null) {{
    return null;
  }}

  if (typeof value === "string") {{
    return value;
  }}

  try {{
    if (typeof value.name === "function") {{
      return value.name();
    }}
  }} catch (_) {{}}

  try {{
    return String(value);
  }} catch (_) {{
    return null;
  }}
}}

const start = new Date("{since}T00:00:00");
const end = new Date("{until}T23:59:59");
const outlook = Application("Microsoft Outlook");
outlook.includeStandardAdditions = true;

let events;
try {{
  events = outlook.calendarEvents.whose({{
    startTime: {{'>=': start}},
    endTime: {{'<=': end}}
  }})();
}} catch (error) {{
  throw new Error("Outlook calendar access failed: " + error.message);
}}

const payload = events.map((event) => {{
  let organizer = null;
  let isOrganizer = false;

  try {{
    organizer = normalizeOrganizer(event.organizer());
  }} catch (_) {{}}

  try {{
    isOrganizer = Boolean(event.isOrganizer());
  }} catch (_) {{}}

  return {{
    subject: event.subject() || null,
    start: isoDate(event.startTime()),
    end: isoDate(event.endTime()),
    location: event.location() || null,
    organizer,
    is_organizer: isOrganizer
  }};
}});

JSON.stringify(payload);
"#,
        since = options.since.format("%Y-%m-%d"),
        until = options.until.format("%Y-%m-%d"),
    );

    let output = Command::new("/usr/bin/osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(script)
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow!(
            "osascript exited {}: {}",
            output.status,
            stderr_or_stdout(&output)
        ));
    }

    let events = parse_raw_outlook_events(String::from_utf8_lossy(&output.stdout).as_ref())?;
    Ok(normalize_outlook_events(events))
}

fn stderr_or_stdout(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
