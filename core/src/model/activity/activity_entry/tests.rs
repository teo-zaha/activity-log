//! The entry shapes on the wire, byte for byte: what `GET /api/activity`, the events, the
//! export and the cache send. Each fixture is read as its kind and written back unchanged.

use super::ActivityEntry;
use crate::model::Source;

const GIT: &str = r#"{"source":"git","connection":"work","id":"1f3a","timestamp":"2026-11-17T09:30:00+01:00","repo":"api","hash":"1f3a","message":"Fix the parser","author":"dev@example.org","branch":"main"}"#;
const GIT_WITH_PUSH: &str = r#"{"source":"git","connection":"work","id":"1f3a","timestamp":"2026-11-17T09:30:00+01:00","repo":"api","hash":"1f3a","message":"Fix the parser","author":"dev@example.org","branch":"main","gitlabEvent":{"source":"gitlab","connection":"oss","id":"77","type":"pushedTo","timestamp":"2026-11-17T08:31:00.000Z","title":"","targetType":"unknown","branch":"main","commitCount":1,"pushedCommit":"1f3a"}}"#;
const GITLAB: &str = r#"{"source":"gitlab","connection":"oss","id":"78","type":"commentedOn","timestamp":"2026-11-17T08:40:00.000Z","title":"Parser fix","targetType":"DiffNote","projectId":42,"mrIid":12,"noteBody":"Looks good","noteFile":"src/parser.rs","noteResolved":false}"#;
/// A merge request in a stack: one below, the base reached; above, a level cut for each reason
/// and one complete.
const GITLAB_STACKED: &str = r#"{"source":"gitlab","connection":"oss","id":"79","type":"opened","timestamp":"2026-11-17T08:45:00.000Z","title":"Parse B","targetType":"MergeRequest","projectId":42,"mrIid":12,"changeStack":{"current":{"number":12,"title":"Parse B","webUrl":"https://gitlab.example.org/acme/widgets/-/merge_requests/12"},"below":[{"number":11,"title":"Parse A","webUrl":"https://gitlab.example.org/acme/widgets/-/merge_requests/11"}],"bottom":{"kind":"reached","branch":"main"},"above":{"changes":[{"change":{"number":13,"title":"Parse C"},"above":{"changes":[],"end":{"kind":"cut","reason":"limit"}}},{"change":{"number":14,"title":"Parse D"},"above":{"changes":[{"change":{"number":15,"title":"Parse E"},"above":{"changes":[],"end":{"kind":"cut","reason":"unreadable"}}}],"end":{"kind":"cut","reason":"throttled"}}},{"change":{"number":16,"title":"Parse F"},"above":{"changes":[],"end":{"kind":"cut","reason":"failed"}}}],"end":{"kind":"cut","reason":"cycle"}}}}"#;
/// The other ways down: several changes below, and a cut.
const GITLAB_AMBIGUOUS: &str = r#"{"source":"gitlab","connection":"oss","id":"80","type":"approved","timestamp":"2026-11-17T08:50:00.000Z","title":"Parse B","targetType":"MergeRequest","projectId":42,"mrIid":12,"changeStack":{"current":{"number":12,"title":"Parse B"},"below":[],"bottom":{"kind":"ambiguous"},"above":{"changes":[],"end":{"kind":"complete"}}}}"#;
const GITLAB_CUT_BELOW: &str = r#"{"source":"gitlab","connection":"oss","id":"81","type":"approved","timestamp":"2026-11-17T08:55:00.000Z","title":"Parse B","targetType":"MergeRequest","projectId":42,"mrIid":12,"changeStack":{"current":{"number":12,"title":"Parse B"},"below":[],"bottom":{"kind":"cut","reason":"failed"},"above":{"changes":[],"end":{"kind":"complete"}}}}"#;
const JIRA: &str = r#"{"source":"jira","connection":"work","id":"OPS-1:status:9","type":"statusChanged","timestamp":"2026-11-17T10:00:00+00:00","key":"OPS-1","title":"Rotate keys","status":"Done","issueType":"Task","fromStatus":"In Progress","toStatus":"Done"}"#;
const OUTLOOK: &str = r#"{"source":"outlook","connection":"outlook","id":"AAMk1","timestamp":"2026-11-17T13:00:00+00:00","title":"Planning","location":"Room 4","organizer":"lead@example.org","isOrganizer":false,"durationMinutes":45,"busyStatus":"busy","meetingWeight":"busy","description":"Agenda","descriptionBlocks":[{"kind":"paragraph","segments":[{"kind":"text","text":"Agenda"}]}],"joinUrl":"https://meet.example.org/j/1"}"#;
const OUTLOOK_BARE: &str = r#"{"source":"outlook","connection":"outlook","id":"AAMk2","timestamp":"2026-11-17T15:00:00+00:00","title":"Focus","durationMinutes":30,"meetingWeight":"none"}"#;
const SYSTEM: &str = r#"{"source":"system","connection":"system","id":"boot-1","type":"boot","timestamp":"2026-11-17T07:58:00+01:00","label":"Boot"}"#;
const CALENDAR: &str = r#"{"source":"calendar","connection":"team","id":"uid-1/2026-11-17T14:00:00+00:00","timestamp":"2026-11-17T14:00:00+00:00","title":"Review","location":"Online","organizer":"lead@example.org","durationMinutes":60,"busyStatus":"tentative","meetingWeight":"tentative","joinUrl":"https://meet.example.org/j/2"}"#;

