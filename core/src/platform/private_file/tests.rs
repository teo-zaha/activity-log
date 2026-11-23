use super::{
    Found,
    acl::{self, ADMINISTRATORS, AceView, Expected, FILE_ALL_ACCESS},
    linux_enforces_permissions, macos_enforces_permissions, removable,
};

#[test]
fn only_file_systems_that_enforce_permissions_locally_are_allowed_on_linux() {
    for (name, f_type) in [
        ("ext4", 0xEF53),
        ("xfs", 0x5846_5342),
        ("btrfs", 0x9123_683E),
        ("f2fs", 0xF2F5_2010),
        ("zfs", 0x2FC1_2FC1),
        ("bcachefs", 0xCA45_1A4E),
        ("tmpfs", 0x0102_1994),
    ] {
        assert!(linux_enforces_permissions(f_type), "{name}");
    }
    for (name, f_type) in [
        ("fuse (sshfs, rclone)", 0x6573_5546),
        ("nfs", 0x6969),
        ("cifs", 0xFF53_4D42),
        ("smb2", 0xFE53_4D42),
        ("9p", 0x0102_1997),
        ("vboxsf", 0x786F_4256),
        ("virtiofs", 0x6A65_6A63),
        ("vfat", 0x4d44),
        ("exfat", 0x2011_BAB0),
        // Its upper layer cannot be proven to be on one of the listed file systems.
        ("overlayfs", 0x794C_7630),
    ] {
        assert!(!linux_enforces_permissions(f_type), "{name}");
    }
}

#[test]
fn only_volumes_that_keep_ownership_are_allowed_on_macos() {
    assert!(macos_enforces_permissions("apfs", false));
    assert!(macos_enforces_permissions("hfs", false));
    assert!(!macos_enforces_permissions("apfs", true));
    for name in ["smbfs", "nfs", "macfuse", "msdos", "exfat", "webdav"] {
        assert!(!macos_enforces_permissions(name, false), "{name}");
    }
}

/// Windows cleanup deletes only a folder of plain files; anything else keeps it whole.
#[test]
fn a_staging_folder_with_anything_but_plain_files_is_kept() {
    assert!(removable(&[]));
    assert!(removable(&[Found::File, Found::File]));
    assert!(!removable(&[Found::File, Found::Other]));
    assert!(!removable(&[Found::Other]));
}

/// What every system's writers rely on: a replace that lands owner-only, or fails at a named
/// step leaving the old file, and names that stay inside the staging folder.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
mod replace {
    use std::{fs, path::Path};

    use super::super::{ReplaceError, ReplaceStep, open_folder, private_staging};

    fn names(folder: &Path) -> Vec<String> {
        let mut names = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    /// The user's alone: mode 0600 on Unix; on Windows protected, with one full-access entry
    /// for the user.
    fn assert_owner_only(path: &Path) {
        assert!(!super::super::readable_by_others(path).unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777;
            assert_eq!(mode, 0o600);
        }
        #[cfg(windows)]
        assert!(super::super::windows::is_created_owner_only(path).unwrap());
    }

    #[test]
    fn replaces_a_file_with_an_owner_only_one_and_removes_the_staging_folder() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        fs::write(&target, "old").unwrap();

        open_folder(dir.path())
            .unwrap()
            .replace("config.toml", b"new")
            .unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_owner_only(&target);
        assert_eq!(names(dir.path()), ["config.toml"]);
    }

    #[test]
    fn creates_a_missing_file_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("cache.json");

