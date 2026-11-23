//! An item in the Secret Service's default collection (GNOME Keyring, KWallet, KeePassXC),
//! found by the attributes `application=activity-log` and `purpose=config-key`, through
//! `oo7`'s D-Bus backend directly (its `Keyring` would choose the Flatpak portal in a sandbox).
//! Every call but an unlock prompt, which waits for the user, is bounded by [`CALL`].

use std::{future::Future, time::Duration};

use anyhow::anyhow;
use oo7::{
    dbus::{Collection, Error as DbusError, Service, ServiceError},
    zbus,
};
use zeroize::Zeroizing;

use super::{ItemName, Place, key_of};
use crate::secrets::{Created, DataKey, Interaction, Lookup, StoreError, StoreErrorKind};

const CALL: Duration = Duration::from_secs(5);
const LABEL: &str = "Activity Log config key";

fn attributes(name: ItemName) -> [(&'static str, &'static str); 2] {
    let purpose = match name {
        ItemName::ConfigKey => "config-key",
        ItemName::Test => "config-key-test",
    };
    [("application", "activity-log"), ("purpose", purpose)]
}

pub(super) async fn read(place: Place, interaction: Interaction) -> Result<Lookup, StoreError> {
    let (_service, collection) = default_collection().await?;
    unlock(&collection, interaction).await?;
    let attributes = attributes(place.name);
    let items = call(
        "Searching the keyring",
        collection.search_items(&attributes),
    )
    .await?;
    let item = match items.as_slice() {
        [] => return Ok(Lookup::Absent),
        [item] => item,
        more => {
            return Err(StoreError::failed(anyhow!(
                "The keyring holds {} config key items",
                more.len()
            )));
        }
    };
    if call("Checking whether the item is locked", item.is_locked()).await? {
        match interaction {
            Interaction::Never => {
                return Err(StoreError::refused(anyhow!(
                    "The config key item is locked"
                )));
            }
            Interaction::Allowed => item
                .unlock(None)
                .await
                .map_err(|err| error(err, "Unlocking the config key item", Step::Use))?,
        }
    }
    let secret = call("Reading the config key", item.secret()).await?;
    key_of(secret.as_bytes())
}

pub(super) async fn create_if_absent(place: Place, key: DataKey) -> Result<Created, StoreError> {
    let (_service, collection) = default_collection().await?;
    unlock(&collection, Interaction::Allowed).await?;
    let attributes = attributes(place.name);
    // `CreateItem` without replacing adds a second item beside one that exists, so it is
    // looked for first; the key-creation lock keeps our other processes out meanwhile.
    let found = call(
        "Searching the keyring",
        collection.search_items(&attributes),
    )
    .await?;
    if !found.is_empty() {
        return Ok(Created::AlreadyThere);
    }
    let secret = Zeroizing::new(key.bytes().to_vec());
    call(
        "Adding the config key to the keyring",
        collection.create_item(LABEL, &attributes, secret, false, None),
    )
    .await?;
    Ok(Created::Stored)
}

/// The service and its default collection answer; nothing is unlocked or read.
pub(super) async fn probe(_place: Place) -> Result<(), StoreError> {
    default_collection().await.map(drop)
}

#[cfg(feature = "real-store-tests")]
pub(super) async fn delete(place: Place) -> Result<(), StoreError> {
    let (_service, collection) = default_collection().await?;
    unlock(&collection, Interaction::Allowed).await?;
    let attributes = attributes(place.name);
    for item in call(
        "Searching the keyring",
        collection.search_items(&attributes),
    )
    .await?
    {
        call("Deleting the config key", item.delete(None)).await?;
    }
    Ok(())
}

/// The session's Secret Service and its default collection, never one created for us (that
/// would show a dialog to name it): a service without one is no store here.
async fn default_collection() -> Result<(Service, Collection), StoreError> {
    let service = bounded(
        "Connecting to the Secret Service",
        Step::Connect,
        Service::new(),
    )
    .await?;
    let collection = bounded(
        "Finding the default collection",
        Step::Connect,
        service.with_alias(Service::DEFAULT_COLLECTION),
    )
    .await?
    .ok_or_else(|| {
        StoreError::unavailable(anyhow!("The Secret Service has no default collection"))
    })?;
    Ok((service, collection))
}

/// A locked collection is noticed by its `Locked` property; only an interactive read unlocks
/// it, through the service's prompt, which waits as long as the user does.
async fn unlock(collection: &Collection, interaction: Interaction) -> Result<(), StoreError> {
    if !call(
        "Checking whether the keyring is locked",
        collection.is_locked(),
    )
    .await?
    {
        return Ok(());
    }
    match interaction {
        Interaction::Never => Err(StoreError::refused(anyhow!("The keyring is locked"))),
        Interaction::Allowed => collection
            .unlock(None)
            .await
            .map_err(|err| error(err, "Unlocking the keyring", Step::Use)),
    }
}

/// Which part of the exchange failed: reaching the service, where a missing bus or service
/// means there is no store, or using it, where any failure is the store's.
#[derive(Clone, Copy)]
enum Step {
    Connect,
    Use,
}

async fn call<T>(
    doing: &str,
    work: impl Future<Output = Result<T, DbusError>>,
) -> Result<T, StoreError> {
    bounded(doing, Step::Use, work).await
}

async fn bounded<T>(
    doing: &str,
    step: Step,
    work: impl Future<Output = Result<T, DbusError>>,
) -> Result<T, StoreError> {
    match tokio::time::timeout(CALL, work).await {
        Ok(answer) => answer.map_err(|err| error(err, doing, step)),
        Err(_) => Err(StoreError::failed(anyhow!(
            "{doing}: no answer within {} s",
            CALL.as_secs()
        ))),
    }
}

fn error(err: DbusError, doing: &str, step: Step) -> StoreError {
    let kind = match (&err, step) {
        (DbusError::Dismissed, _) | (DbusError::Service(ServiceError::IsLocked(_)), _) => {
            StoreErrorKind::Refused
        }
        (DbusError::ZBus(err) | DbusError::Service(ServiceError::ZBus(err)), Step::Connect)
            if no_service(err) =>
        {
            StoreErrorKind::Unavailable
        }
        _ => StoreErrorKind::Failed,
    };
    StoreError::new(kind, anyhow!("{doing}: {err}"))
}

/// No session bus (no address, nothing listening), or no Secret Service on it that the bus
/// could start: D-Bus's own error names, not text.
fn no_service(err: &zbus::Error) -> bool {
    const SERVICE_UNKNOWN: &str = "org.freedesktop.DBus.Error.ServiceUnknown";
    const NAME_HAS_NO_OWNER: &str = "org.freedesktop.DBus.Error.NameHasNoOwner";
    match err {
        zbus::Error::Address(_) | zbus::Error::InputOutput(_) => true,
        zbus::Error::FDO(err) => matches!(
            **err,
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_)
        ),
        zbus::Error::MethodError(name, _, _) => {
            matches!(name.as_str(), SERVICE_UNKNOWN | NAME_HAS_NO_OWNER)
        }
        _ => false,
    }
}
