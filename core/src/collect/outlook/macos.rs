use anyhow::Result;

use crate::{
    model::{CollectOptions, OutlookEntry},
    platform::macos::run_apple_script,
};

use super::{normalize_outlook_events, parse_raw_outlook_events};

pub(super) async fn collect_outlook(options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
    let script = build_collect_script(&options.start.to_rfc3339(), &options.end.to_rfc3339());
    let output = run_apple_script(&script)?;
    let events = parse_raw_outlook_events(&output)?;
    Ok(normalize_outlook_events(events, options))
}

fn build_collect_script(start: &str, end: &str) -> String {
    format!(
        r#"
use framework "Foundation"
use scripting additions

on iso8601Formatter()
  return current application's NSISO8601DateFormatter's new()
end iso8601Formatter

on parseIsoDate(isoText)
  return (my iso8601Formatter()'s dateFromString:isoText)
end parseIsoDate

on isoTextForDate(dateValue)
  return ((my iso8601Formatter()'s stringFromDate:dateValue) as text)
end isoTextForDate

on jsonTextValue(value)
  if value is missing value then
    return current application's NSNull's null()
  end if

  return value as text
end jsonTextValue

set startDate to my parseIsoDate("{start}")
set endDate to my parseIsoDate("{end}")

-- Never launch Outlook just to read the calendar; collect only from a running instance.
if application id "com.microsoft.Outlook" is not running then error "Outlook is not running. Open Outlook to include its calendar."

tell application "Microsoft Outlook"
  if (count of exchange accounts) is 0 then error "Outlook not signed in"
  if not (exists default calendar) then error "Outlook default calendar unavailable"

  set defaultCalendar to default calendar
  set matchingEvents to every calendar event of defaultCalendar whose end time is greater than or equal to startDate and start time is less than or equal to endDate
  set payload to current application's NSMutableArray's array()

  repeat with eventRef in matchingEvents
    set organizerName to missing value
    set organizerFlag to false

    try
      set organizerName to organizer of eventRef
    end try

    try
      set organizerFlag to is organizer of eventRef
    end try

    set eventRecord to current application's NSMutableDictionary's dictionary()
    (eventRecord's setObject:(my jsonTextValue(subject of eventRef)) forKey:"subject")
    (eventRecord's setObject:(my isoTextForDate(start time of eventRef)) forKey:"start")
    (eventRecord's setObject:(my isoTextForDate(end time of eventRef)) forKey:"end")
    (eventRecord's setObject:(my jsonTextValue(location of eventRef)) forKey:"location")
    (eventRecord's setObject:(my jsonTextValue(organizerName)) forKey:"organizer")
    (eventRecord's setObject:organizerFlag forKey:"is_organizer")
    (payload's addObject:eventRecord)
  end repeat
end tell

set jsonError to reference
set jsonData to current application's NSJSONSerialization's dataWithJSONObject:payload options:0 |error|:jsonError
if jsonData is missing value then error (((jsonError's contents)'s localizedDescription()) as text)
return ((current application's NSString's alloc()'s initWithData:jsonData encoding:(current application's NSUTF8StringEncoding)) as text)
"#
    )
}
