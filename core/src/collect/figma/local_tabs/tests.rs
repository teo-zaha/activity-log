use std::fs;

use super::*;

#[test]
fn reads_owned_tabs_and_keeps_only_well_typed_rows_for_each_account() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings.json");
    fs::write(
        &path,
        br#"{"lastOpenWindow":{"tabs":[
        {"path":"/design/ExampleFile", "userId":"1001"},
        {"path":"/design/ExampleFile/branch/ExampleBranch", "userId":1001},
        {"path":"/design/AnotherFile", "userId":"2002"},
        {"path":"/design/NoAccount"},
        {"path":"/design/WrongAccountType", "userId":{}},
        {"path":["/design/WrongPathType"], "userId":"1001"},
        {"userId":"1001"},
        null
    ]}}"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    let outcome = read(&path);
    let keys = |user| {
        outcome
            .files_for(user)
            .map(FigmaFileKey::as_str)
            .collect::<Vec<_>>()
    };
    assert_eq!(keys("1001"), ["ExampleFile", "ExampleBranch"]);
    assert_eq!(keys("2002"), ["AnotherFile"]);
    assert_eq!(outcome.warning(), None);
}

#[test]
fn distinguishes_absent_unrecognized_and_oversize_input() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings.json");
    assert!(matches!(read(&path), Outcome::Absent));
    fs::write(&path, br#"{"lastOpenWindow":{"tabs":42}}"#).unwrap();
    assert!(matches!(read(&path), Outcome::Unrecognized));
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(u64::from(LIMIT) + 1)
        .unwrap();
    assert!(matches!(read(&path), Outcome::TooLarge));
}

#[cfg(unix)]
#[test]
fn refuses_world_writable_input_and_never_follows_a_symlink() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings.json");
    fs::write(&path, br#"{"lastOpenWindow":{"tabs":[]}}"#).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(matches!(read(&path), Outcome::NotOwned));

    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let link = temp.path().join("link.json");
    symlink(&path, &link).unwrap();
    assert!(
        matches!(read(&link), Outcome::Unreadable(error) if error.raw_os_error() == Some(libc::ELOOP))
    );
}
