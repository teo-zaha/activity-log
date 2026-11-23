//! A system without a secure store this program knows: every answer is `Unavailable`, so the
//! CLI uses the key file and the desktop app refuses to seal.

use anyhow::anyhow;

use super::Place;
use crate::secrets::{Created, DataKey, Interaction, Lookup, StoreError};

fn unavailable() -> StoreError {
    StoreError::unavailable(anyhow!(
        "This system has no secure store Activity Log can use"
    ))
}

pub(super) async fn read(_place: Place, _interaction: Interaction) -> Result<Lookup, StoreError> {
    Err(unavailable())
}

pub(super) async fn create_if_absent(_place: Place, _key: DataKey) -> Result<Created, StoreError> {
    Err(unavailable())
}

pub(super) async fn probe(_place: Place) -> Result<(), StoreError> {
    Err(unavailable())
}

#[cfg(feature = "real-store-tests")]
pub(super) async fn delete(_place: Place) -> Result<(), StoreError> {
    Err(unavailable())
}
