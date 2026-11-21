use crate::{
    collect::system::collect_system,
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

/// Answers on a system this provider does not run on; the runtime checks the OS first.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod fallback {
    pub(super) async fn check_system_support() -> anyhow::Result<()> {
        anyhow::bail!("OS not supported")
    }
}

crate::provider! {
    SystemProvider(Source::System, &[SupportedOs::Windows, SupportedOs::Macos]) {
        async fn startup_check(&self, _context) {
            match imp::check_system_support().await {
                Ok(()) => ProviderCheckResult::available(),
                Err(err) => ProviderCheckResult::unavailable_from(&err),
            }
        }

        async fn collect(&self, options, _context) {
            let entries = collect_system(&options).await?;
            Ok(entries.into_activity_entries())
        }

        fn description(&self) -> &'static str {
            "When this computer started and shut down"
        }
    }
}
