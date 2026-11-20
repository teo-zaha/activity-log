use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::backend_error_kind::BackendErrorKind;

/// Why the backend refused or failed a request: the body of every API error and what every
/// desktop command fails with. `message` is for people; code decides on `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackendError {
    pub kind: BackendErrorKind,
    /// Sent as `error`, the field API error bodies always had.
    #[serde(rename = "error")]
    #[ts(rename = "error")]
    pub message: String,
}

impl BackendError {
    pub fn new(kind: BackendErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn no_data() -> Self {
        Self::new(BackendErrorKind::NoData, "No data yet")
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(BackendErrorKind::InvalidRequest, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(BackendErrorKind::Internal, message)
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BackendError {}