fn round_trip(fixture: &str, source: Source) {
    let entry = serde_json::from_str::<ActivityEntry>(fixture).unwrap();
    assert_eq!(entry.source(), source, "{fixture}");
    assert_eq!(serde_json::to_string(&entry).unwrap(), fixture);
}

#[test]
fn every_kind_keeps_its_wire_shape() {
    round_trip(GIT, Source::Git);
    round_trip(GIT_WITH_PUSH, Source::Git);
    round_trip(GITLAB, Source::Gitlab);
    round_trip(GITLAB_STACKED, Source::Gitlab);
    round_trip(GITLAB_AMBIGUOUS, Source::Gitlab);
    round_trip(GITLAB_CUT_BELOW, Source::Gitlab);
    round_trip(JIRA, Source::Jira);
    round_trip(OUTLOOK, Source::Outlook);
    round_trip(OUTLOOK_BARE, Source::Outlook);
    round_trip(SYSTEM, Source::System);
    round_trip(CALENDAR, Source::Calendar);
}

/// B38: an entry is read as the kind its `source` names, or not at all.
#[test]
fn an_entry_whose_fields_fit_another_kind_is_refused() {
    let jira_shaped_as_git = GIT.replacen(r#""source":"git""#, r#""source":"jira""#, 1);
    assert!(serde_json::from_str::<ActivityEntry>(&jira_shaped_as_git).is_err());
    let calendar_shaped_as_outlook =
        OUTLOOK_BARE.replacen(r#""source":"outlook""#, r#""source":"calendar""#, 1);
    let read = serde_json::from_str::<ActivityEntry>(&calendar_shaped_as_outlook);
    assert!(read.is_ok_and(|entry| entry.source() == Source::Calendar));
}

#[test]
fn an_unknown_or_missing_source_is_refused() {
    let unknown = SYSTEM.replacen(r#""source":"system""#, r#""source":"teams""#, 1);
    assert!(serde_json::from_str::<ActivityEntry>(&unknown).is_err());
    let missing = SYSTEM.replacen(r#""source":"system","#, "", 1);
    assert!(serde_json::from_str::<ActivityEntry>(&missing).is_err());
}

/// An entry always names its connection: the runtime stamps it before an entry is cached or
/// sent, so one without it was never written by this version.
#[test]
fn an_entry_without_its_connection_is_refused() {
    let unstamped = SYSTEM.replacen(r#""connection":"system","#, "", 1);
    assert!(serde_json::from_str::<ActivityEntry>(&unstamped).is_err());
    let empty = SYSTEM.replacen(r#""connection":"system""#, r#""connection":"""#, 1);
    assert!(serde_json::from_str::<ActivityEntry>(&empty).is_err());
}

#[test]
fn a_nested_push_must_be_a_gitlab_entry() {
    let wrong = GIT_WITH_PUSH.replacen(r#""source":"gitlab""#, r#""source":"jira""#, 1);
    assert!(serde_json::from_str::<ActivityEntry>(&wrong).is_err());
}

/// A stack that breaks its rules is refused with the entry it is on.
#[test]
fn an_entry_with_an_invalid_stack_is_refused() {
    let lonely = GITLAB_AMBIGUOUS.replacen(
        r#"{"kind":"ambiguous"}"#,
        r#"{"kind":"reached","branch":"main"}"#,
        1,
    );
    assert!(serde_json::from_str::<ActivityEntry>(&lonely).is_err());
    let with_user_info = GITLAB_STACKED.replacen("https://gitlab", "https://jdoe:secret@gitlab", 1);
    assert!(serde_json::from_str::<ActivityEntry>(&with_user_info).is_err());
}
