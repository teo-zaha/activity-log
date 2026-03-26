use std::process::Command;

pub fn check_outlook_installed() -> anyhow::Result<()> {
    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(
            r#"
try {
  New-Object -ComObject Outlook.Application -ErrorAction Stop | Out-Null
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
