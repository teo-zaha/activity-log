use std::process::Command;

pub fn check_outlook_ready() -> anyhow::Result<()> {
    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(
            r#"
try {
  $ol = New-Object -ComObject Outlook.Application -ErrorAction Stop
  $ns = $ol.GetNamespace("MAPI")

  $currentUser = $ns.CurrentUser
  if ($null -eq $currentUser -or [string]::IsNullOrWhiteSpace($currentUser.Name)) {
    throw "Outlook is not signed in"
  }

  $calendar = $ns.GetDefaultFolder(9)
  if ($null -eq $calendar) {
    throw "Outlook calendar unavailable"
  }

  exit 0
} catch {
  Write-Error $_
  exit 1
}
"#,
        )
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
