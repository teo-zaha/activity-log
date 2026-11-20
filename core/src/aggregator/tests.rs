use chrono::{TimeZone, Utc};

use super::{
    carried_for, collect_activity_from_results, make_ids_unique, merge_git_with_gitlab, within,
};
use crate::model::{
    ActivityEntry, CalendarEntry, CollectOptions, ConnectionId, GitEntry, GitLabEntry, GitLabType,
    MeetingWeight, Source,
};

fn push(commit_to: &str) -> ActivityEntry {
    push_at(commit_to, "2026-03-10T10:05:00Z")
}

fn push_at(commit_to: &str, timestamp: &str) -> ActivityEntry {
    ActivityEntry::GitLab(GitLabEntry {
        source: crate::model::GitLabTag,
        connection: "work".into(),
        id: commit_to.to_string(),
        entry_type: GitLabType::PushedTo,
        timestamp: timestamp.into(),
        title: Some("repo".to_string()),
        target_type: Some("Project".to_string()),
        mr_iid: None,
        issue_iid: None,
        branch: Some("main".to_string()),
        commit_count: Some(1),
        note_body: None,
        note_file: None,
        note_resolved: None,
        pushed_commit: Some(commit_to.to_string()),
    })
}

/// GitLab is in before Git: its push shows on its own until the commit it pushed arrives.
#[test]
fn a_push_shows_until_its_git_commit_is_collected() {
    let options = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    };
    let git = ConnectionId::new(Source::Git, "laptop");
    let gitlab = ConnectionId::new(Source::Gitlab, "work");

    let partial = collect_activity_from_results(
        options,
        vec![(gitlab.clone(), vec![push("abc123")])],
        vec![git.clone()],
        Vec::new(),
    );
    assert!(matches!(
        partial.log.entries.as_slice(),
        [ActivityEntry::GitLab(_)]
    ));
    assert_eq!(partial.summary.entry_count_for(&gitlab), 1);

    let mut commit = GitEntry::new(
        "repo".to_string(),
        "abc123".to_string(),
        "message".to_string(),
        "me@example.com".to_string(),
        Some("main".to_string()),
        "2026-03-10T10:00:00Z".into(),
    );
    commit.connection = "laptop".into();
    let complete = collect_activity_from_results(
        options,
        vec![
            (git, vec![ActivityEntry::Git(commit)]),
            (gitlab, vec![push("abc123"), push("def456")]),
        ],
        Vec::new(),
        Vec::new(),
    );
    assert!(matches!(
        complete.log.entries.as_slice(),
        [
            ActivityEntry::GitLab(_),
            ActivityEntry::Git(GitEntry {
                gitlab_event: Some(_),
                ..
            })
        ]
    ));
    assert_eq!(complete.summary.merged_count, 1);
}

#[test]
fn sorts_entries_newest_first_across_offsets() {
    let calendar = ConnectionId::new(Source::Calendar, "team");
    let log = collect_activity_from_results(
        march(),
        vec![(
            calendar,
            vec![
                meeting("2026-03-10T09:30:00+01:00", 30),
                meeting("2026-03-10T08:45:00Z", 30),
                meeting("2026-03-10T08:00:00Z", 30),
            ],
        )],
        Vec::new(),
        Vec::new(),
    )
    .log;

    let order = log
        .entries
        .iter()
        .map(|entry| entry.timestamp().as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            "2026-03-10T08:45:00Z",
            "2026-03-10T09:30:00+01:00",
            "2026-03-10T08:00:00Z"
        ]
    );
}

