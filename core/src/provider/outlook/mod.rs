use crate::{
    collect::outlook::collect_outlook,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

pub mod macos;
pub mod windows;

crate::provider! {
    OutlookProvider(Source::Outlook, &[SupportedOs::Windows, SupportedOs::Macos]) {
        async fn startup_check(&self, current_os, _context) {
            let result = match current_os {
                SupportedOs::Windows => windows::check_outlook_ready().await,
                SupportedOs::Macos => macos::check_outlook_ready().await,
                SupportedOs::Linux => Err(anyhow::anyhow!("OS not supported")),
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
