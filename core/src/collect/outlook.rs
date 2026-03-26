use std::process::Command;

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::{
    model::{CollectOptions, OutlookEntry, outlook::raw_outlook_event::RawOutlookEvent},
    utils::ensure_valid_date,
};

pub fn collect_outlook(options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
    ensure_valid_date(&options.since, "since")?;
    ensure_valid_date(&options.until, "until")?;

    let script = format!(
        r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
try {{
  $ol = New-Object -ComObject Outlook.Application -ErrorAction Stop
}} catch {{
  Write-Error "Outlook is not installed or not running: $_"
  exit 1
}}
$ns  = $ol.GetNamespace("MAPI")
$cal = $ns.GetDefaultFolder(9)
$items = $cal.Items
$items.IncludeRecurrences = $true
$items.Sort("[Start]")
$start = [DateTime]::Parse("{since}T00:00:00")
$end   = [DateTime]::Parse("{until}T23:59:59")
$filter = "[Start] >= '" + $start.ToString("g") + "' AND [Start] <= '" + $end.ToString("g") + "'"
$filtered = $items.Restrict($filter)
$result = @()
foreach ($ev in $filtered) {{
  $result += [PSCustomObject]@{{
    subject      = $ev.Subject
    start        = $ev.Start.ToUniversalTime().ToString("o")
    end          = $ev.End.ToUniversalTime().ToString("o")
    location     = $ev.Location
    organizer    = $ev.Organizer
    is_organizer = ($ev.MeetingStatus -eq 1 -or $ev.MeetingStatus -eq 0)
  }}
}}
if ($result.Count -gt 0) {{ $result | ConvertTo-Json -Compress }} else {{ '[]' }}
"#,
        since = options.since,
        until = options.until,
    );

    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script)
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "PowerShell exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() || stdout == "[]" {
        return Ok(Vec::new());
    }

    let parsed = serde_json::from_str::<Value>(&stdout)?;
    let events = if let Some(array) = parsed.as_array() {
        array
            .iter()
            .cloned()
            .map(serde_json::from_value)
            .collect::<Result<Vec<RawOutlookEvent>, _>>()?
    } else {
        vec![serde_json::from_value::<RawOutlookEvent>(parsed)?]
    };

    Ok(events
        .into_iter()
        .map(|event| {
            let duration_minutes = chrono::DateTime::parse_from_rfc3339(&event.end)
                .and_then(|end| {
                    chrono::DateTime::parse_from_rfc3339(&event.start)
                        .map(|start| (end - start).num_minutes())
                })
                .unwrap_or_default();
            OutlookEntry {
                source: "outlook".to_string(),
                timestamp: event.start,
                title: event.subject.unwrap_or_else(|| "(No title)".to_string()),
                location: event.location,
                organizer: event.organizer,
                is_organizer: event.is_organizer.unwrap_or(false),
                duration_minutes: duration_minutes as i32,
            }
        })
        .collect())
}
