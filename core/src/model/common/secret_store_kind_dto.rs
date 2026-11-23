use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which store holds the key a sealed secret needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum SecretStoreKindDto {
    Os,
    File,
}
