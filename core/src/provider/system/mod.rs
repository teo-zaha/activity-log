use crate::{
    collect::system::collect_system,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

pub mod macos;
pub mod windows;

pub enum SystemSupport {
    Available,
    Degraded(String),
    Unavailable(String),
}

crate::provider! {
    SystemProvider(Source::System, &[SupportedOs::Windows, SupportedOs::Macos]) {
        async fn startup_check(&self, current_os, _context) {
            let result = match current_os {
                SupportedOs::Windows => windows::check_system_support().await,
                SupportedOs::Macos => macos::check_system_support().await,
                SupportedOs::Linux => SystemSupport::Unavailable("OS not supported".to_string()),
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
