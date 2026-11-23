use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::secret_store_kind_dto::SecretStoreKindDto;

/// How a set secret is stored, read from the file and the key already held, never from a
/// store and never its value. Only `sealed` opens here as far as anything can tell without
/// asking the store; the others say what keeps it from opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum SecretStoredDto {
    Sealed {
        store: SecretStoreKindDto,
    },
    /// Sealed for another server or TLS setting than the table names now (its binding tag, a
    /// hint only).
    SealedForOtherServer {
        store: SecretStoreKindDto,
    },
    /// Sealed with another key than the one this program holds: another computer's, or one
    /// from before its store was reset. Said only once a key is held.
    SealedWithOtherKey {
        store: SecretStoreKindDto,
    },
    /// Sealed by a newer version of Activity Log.
    SavedByNewerVersion,
    /// A sealed text that does not read.
    Unreadable,
    /// Written as plain text (by hand, or by a build before sealing).
    PlainText,
}
