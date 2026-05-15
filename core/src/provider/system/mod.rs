use crate::{
    collect::system::collect_system,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

pub enum SystemSupport {
    Available,
    Degraded(String),
    Unavailable(String),
}

fn unsupported_system_support() -> SystemSupport {
    SystemSupport::Unavailable("OS not supported".to_string())
}

#[cfg(target_os = "windows")]
use self::windows as windows_impl;
#[cfg(not(target_os = "windows"))]
mod windows_impl {
    use super::{SystemSupport, unsupported_system_support};

    pub(super) async fn check_system_support() -> SystemSupport {
        unsupported_system_support()
    }
}

#[cfg(target_os = "macos")]
use self::macos as macos_impl;
#[cfg(not(target_os = "macos"))]
mod macos_impl {
    use super::{SystemSupport, unsupported_system_support};

    pub(super) async fn check_system_support() -> SystemSupport {
        unsupported_system_support()
    }
}

mod linux_impl {
    use super::{SystemSupport, unsupported_system_support};

    pub(super) async fn check_system_support() -> SystemSupport {
        unsupported_system_support()
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
                SystemSupport::Available => ProviderCheckResult::available(),
                SystemSupport::Degraded(reason) => {
                    ProviderCheckResult::degraded(reason)
                }
                SystemSupport::Unavailable(reason) => {
                    ProviderCheckResult::unavailable(reason)
                }
            }
        }

        async fn collect(&self, current_os, options, _context) {
            let entries = collect_system(&options, current_os).await?;
            Ok(entries.into_activity_entries())
        }
    }
}
