use anyhow::{Result, anyhow};

use crate::model::{CollectOptions, SystemEntry};
use crate::platform::powershell_command;

use super::{WINDOWS_SYSTEM_EVENTS, map_windows_events, parse_raw_system_events};

pub(super) async fn collect_system(options: &CollectOptions) -> Result<Vec<SystemEntry>> {
    let system_raw = query_log(
        "System",
        &WINDOWS_SYSTEM_EVENTS
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<Vec<_>>(),
        &options.since.format("%Y-%m-%d").to_string(),
        &options.until.format("%Y-%m-%d").to_string(),
    )
    .await?;

    Ok(map_windows_events(&system_raw, WINDOWS_SYSTEM_EVENTS))
}

async fn query_log(
    log_name: &str,
    ids: &[i32],
    since: &str,
    until: &str,
) -> Result<Vec<crate::model::system::raw_system_event::RawSystemEvent>> {
    let ids = ids
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let script = format!(
        r#"
$ids   = @({ids})
$start = [DateTime]'{since}T00:00:00'
$end   = [DateTime]'{until}T23:59:59'
$events = Get-WinEvent -FilterHashtable @{{LogName='{log_name}'; Id=$ids}} -ErrorAction SilentlyContinue |
  Where-Object {{ $_.TimeCreated -ge $start -and $_.TimeCreated -le $end }} |
  Select-Object @{{n='id';e={{$_.Id}}}}, @{{n='ts';e={{$_.TimeCreated.ToUniversalTime().ToString('o')}}}}
if ($events) {{ $events | ConvertTo-Json -Compress }} else {{ '[]' }}
"#
    );

    let output = powershell_command()
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

    parse_raw_system_events(String::from_utf8_lossy(&output.stdout).as_ref())
}
