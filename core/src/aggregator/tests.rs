use chrono::{TimeZone, Utc};

use super::{collect_activity_from_results, compare_timestamps_desc};
use crate::model::{
    ActivityEntry, CollectOptions, ConnectionId, GitEntry, GitLabEntry, GitLabType, Source,
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
    ActivityEntry::GitLab(GitLabEntry {
        source: "gitlab".to_string(),
        connection: "work".to_string(),
        id: commit_to.to_string(),
        entry_type: GitLabType::PushedTo,
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
