use crate::platform::macos::{outlook_is_installed, run_apple_script};

pub async fn check_outlook_ready() -> anyhow::Result<()> {
    if !outlook_is_installed() {
        return Err(anyhow::anyhow!("Outlook not installed"));
    }

    run_apple_script(
        r#"
tell application "Microsoft Outlook"
  if (count of exchange accounts) is 0 then error "Outlook not signed in"
  if not (exists default calendar) then error "Outlook default calendar unavailable"
  set defaultCalendar to default calendar
  set _ to count of calendar events of defaultCalendar
end tell
return "ok"
"#,
    )
    .map(|_| ())
    .map_err(|err| anyhow::anyhow!("Outlook AppleScript access failed: {err}"))
}
