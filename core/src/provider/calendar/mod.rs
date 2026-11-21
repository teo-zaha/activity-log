//! Calendars published as ICS feeds: Outlook, Google, iCloud and others, or a local `.ics`.

use crate::{
    collect::calendar::{check_feed, collect_calendar},
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::{connection::Instances, provider_check_result::ProviderCheckResult},
};

crate::provider_config! {
    pub struct CalendarConfig {
        pub ics_url => CALENDAR_ICS_URL "Calendar ICS URL": Password,
    }
}

crate::provider! {
    CalendarProvider(Source::Calendar, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], config: CalendarConfig) {
        async fn startup_check(&self, _context, config) {
            match check_feed(&config.ics_url).await {
                Ok(_) => ProviderCheckResult::available(),
                Err(err) => ProviderCheckResult::unavailable_from(&err),
            }
        }

        async fn collect(&self, options, context) {
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
