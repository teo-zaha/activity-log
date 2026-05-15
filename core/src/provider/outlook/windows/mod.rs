use tokio::process::Command;

pub async fn check_outlook_ready() -> anyhow::Result<()> {
    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(
            r#"
try {
  $ol = New-Object -ComObject Outlook.Application -ErrorAction Stop
  $ns = $ol.GetNamespace("MAPI")
  if ($null -eq $ns) {
    throw "Outlook MAPI namespace unavailable"
  }

  $calendar = $ns.GetDefaultFolder(9)
  if ($null -eq $calendar) {
    throw "Outlook calendar unavailable"
  }

  $null = $calendar.Items

  exit 0
} catch {
  Write-Error $_
  exit 1
}
"#,
        )
        .output()
        .await?;

    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
