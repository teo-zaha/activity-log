use serde::Serialize;
use ts_rs::TS;

/// What `GET /api/events` sends every `RUNTIME_PING_SECONDS` as its `ping` event, so a reader
/// behind any proxy can tell a stream that went quiet from one that is gone: the host that
/// sends it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct RuntimePing {
    pub epoch: String,
}
