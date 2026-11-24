use chrono::{TimeZone, Utc};

use super::{
    carried_for, collect_activity_from_results, fold_hosted_commits, make_ids_unique, within,
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
        project_id: None,
        mr_iid: None,
        issue_iid: None,
        branch: Some("main".to_string()),
        commit_count: Some(1),
        note_body: None,
        note_file: None,
        note_resolved: None,
        pushed_commit: Some(commit_to.to_string()),
        change_stack: None,
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

fn entries_of(
    carried: &[crate::provider::provider_adapter::ConnectionEntries],
) -> impl Iterator<Item = (&ConnectionId, &[ActivityEntry])> {
    carried.iter().map(|(id, entries)| (id, entries.as_slice()))
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

    let shown = carried_for(entries_of(&carried), &pending, &finished);
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
        entries_of(&[(work.clone(), vec![push("abc123")])]),
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

    assert!(carried_for(entries_of(&carried), &[], &[]).is_empty());
    assert_eq!(carried_for(entries_of(&carried), &[work], &[]).len(), 1);
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
        connection: "work".into(),
        id: commit_to.to_string(),
        entry_type: crate::model::GitLabType::PushedTo,
        timestamp: "2026-03-10T10:05:00Z".into(),
        title: Some("repo".to_string()),
        target_type: Some("Project".to_string()),
        project_id: None,
        mr_iid: None,
        issue_iid: None,
        branch: Some("main".to_string()),
        commit_count: Some(1),
        note_body: None,
        note_file: None,
        note_resolved: None,
        pushed_commit: Some(commit_to.to_string()),
        change_stack: None,
    }
}

#[test]
fn merges_git_and_gitlab_push_events() {
    let (merged_git, remaining_gitlab) =
        fold_gitlab(vec![git_commit("abc123")], vec![gitlab_push("abc123")]);

    assert!(merged_git[0].gitlab_event.is_some());
    assert!(remaining_gitlab.is_empty());
}

#[test]
fn a_push_without_its_git_commit_stays_an_entry() {
    let (merged_git, remaining_gitlab) = fold_gitlab(
        vec![git_commit("abc123")],
        vec![gitlab_push("abc123"), gitlab_push("def456")],
    );

    assert!(merged_git[0].gitlab_event.is_some());
    let [unmatched] = remaining_gitlab.as_slice() else {
        panic!("expected the unmatched push, got {remaining_gitlab:?}");
    };
    assert_eq!(unmatched.pushed_commit.as_deref(), Some("def456"));

    let (_, remaining) = fold_gitlab(vec![], vec![gitlab_push("abc123")]);
    assert_eq!(remaining.len(), 1, "nothing to fold into yet");
}

