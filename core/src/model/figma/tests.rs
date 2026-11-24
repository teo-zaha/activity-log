use serde_json::json;
use ts_rs::TS;

use super::{FigmaComment, FigmaEntry, FigmaVersion};

#[test]
fn versions_require_the_nullable_label_and_description() {
    let version = json!({
        "id": "142",
        "created_at": "2026-11-23T09:00:00Z",
        "user": {"id": "1001"},
        "label": null,
        "description": null
    });
    assert!(serde_json::from_value::<FigmaVersion>(version.clone()).is_ok());
    for field in ["label", "description"] {
        let mut incomplete = version.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<FigmaVersion>(incomplete).is_err());
    }
}

#[test]
fn root_comment_ignores_a_non_string_node_id() {
    let mut comment = json!({
        "id": "11",
        "user": {"id": "1001"},
        "created_at": "2026-11-23T09:00:00Z",
        "parent_id": "",
        "message": "Looks good",
        "client_meta": {"node_id": 42}
    });
    let read = serde_json::from_value::<FigmaComment>(comment.clone()).unwrap();
    assert_eq!(read.parent_id.as_deref(), Some(""));
    assert_eq!(read.node_id(), None);

    comment["message"] = json!(42);
    assert!(serde_json::from_value::<FigmaComment>(comment).is_err());
}

#[test]
fn figma_activity_is_flattened_in_the_entry_declaration() {
    let declaration = FigmaEntry::decl(&ts_rs::Config::default());
    assert!(
        declaration.contains(r#"{ "type": "edited", versionId: string, }"#),
        "{declaration}"
    );
    assert!(
        declaration.contains(r#"fileUrl: string, } & ("#),
        "{declaration}"
    );
}
