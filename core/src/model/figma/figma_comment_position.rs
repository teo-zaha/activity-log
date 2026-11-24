use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Canvas positions have no node; frame-relative positions carry its id.
#[derive(Debug, Deserialize)]
pub(crate) struct FigmaCommentPosition {
    #[serde(default, deserialize_with = "read_node_id")]
    pub(crate) node_id: Option<String>,
}

fn read_node_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::String(node_id) => Ok(Some(node_id)),
        _ => Ok(None),
    }
}
