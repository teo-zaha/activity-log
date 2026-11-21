use crate::{
    collect::outlook::collect_outlook,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
use self::fallback as imp;
#[cfg(target_os = "macos")]
use self::macos as imp;
#[cfg(target_os = "windows")]
use self::windows as imp;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod fallback {
    pub(super) async fn check_outlook_ready() -> anyhow::Result<()> {
        anyhow::bail!("OS not supported")
    }
}

crate::provider! {
    OutlookProvider(Source::Outlook, &[SupportedOs::Windows, SupportedOs::Macos]) {
        async fn startup_check(&self, _current_os, _context) {
            match imp::check_outlook_ready().await {
                Ok(()) => ProviderCheckResult::available(),
                Err(err) => ProviderCheckResult::unavailable_from(&err),
            }
        }

        async fn collect(&self, _current_os, options, _context) {
            let entries = collect_outlook(&options).await?;
            Ok(entries.into_activity_entries())
        }

        fn description(&self) -> &'static str {
            "Meetings from the Outlook desktop app"
        }
    }
}
