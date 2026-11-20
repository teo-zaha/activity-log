use std::collections::{BTreeMap, BTreeSet};

use chrono::{TimeZone, Utc};

use super::{Candidate, ResultStore, Scope, Seed};
use crate::{
    aggregator::collect_activity_from_results,
    model::{
        ActivityEntry, ActivityLog, CalendarEntry, CollectOptions, ConnectionId, GitEntry,
        GitLabEntry, GitLabType, MeetingWeight, Source,
    },
    provider::connection::{Connection, ConnectionLocation, ConnectionValues},
    runtime::{
        keys::KeyTable,
        report::{Outcome, RunReport, Slot, Stored},
    },
};

fn march() -> CollectOptions {
    CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

fn git() -> ConnectionId {
    ConnectionId::new(Source::Git, "laptop")
}

fn gitlab() -> ConnectionId {
    ConnectionId::new(Source::Gitlab, "work")
}

fn jira() -> ConnectionId {
    ConnectionId::new(Source::Jira, "work")
}

fn calendar() -> ConnectionId {
    ConnectionId::new(Source::Calendar, "team")
}

fn connection(id: &ConnectionId, token: &str) -> Connection {
    Connection::new(
        id.clone(),
        id.name.clone(),
        ConnectionLocation::Local,
        ConnectionValues::new(BTreeMap::from([(
            "TOKEN".to_string(),
            Some(token.to_string()),
        )])),
    )
}

fn keys_of(ids: &[ConnectionId]) -> KeyTable {
    let mut keys = KeyTable::default();
    keys.install(&ids.iter().map(|id| connection(id, "a")).collect::<Vec<_>>());
    keys
}

fn commit(hash: &str) -> ActivityEntry {
    let mut commit = GitEntry::new(
        "repo".to_string(),
        hash.to_string(),
        format!("commit {hash}"),
        "me@example.com".to_string(),
        Some("main".to_string()),
        "2026-03-10T10:00:00Z".into(),
    );
    commit.connection = "laptop".into();
    ActivityEntry::Git(commit)
}

fn push(id: &str, sha: &str, timestamp: &str) -> ActivityEntry {
    ActivityEntry::GitLab(GitLabEntry {
        source: crate::model::GitLabTag,
        connection: "work".into(),
        id: id.to_string(),
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
        pushed_commit: Some(sha.to_string()),
    })
}

fn meeting(timestamp: &str) -> ActivityEntry {
    ActivityEntry::Calendar(CalendarEntry {
        source: crate::model::CalendarTag,
        connection: "team".into(),
        id: timestamp.to_string(),
        timestamp: timestamp.into(),
        title: Some("Offsite".to_string()),
        location: None,
        organizer: None,
        meeting: crate::model::MeetingBody {
            duration_minutes: 60,
            busy_status: None,
            meeting_weight: MeetingWeight::Busy,
            description: None,
            description_blocks: Vec::new(),
            join_url: None,
        },
    })
}

fn stored(keys: &KeyTable, id: &ConnectionId, entries: Vec<ActivityEntry>) -> Stored {
    Stored {
        key: keys.key(id).unwrap(),
        entries: entries.into(),
    }
}

fn collected(keys: &KeyTable, id: &ConnectionId, entries: Vec<ActivityEntry>) -> Slot {
    Slot::Settled(Outcome::Collected(stored(keys, id, entries)))
}

fn store(slots: Vec<(ConnectionId, Slot)>) -> ResultStore {
    ResultStore::new(march(), RunReport { slots })
}

fn candidates(ids: &[ConnectionId]) -> Vec<Candidate> {
    ids.iter()
        .map(|id| Candidate {
            id: id.clone(),
            listable: true,
        })
        .collect()
}

fn kinds(sources: &[Source]) -> Scope {
    Scope::Kinds(sources.iter().cloned().collect::<BTreeSet<_>>())
}

/// The log as it goes over the wire, without the instant it was built.
fn wire(log: &ActivityLog) -> serde_json::Value {
    let mut value = serde_json::to_value(log).unwrap();
    value["meta"]["generated"] = serde_json::Value::Null;
    value
}

/// Each push as it shows: folded into a commit or on its own.
fn pushes(log: &ActivityLog) -> Vec<String> {
    let mut shown = log
        .entries
        .iter()
        .filter_map(|entry| match entry {
            ActivityEntry::Git(commit) => commit
                .gitlab_event
                .as_ref()
                .map(|push| format!("folded {}", push.id)),
            ActivityEntry::GitLab(push) => Some(format!("visible {}", push.id)),
            _ => None,
        })
        .collect::<Vec<_>>();
    shown.sort();
    shown
}

/// The report of a run started from `seeds`: collected connections pending with what they
/// carry, the others kept.
fn started(seeds: Vec<(ConnectionId, Seed)>) -> ResultStore {
    store(
        seeds
            .into_iter()
            .map(|(id, seed)| match seed {
                Seed::Collect { carried } => (id, Slot::Pending { carried }),
                Seed::Keep(outcome) => (id, Slot::Kept(outcome)),
            })
            .collect(),
    )
}

#[test]
fn the_projection_is_the_aggregated_log_of_its_connections() {
    let ids = [git(), gitlab(), calendar()];
    let keys = keys_of(&ids);
    let entries = [
        vec![commit("abc123")],
        vec![push("push-a", "abc123", "2026-03-10T11:00:00Z")],
        vec![meeting("2026-03-09T08:00:00Z")],
    ];
    let shown = store(
        ids.iter()
            .zip(entries.clone())
            .map(|(id, entries)| (id.clone(), collected(&keys, id, entries)))
            .collect(),
    );

    let expected = collect_activity_from_results(
        march(),
        ids.iter().cloned().zip(entries).collect(),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(wire(&shown.project()), wire(&expected.log));
    assert_eq!(shown.project().summary.merged_count, 1);
}

/// B19: the store keeps each connection's own entries, so two pushes of one commit come back
/// whichever kind is collected again.
#[test]
fn two_pushes_of_one_commit_survive_a_recheck_of_gitlab_and_of_git() {
    let ids = [git(), gitlab()];
    let keys = keys_of(&ids);
    let both = vec![
        push("push-a", "abc123", "2026-03-10T11:00:00Z"),
        push("push-b", "abc123", "2026-03-10T12:00:00Z"),
    ];
    let ready = store(vec![
        (git(), collected(&keys, &git(), vec![commit("abc123")])),
        (gitlab(), collected(&keys, &gitlab(), both.clone())),
    ]);
    let before = pushes(&ready.project());
    assert_eq!(before, ["folded push-a", "visible push-b"]);

    for kind in [Source::Gitlab, Source::Git] {
        let running = started(ready.plan(
            march(),
            &candidates(&ids),
            &kinds(std::slice::from_ref(&kind)),
            &keys,
        ));
        assert_eq!(pushes(&running.project()), before, "while {kind} collects");

        let collected_again = store(vec![
            (git(), collected(&keys, &git(), vec![commit("abc123")])),
            (gitlab(), collected(&keys, &gitlab(), both.clone())),
        ]);
        let next =
            started(collected_again.plan(march(), &candidates(&ids), &kinds(&[kind]), &keys));
        assert_eq!(pushes(&next.project()), before);
    }
}

#[test]
fn a_run_started_over_a_pending_connection_carries_what_the_first_one_collected() {
    let ids = [gitlab()];
    let mut keys = keys_of(&ids);
    let a = store(vec![(
        gitlab(),
        collected(
            &keys,
            &gitlab(),
            vec![push("push-a", "abc123", "2026-03-10T11:00:00Z")],
        ),
    )]);
    let b = started(a.plan(march(), &candidates(&ids), &Scope::All, &keys));

    // A label edit keeps the key: C still carries A's entries.
    let mut renamed = connection(&gitlab(), "a");
    renamed.label = "Renamed".to_string();
    keys.install(&[renamed]);
    let c = b.plan(march(), &candidates(&ids), &Scope::All, &keys);
    let [
        (
            _,
            Seed::Collect {
                carried: Some(carried),
            },
        ),
    ] = c.as_slice()
    else {
        panic!("expected C to carry A's entries, got {c:?}");
    };
    assert_eq!(carried.key, keys.key(&gitlab()).unwrap());
    assert_eq!(carried.entries.len(), 1);

    // A token edit gives a new key: nothing of A's is carried.
    keys.install(&[connection(&gitlab(), "b")]);
    let c = b.plan(march(), &candidates(&ids), &Scope::All, &keys);
    assert!(matches!(
        c.as_slice(),
        [(_, Seed::Collect { carried: None })]
    ));
}

#[test]
fn failed_skipped_and_empty_connections_show_and_carry_differently() {
    let ids = [jira(), calendar(), gitlab(), git()];
    let keys = keys_of(&ids);
    let ready = store(vec![
        (
            jira(),
            Slot::Settled(Outcome::Failed {
                key: keys.key(&jira()).unwrap(),
                failure: crate::runtime::CollectFailure::of(&anyhow::anyhow!("unreachable host")),
            }),
        ),
        (calendar(), Slot::Skipped),
        (gitlab(), collected(&keys, &gitlab(), Vec::new())),
        (git(), collected(&keys, &git(), vec![commit("abc123")])),
    ]);

    let counted = ready
        .project()
        .summary
        .connections
        .into_iter()
        .map(|count| (count.connection, count.count))
        .collect::<Vec<_>>();
    assert_eq!(counted, [(jira(), 0), (gitlab(), 0), (git(), 1)]);

    let again = ready.plan(march(), &candidates(&ids), &kinds(&[Source::Git]), &keys);
    let seeds = again
        .iter()
        .map(|(id, seed)| match seed {
            Seed::Keep(Outcome::Failed { .. }) => format!("{id} kept failed"),
            Seed::Keep(Outcome::Collected(stored)) => {
                format!("{id} kept {}", stored.entries.len())
            }
            Seed::Collect { carried } => format!(
                "{id} collected, carrying {}",
                carried.as_ref().map_or(0, |stored| stored.entries.len())
            ),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        seeds,
        [
            "jira/work kept failed",
            "gitlab/work kept 0",
            "git/laptop collected, carrying 1"
        ],
        "the skipped calendar is not listed while its kind is not collected"
    );

    let whole = ready.plan(march(), &candidates(&ids), &Scope::All, &keys);
    assert_eq!(
        whole.len(),
        4,
        "a skipped connection is checked again with its kind"
    );
    assert!(
        matches!(&whole[0], (_, Seed::Collect { carried: None })),
        "a failed connection carries nothing"
    );
}

#[test]
fn a_plan_follows_the_new_runtimes_connections() {
    let keys = keys_of(&[git(), gitlab(), jira()]);
    let ready = store(vec![
        (git(), collected(&keys, &git(), vec![commit("abc123")])),
        (
            calendar(),
            Slot::Settled(Outcome::Collected(Stored {
                key: keys.key(&git()).unwrap(),
                entries: vec![meeting("2026-03-09T08:00:00Z")].into(),
            })),
        ),
    ]);
    let mut candidates = candidates(&[git(), gitlab(), jira()]);
    candidates[2].listable = false;

    let plan = ready.plan(
        march(),
        &candidates,
        &kinds(&[Source::Gitlab, Source::Jira]),
        &keys,
    );

    let ids = plan.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
    // The removed calendar is gone, the new GitLab connection is collected without a carry,
    // and the unusable Jira one is not listed.
    assert_eq!(ids, [git(), gitlab()]);
    assert!(matches!(plan[0].1, Seed::Keep(_)));
    assert!(matches!(plan[1].1, Seed::Collect { carried: None }));
}

#[test]
fn another_range_carries_only_what_falls_inside_it() {
    let keys = keys_of(&[calendar()]);
    let ready = store(vec![(
        calendar(),
        collected(
            &keys,
            &calendar(),
            vec![
                meeting("2026-03-09T08:00:00Z"),
                meeting("2026-03-20T08:00:00Z"),
            ],
        ),
    )]);
    let later = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 15, 0, 0, 0).unwrap(),
        ..march()
    };

    let plan = ready.plan(later, &candidates(&[calendar()]), &Scope::All, &keys);

    let [
        (
            _,
            Seed::Collect {
                carried: Some(carried),
            },
        ),
    ] = plan.as_slice()
    else {
        panic!("expected a carry, got {plan:?}");
    };
    assert_eq!(carried.entries.len(), 1);
    assert_eq!(
        carried.entries[0].timestamp().as_str(),
        "2026-03-20T08:00:00Z"
    );
}

#[test]
fn eviction_drops_what_another_key_collected() {
    let ids = [git(), gitlab(), jira()];
    let mut keys = keys_of(&ids);
    let mut shown = store(vec![
        (git(), collected(&keys, &git(), vec![commit("abc123")])),
        (
            gitlab(),
            collected(
                &keys,
                &gitlab(),
                vec![push("push-a", "abc123", "2026-03-10T11:00:00Z")],
            ),
        ),
        (
            jira(),
            Slot::Pending {
                carried: Some(stored(
                    &keys,
                    &jira(),
                    vec![meeting("2026-03-09T08:00:00Z")],
                )),
            },
        ),
    ]);
    assert!(!shown.evict(&keys));

    keys.install(&[
        connection(&git(), "a"),
        connection(&gitlab(), "b"),
        connection(&jira(), "b"),
    ]);
    assert!(shown.evict(&keys));

    let log = shown.project();
    assert_eq!(
        pushes(&log),
        Vec::<String>::new(),
        "the nested push goes with its key"
    );
    assert_eq!(
        log.entries.len(),
        1,
        "only the commit stays: {:?}",
        log.entries
    );
    assert_eq!(log.meta.pending, [jira()]);

    keys.install(&[connection(&git(), "a")]);
    assert!(shown.evict(&keys));
    assert_eq!(shown.project().meta.pending, Vec::<ConnectionId>::new());
}