#[test]
fn two_pushes_of_one_commit_fold_one_and_keep_the_other() {
    let mut other_branch = gitlab_push("abc123");
    other_branch.id = "abc123-release".to_string();
    other_branch.branch = Some("release".to_string());

    let (merged_git, remaining_gitlab) = fold_gitlab(
        vec![git_commit("abc123")],
        vec![gitlab_push("abc123"), other_branch],
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

    let (merged_git, remaining_gitlab) = fold_gitlab(vec![git_commit("abc123")], vec![push]);

    assert!(merged_git[0].gitlab_event.is_some());
    assert!(remaining_gitlab.is_empty());
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

    let (merged_git, remaining_gitlab) = fold_gitlab(vec![git_commit("abc123")], vec![z, a]);

    let folded = merged_git[0].gitlab_event.as_ref().unwrap();
    assert_eq!(folded.connection, "a");
    assert_eq!(remaining_gitlab[0].connection, "z");
}

#[test]
fn two_clones_take_two_pushes_in_order() {
    let mut wsl_clone = git_commit("abc123");
    wsl_clone.connection = "wsl".into();
    let mut local = git_commit("abc123");
    local.connection = "work".into();
    let mut other_push = gitlab_push("abc123");
    other_push.id = "second".into();

    let (merged_git, remaining_gitlab) = fold_gitlab(
        vec![wsl_clone, local],
        vec![other_push, gitlab_push("abc123")],
    );

    assert_eq!(merged_git[0].gitlab_event.as_ref().unwrap().id, "second");
    assert_eq!(merged_git[1].gitlab_event.as_ref().unwrap().id, "abc123");
    assert!(remaining_gitlab.is_empty());
}

fn fold_gitlab(
    commits: Vec<GitEntry>,
    pushes: Vec<GitLabEntry>,
) -> (Vec<GitEntry>, Vec<GitLabEntry>) {
    let (commits, remaining) = fold_hosted_commits(
        commits,
        pushes.into_iter().map(ActivityEntry::GitLab).collect(),
    );
    let remaining = remaining
        .into_iter()
        .map(|entry| match entry {
            ActivityEntry::GitLab(push) => push,
            other => panic!("expected a GitLab entry, got {other:?}"),
        })
        .collect();
    (commits, remaining)
}

fn meeting_in(connection: &str, timestamp: &str) -> ActivityEntry {
    let mut entry = meeting(timestamp, 30);
    if let ActivityEntry::Calendar(meeting) = &mut entry {
        meeting.connection = connection.into();
    }
    entry
}

fn push_in(connection: &str, commit_to: &str, timestamp: &str) -> ActivityEntry {
    let mut entry = push_at(commit_to, timestamp);
    if let ActivityEntry::GitLab(push) = &mut entry {
        push.connection = connection.into();
    }
    entry
}

/// A run with every kind of slot: repeated ids, ties in time (one written with an offset, two
/// unfolded pushes of two connections),
/// carried entries (one a finished connection has too), a push folded into a carried commit, a
/// second push of a folded commit, a failed, a skipped and a kept connection.
fn projected_fixture() -> String {
    use crate::runtime::report::{CollectFailure, CollectionKey, Outcome, RunReport, Slot, Stored};

    let stored = |mut entries: Vec<ActivityEntry>| {
        make_ids_unique(&mut entries);
        Stored {
            key: CollectionKey::UNKEYED,
            entries: entries.into(),
        }
    };
    let report = RunReport {
        slots: vec![
            (
                id(Source::Git, "laptop"),
                Slot::Settled(Outcome::Collected(stored(vec![
                    commit_in("laptop", "c1", "2026-03-10T10:00:00Z"),
                    commit_in("laptop", "c2", "2026-03-10T10:00:00Z"),
                    commit_in("laptop", "c1", "2026-03-10T09:00:00Z"),
                    commit_in("laptop", "c3", "2026-03-10T11:00:00+01:00"),
                ]))),
            ),
            (
                id(Source::Gitlab, "work"),
                Slot::Settled(Outcome::Collected(stored(vec![
                    push_in("work", "c1", "2026-03-10T10:05:00Z"),
                    push_in("work", "c1", "2026-03-10T10:06:00Z"),
                    push_in("work", "c3", "2026-03-10T10:00:00Z"),
                    push_in("work", "zz", "2026-03-10T10:00:00Z"),
                    push_in("work", "w1", "2026-03-09T09:30:00Z"),
                ]))),
            ),
            // Its push is at `zz`'s instant and folds into nothing: the two keep connection order.
            (
                id(Source::Gitlab, "oss"),
                Slot::Settled(Outcome::Collected(stored(vec![push_in(
                    "oss",
                    "yy",
                    "2026-03-10T10:00:00Z",
                )]))),
            ),
            (
                id(Source::Git, "wsl"),
                Slot::Pending {
                    carried: Some(stored(vec![
                        commit_in("wsl", "c2", "2026-03-10T10:00:00Z"),
                        commit_in("wsl", "w1", "2026-03-09T09:00:00Z"),
                    ])),
                },
            ),
            (
                id(Source::Calendar, "team"),
                Slot::Settled(Outcome::Collected(stored(vec![
                    meeting_in("team", "2026-03-10T10:00:00Z"),
                    meeting_in("team", "2026-03-10T10:00:00Z"),
                    meeting_in("team", "2026-03-10T09:00:00Z"),
                ]))),
            ),
            (
                id(Source::Jira, "tickets"),
                Slot::Settled(Outcome::Failed {
                    key: CollectionKey::UNKEYED,
                    failure: CollectFailure::timed_out(std::time::Duration::from_secs(1)),
                }),
            ),
            (id(Source::System, "system"), Slot::Skipped),
            (
                id(Source::Calendar, "other"),
                Slot::Kept(Outcome::Collected(stored(vec![meeting_in(
                    "other",
                    "2026-03-10T10:00:00Z",
                )]))),
            ),
            (
                id(Source::Calendar, "later"),
                Slot::Pending {
                    carried: Some(stored(vec![meeting_in("later", "2026-03-10T10:00:00Z")])),
                },
            ),
        ],
    };
    let mut log = report.project(march()).log;
    log.meta.generated = "generated".to_string();
    serde_json::to_string_pretty(&log).unwrap() + "\n"
}

/// The projection's bytes, entry order included, as they were before its rework (step 12).
#[test]
fn a_projection_keeps_its_bytes_and_its_order() {
    assert_eq!(projected_fixture(), include_str!("projection.golden.json"));
}

/// `make_ids_unique` as it was before it kept a counter per id (step 12): the oracle the
/// counter version must agree with.
fn make_ids_unique_by_search(entries: &mut [ActivityEntry]) -> usize {
    let mut taken = entries
        .iter()
        .map(|entry| entry.id().to_string())
        .collect::<std::collections::HashSet<_>>();
    let mut seen = std::collections::HashSet::new();
    let mut renamed = 0;
    for entry in entries {
        if seen.insert(entry.id().to_string()) {
            continue;
        }
        let unique = (2..)
            .map(|n| format!("{}#{n}", entry.id()))
            .find(|candidate| !taken.contains(candidate))
            .expect("an unused id exists");
        taken.insert(unique.clone());
        seen.insert(unique.clone());
        *entry.id_mut() = unique;
        renamed += 1;
    }
    renamed
}

#[test]
fn numbering_by_counter_agrees_with_searching_from_two() {
    let ids = ["x", "x#2", "x#3", "y", "x#2#2", "x#5"];
    // Every sequence of up to five ids from `ids`, so originals that look numbered come before,
    // between and after the repeats.
    let mut sequences: Vec<Vec<&str>> = vec![Vec::new()];
    let mut last = sequences.clone();
    for _ in 0..5 {
        last = last
            .iter()
            .flat_map(|sequence| {
                ids.iter().map(move |id| {
                    let mut longer = sequence.clone();
                    longer.push(*id);
                    longer
                })
            })
            .collect();
        sequences.extend(last.iter().cloned());
    }
    for sequence in sequences {
        let entries = sequence
            .iter()
            .map(|id| {
                let mut entry = push(id);
                *entry.id_mut() = id.to_string();
                entry
            })
            .collect::<Vec<_>>();
        let (mut counted, mut searched) = (entries.clone(), entries);
        assert_eq!(
            make_ids_unique(&mut counted),
            make_ids_unique_by_search(&mut searched),
            "{sequence:?}"
        );
        assert_eq!(
            counted.iter().map(ActivityEntry::id).collect::<Vec<_>>(),
            searched.iter().map(ActivityEntry::id).collect::<Vec<_>>(),
            "{sequence:?}"
        );
    }
}

/// Times the projection every command answer and read waits on under the host's state lock, and
/// numbering a connection's repeated ids:
/// `cargo test --release -p core --lib benchmark_projection -- --ignored --nocapture`.
#[test]
#[ignore]
fn benchmark_projection() {
    use std::{hint::black_box, time::Instant};

    use crate::{
        model::{CalendarTag, GitLabTag, MeetingBody, Timestamp},
        runtime::report::{CollectionKey, Outcome, RunReport, Slot, Stored},
    };

    fn timestamp(i: usize) -> Timestamp {
        Utc.timestamp_opt(1_777_000_000 + (i * 71 % 53_999) as i64, 0)
            .unwrap()
            .into()
    }
    fn meeting(i: usize, connection: &str) -> ActivityEntry {
        ActivityEntry::Calendar(CalendarEntry {
            source: CalendarTag,
            connection: connection.into(),
            id: i.to_string(),
            timestamp: timestamp(i),
            title: Some("Planning and review".into()),
            location: None,
            organizer: None,
            meeting: MeetingBody {
                duration_minutes: 30,
                busy_status: None,
                meeting_weight: MeetingWeight::Busy,
                description: Some(
                    "A recurring planning meeting with participants and agenda".repeat(4),
                ),
                description_blocks: vec![],
                join_url: None,
            },
        })
    }
    fn commit(i: usize) -> ActivityEntry {
        let mut entry = GitEntry::new(
            "repo".into(),
            i.to_string(),
            "A commit subject".into(),
            "dev@example.org".into(),
            Some("main".into()),
            timestamp(i),
        );
        entry.connection = "git".into();
        ActivityEntry::Git(entry)
    }
    fn push(i: usize) -> ActivityEntry {
        ActivityEntry::GitLab(GitLabEntry {
            source: GitLabTag,
            connection: "gitlab".into(),
            id: i.to_string(),
            title: Some("repo".into()),
            entry_type: GitLabType::PushedTo,
            timestamp: timestamp(i + 1),
            target_type: Some("Project".into()),
            project_id: None,
            mr_iid: None,
            issue_iid: None,
            branch: Some("main".into()),
            commit_count: Some(1),
            note_body: None,
            note_file: None,
            note_resolved: None,
            pushed_commit: Some((i * 2).to_string()),
            change_stack: None,
        })
    }
    fn stored(entries: Vec<ActivityEntry>) -> Stored {
        Stored {
            key: CollectionKey::UNKEYED,
            entries: entries.into(),
        }
    }
    fn median(label: &str, mut run: impl FnMut() -> u128) {
        let mut samples = (0..19).map(|_| run()).collect::<Vec<_>>();
        samples.drain(..2);
        samples.sort_unstable();
        println!(
            "{label}: median {} µs (min {} µs, max {} µs)",
            samples[8], samples[0], samples[16]
        );
    }

    let mut slots = vec![
        (
            id(Source::Git, "git"),
            Slot::Settled(Outcome::Collected(stored(
                (0..10_000).map(commit).collect(),
            ))),
        ),
        (
            id(Source::Gitlab, "gitlab"),
            Slot::Settled(Outcome::Collected(stored((0..8_000).map(push).collect()))),
        ),
    ];
    for n in 0..26 {
        let name = format!("team-{n}");
        let stored = stored((0..1_000).map(|i| meeting(i + 1000 * n, &name)).collect());
        let slot = if n < 22 {
            Slot::Settled(Outcome::Collected(stored))
        } else {
            Slot::Pending {
                carried: Some(stored),
            }
        };
        slots.push((id(Source::Calendar, &name), slot));
    }
    let report = RunReport { slots };
    let options = CollectOptions {
        start: Utc.timestamp_opt(1_776_000_000, 0).unwrap(),
        end: Utc.timestamp_opt(1_778_000_000, 0).unwrap(),
    };
    median("44k entries of 28 connections, 4 pending", || {
        let started = Instant::now();
        let log = black_box(report.project(options));
        let elapsed = started.elapsed().as_micros();
        assert_eq!(log.log.entries.len(), 39_000);
        elapsed
    });
    let mut repeated = vec![meeting(0, "same"); 3_000];
    *repeated[1].id_mut() = "0#2".into();
    median("3k entries with one id", || {
        let mut entries = repeated.clone();
        let started = Instant::now();
        let renamed = black_box(make_ids_unique(&mut entries));
        let elapsed = started.elapsed().as_micros();
        assert_eq!(renamed, 2_998);
        elapsed
    });
}
