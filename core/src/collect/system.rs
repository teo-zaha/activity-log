use std::process::Command;

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::{
    collect::log_skip,
    model::{CollectOptions, SystemEntry, SystemEntryType, system::raw_system_event::RawSystemEvent},
    utils::ensure_valid_date,
};

const SYSTEM_EVENTS: &[(i32, SystemEntryType, &str)] = &[
    (6005, SystemEntryType::Boot, "System started"),
    (6006, SystemEntryType::Shutdown, "System shutdown"),
    (6008, SystemEntryType::UnexpectedShutdown, "Unexpected shutdown"),
    (42, SystemEntryType::Sleep, "System entering sleep"),
    (107, SystemEntryType::Wake, "System resumed from sleep"),
];

const SECURITY_EVENTS: &[(i32, SystemEntryType, &str)] = &[
    (4800, SystemEntryType::Lock, "Workstation locked"),
    (4801, SystemEntryType::Unlock, "Workstation unlocked"),
];

pub fn collect_system(options: &CollectOptions) -> Result<Vec<SystemEntry>> {
    if cfg!(not(target_os = "windows")) {
        log_skip("system", "Windows Event Log is only available on Windows");
        return Ok(Vec::new());
    }

    ensure_valid_date(&options.since, "since")?;
    ensure_valid_date(&options.until, "until")?;

    let system_raw = query_log(
        "System",
        &SYSTEM_EVENTS.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(),
        &options.since,
        &options.until,
    )?;

    let security_raw = match query_log(
        "Security",
        &SECURITY_EVENTS.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(),
        &options.since,
        &options.until,
    ) {
        Ok(events) => events,
        Err(err) => {
            eprintln!("  [system] Security log unavailable: {err}");
            Vec::new()
        }
    };

    if security_raw.is_empty() {
        eprintln!(
            "  [system] lock/unlock events unavailable.\n  To enable precise events, run PowerShell as admin:\n    auditpol /set /subcategory:'Other Logon/Logoff Events' /success:enable"
        );
    } else {
        eprintln!("  [system] lock/unlock via security ({} events)", security_raw.len());
    }

    let mut entries = Vec::new();
    entries.extend(map_events(&system_raw, SYSTEM_EVENTS));
    entries.extend(map_events(&security_raw, SECURITY_EVENTS));
    entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok(entries)
}

fn query_log(log_name: &str, ids: &[i32], since: &str, until: &str) -> Result<Vec<RawSystemEvent>> {
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
    if let Some(array) = parsed.as_array() {
        Ok(array
            .iter()
            .cloned()
            .map(serde_json::from_value)
            .collect::<Result<Vec<RawSystemEvent>, _>>()?)
    } else {
        Ok(vec![serde_json::from_value::<RawSystemEvent>(parsed)?])
    }
}

fn map_events(raw: &[RawSystemEvent], mapping: &[(i32, SystemEntryType, &str)]) -> Vec<SystemEntry> {
    raw.iter()
        .map(|event| {
            let (_, entry_type, label) = mapping
                .iter()
                .find(|(id, _, _)| *id == event.id)
                .cloned()
                .unwrap_or((event.id, SystemEntryType::Boot, "Unknown event"));
            SystemEntry {
                source: "system".to_string(),
                entry_type,
                timestamp: event.ts.clone(),
                label: if label == "Unknown event" {
                    format!("Event {}", event.id)
                } else {
                    label.to_string()
                },
            }
        })
        .collect()
}