        open_folder(dir.path())
            .unwrap()
            .replace("cache.json", b"new")
            .unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_owner_only(&target);
        assert_eq!(names(dir.path()), ["cache.json"]);
    }

    /// The new file is written; moving it over the target fails (a folder with something in
    /// it): the failure is the publishing step's, the target stays, and the staging folder
    /// goes, file and all.
    #[test]
    fn a_failed_replace_names_its_step_and_leaves_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("kept"), "old").unwrap();

        let failed = open_folder(dir.path())
            .unwrap()
            .replace("config.toml", b"new");

        assert!(
            matches!(
                failed,
                Err(ReplaceError::Failed {
                    step: ReplaceStep::Publishing,
                    ..
                })
            ),
            "{failed:?}"
        );
        assert_eq!(fs::read_to_string(target.join("kept")).unwrap(), "old");
        assert_eq!(names(dir.path()), ["config.toml"]);
    }

    /// A name that is no single entry fails the check, before anything is created.
    #[test]
    fn a_name_out_of_the_folder_fails_the_check() {
        let dir = tempfile::tempdir().unwrap();

        let failed = open_folder(dir.path())
            .unwrap()
            .replace("../config.toml", b"new");

        assert!(
            matches!(
                failed,
                Err(ReplaceError::Failed {
                    step: ReplaceStep::Checking,
                    ..
                })
            ),
            "{failed:?}"
        );
        assert!(names(dir.path()).is_empty());
    }

    /// Names are single entries of the staging folder, never paths out of it.
    #[test]
    fn names_outside_the_staging_folder_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let staging = private_staging(&dir.path().join("config.toml")).unwrap();
        let outside = dir.path().join("outside");

        for name in [
            "",
            ".",
            "..",
            "../outside",
            "a/b",
            outside.to_str().unwrap(),
        ] {
            assert!(staging.create(name).is_err(), "{name:?}");
            assert!(staging.open(name).is_err(), "{name:?}");
            assert!(staging.publish(name).is_err(), "{name:?}");
        }
        assert!(!outside.exists());
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use std::{
        fs,
        io::Write,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };

    use super::super::{PrivateFileError, private_staging, readable_by_others};

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777
    }

    fn names(folder: &Path) -> Vec<String> {
        let mut names = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn publish(target: &Path, content: &str) {
        let staging = private_staging(target).unwrap();
        let mut file = staging.create("new").unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.sync_all().unwrap();
        drop(file);
        staging.publish("new").unwrap();
    }

    #[test]
    fn publishes_an_owner_only_file_and_removes_the_staging_folder() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();

        let staging = private_staging(&target).unwrap();
        assert_eq!(mode(staging.path()), 0o700);
        let file = staging.create("new").unwrap();
        assert_eq!(mode(&staging.path().join("new")), 0o600);
        assert!(
            staging.create("new").is_err(),
            "an existing name is never reused"
        );
        drop(file);
        drop(staging);
        assert_eq!(names(dir.path()), ["config.toml"]);

        publish(&target, "new");
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(mode(&target), 0o600);
        assert!(!readable_by_others(&target).unwrap());
        assert_eq!(names(dir.path()), ["config.toml"]);
    }

    #[test]
    fn a_folder_other_accounts_can_write_to_is_refused_before_anything_is_created() {
        let dir = tempfile::tempdir().unwrap();
        for shared in [0o1777, 0o777, 0o775] {
            let folder = dir.path().join(format!("shared-{shared:o}"));
            fs::create_dir(&folder).unwrap();
            fs::set_permissions(&folder, fs::Permissions::from_mode(shared)).unwrap();

            let refused = private_staging(&folder.join("config.toml"));

            assert!(
                matches!(refused, Err(PrivateFileError::Unprotectable { .. })),
                "{shared:o}"
            );
            assert!(names(&folder).is_empty(), "{shared:o}");
        }
    }

    /// An editor run through the private command creates owner-only backups, whatever the
    /// app's own umask.
    #[test]
    fn files_an_editor_creates_are_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let staging = private_staging(&dir.path().join("config.toml")).unwrap();

        let status = super::super::private_command(std::ffi::OsStr::new("sh"))
            .args(["-c", "echo backup > config.toml~"])
            .current_dir(staging.path())
            .status()
            .unwrap();

        assert!(status.success());
        assert_eq!(mode(&staging.path().join("config.toml~")), 0o600);
    }

    /// The staging folder is renamed between writing and publishing, and a link to another
    /// folder is planted at its old path: publication still moves the file written through the
    /// handle, and the link is never followed.
    #[test]
    fn publication_goes_through_handles_not_paths() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::write(elsewhere.join("new"), "planted").unwrap();

        let staging = private_staging(&target).unwrap();
        let mut file = staging.create("new").unwrap();
        file.write_all(b"written through the handle").unwrap();
        drop(file);
        let moved: PathBuf = dir.path().join("moved");
        fs::rename(staging.path(), &moved).unwrap();
        std::os::unix::fs::symlink(&elsewhere, staging.path()).unwrap();

        staging.publish("new").unwrap();
        let old_path = staging.path().to_path_buf();
        drop(staging);

        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "written through the handle"
        );
        assert_eq!(
            fs::read_to_string(elsewhere.join("new")).unwrap(),
            "planted"
        );
        assert!(fs::symlink_metadata(&old_path).unwrap().is_symlink());
        assert!(names(&moved).is_empty(), "the moved folder was emptied");
    }

    /// Two writers creating the lock file at once both get it (macOS can fail one `openat`
    /// with ENOENT in that race).
    #[test]
    fn two_writers_creating_the_lock_file_at_once_both_open_it() {
        for _ in 0..20 {
            let dir = tempfile::tempdir().unwrap();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let threads = [0, 1].map(|_| {
                let (path, barrier) = (dir.path().to_path_buf(), barrier.clone());
                std::thread::spawn(move || {
                    let folder = super::super::open_folder(&path).unwrap();
                    barrier.wait();
                    folder.open_lock("config.toml.lock").map(drop)
                })
            });
            for thread in threads {
                thread.join().unwrap().unwrap();
            }
        }
    }

    #[test]
    fn a_file_others_can_read_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.toml");
        fs::write(&file, "").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(readable_by_others(&file).unwrap());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!readable_by_others(&file).unwrap());
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

    use super::super::{
        PrivateFileError,
        macos::{FAIL_CLEAR_ACL, acl_entry_count},
        private_staging, readable_by_others,
    };

    /// `/bin/chmod`, since a GNU `chmod` on the PATH has no `+a`.
    fn add_acl(path: &Path, entry: &str) {
        let status = Command::new("/bin/chmod")
            .arg("+a")
            .arg(entry)
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn nothing_inherits_an_entry_for_everyone_from_the_parent() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("settings");
        fs::create_dir(&parent).unwrap();
        add_acl(
            &parent,
            "everyone allow read,file_inherit,directory_inherit",
        );
        let target = parent.join("config.toml");

        let staging = private_staging(&target).unwrap();
        assert_eq!(acl_entry_count(staging.path()).unwrap(), 0);
        assert_eq!(mode(staging.path()), 0o700);
        // What an editor does in `config edit`'s session folder: a file created by path.
        let backup = staging.path().join("config.toml~");
        fs::write(&backup, "backup").unwrap();
        assert_eq!(acl_entry_count(&backup).unwrap(), 0);
        let file = staging.create("new").unwrap();
        drop(file);
        staging.publish("new").unwrap();
        drop(staging);

        assert_eq!(acl_entry_count(&target).unwrap(), 0);
        assert_eq!(mode(&target), 0o600);
        assert!(!readable_by_others(&target).unwrap());
    }

    #[test]
    fn a_volume_that_keeps_an_acl_is_refused_with_nothing_written() {
        let dir = tempfile::tempdir().unwrap();
        FAIL_CLEAR_ACL.set(true);
        let refused = private_staging(&dir.path().join("config.toml"));
        FAIL_CLEAR_ACL.set(false);

        assert!(matches!(
            refused,
            Err(PrivateFileError::Unprotectable { .. })
        ));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_parent_others_may_write_to_through_an_acl_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        add_acl(dir.path(), "everyone allow add_file");

        assert!(matches!(
            private_staging(&dir.path().join("config.toml")),
            Err(PrivateFileError::Unprotectable { .. })
        ));
    }

    /// An entry that lets only the owner add files does not make the folder shared.
    #[test]
    fn an_entry_for_the_owner_alone_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let user = String::from_utf8(
            Command::new("/usr/bin/id")
                .arg("-un")
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        add_acl(dir.path(), &format!("user:{} allow add_file", user.trim()));

        assert!(private_staging(&dir.path().join("config.toml")).is_ok());
    }

    #[test]
    fn a_file_with_an_entry_for_everyone_is_readable_by_others() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.toml");
        fs::write(&file, "").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        add_acl(&file, "everyone allow read");

        assert!(readable_by_others(&file).unwrap());
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    use super::super::private_staging;

    /// A parent whose default ACL grants `other` read: the staging folder and the published file
    /// get no ACL. Skipped where `setfacl` or ACL support is missing.
    #[test]
    fn nothing_inherits_a_default_acl_from_the_parent() {
        let dir = tempfile::tempdir().unwrap();
        let applied = Command::new("setfacl")
            .args(["-d", "-m", "o::r"])
            .arg(dir.path())
            .status();
        if !applied.is_ok_and(|status| status.success()) {
            eprintln!("skipped: setfacl or ACL support is missing");
            return;
        }
        let target = dir.path().join("config.toml");

        let staging = private_staging(&target).unwrap();
        assert!(!has_acl(staging.path()));
        assert_eq!(mode(staging.path()), 0o700);
        drop(staging.create("new").unwrap());
        staging.publish("new").unwrap();
        drop(staging);

        assert!(!has_acl(&target));
        assert_eq!(mode(&target), 0o600);
    }

    fn mode(path: &std::path::Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    fn has_acl(path: &std::path::Path) -> bool {
        let output = Command::new("getfacl")
            .args(["--skip-base", "--absolute-names"])
            .arg(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "getfacl failed for {}",
            path.display()
        );
        !output.stdout.is_empty()
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::{fs, io::Write};

    use super::super::{
        open_folder, private_staging,
        windows::{code, win},
    };

    /// A reader holding the target open (with delete sharing, as Rust and most programs open
    /// files) does not stop the replace: the rename has POSIX semantics.
    #[test]
    fn a_target_open_elsewhere_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        fs::write(&target, "old").unwrap();
        let reader = fs::File::open(&target).unwrap();

        open_folder(dir.path())
            .unwrap()
            .replace("config.toml", b"new")
            .unwrap();
        drop(reader);

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    /// The staging folder is held open without delete sharing, so it cannot be moved away
    /// between writing and publishing; the file written through the handle is the one
    /// published, into the parent's handle.
    #[test]
    fn the_staging_folder_cannot_be_moved_away_before_publishing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        let staging = private_staging(&target).unwrap();
        let mut file = staging.create("new").unwrap();
        file.write_all(b"written through the handle").unwrap();
        drop(file);

        assert!(fs::rename(staging.path(), dir.path().join("moved")).is_err());
        staging.publish("new").unwrap();
        drop(staging);

        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "written through the handle"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    /// `config edit` on Windows: once released, the copy can be written in place and replaced
    /// by a rename-save, is read back, and can no longer be published; cleanup removes all.
    #[test]
    fn a_released_copy_belongs_to_the_editor() {
        let dir = tempfile::tempdir().unwrap();
        let staging = private_staging(&dir.path().join("config.toml")).unwrap();
        let mut copy = staging.create("config.toml").unwrap();
        copy.write_all(b"original").unwrap();
        drop(copy);
        staging.release("config.toml");
        let path = staging.path().join("config.toml");

        fs::write(&path, "in place").unwrap();
        let saved = staging.path().join("config.toml.tmp");
        fs::write(&saved, "renamed").unwrap();
        fs::rename(&saved, &path).unwrap();
        let mut read = String::new();
        std::io::Read::read_to_string(&mut staging.open("config.toml").unwrap(), &mut read)
            .unwrap();

        assert_eq!(read, "renamed");
        assert!(staging.publish("config.toml").is_err());
        drop(staging);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    /// A folder inside (an editor's backup folder) keeps the whole staging folder: nothing is
    /// removed by path.
    #[test]
    fn a_staging_folder_holding_a_folder_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let staging = private_staging(&dir.path().join("config.toml")).unwrap();
        let kept = staging.path().to_path_buf();
        fs::create_dir(kept.join("backups")).unwrap();

        drop(staging);

        assert!(kept.join("backups").is_dir());
    }

    /// `windows` reports Win32 errors as HRESULTs; matching on the Win32 code must still work.
    #[test]
    fn win32_errors_keep_their_code() {
        let invalid = ::windows::core::Error::from(::windows::core::HRESULT::from_win32(87));
        assert_eq!(code(&win(invalid)), Some(87));
        assert_eq!(
            code(&std::io::Error::from_raw_os_error(0x8007_00B7_u32 as i32)),
            Some(183)
        );
        assert_eq!(code(&std::io::Error::from_raw_os_error(183)), Some(183));
    }
}

const USER: &str = "S-1-5-21-1-2-3-1001";
const OTHER: &str = "S-1-5-21-1-2-3-1002";

fn ace(kind: u8, flags: u8, mask: u32, sid: Option<&str>) -> AceView {
    AceView {
        kind,
        flags,
        mask,
        sid: sid.map(str::to_string),
    }
}

/// Windows parent check: another account's create or delete-child right, in any allow form,
/// makes the folder unfit; deny entries, inherit-only entries and trusted principals do not.
#[test]
fn windows_grants_to_others_are_found_in_every_allow_form() {
    const ADD_FILE: u32 = 0x2;
    let grants = |aces: &[AceView]| acl::grants_others(Some(aces), USER, ADD_FILE);

    assert!(
        acl::grants_others(None, USER, ADD_FILE),
        "no DACL grants everyone"
    );
    assert!(grants(&[ace(
        acl::ACCESS_ALLOWED,
        0,
        ADD_FILE,
        Some(OTHER)
    )]));
    for conditional_or_object in [5, 9, 11, 13] {
        assert!(
            grants(&[ace(conditional_or_object, 0, ADD_FILE, None)]),
            "type {conditional_or_object}"
        );
    }
    assert!(!grants(&[ace(1, 0, ADD_FILE, None)]), "deny");
    assert!(
        !grants(&[ace(acl::ACCESS_ALLOWED, 0x8, ADD_FILE, Some(OTHER))]),
        "inherit only"
    );
    assert!(
        !grants(&[ace(acl::ACCESS_ALLOWED, 0, 0x1, Some(OTHER))]),
        "read only"
    );
    for trusted in [USER, "S-1-5-18", ADMINISTRATORS] {
        assert!(
            !grants(&[ace(acl::ACCESS_ALLOWED, 0, ADD_FILE, Some(trusted))]),
            "{trusted}"
        );
    }
}

/// Windows read-back: the staging folder must be protected with one full-access ACE that
/// files and folders inside inherit; a created file protected with one full-access ACE; an
/// editor's file only allow ACEs for the user.
#[test]
fn windows_read_back_accepts_only_the_owner_only_forms() {
    let own_folder = || ace(acl::ACCESS_ALLOWED, 0x3, FILE_ALL_ACCESS, Some(USER));
    let own_file = || ace(acl::ACCESS_ALLOWED, 0, FILE_ALL_ACCESS, Some(USER));
    let check = |protected, aces: &[AceView], expected| {
        acl::owner_only(USER, USER, protected, Some(aces), expected)
    };

    assert!(check(true, &[own_folder()], Expected::Folder));
    assert!(
        !check(false, &[own_folder()], Expected::Folder),
        "not protected"
    );
    assert!(
        !check(true, &[own_file()], Expected::Folder),
        "no inheritance"
    );
    assert!(
        !check(
            true,
            &[ace(acl::ACCESS_ALLOWED, 0x3, 0x1, Some(USER))],
            Expected::Folder
        ),
        "not full access"
    );
    assert!(
        !check(
            true,
            &[own_folder(), ace(acl::ACCESS_ALLOWED, 0, 0x1, Some(OTHER))],
            Expected::Folder
        ),
        "a second ACE"
    );
    assert!(check(true, &[own_file()], Expected::CreatedFile));
    assert!(
        !check(false, &[own_file()], Expected::CreatedFile),
        "not protected"
    );
    assert!(check(
        false,
        &[ace(acl::ACCESS_ALLOWED, 0x10, FILE_ALL_ACCESS, Some(USER))],
        Expected::EditorFile
    ));
    assert!(!check(false, &[], Expected::EditorFile), "empty DACL");
    assert!(
        !check(
            false,
            &[ace(acl::ACCESS_ALLOWED, 0x10, 0x1, Some(OTHER))],
            Expected::EditorFile
        ),
        "another account"
    );
    assert!(
        !acl::owner_only(USER, USER, true, None, Expected::CreatedFile),
        "no DACL"
    );
    assert!(
        !acl::owner_only(
            OTHER,
            USER,
            true,
            Some(&[own_file()]),
            Expected::CreatedFile
        ),
        "owner"
    );
    assert!(acl::owner_only(
        ADMINISTRATORS,
        USER,
        true,
        Some(&[own_file()]),
        Expected::CreatedFile
    ));
}
