use serde_json::json;

use super::map_gitlab_event;
use crate::model::GitLabType;

#[test]
fn maps_gitlab_event_fields() {
    let event = json!({
        "id": 4711,
        "action_name": "pushed to",
        "created_at": "2026-03-01T10:30:00Z",
        "target_title": "feature/activity",
        "target_type": "MergeRequest",
        "target_iid": 12,
        "push_data": { "ref": "main", "commit_count": 3, "commit_to": "abc123" },
        "note": { "body": "Looks good", "resolved": true, "position": { "new_path": "src/lib.rs" } }
    });

    let entry = map_gitlab_event(&event).unwrap();
    assert_eq!(entry.entry_type, GitLabType::PushedTo);
    assert_eq!(entry.branch.as_deref(), Some("main"));
    assert_eq!(entry.commit_count, Some(3));
    assert_eq!(entry.note_file.as_deref(), Some("src/lib.rs"));
    assert_eq!(entry.pushed_commit.as_deref(), Some("abc123"));
    assert_eq!(entry.id, "4711");
    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["pushedCommit"], "abc123");
    assert!(json.get("raw").is_none());
}

#[test]
fn only_a_push_names_the_commit_it_pushed() {
    let event = json!({
        "action_name": "commented on",
        "created_at": "2026-03-01T10:30:00Z",
        "target_type": "Note",
        "push_data": { "commit_to": "abc123" }
    });

    let entry = map_gitlab_event(&event).unwrap();
    assert_eq!(entry.pushed_commit, None);
    assert!(
        entry.id.starts_with("h:"),
        "an event without an id gets a stable one"
    );
    assert_eq!(map_gitlab_event(&event).unwrap().id, entry.id);
    assert!(
        serde_json::to_value(&entry)
            .unwrap()
            .get("pushedCommit")
            .is_none()
    );
}
