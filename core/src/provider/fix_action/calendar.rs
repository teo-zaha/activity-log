use url::Url;

use crate::model::{CalendarFeedGuide, CalendarFeedService, WebFixAction};

const OUTLOOK_INSTRUCTIONS: &str = "In Settings → Calendar → Shared calendars, publish a calendar and copy its ICS link. Your organization may hide publishing.";
const OUTLOOK_RECOVERY: &str = "This link may have been unpublished or changed. Publish it again and paste the current ICS link.";
const PUBLIC_LINK: &str = " Anyone with the link can read the calendar details you publish.";
const PRIVATE_LINK: &str = " Only share calendar links with apps you trust.";

struct Guide {
    service: CalendarFeedService,
    label: &'static str,
    base: &'static str,
    path: &'static [&'static str],
    query: &'static [(&'static str, &'static str)],
    instructions: &'static str,
    publishes: bool,
    recovery: Option<&'static str>,
    feed_hosts: &'static [&'static str],
}

const GUIDES: [Guide; 5] = [
    Guide {
        service: CalendarFeedService::OutlookWork,
        label: "Microsoft 365 / Outlook on the web",
        base: "https://outlook.office.com/",
        path: &["calendar", ""],
        query: &[],
        instructions: OUTLOOK_INSTRUCTIONS,
        publishes: true,
        recovery: Some(OUTLOOK_RECOVERY),
        feed_hosts: &[
            "outlook.office.com",
            "outlook.office365.com",
            "outlook.cloud.microsoft",
        ],
    },
    Guide {
        service: CalendarFeedService::OutlookCom,
        label: "Outlook.com",
        base: "https://outlook.live.com/",
        path: &["calendar", ""],
        query: &[],
        instructions: "In Settings → Calendar → Shared calendars, publish a calendar and copy its ICS link.",
        publishes: true,
        recovery: Some(OUTLOOK_RECOVERY),
        feed_hosts: &["outlook.live.com"],
    },
    Guide {
        service: CalendarFeedService::GoogleCalendar,
        label: "Google Calendar",
        base: "https://calendar.google.com/",
        path: &["calendar", "u", "0", "r", "settings"],
        query: &[],
        instructions: "Under Settings for my calendars, choose your calendar and open Integrate calendar. Copy the Secret address in iCal format. Workspace administrators can hide this option.",
        publishes: false,
        recovery: Some(
            "Check the secret iCal address in Google Calendar and paste the current link. If you shared the address, reset it first.",
        ),
        feed_hosts: &["calendar.google.com"],
    },
    Guide {
        service: CalendarFeedService::ICloud,
        label: "iCloud",
        base: "https://www.icloud.com/",
        path: &["calendar", ""],
        query: &[],
        instructions: "Use the calendar’s Share button, turn on Public Calendar, and copy the link. Private sharing requires an Apple Account.",
        publishes: true,
        recovery: None,
        feed_hosts: &[],
    },
    Guide {
        service: CalendarFeedService::Fastmail,
        label: "Fastmail",
        base: "https://app.fastmail.com/",
        path: &["settings", "calendars"],
        query: &[("sessionpicker", "1")],
        instructions: "In Settings → Calendars, edit your calendar. Under Publish, choose a detail level, copy the link, and save.",
        publishes: true,
        recovery: None,
        feed_hosts: &[],
    },
];

impl Guide {
    fn url(&self) -> Option<String> {
        let mut url = Url::parse(self.base).ok()?;
        url.path_segments_mut().ok()?.clear().extend(self.path);
        if !self.query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(self.query.iter().copied());
        }
        Some(url.into())
    }

    fn to_action_guide(&self) -> CalendarFeedGuide {
        let mut instructions = self.instructions.to_owned();
        if self.publishes {
            instructions.push_str(PUBLIC_LINK);
        }
        instructions.push_str(PRIVATE_LINK);
        CalendarFeedGuide {
            service: self.service,
            label: self.label.to_owned(),
            instructions,
            url: self.url(),
        }
    }
}

/// Identify a feed by the original HTTPS address, independent of any redirects.
pub(crate) fn service_for(url: &Url) -> Option<CalendarFeedService> {
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let host = url.host_str()?;
    GUIDES
        .iter()
        .find_map(|guide| guide.feed_hosts.contains(&host).then_some(guide.service))
}

pub(crate) fn field_help() -> WebFixAction {
    WebFixAction::FeedHelp {
        guides: GUIDES.iter().map(Guide::to_action_guide).collect(),
        recovery: None,
    }
}

pub(super) fn recovery(service: Option<CalendarFeedService>) -> WebFixAction {
    if let Some(guide) = GUIDES.iter().find(|guide| Some(guide.service) == service)
        && let Some(recovery) = guide.recovery
    {
        return WebFixAction::FeedHelp {
            guides: vec![guide.to_action_guide()],
            recovery: Some(recovery.to_owned()),
        };
    }
    WebFixAction::FeedHelp {
        guides: Vec::new(),
        recovery: Some(
            "Check the calendar’s sharing settings and paste the current ICS link.".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use CalendarFeedService::{GoogleCalendar, OutlookCom, OutlookWork};

    #[test]
    fn recognizes_only_the_approved_feed_hosts() {
        let recognized = GUIDES
            .iter()
            .flat_map(|guide| guide.feed_hosts.iter().map(|host| (*host, guide.service)))
            .collect::<Vec<_>>();
        assert_eq!(
            recognized,
            [
                ("outlook.office.com", OutlookWork),
                ("outlook.office365.com", OutlookWork),
                ("outlook.cloud.microsoft", OutlookWork),
                ("outlook.live.com", OutlookCom),
                ("calendar.google.com", GoogleCalendar),
            ]
        );
    }
}
