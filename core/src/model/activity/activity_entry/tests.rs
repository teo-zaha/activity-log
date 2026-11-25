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
const GITHUB_OPENED: &str = r#"{"source":"github","connection":"oss","id":"I_x","timestamp":"2026-11-17T10:00:00Z","repository":"acme/widgets","type":"opened","target":{"kind":"issue","number":7,"title":"Fix widget"}}"#;
const GITHUB_LINKED_ISSUE: &str = r#"{"source":"github","connection":"oss","id":"I_y","timestamp":"2026-11-17T10:00:00Z","repository":"acme/widgets","type":"opened","target":{"kind":"issue","number":7,"title":"Fix widget","webUrl":"https://github.com/acme/widgets/issues/7"}}"#;
const GITHUB_OPENED_PR: &str = r#"{"source":"github","connection":"oss","id":"PR_x","timestamp":"2026-11-17T10:00:00Z","repository":"acme/widgets","type":"opened","target":{"kind":"pullRequest","number":12,"title":"Add widget"}}"#;
const GITHUB_LINKED_PR: &str = r#"{"source":"github","connection":"oss","id":"PR_z","timestamp":"2026-11-17T10:00:00Z","repository":"acme/widgets","type":"opened","target":{"kind":"pullRequest","number":12,"title":"Add widget","webUrl":"https://github.com/acme/widgets/pull/12"}}"#;
const GITHUB_STACKED_PR: &str = r#"{"source":"github","connection":"oss","id":"PR_y","timestamp":"2026-11-17T10:00:00Z","repository":"acme/widgets","type":"opened","target":{"kind":"pullRequest","number":12,"title":"Add widget","changeStack":{"current":{"number":12,"title":"Add widget","webUrl":"https://github.com/acme/widgets/pull/12"},"below":[{"number":11,"title":"Parse widget"}],"bottom":{"kind":"reached","branch":"main"},"above":{"changes":[],"end":{"kind":"complete"}}}}}"#;
const GITHUB_REVIEWED: &str = r#"{"source":"github","connection":"oss","id":"PRR_x","timestamp":"2026-11-17T10:05:00.000Z","repository":"acme/widgets","type":"reviewed","pullRequest":{"number":12,"title":"Add widget"},"state":"changesRequested"}"#;
const GITHUB_COMMITTED: &str = r#"{"source":"github","connection":"oss","id":"C_example","timestamp":"2026-11-17T10:10:00Z","repository":"acme/widgets","type":"committed","sha":"0123456789abcdef0123456789abcdef01234567","message":"Add widget","branch":"main","webUrl":"https://github.com/acme/widgets/commit/0123456789abcdef0123456789abcdef01234567"}"#;
const JIRA: &str = r#"{"source":"jira","connection":"work","id":"OPS-1:status:9","type":"statusChanged","timestamp":"2026-11-17T10:00:00+00:00","key":"OPS-1","title":"Rotate keys","status":"Done","issueType":"Task","fromStatus":"In Progress","toStatus":"Done"}"#;
const OUTLOOK: &str = r#"{"source":"outlook","connection":"outlook","id":"AAMk1","timestamp":"2026-11-17T13:00:00+00:00","title":"Planning","location":"Room 4","organizer":"lead@example.org","isOrganizer":false,"durationMinutes":45,"busyStatus":"busy","meetingWeight":"busy","description":"Agenda","descriptionBlocks":[{"kind":"paragraph","segments":[{"kind":"text","text":"Agenda"}]}],"joinUrl":"https://meet.example.org/j/1"}"#;
const OUTLOOK_BARE: &str = r#"{"source":"outlook","connection":"outlook","id":"AAMk2","timestamp":"2026-11-17T15:00:00+00:00","title":"Focus","durationMinutes":30,"meetingWeight":"none"}"#;
const SYSTEM: &str = r#"{"source":"system","connection":"system","id":"boot-1","type":"boot","timestamp":"2026-11-17T07:58:00+01:00","label":"Boot"}"#;
const CALENDAR: &str = r#"{"source":"calendar","connection":"team","id":"uid-1/2026-11-17T14:00:00+00:00","timestamp":"2026-11-17T14:00:00+00:00","title":"Review","location":"Online","organizer":"lead@example.org","durationMinutes":60,"busyStatus":"tentative","meetingWeight":"tentative","joinUrl":"https://meet.example.org/j/2"}"#;