fn march() -> CollectOptions {
    CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

fn commit_in(connection: &str, hash: &str, timestamp: &str) -> ActivityEntry {
    let mut commit = GitEntry::new(
        "repo".to_string(),
        hash.to_string(),
        format!("commit {hash}"),
        "me@example.com".to_string(),
        Some("main".to_string()),
        timestamp.into(),
    );
    commit.connection = connection.into();
    ActivityEntry::Git(commit)
}

fn meeting(timestamp: &str, duration_minutes: i32) -> ActivityEntry {
    ActivityEntry::Calendar(CalendarEntry {
        source: crate::model::CalendarTag,
        connection: "team".into(),
        id: timestamp.to_string(),
        timestamp: timestamp.into(),
        title: Some("Offsite".to_string()),
        location: None,
        organizer: None,
        meeting: crate::model::MeetingBody {
            duration_minutes,
            busy_status: None,
            meeting_weight: MeetingWeight::Busy,
            description: None,
            description_blocks: Vec::new(),
            join_url: None,
        },
    })
}

fn id(source: Source, name: &str) -> ConnectionId {
    ConnectionId::new(source, name)
}

fn hashes(entries: &[ActivityEntry]) -> Vec<(String, bool)> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            ActivityEntry::Git(commit) => {
                Some((commit.hash.clone(), commit.gitlab_event.is_some()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_carried_commit_a_finished_connection_has_is_shown_once() {
    let laptop = id(Source::Git, "laptop");
    let wsl = id(Source::Git, "wsl");
    let carried = vec![
        (
            wsl.clone(),
            vec![
                commit_in("wsl", "abc123", "2026-03-10T10:00:00Z"),
                commit_in("wsl", "def456", "2026-03-11T10:00:00Z"),
            ],
        ),
        (
            id(Source::Git, "server"),
            vec![commit_in("server", "def456", "2026-03-11T10:00:00Z")],
        ),
    ];
    let finished = vec![(
        laptop.clone(),
        vec![commit_in("laptop", "abc123", "2026-03-10T10:00:00Z")],
    )];
    let pending = vec![wsl.clone(), id(Source::Git, "server")];

    let shown = carried_for(&carried, &pending, &finished);
    let log = collect_activity_from_results(march(), finished, pending, shown).log;

    // abc123 is the fresh one; def456, carried by two connections, shows once.
    assert_eq!(
        hashes(&log.entries),
        [("def456".to_string(), false), ("abc123".to_string(), false)]
    );
    assert_eq!(
        log.summary.connections.len(),
        1,
        "carried entries are not counted"
    );
    assert_eq!(log.summary.entry_count_for(&laptop), 1);
}

#[test]
fn a_carried_push_folds_into_a_fresh_commit() {
    let laptop = id(Source::Git, "laptop");
    let work = id(Source::Gitlab, "work");
    let finished = vec![(
        laptop.clone(),
        vec![commit_in("laptop", "abc123", "2026-03-10T10:00:00Z")],
    )];
    let carried = carried_for(
        &[(work.clone(), vec![push("abc123")])],
        std::slice::from_ref(&work),
        &finished,
    );

    let collected = collect_activity_from_results(march(), finished, vec![work], carried);

    assert_eq!(
        hashes(&collected.log.entries),
        [("abc123".to_string(), true)]
    );
    assert_eq!(collected.log.entries.len(), 1);
    assert_eq!(collected.summary.merged_count, 1);
}

#[test]
fn only_pending_connections_carry() {
    let work = id(Source::Gitlab, "work");
    let carried = [(work.clone(), vec![push("abc123")])];

    assert!(carried_for(&carried, &[], &[]).is_empty());
    assert_eq!(carried_for(&carried, &[work], &[]).len(), 1);
}

#[test]
fn a_meeting_that_began_before_the_range_is_within_it() {
    assert!(within(&meeting("2026-02-28T23:00:00Z", 120), march()));
}

#[test]
fn another_range_keeps_what_falls_inside_and_meetings_that_overlap_it() {
    let later = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap(),
        ..march()
    };
    let entries = [
        // From the evening before into the new range.
        meeting("2026-03-09T23:00:00Z", 120),
        // Ended before it.
        meeting("2026-03-09T20:00:00Z", 60),
        commit_in("laptop", "in", "2026-03-10T10:00:00Z"),
        commit_in("laptop", "out", "2026-03-09T10:00:00Z"),
    ];

    let timestamps = entries
        .iter()
        .filter(|entry| within(entry, later))
        .map(|entry| entry.timestamp().to_string())
        .collect::<Vec<_>>();
    assert_eq!(timestamps, ["2026-03-09T23:00:00Z", "2026-03-10T10:00:00Z"]);
}

#[test]
fn numbers_repeated_ids_in_order_without_taking_another_entrys_id() {
    let mut entries = vec![
        push_at("a", "2026-03-10T10:00:00Z"),
        push_at("a", "2026-03-10T11:00:00Z"),
        push_at("a#2", "2026-03-10T12:00:00Z"),
        push_at("a", "2026-03-10T13:00:00Z"),
        push_at("b", "2026-03-10T14:00:00Z"),
    ];

    assert_eq!(make_ids_unique(&mut entries), 2);
    assert_eq!(
        entries.iter().map(ActivityEntry::id).collect::<Vec<_>>(),
        ["a", "a#3", "a#2", "a#4", "b"]
    );
    assert_eq!(
        make_ids_unique(&mut entries),
        0,
        "unique ids stay as they are"
    );
}

fn git_commit(hash: &str) -> GitEntry {
    GitEntry::new(
        "repo".to_string(),
        hash.to_string(),
        "message".to_string(),
        "me@example.com".to_string(),
        Some("main".to_string()),
        "2026-03-10T10:00:00Z".into(),
    )
}

fn gitlab_push(commit_to: &str) -> GitLabEntry {
    GitLabEntry {
        source: crate::model::GitLabTag,
        connection: crate::model::ConnectionName::unstamped(),
        id: commit_to.to_string(),
        entry_type: crate::model::GitLabType::PushedTo,
        timestamp: "2026-03-10T10:05:00Z".into(),
        title: Some("repo".to_string()),
        target_type: Some("Project".to_string()),
        mr_iid: None,
        issue_iid: None,
        branch: Some("main".to_string()),
        commit_count: Some(1),
        note_body: None,
        note_file: None,
        note_resolved: None,
        pushed_commit: Some(commit_to.to_string()),
    }
}

#[test]
fn merges_git_and_gitlab_push_events() {
    let crate::model::MergeResult {
        merged_git,
        remaining_gitlab,
    } = merge_git_with_gitlab(&[git_commit("abc123")], &[gitlab_push("abc123")]);

    assert!(merged_git[0].gitlab_event.is_some());
    assert!(remaining_gitlab.is_empty());
}

#[test]
fn a_push_without_its_git_commit_stays_an_entry() {
    let crate::model::MergeResult {
        merged_git,
        remaining_gitlab,
    } = merge_git_with_gitlab(
        &[git_commit("abc123")],
        &[gitlab_push("abc123"), gitlab_push("def456")],
    );

    assert!(merged_git[0].gitlab_event.is_some());
    let [unmatched] = remaining_gitlab.as_slice() else {
        panic!("expected the unmatched push, got {remaining_gitlab:?}");
    };
    assert_eq!(unmatched.pushed_commit.as_deref(), Some("def456"));

    let alone = merge_git_with_gitlab(&[], &[gitlab_push("abc123")]);
    assert_eq!(alone.remaining_gitlab.len(), 1, "nothing to fold into yet");
}

#[test]
fn two_pushes_of_one_commit_fold_one_and_keep_the_other() {
    let mut other_branch = gitlab_push("abc123");
    other_branch.id = "abc123-release".to_string();
    other_branch.branch = Some("release".to_string());

    let crate::model::MergeResult {
        merged_git,
        remaining_gitlab,
    } = merge_git_with_gitlab(
        &[git_commit("abc123")],
        &[gitlab_push("abc123"), other_branch],
    );

    let folded = merged_git[0].gitlab_event.as_ref().expect("one push folds");
    assert_eq!(folded.branch.as_deref(), Some("main"));
    let [kept] = remaining_gitlab.as_slice() else {
        panic!("expected the other push to stay, got {remaining_gitlab:?}");
    };
    assert_eq!(kept.branch.as_deref(), Some("release"));
}

/// A push that created its branch folds into its commit like any other push.
#[test]
fn a_push_of_a_new_branch_folds_into_its_commit() {
    let mut push = gitlab_push("abc123");
    push.entry_type = crate::model::GitLabType::PushedNew;

    let merged = merge_git_with_gitlab(&[git_commit("abc123")], &[push]);

    assert!(merged.merged_git[0].gitlab_event.is_some());
    assert!(merged.remaining_gitlab.is_empty());
}

/// Pushes of one commit at the same instant are told apart by connection, then id, whatever
/// offset each instant is written in.
#[test]
fn pushes_at_one_instant_fold_by_connection_not_by_how_the_instant_is_written() {
    let mut a = gitlab_push("abc123");
    a.connection = "a".into();
    a.timestamp = "2026-03-10T11:05:00+01:00".into();
    let mut z = gitlab_push("abc123");
    z.connection = "z".into();
    z.timestamp = "2026-03-10T10:05:00Z".into();

    let merged = merge_git_with_gitlab(&[git_commit("abc123")], &[z, a]);

    let folded = merged.merged_git[0].gitlab_event.as_ref().unwrap();
    assert_eq!(folded.connection, "a");
    assert_eq!(merged.remaining_gitlab[0].connection, "z");
}

#[test]
fn a_push_folds_into_one_commit_even_when_two_clones_have_it() {
    let mut wsl_clone = git_commit("abc123");
    wsl_clone.connection = "wsl".into();

    let merged =
        merge_git_with_gitlab(&[git_commit("abc123"), wsl_clone], &[gitlab_push("abc123")]);

    let folded = merged
        .merged_git
        .iter()
        .filter(|commit| commit.gitlab_event.is_some())
        .count();
    assert_eq!(folded, 1, "the push shows once");
    assert!(merged.remaining_gitlab.is_empty());
}
