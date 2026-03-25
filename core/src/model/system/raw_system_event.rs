use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct RawSystemEvent {
    pub id: i32,
    pub ts: String,
}
