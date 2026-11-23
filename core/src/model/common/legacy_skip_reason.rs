use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why a value of 0.0.7's settings was not imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum LegacySkipReason {
    /// Today's settings refuse it (`settings::view::normalize`).
    Invalid,
    /// A token without a server address to bind it to.
    NoAddress,
    /// A `WSL_GIT_DIRS` item that is not `Distro:/absolute/path`.
    InvalidWslRepo,
    /// A `.env` line that refers to a variable (`$VAR`, `${VAR}`), which 0.0.7 filled in from
    /// its environment: the import reads none.
    ReadsEnvironment,
}
