//! Calendars published as ICS feeds: Outlook, Google, iCloud and others, or a local `.ics`.

use crate::{
    collect::calendar::{check_feed, collect_calendar},
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::{connection::Instances, provider_check_result::ProviderCheckResult},
};

crate::provider_config! {
    pub struct CalendarConfig {
        pub ics_url => CALENDAR_ICS_URL: Password,
    }
}

crate::provider! {
    CalendarProvider(Source::Calendar, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], CalendarConfig) {
        async fn startup_check(&self, _current_os, context) {
            let config = match context.config::<CalendarConfig>() {
                Ok(config) => config,
                Err(err) => return ProviderCheckResult::unavailable(format!("{err:#}")),
            };
            match check_feed(&config.ics_url).await {
                Ok(_) => ProviderCheckResult::available(),
                Err(err) => ProviderCheckResult::unavailable(format!("{err:#}")),
            }
        }

        async fn collect(&self, _current_os, options, context) {
            let config = context.config::<CalendarConfig>()?;
            let entries = collect_calendar(&config.ics_url, &options).await?;
            Ok(entries.into_activity_entries())
        }

        fn description(&self) -> &'static str {
            "Events from an ICS calendar link or .ics file"
        }

        fn instances(&self) -> Instances {
            Instances::Multiple
        }
    }
}
