use tokio::process::Command;

pub async fn check_outlook_ready() -> anyhow::Result<()> {
    if !outlook_installed().await? {
        return Err(anyhow::anyhow!("Outlook not installed"));
    }

    let output = Command::new("/usr/bin/osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(
            r#"
const outlook = Application("Microsoft Outlook");
outlook.includeStandardAdditions = true;

let accounts;
try {
  accounts = outlook.exchangeAccounts();
} catch (error) {
  throw new Error("Outlook AppleScript access failed: " + error.message);
}

if (!Array.isArray(accounts) || accounts.length === 0) {
  throw new Error("Outlook not signed in");
}

let calendars;
try {
  calendars = outlook.calendars();
} catch (error) {
  throw new Error("Outlook calendar access failed: " + error.message);
}

if (!Array.isArray(calendars) || calendars.length === 0) {
  throw new Error("Outlook calendar unavailable");
}

"ok";
"#,
        )
        .output()
        .await?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let message = if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        stderr
    };

    Err(anyhow::anyhow!(
        "Outlook AppleScript access failed: {message}"
    ))
}

async fn outlook_installed() -> anyhow::Result<bool> {
    let output = Command::new("/usr/bin/mdfind")
        .arg("kMDItemCFBundleIdentifier == 'com.microsoft.Outlook'")
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Spotlight lookup failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}
