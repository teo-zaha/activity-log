use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::busy_status::BusyStatus;

/// How a meeting counts towards meeting time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
pub enum MeetingWeight {
    Busy,
    /// Counted apart, and only where no busy meeting runs.
    Tentative,
    None,
}

impl MeetingWeight {
    /// Busy and working elsewhere count; a meeting without a status counts as busy. Free time
    /// and time out of office are no meeting time.
    pub fn of(status: Option<BusyStatus>) -> Self {
        match status {
            None | Some(BusyStatus::Busy | BusyStatus::WorkingElsewhere) => Self::Busy,
            Some(BusyStatus::Tentative) => Self::Tentative,
            Some(BusyStatus::Free | BusyStatus::OutOfOffice) => Self::None,
        }
    }
}
