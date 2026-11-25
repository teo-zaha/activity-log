use anyhow::Result;
#[cfg(any(target_os = "macos", test))]
use chrono::DateTime;
#[cfg(any(target_os = "macos", test))]
use serde_json::Value;

#[cfg(any(target_os = "windows", target_os = "macos", test))]
use crate::model::BusyStatus;
#[cfg(any(target_os = "macos", test))]
use crate::model::outlook::raw_outlook_event::RawOutlookEvent;
use crate::model::{CollectOptions, OutlookEntry};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "windows", target_os = "macos", test))]
mod normalize;
#[cfg(test)]
mod tests;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
use self::fallback as imp;
#[cfg(target_os = "macos")]
use self::macos as imp;
#[cfg(target_os = "macos")]
pub(crate) use self::macos::OutlookNotRunning;
#[cfg(target_os = "windows")]
use self::windows as imp;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod fallback {
    use anyhow::{Result, bail};

    use crate::model::{CollectOptions, OutlookEntry};

    pub(super) async fn collect_outlook(_options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
        bail!("Outlook collection is not supported on this OS")
    }
}

pub async fn collect_outlook(options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
    imp::collect_outlook(options).await
}

/// Exercises the same classified failure as the real Outlook script without launching it.
#[cfg(all(target_os = "macos", feature = "test-support"))]
#[doc(hidden)]
pub fn stopped_outlook_failure_for_test() -> crate::runtime::CollectFailure {
    let error = OutlookNotRunning::classify(crate::platform::macos::AppleScriptError {
        code: Some(-600),
        message: "Outlook is not running.".into(),
    });
    crate::runtime::CollectFailure::of(&error)
}

/// What to do when Outlook has not registered itself for other apps to attach to.
#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutlookFallback {
    /// Classic Outlook runs: ask COM for its application object. Outlook is single-instance, so
    /// this returns the open window's instance instead of starting a second one.
    Connect,
    /// Outlook runs, but this app is elevated and Windows keeps it from reaching Outlook.
    Elevated,
    /// Only the new Outlook runs, which has no COM interface.
    NewOutlookOnly,
    NotRunning,
}

#[cfg(any(target_os = "windows", test))]
impl OutlookFallback {
    /// Decides from the executable names of the running processes. Outlook often registers its
    /// running object late or not at all (it is per integrity level, too), so a failed lookup
    /// alone does not mean it is closed.
    pub(crate) fn decide<'a>(
        process_names: impl IntoIterator<Item = &'a str>,
        elevated: bool,
    ) -> Self {
        let (mut classic, mut new) = (false, false);
        for name in process_names {
            classic |= name.eq_ignore_ascii_case("outlook.exe");
            new |= name.eq_ignore_ascii_case("olk.exe");
        }
        match (classic, new) {
            (true, _) if elevated => Self::Elevated,
            (true, _) => Self::Connect,
            (false, true) => Self::NewOutlookOnly,
            (false, false) => Self::NotRunning,
        }
    }

    /// Why Outlook cannot be read; `None` for `Connect`, which is still worth a try.
    pub(crate) fn problem(self) -> Option<crate::provider::source_failure::SourceProblem> {
        use crate::provider::source_failure::SourceProblem;
        match self {
            Self::Connect => None,
            Self::Elevated => Some(SourceProblem::PermissionDenied),
            Self::NewOutlookOnly => Some(SourceProblem::NotInstalled),
            Self::NotRunning => Some(SourceProblem::NotRunning),
        }
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Connect => "Outlook is running but could not be reached",
            Self::Elevated => {
                "Outlook is open, but Activity Log runs as administrator and Windows keeps it from reaching Outlook. Start Activity Log without administrator rights."
            }
            Self::NewOutlookOnly => {
                "Only the new Outlook is running. Use its New Outlook switch or Help → Go to classic Outlook if available. If classic Outlook is unavailable, add a Calendar feed."
            }
            Self::NotRunning => "Outlook is not running. Open classic Outlook from Start.",
        }
    }
}