const FIGMA_VERSION: &str = r#"{"source":"figma","connection":"design","id":"F1:version:7","timestamp":"2026-11-17T09:30:00.000Z","fileKey":"F1","fileName":"Checkout","editor":"figma","fileUrl":"https://www.figma.com/file/F1","type":"versionSaved","versionId":"7","label":"Checkout v2","description":"Updated labels"}"#;
const FIGMA_EDITED: &str = r#"{"source":"figma","connection":"design","id":"F1:version:8","timestamp":"2026-11-17T09:31:00Z","fileKey":"F1","fileName":"Checkout","fileUrl":"https://www.figma.com/file/F1","type":"edited","versionId":"8"}"#;
const FIGMA_COMMENTED: &str = r#"{"source":"figma","connection":"design","id":"F1:comment:9","timestamp":"2026-11-17T09:32:00Z","fileKey":"F1","fileName":"Checkout","editor":"figma","fileUrl":"https://www.figma.com/file/F1?node-id=3%3A4","type":"commented","commentId":"9","message":"Looks good","nodeId":"3:4","resolved":false}"#;
const FIGMA_REPLIED: &str = r#"{"source":"figma","connection":"design","id":"F1:comment:10","timestamp":"2026-11-17T09:33:00Z","fileKey":"F1","fileName":"Checkout","editor":"figma","fileUrl":"https://www.figma.com/file/F1?node-id=3%3A4","type":"replied","commentId":"10","parentId":"9","nodeId":"3:4"}"#;

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
    round_trip(GITHUB_OPENED, Source::Github);
    round_trip(GITHUB_LINKED_ISSUE, Source::Github);
    round_trip(GITHUB_OPENED_PR, Source::Github);
    round_trip(GITHUB_LINKED_PR, Source::Github);
    round_trip(GITHUB_STACKED_PR, Source::Github);
    round_trip(GITHUB_REVIEWED, Source::Github);
    round_trip(GITHUB_COMMITTED, Source::Github);
    round_trip(
        &GITHUB_COMMITTED.replace(r#""message":"Add widget","#, ""),
        Source::Github,
    );
    round_trip(JIRA, Source::Jira);
    round_trip(OUTLOOK, Source::Outlook);
    round_trip(OUTLOOK_BARE, Source::Outlook);
    round_trip(SYSTEM, Source::System);
    round_trip(CALENDAR, Source::Calendar);
    for fixture in [FIGMA_VERSION, FIGMA_EDITED, FIGMA_COMMENTED, FIGMA_REPLIED] {
        round_trip(fixture, Source::Figma);
    }
}

#[test]
fn git_keeps_its_wire_shape_with_github_and_gitlab_attached() {
    for git in [GIT, GIT_WITH_PUSH] {
        let git = git.replace("1f3a", "0123456789abcdef0123456789abcdef01234567");
        let base = git.strip_suffix('}').unwrap();
        let folded = format!(r#"{base},"githubEvent":{GITHUB_COMMITTED}}}"#);
        round_trip(&folded, Source::Git);
    }
}

#[test]
fn a_folded_github_event_must_be_a_github_commit() {
    let base = GIT.strip_suffix('}').unwrap();
    for event in [GITHUB_OPENED, GITLAB] {
        let folded = format!(r#"{base},"githubEvent":{event}}}"#);
        assert!(serde_json::from_str::<ActivityEntry>(&folded).is_err());
    }
    let mut missing_sha: serde_json::Value = serde_json::from_str(GITHUB_COMMITTED).unwrap();
    missing_sha.as_object_mut().unwrap().remove("sha");
    let folded = format!(r#"{base},"githubEvent":{missing_sha}}}"#);
    assert!(serde_json::from_str::<ActivityEntry>(&folded).is_err());
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
fn a_github_review_requires_a_pull_request_and_a_submitted_state() {
    let wrong_item = GITHUB_REVIEWED.replacen(r#""pullRequest":"#, r#""target":"#, 1);
    assert!(serde_json::from_str::<ActivityEntry>(&wrong_item).is_err());
    let pending = GITHUB_REVIEWED.replacen("changesRequested", "pending", 1);
    assert!(serde_json::from_str::<ActivityEntry>(&pending).is_err());
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

#[test]
fn figma_requires_its_own_entry_fields_connection_and_activity() {
    for malformed in [
        GIT.replacen(r#""source":"git""#, r#""source":"figma""#, 1),
        FIGMA_EDITED.replacen(r#""connection":"design""#, r#""connection":"""#, 1),
        FIGMA_EDITED.replacen(r#""type":"edited""#, r#""type":"viewed""#, 1),
        FIGMA_VERSION.replacen(r#""label":"Checkout v2","#, "", 1),
    ] {
        assert!(
            serde_json::from_str::<ActivityEntry>(&malformed)
                .unwrap_err()
                .is_data()
        );
    }
}
