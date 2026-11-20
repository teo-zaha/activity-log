use serde::{Deserialize, Serialize};
use strum::EnumIter;
use ts_rs::TS;

/// What kind of failure a backend answer is. Callers decide on this, never on the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, EnumIter)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum BackendErrorKind {
    /// Nothing has been collected yet.
    NoData,
    /// The request cannot be served as sent: a bad range, zone, link, label or value. Sending it
    /// again fails the same way.
    InvalidRequest,
    /// The connection or route asked for does not exist (any more).
    NotFound,
    /// The kind allows one connection and has it.
    AlreadyExists,
    /// Another program held the settings file for the whole wait; trying again may work.
    Busy,
    /// Another program changed what the request was based on; nothing was saved.
    Conflict,
    /// There is no folder for the settings.
    NoLocation,
    /// The settings' location cannot keep files private.
    Unprotectable,
    /// The settings file cannot be read as a configuration.
    BadConfig,
    /// The request is larger than the backend reads.
    TooLarge,
    /// Anything else failed in the backend.
    Internal,
}
