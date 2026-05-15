use crate::{
    collect::outlook::collect_outlook,
    model::{IntoActivityEntries, Source, SupportedOs},
    platform::unsupported_result,
    provider::provider_check_result::ProviderCheckResult,
};

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
use self::windows as windows_impl;
#[cfg(not(target_os = "windows"))]
mod windows_impl {
    pub(super) async fn check_outlook_ready() -> anyhow::Result<()> {
        super::unsupported_result("OS not supported")
    }
}

#[cfg(target_os = "macos")]
use self::macos as macos_impl;
#[cfg(not(target_os = "macos"))]
mod macos_impl {
    pub(super) async fn check_outlook_ready() -> anyhow::Result<()> {
        super::unsupported_result("OS not supported")
    }
}

mod linux_impl {
    pub(super) async fn check_outlook_ready() -> anyhow::Result<()> {
        super::unsupported_result("OS not supported")
    }
}

crate::provider! {
    OutlookProvider(Source::Outlook, &[SupportedOs::Windows, SupportedOs::Macos]) {
        async fn startup_check(&self, current_os, _context) {
            let result = match current_os {
                SupportedOs::Windows => windows_impl::check_outlook_ready().await,
                SupportedOs::Macos => macos_impl::check_outlook_ready().await,
                SupportedOs::Linux => linux_impl::check_outlook_ready().await,
            };
            match result {
                Ok(()) => ProviderCheckResult::available(),
                Err(err) => ProviderCheckResult::unavailable(err.to_string()),
            }
        }

        async fn collect(&self, current_os, options, _context) {
            let entries = collect_outlook(&options, current_os).await?;
            Ok(entries.into_activity_entries())
        }
    }
}
