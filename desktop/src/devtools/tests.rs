use std::ffi::OsStr;

use super::asked;

#[test]
fn opens_the_inspector_only_for_one() {
    assert!(asked(Some(OsStr::new("1"))));
    assert!(!asked(None));
    assert!(!asked(Some(OsStr::new("0"))));
    assert!(!asked(Some(OsStr::new("true"))));
    assert!(!asked(Some(OsStr::new(""))));
}
