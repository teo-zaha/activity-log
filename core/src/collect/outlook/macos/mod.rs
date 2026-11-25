use anyhow::Result;
use chrono::{DateTime, Duration, SecondsFormat, Utc};

use crate::{
    diagnostics,
    model::{CollectOptions, OutlookEntry},
    platform::macos::{AppleScriptError, run_apple_script},
    provider::source_failure::SourceProblem,
};

use super::{normalize::normalize_outlook_events, parse_outlook_columns};

#[cfg(test)]
mod tests;

/// The Outlook calendar script could not reach a running instance.
#[derive(Debug)]
pub(crate) struct OutlookNotRunning(AppleScriptError);

impl OutlookNotRunning {
    pub(crate) fn classify(error: AppleScriptError) -> anyhow::Error {
        if error.problem() == Some(SourceProblem::NotRunning) {
            Self(error).into()
        } else {
            error.into()
        }
    }
}

impl std::fmt::Display for OutlookNotRunning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Open Outlook to include its calendar")
    }
}

impl std::error::Error for OutlookNotRunning {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

/// The calendar's events, read through Outlook's AppleScript dictionary in one script: each
/// property for all events at once (one request to Outlook per property, not per event and
/// property), busy status and body included as on Windows. Should this Outlook's dictionary
/// lack those two terms, the script does not compile, and the events are read without them.
pub(super) async fn collect_outlook(options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
    let (start, end) = script_bounds(options);
    let output = match run_apple_script(&collect_script(Fields::All), &[&start, &end]).await {
        Err(err) if err.is_syntax() => {
            diagnostics::warn(
                "outlook",
                format!(
                    "Outlook's scripting dictionary has no busy status or body ({err}); reading meetings without them"
                ),
            );
            run_apple_script(&collect_script(Fields::Basic), &[&start, &end]).await
        }
        result => result,
    }.map_err(OutlookNotRunning::classify)?;
    let (events, unread) = parse_outlook_columns(&output)?;
    if !unread.is_empty() {
        diagnostics::warn(
            "outlook",
            format!(
                "Outlook did not give {} for this range; meetings come without it",
                unread.join(", ")
            ),
        );
    }
    Ok(normalize_outlook_events(events, options))
}

/// What the script reads besides the times, subject, place and organizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fields {
    /// Busy status and body too.
    All,
    Basic,
}

/// The range as whole seconds, widened to cover it: Foundation's ISO 8601 reader takes no
/// fractions (it gives no date at all), and `normalize_outlook_events` applies the exact range.
pub(crate) fn script_bounds(options: &CollectOptions) -> (String, String) {
    let floor =
        |at: DateTime<Utc>| at - Duration::nanoseconds(i64::from(at.timestamp_subsec_nanos()));
    let start = floor(options.start);
    let end = match floor(options.end) {
        whole if whole == options.end => whole,
        whole => whole + Duration::seconds(1),
    };
    let text = |at: DateTime<Utc>| at.to_rfc3339_opts(SecondsFormat::Secs, true);
    (text(start), text(end))
}

pub(crate) fn collect_script(fields: Fields) -> String {
    let extras = match fields {
        Fields::All => EXTRAS,
        Fields::Basic => "",
    };
    COLLECT_SCRIPT.replace("--EXTRAS--", extras) + ROWS_LINE_UP
}

/// Whether the lists read again at the end are those read first: the starts and subjects (case
/// counts, as in `parse_outlook_columns`), and the ids where Outlook gave them. Ids can be missing or repeated, so they alone do not
/// tell two events at the same start apart.
pub(crate) const ROWS_LINE_UP: &str = r#"
on rowsLineUp(ids, starts, subjects, idsAgain, startsAgain, subjectsAgain)
  -- AppleScript compares text ignoring case by default; the subjects must match as written.
  considering case
    if startsAgain is not equal to starts then return false
    if subjectsAgain is not equal to subjects then return false
    if ids is missing value then return true
    return idsAgain is equal to ids
  end considering
end rowsLineUp
"#;

/// Reads busy status and body; a failed read leaves them out for every event, and says so.
const EXTRAS: &str = r#"
      try
        set eventStatuses to free busy status of matchingEvents
      on error
        set end of unreadFields to "busy_status"
      end try
      try
        set eventBodies to plain text content of matchingEvents
      on error
        set end of unreadFields to "body"
      end try"#;

