use std::fmt;

use crate::{model::CalendarFeedService, provider::remote_error::RemoteFailure};

#[derive(Debug)]
pub(crate) struct FeedUnavailable {
    pub service: Option<CalendarFeedService>,
    pub failure: RemoteFailure,
}

impl fmt::Display for FeedUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Could not read the calendar feed")
    }
}

impl std::error::Error for FeedUnavailable {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.failure)
    }
}
