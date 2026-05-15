use anyhow::{Result, anyhow};
use tokio::process::Command;

use crate::model::{CollectOptions, OutlookEntry};

use super::{normalize_outlook_events, parse_raw_outlook_events};

pub(super) async fn collect_outlook(options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
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
        since = options.since.format("%Y-%m-%d"),
        until = options.until.format("%Y-%m-%d"),
    );

    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script)
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow!(
            "PowerShell exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let events = parse_raw_outlook_events(String::from_utf8_lossy(&output.stdout).as_ref())?;
    Ok(normalize_outlook_events(events))
}