/// Answers JSON with one list per property, in the same event order (see
/// `parse_outlook_columns`); a property Outlook would not give has no list.
const COLLECT_SCRIPT: &str = r#"
use framework "Foundation"
use scripting additions

on run argv
  set startDate to my parsedDate(item 1 of argv)
  set endDate to my parsedDate(item 2 of argv)

  -- Never launch Outlook just to read the calendar; collect only from a running instance.
  if application id "com.microsoft.Outlook" is not running then error "Outlook is not running." number -600

  tell application "Microsoft Outlook"
    if (count of exchange accounts) is 0 then error "Outlook not signed in"
    if not (exists default calendar) then error "Outlook default calendar unavailable"

    set defaultCalendar to default calendar
    set matchingEvents to a reference to (every calendar event of defaultCalendar whose end time is greater than or equal to startDate and start time is less than or equal to endDate)

    -- The lists line up only while the calendar stays the same: each event's start, subject and
    -- id are read again at the end, and everything once more if they changed.
    repeat with readAttempt from 1 to 2
      set unreadFields to {}
      set eventIds to missing value
      set eventOrganizers to missing value
      set organizerFlags to missing value
      set eventStatuses to missing value
      set eventBodies to missing value

      try
        set eventIds to id of matchingEvents
      on error
        set end of unreadFields to "id"
      end try
      set eventSubjects to subject of matchingEvents
      set eventStarts to start time of matchingEvents
      set eventEnds to end time of matchingEvents
      set eventLocations to location of matchingEvents
      try
        set eventOrganizers to organizer of matchingEvents
      on error
        set end of unreadFields to "organizer"
      end try
      try
        set organizerFlags to is organizer of matchingEvents
      on error
        set end of unreadFields to "is_organizer"
      end try
--EXTRAS--

      set listsLineUp to false
      try
        set idsAgain to missing value
        if eventIds is not missing value then set idsAgain to id of matchingEvents
        set listsLineUp to my rowsLineUp(eventIds, eventStarts, eventSubjects, idsAgain, start time of matchingEvents, subject of matchingEvents)
      end try
      if listsLineUp then exit repeat
      if readAttempt is 2 then error "Outlook's calendar changed while it was read; collect again."
    end repeat

    set statusNames to missing value
    if eventStatuses is not missing value then
      set statusNames to {}
      repeat with statusRef in eventStatuses
        try
          set end of statusNames to (contents of statusRef) as text
        on error
          set end of statusNames to missing value
        end try
      end repeat
    end if
  end tell

  set payload to current application's NSMutableDictionary's dictionary()
  my putList(payload, "id", eventIds)
  my putList(payload, "subject", eventSubjects)
  my putDates(payload, "start", eventStarts)
  my putDates(payload, "end", eventEnds)
  my putList(payload, "location", eventLocations)
  my putList(payload, "organizer", eventOrganizers)
  my putList(payload, "is_organizer", organizerFlags)
  my putList(payload, "busy_status", statusNames)
  my putList(payload, "body", eventBodies)
  my putList(payload, "unread", unreadFields)

  -- With `reference` for its error, the call answers the data and the error as a list.
  set {jsonData, jsonError} to current application's NSJSONSerialization's dataWithJSONObject:payload options:0 |error|:(reference)
  if jsonData is missing value then error ((jsonError's localizedDescription()) as text)
  return ((current application's NSString's alloc()'s initWithData:jsonData encoding:(current application's NSUTF8StringEncoding)) as text)
end run

on parsedDate(isoText)
  set parsed to (current application's NSISO8601DateFormatter's new()'s dateFromString:isoText)
  if parsed is missing value then error "Cannot read the range " & isoText
  return parsed as date
end parsedDate

-- Missing values in a list become JSON null.
on putList(payload, jsonKey, listValue)
  if listValue is not missing value then (payload's setObject:(current application's NSArray's arrayWithArray:listValue) forKey:jsonKey)
end putList

-- Dates as seconds since 1970, read for the whole list at once.
on putDates(payload, jsonKey, listValue)
  (payload's setObject:((current application's NSArray's arrayWithArray:listValue)'s valueForKey:"timeIntervalSince1970") forKey:jsonKey)
end putDates
"#;
