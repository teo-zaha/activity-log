use chrono::{TimeZone, Utc};

use super::{
    carried_for, carry_over, collect_activity_from_results, compare_timestamps_desc,
    entries_by_connection, make_ids_unique, merge_git_with_gitlab,
};
use crate::model::{
    ActivityEntry, ActivityLog, CalendarEntry, CollectOptions, ConnectionId, GitEntry, GitLabEntry,
    GitLabType, MeetingWeight, Source,
};

#[test]
fn sorts_mixed_offsets_by_instant_desc() {
    let earlier = "2026-03-10T09:30:00+01:00";
    let later = "2026-03-10T08:45:00Z";

    assert_eq!(
        compare_timestamps_desc(later, earlier),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        compare_timestamps_desc(earlier, later),
        std::cmp::Ordering::Greater
    );
}

fn push(commit_to: &str) -> ActivityEntry {
    push_at(commit_to, "2026-03-10T10:05:00Z")
}

fn push_at(commit_to: &str, timestamp: &str) -> ActivityEntry {
    ActivityEntry::GitLab(GitLabEntry {
        source: "gitlab".to_string(),
        connection: "work".to_string(),
        id: commit_to.to_string(),
        entry_type: GitLabType::PushedTo,
        timestamp: timestamp.to_string(),
        title: "repo".to_string(),
        target_type: "Project".to_string(),
        mr_iid: None,
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
        "main".to_string(),
        "2026-03-10T10:00:00Z".to_string(),
    );
    commit.connection = "laptop".to_string();
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
        "main".to_string(),
        timestamp.to_string(),
    );
    commit.connection = connection.to_string();
    ActivityEntry::Git(commit)
}

fn meeting(timestamp: &str, duration_minutes: i32) -> ActivityEntry {
    ActivityEntry::Calendar(CalendarEntry {
        source: "calendar".to_string(),
        connection: "team".to_string(),
        id: timestamp.to_string(),
        timestamp: timestamp.to_string(),
        title: "Offsite".to_string(),
        location: None,
        organizer: None,
        duration_minutes,
        busy_status: None,
        meeting_weight: MeetingWeight::Busy,
        description: None,
        description_blocks: Vec::new(),
        join_url: None,
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

fn a_log(entries: Vec<(ConnectionId, Vec<ActivityEntry>)>) -> ActivityLog {
    collect_activity_from_results(march(), entries, Vec::new(), Vec::new()).log
}

#[test]
fn a_log_taken_apart_by_connection_builds_the_same_log_again() {
    let log = a_log(vec![
        (
            id(Source::Git, "laptop"),
            vec![
                commit_in("laptop", "abc123", "2026-03-10T10:00:00Z"),
                commit_in("laptop", "def456", "2026-03-11T10:00:00Z"),
            ],
        ),
        (
            id(Source::Gitlab, "work"),
            vec![push("abc123"), push_at("fff000", "2026-03-12T10:00:00Z")],
        ),
        (
            id(Source::Calendar, "team"),
            vec![meeting("2026-03-09T08:00:00Z", 60)],
        ),
    ]);
    assert_eq!(log.summary.merged_count, 1);

    let rebuilt = a_log(entries_by_connection(&log));

    assert_eq!(
        serde_json::to_value(&rebuilt.entries).unwrap(),
        serde_json::to_value(&log.entries).unwrap()
    );
    let gitlab = entries_by_connection(&log)
        .into_iter()
        .find(|(id, _)| id.source == Source::Gitlab)
        .unwrap();
    assert_eq!(gitlab.1.len(), 2, "the folded push is given back to GitLab");
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
fn collecting_the_same_range_again_keeps_a_meeting_that_began_before_it() {
    let shown = a_log(vec![(
        id(Source::Calendar, "team"),
        vec![meeting("2026-02-28T23:00:00Z", 120)],
    )]);

    let carried = carry_over(&shown, march());

    assert_eq!(carried.len(), 1);
    assert_eq!(carried[0].1.len(), 1);
}

#[test]
fn another_range_keeps_what_falls_inside_and_meetings_that_overlap_it() {
    let shown = a_log(vec![
        (
            id(Source::Calendar, "team"),
            vec![
                // From the evening before into the new range.
                meeting("2026-03-09T23:00:00Z", 120),
                // Ended before it.
                meeting("2026-03-09T20:00:00Z", 60),
            ],
        ),
        (
            id(Source::Git, "laptop"),
            vec![
                commit_in("laptop", "in", "2026-03-10T10:00:00Z"),
                commit_in("laptop", "out", "2026-03-09T10:00:00Z"),
            ],
        ),
    ]);
    let later = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap(),
        ..march()
    };

    let carried = carry_over(&shown, later);

    let timestamps = carried
        .iter()
        .flat_map(|(_, entries)| entries)
        .map(|entry| entry.timestamp().to_string())
        .collect::<Vec<_>>();
    assert_eq!(timestamps, ["2026-03-10T10:00:00Z", "2026-03-09T23:00:00Z"]);
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
        "main".to_string(),
        "2026-03-10T10:00:00Z".to_string(),
    )
}

fn gitlab_push(commit_to: &str) -> GitLabEntry {
    GitLabEntry {
        source: "gitlab".to_string(),
        connection: String::new(),
        id: commit_to.to_string(),
        entry_type: crate::model::GitLabType::PushedTo,
        timestamp: "2026-03-10T10:05:00Z".to_string(),
        title: "repo".to_string(),
        target_type: "Project".to_string(),
        mr_iid: None,
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

#[test]
fn a_push_folds_into_one_commit_even_when_two_clones_have_it() {
    let mut wsl_clone = git_commit("abc123");
    wsl_clone.connection = "wsl".to_string();

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

/// Rebuilding a log from its parts (a recheck of another source does) keeps the same push
/// folded and the same one visible.
#[test]
fn two_pushes_of_one_commit_keep_their_places_when_the_log_is_rebuilt() {
    let with_id = |id: &str, at: &str| {
        let ActivityEntry::GitLab(mut push) = push_at("abc123", at) else {
            unreachable!()
        };
        push.id = id.to_string();
        ActivityEntry::GitLab(push)
    };
    let log = a_log(vec![
        (
            id(Source::Git, "laptop"),
            vec![commit_in("laptop", "abc123", "2026-03-10T10:00:00Z")],
        ),
        (
            id(Source::Gitlab, "work"),
            vec![
                with_id("push-a", "2026-03-10T12:00:00Z"),
                with_id("push-b", "2026-03-10T11:00:00Z"),
            ],
        ),
    ]);
    let placement = |log: &ActivityLog| {
        log.entries
            .iter()
            .map(|entry| match entry {
                ActivityEntry::Git(commit) => {
                    format!("folded {}", commit.gitlab_event.as_ref().unwrap().id)
                }
                other => format!("visible {}", other.id()),
            })
            .collect::<Vec<_>>()
    };

    let mut rebuilt = log.clone();
    for _ in 0..3 {
        rebuilt = a_log(entries_by_connection(&rebuilt));
        assert_eq!(placement(&rebuilt), placement(&log));
    }
}
