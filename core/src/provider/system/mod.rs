use crate::{
    collect::system::collect_system,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

/// Answers on a system this provider does not run on; the runtime checks the OS first.
async fn unsupported() -> anyhow::Result<()> {
    crate::platform::unsupported_result("OS not supported")
}

#[cfg(target_os = "windows")]
use self::windows as windows_impl;
#[cfg(not(target_os = "windows"))]
mod windows_impl {
    pub(super) async fn check_system_support() -> anyhow::Result<()> {
        super::unsupported().await
    }
}

#[cfg(target_os = "macos")]
use self::macos as macos_impl;
#[cfg(not(target_os = "macos"))]
mod macos_impl {
    pub(super) async fn check_system_support() -> anyhow::Result<()> {
        super::unsupported().await
    }
}

mod linux_impl {
    pub(super) async fn check_system_support() -> anyhow::Result<()> {
        super::unsupported().await
    }
}

crate::provider! {
    SystemProvider(Source::System, &[SupportedOs::Windows, SupportedOs::Macos]) {
        async fn startup_check(&self, current_os, _context) {
            let result = match current_os {
                SupportedOs::Windows => windows_impl::check_system_support().await,
                SupportedOs::Macos => macos_impl::check_system_support().await,
                SupportedOs::Linux => linux_impl::check_system_support().await,
            };
            match result {
                Ok(()) => ProviderCheckResult::available(),
                Err(err) => ProviderCheckResult::unavailable_from(&err),
            }
        }

        async fn collect(&self, current_os, options, _context) {
            let entries = collect_system(&options, current_os).await?;
            Ok(entries.into_activity_entries())
        }

        fn description(&self) -> &'static str {
            "When this computer started and shut down"
        }
    }
}
