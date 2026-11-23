use super::{Choice, ForcedStore, FrontEnd, OsAnswer, UnknownStore};
use crate::{
    model::{KeyFileReason, NoStoreReason, OsStoreState, SecretStoreInfo},
    profile::Profile,
    secrets::StoreErrorKind,
};

fn choice(front_end: FrontEnd, forced: Option<ForcedStore>) -> Choice {
    Choice {
        front_end,
        profile: Profile::Installed,
        forced,
        has_os_store: true,
    }
}

const READY: SecretStoreInfo = SecretStoreInfo::Os {
    state: OsStoreState::Ready,
};
const REFUSED: SecretStoreInfo = SecretStoreInfo::Os {
    state: OsStoreState::Refused,
};
const FILE_NO_OS: SecretStoreInfo = SecretStoreInfo::File {
    why: KeyFileReason::NoOsStore,
};
const NOTHING: SecretStoreInfo = SecretStoreInfo::Unavailable {
    why: NoStoreReason::NoOsStore,
};
const UNAVAILABLE: OsAnswer = OsAnswer::Error(StoreErrorKind::Unavailable);

/// The desktop app has the OS store or nothing; the CLI and the API fall back to the key
/// file only where there is no OS store, never over a refusal.
#[test]
fn each_front_end_shows_its_store() {
    for (front_end, unavailable) in [
        (FrontEnd::Desktop, NOTHING),
        (FrontEnd::Cli, FILE_NO_OS),
        (FrontEnd::Api, FILE_NO_OS),
    ] {
        let choice = choice(front_end, None);
        assert_eq!(choice.info(OsAnswer::NotAsked), READY);
        assert_eq!(choice.info(OsAnswer::Ready), READY);
        assert_eq!(choice.info(UNAVAILABLE), unavailable, "{front_end:?}");
        for kind in [StoreErrorKind::Refused, StoreErrorKind::Failed] {
            assert_eq!(choice.info(OsAnswer::Error(kind)), REFUSED, "{front_end:?}");
        }
    }
}

/// `file` forces the key file in the CLI and the API; `os` takes the CLI's fallback away; the
/// desktop app reads neither.
#[test]
fn the_variable_chooses_where_the_cli_and_the_api_keep_the_key() {
    for front_end in [FrontEnd::Cli, FrontEnd::Api] {
        let forced = choice(front_end, Some(ForcedStore::File));
        for answer in [OsAnswer::NotAsked, OsAnswer::Ready, UNAVAILABLE] {
            assert_eq!(
                forced.info(answer),
                SecretStoreInfo::File {
                    why: KeyFileReason::Forced
                }
            );
        }
        assert_eq!(
            choice(front_end, Some(ForcedStore::Os)).info(UNAVAILABLE),
            NOTHING
        );
    }
    assert_eq!(
        choice(FrontEnd::Desktop, Some(ForcedStore::File)).info(OsAnswer::Ready),
        READY
    );
}

#[test]
fn the_dev_profile_always_uses_its_key_file() {
    for front_end in [FrontEnd::Desktop, FrontEnd::Cli, FrontEnd::Api] {
        let dev = Choice {
            profile: Profile::Dev,
            has_os_store: false,
            ..choice(front_end, Some(ForcedStore::Os))
        };
        assert_eq!(
            dev.info(OsAnswer::NotAsked),
            SecretStoreInfo::File {
                why: KeyFileReason::DevProfile
            }
        );
    }
}

#[test]
fn without_an_os_store_the_front_end_decides() {
    let without = |front_end| Choice {
        has_os_store: false,
        ..choice(front_end, None)
    };
    assert_eq!(without(FrontEnd::Desktop).info(OsAnswer::NotAsked), NOTHING);
    assert_eq!(without(FrontEnd::Cli).info(OsAnswer::NotAsked), FILE_NO_OS);
}

#[test]
fn the_variable_names_os_or_file() {
    assert_eq!(ForcedStore::parse(None), Ok(None));
    assert_eq!(ForcedStore::parse(Some("")), Ok(None));
    assert_eq!(ForcedStore::parse(Some("os")), Ok(Some(ForcedStore::Os)));
    assert_eq!(
        ForcedStore::parse(Some("file")),
        Ok(Some(ForcedStore::File))
    );
    assert_eq!(
        ForcedStore::parse(Some("keychain")),
        Err(UnknownStore("keychain".to_string()))
    );
}