/// The events of the macOS script's answer (`OutlookColumns`), times as RFC 3339 UTC in whole
/// seconds, the way they have always been written (event ids end with them), and the
/// properties Outlook would not give.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_outlook_columns(stdout: &str) -> Result<(Vec<RawOutlookEvent>, Vec<String>)> {
    use anyhow::{anyhow, bail};
    use chrono::SecondsFormat;

    use crate::model::outlook::outlook_columns::OutlookColumns;

    let columns = serde_json::from_str::<OutlookColumns>(stdout.trim())?;
    let count = columns.start.len();
    let lengths = [
        Some(columns.subject.len()),
        Some(columns.end.len()),
        columns.id.as_ref().map(Vec::len),
        columns.location.as_ref().map(Vec::len),
        columns.organizer.as_ref().map(Vec::len),
        columns.is_organizer.as_ref().map(Vec::len),
        columns.busy_status.as_ref().map(Vec::len),
        columns.body.as_ref().map(Vec::len),
    ];
    if lengths.into_iter().flatten().any(|len| len != count) {
        bail!("Outlook answered lists of different lengths");
    }
    let time = |seconds: f64| {
        DateTime::<chrono::Utc>::from_timestamp(seconds.round() as i64, 0)
            .map(|at| at.to_rfc3339_opts(SecondsFormat::Secs, true))
            .ok_or_else(|| anyhow!("Outlook answered a time out of range"))
    };
    // The script checks that the lists line up by the events' ids and starts. An event whose
    // id is missing or repeated in the read is told apart only by its start and subject: events
    // sharing both could have traded their other properties, which are left out for them.
    let ids = (0..count)
        .map(|index| match columns.id.as_ref().map(|ids| &ids[index]) {
            Some(Value::String(id)) => Some(id.clone()),
            Some(Value::Number(id)) => Some(id.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut id_counts = std::collections::HashMap::<&str, usize>::new();
    for id in ids.iter().flatten() {
        *id_counts.entry(id.as_str()).or_default() += 1;
    }
    let identified = |index: usize| ids[index].as_deref().is_some_and(|id| id_counts[id] == 1);
    let key = |index: usize| {
        (
            columns.start[index].to_bits(),
            columns.subject[index].as_deref(),
        )
    };
    let mut keys = std::collections::HashMap::<_, usize>::new();
    for index in (0..count).filter(|&index| !identified(index)) {
        *keys.entry(key(index)).or_default() += 1;
    }
    let unaligned = (0..count)
        .map(|index| !identified(index) && keys[&key(index)] > 1)
        .collect::<Vec<_>>();
    let ambiguous = unaligned.iter().filter(|unaligned| **unaligned).count();
    if ambiguous > 0 {
        crate::diagnostics::warn(
            "outlook",
            format!(
                "{ambiguous} events without an id of their own share their start and subject with another: they come without location, organizer, busy status and description"
            ),
        );
    }
    let item = |column: &Option<Vec<Option<String>>>, index: usize| {
        column
            .as_ref()
            .filter(|_| !unaligned[index])
            .and_then(|values| values[index].clone())
    };
    let events = (0..count)
        .map(|index| {
            Ok(RawOutlookEvent {
                id: ids[index].clone(),
                subject: columns.subject[index].clone(),
                start: time(columns.start[index])?,
                end: time(columns.end[index])?,
                location: item(&columns.location, index),
                organizer: item(&columns.organizer, index),
                is_organizer: columns
                    .is_organizer
                    .as_ref()
                    .filter(|_| !unaligned[index])
                    .and_then(|flags| flags[index]),
                // Only the default calendar is read.
                is_default_calendar: Some(true),
                busy_status: item(&columns.busy_status, index)
                    .as_deref()
                    .and_then(busy_status_from_mac),
                body: item(&columns.body, index),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((events, columns.unread))
}

/// Outlook for Mac's `free busy status`, as the script writes it.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn busy_status_from_mac(name: &str) -> Option<BusyStatus> {
    match name.trim().to_ascii_lowercase().as_str() {
        "free" => Some(BusyStatus::Free),
        "tentative" => Some(BusyStatus::Tentative),
        "busy" => Some(BusyStatus::Busy),
        "out of office" => Some(BusyStatus::OutOfOffice),
        "working elsewhere" => Some(BusyStatus::WorkingElsewhere),
        _ => None,
    }
}

/// Whether the user organizes an event, from `AppointmentItem.MeetingStatus`: a plain
/// appointment (`olNonMeeting`, 0) or a meeting they set up (`olMeeting`, 1). A read that failed
/// (Outlook's Object Model Guard can refuse it) leaves it unknown.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn is_organizer_from_com(meeting_status: Option<i32>) -> Option<bool> {
    meeting_status.map(|status| matches!(status, 0 | 1))
}

/// `AppointmentItem.BusyStatus` (`OlBusyStatus`).
#[cfg(any(target_os = "windows", test))]
pub(crate) fn busy_status_from_com(value: i32) -> Option<BusyStatus> {
    match value {
        0 => Some(BusyStatus::Free),
        1 => Some(BusyStatus::Tentative),
        2 => Some(BusyStatus::Busy),
        3 => Some(BusyStatus::OutOfOffice),
        4 => Some(BusyStatus::WorkingElsewhere),
        _ => None,
    }
}
