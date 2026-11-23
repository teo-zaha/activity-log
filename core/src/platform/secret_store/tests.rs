use super::{Status, key_of, macos_status, windows_status};
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
