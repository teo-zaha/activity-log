use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use super::{FigmaActivity, FigmaComment, FigmaVersion};

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
fn autosave_checkpoint_flattens_as_an_edited_activity() {
    #[derive(Serialize, Deserialize, TS)]
    struct Entry {
        file: String,
        #[serde(flatten)]
        #[ts(flatten)]
        activity: FigmaActivity,
    }

    let json = r#"{"file":"Checkout","type":"edited","versionId":"00142"}"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    assert!(
        matches!(&entry.activity, FigmaActivity::Edited { version_id } if version_id == "00142")
    );
    assert_eq!(serde_json::to_string(&entry).unwrap(), json);
    let declaration = Entry::decl(&ts_rs::Config::default());
    assert!(
        declaration.contains("{ file: string, } & ("),
        "{declaration}"
    );
    assert!(
        declaration.contains(r#"{ "type": "edited", versionId: string, }"#),
        "{declaration}"
    );
}
