use crate::platform::macos::outlook_is_installed;

/// Only checks that Outlook is installed: scripting it would launch Outlook at app start.
/// Whether it is running and signed in is decided per collection.
pub async fn check_outlook_ready() -> anyhow::Result<()> {
    if outlook_is_installed() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Outlook not installed"))
    }
}
