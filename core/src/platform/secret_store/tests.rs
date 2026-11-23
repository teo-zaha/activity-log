use super::{Search, Status, bus_io_kind, key_of, macos_status, win32_code_of, windows_status};
use crate::secrets::{KEY_LEN, Lookup, StoreErrorKind};

/// Only "no such item" is absent; a missing or locked keychain, a cancelled dialog and every
/// other status are not, so no key is ever created over them.
#[test]
fn macos_statuses_are_absent_only_when_the_item_is_missing() {
    use StoreErrorKind::{Failed, Refused, Unavailable};
    for (code, status) in [
        (-25300, Status::Absent),
        (-25299, Status::Duplicate),
        (-25307, Status::Error(Unavailable)),
        (-25291, Status::Error(Unavailable)),
        (-25294, Status::Error(Unavailable)),
        (-25308, Status::Error(Refused)),
        (-25315, Status::Error(Refused)),
        (-25293, Status::Error(Refused)),
        (-128, Status::Error(Refused)),
        (-25295, Status::Error(Failed)),
        (-34018, Status::Error(Failed)),
        (-1, Status::Error(Failed)),
    ] {
        assert_eq!(macos_status(code), status, "OSStatus {code}");
    }
}

#[test]
fn windows_codes_are_absent_only_when_the_credential_is_missing() {
    use StoreErrorKind::{Failed, Unavailable};
    for (code, status) in [
        (1168, Status::Absent),
        (1312, Status::Error(Unavailable)),
        (5, Status::Error(Failed)),
        (1004, Status::Error(Failed)),
        (0x8007_0490, Status::Error(Failed)),
    ] {
        assert_eq!(windows_status(code), status, "error {code}");
    }
}

/// An item that holds anything but a key is a failed store, never an absent key, so it is
/// neither used nor replaced.
#[test]
fn an_item_is_a_key_only_with_a_keys_length() {
    assert!(matches!(key_of(&[7; KEY_LEN]), Ok(Lookup::Found(_))));
    for bytes in [&[][..], &[7; KEY_LEN - 1][..], &[7; KEY_LEN + 1][..]] {
        let err = key_of(bytes).unwrap_err();
        assert_eq!(err.kind(), StoreErrorKind::Failed);
    }
}

/// `HRESULT_FROM_WIN32` codes give their Win32 code; another facility's stays whole.
#[test]
fn win32_codes_are_read_out_of_their_hresult() {
    for (hresult, code) in [
        (0x8007_0490_u32 as i32, 1168),
        (0x8007_0520_u32 as i32, 1312),
        (0x8007_0005_u32 as i32, 5),
        (0x8000_4005_u32 as i32, 0x8000_4005),
        (0x8009_0016_u32 as i32, 0x8009_0016),
    ] {
        assert_eq!(win32_code_of(hresult), code, "{hresult:#x}");
    }
    assert_eq!(
        windows_status(win32_code_of(0x8007_0490_u32 as i32)),
        Status::Absent
    );
}

/// An empty search is absent only in a collection that is unlocked once it answered: a
/// locked one (KeePassXC answers every search of a locked collection empty) is no answer.
#[test]
fn an_empty_search_of_a_locked_collection_is_not_absent() {
    assert_eq!(Search::of(0, false), Search::Empty);
    assert_eq!(Search::of(0, true), Search::EmptyLocked);
    assert_eq!(Search::of(1, false), Search::One);
    assert_eq!(Search::of(1, true), Search::One);
    assert_eq!(Search::of(3, false), Search::Several(3));
}

/// Only no bus at all is no store; any other I/O failure reaching it is the store's.
#[test]
fn only_a_missing_bus_is_no_store() {
    use std::io::ErrorKind;
    assert_eq!(
        bus_io_kind(ErrorKind::NotFound),
        StoreErrorKind::Unavailable
    );
    assert_eq!(
        bus_io_kind(ErrorKind::ConnectionRefused),
        StoreErrorKind::Unavailable
    );
    for kind in [
        ErrorKind::PermissionDenied,
        ErrorKind::ConnectionReset,
        ErrorKind::TimedOut,
        ErrorKind::UnexpectedEof,
        ErrorKind::Other,
    ] {
        assert_eq!(bus_io_kind(kind), StoreErrorKind::Failed, "{kind:?}");
    }
}
