use serde_json::json;

use super::{commit_to_sha, map_gitlab_event};
use crate::model::GitLabType;

#[test]
fn maps_gitlab_event_fields() {
    let event = json!({
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
    assert_eq!(commit_to_sha(&entry), Some("abc123"));
}
