use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Arc, Barrier},
    time::Duration,
};

use super::{EditError, Locked, TryLock, edit_config, edit_config_with, open_parent};

fn lock_with(path: &Path, try_lock: TryLock, wait: Duration) -> Result<Locked, EditError> {
    let (folder, path) = open_parent(path)?;
    super::lock_with(folder, path, try_lock, wait)
}
use crate::{
    model::{ConnectionId, Source},
    provider::{
        connection::ConnectionLocation,
        gitlab::{GITLAB_TOKEN, GITLAB_URL, GITLAB_USER},
        provider_config::ConfigSource,
    },
    settings::{ConfigFile, ConflictKind, FieldId, Precondition},
};

fn config(text: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, text).unwrap();
    (dir, path)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn load(path: &Path) -> ConfigFile {
    ConfigFile::load(path).unwrap()
}

fn id(text: &str) -> ConnectionId {
    text.parse().unwrap()
}

fn value(path: &Path, connection: &str, key: &str) -> Option<String> {
    load(path)
        .connections()
        .unwrap()
        .connections
        .into_iter()
        .find(|found| found.id == id(connection))
        .and_then(|found| found.values().get(key))
}

/// A base for changing `touched` of `connection`, as the file is now.
fn base(path: &Path, connection: &str, touched: &[FieldId]) -> Precondition {
    Precondition::Unchanged(load(path).edit_base(&id(connection), touched).unwrap())
}

/// Another cooperating writer.
fn other_writer(path: &Path, change: impl FnOnce(&mut ConfigFile) -> anyhow::Result<()>) {
    edit_config(path, Precondition::None, change).unwrap();
}

fn conflict(result: Result<impl Sized, EditError>) -> (ConflictKind, Vec<FieldId>) {
    match result {
        Err(EditError::Conflict(conflict)) => (conflict.kind, conflict.fields),
        Err(other) => panic!("expected a conflict, got {other}"),
        Ok(_) => panic!("expected a conflict, the edit went through"),
    }
}

const WORK: &str = "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\nurl = \"https://gitlab.example.com\"\ntoken = \"one\"\n";

#[test]
fn two_threads_editing_at_once_both_land() {
    let (_dir, path) = config(WORK);
    let barrier = Arc::new(Barrier::new(2));
    let threads = [
        std::thread::spawn({
            let (path, barrier) = (path.clone(), barrier.clone());
            move || {
                barrier.wait();
                edit_config(&path, Precondition::None, |file| {
                    file.add(&Source::Jira, Some("Jira"), ConnectionLocation::Local)
                })
                .unwrap();
            }
        }),
        std::thread::spawn({
            let (path, barrier) = (path.clone(), barrier.clone());
            move || {
                barrier.wait();
                edit_config(&path, Precondition::None, |file| {
                    file.rename(&id("gitlab/work"), "Company")
                })
                .unwrap();
            }
        }),
    ];
    for thread in threads {
        thread.join().unwrap();
    }

    let connections = load(&path).connections().unwrap().connections;
    let labels = connections
        .iter()
        .map(|connection| connection.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["Company", "Jira"]);
}

#[test]
fn a_held_lock_makes_a_writer_busy_and_leaves_the_file() {
    let (dir, path) = config(WORK);
    let held = File::create(dir.path().join("config.toml.lock")).unwrap();
    held.lock().unwrap();

    let busy = lock_with(
        &path,
        &|file: &File| file.try_lock(),
        Duration::from_millis(120),
    );

    assert!(matches!(busy, Err(EditError::Busy)));
    assert_eq!(
        EditError::Busy.to_string(),
        "The settings file is busy; try again."
    );
    assert_eq!(read(&path), WORK);
}

/// A file system without locks must not look like another writer.
#[test]
fn a_lock_error_other_than_contention_is_reported_as_such() {
    let (_dir, path) = config(WORK);
    let failed = lock_with(
        &path,
        &|_: &File| {
            Err(fs::TryLockError::Error(std::io::Error::other(
                "locks are not supported here",
            )))
        },
        Duration::from_secs(5),
    );

    match failed {
        Err(EditError::Io(err)) => {
            assert!(format!("{err:#}").contains("locks are not supported here"));
        }
        Err(other) => panic!("expected an I/O error, got {other}"),
        Ok(_) => panic!("expected an I/O error"),
    }
}

#[test]
fn a_rejected_change_writes_nothing() {
    let (_dir, path) = config(WORK);
    let rejected = edit_config(&path, Precondition::None, |file| {
        file.rename(&id("gitlab/nope"), "x")
    });

    assert!(matches!(rejected, Err(EditError::Rejected(_))));
    assert_eq!(read(&path), WORK);
}

#[test]
fn an_edit_of_one_legacy_connection_writes_an_instance_into_that_table_only() {
    let text = "# my settings\n[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\n\n[[gitlab]]\nname = \"oss\"\nlabel = \"OSS\" # public\n\n[[git]]\n";
    let (_dir, path) = config(text);

    let edited = edit_config(&path, Precondition::None, |file| {
        file.set(&id("gitlab/oss"), GITLAB_USER, Some("me"))
    })
    .unwrap();

    let written = read(&path);
    let instance = written
        .lines()
        .find_map(|line| line.strip_prefix("instance = "))
        .unwrap();
    assert_eq!(
        written,
        format!(
            "# my settings\n[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\n\n[[gitlab]]\nname = \"oss\"\nlabel = \"OSS\" # public\nuser = \"me\"\ninstance = {instance}\n\n[[git]]\n"
        )
    );
    assert!(edited.warnings.is_empty());
    assert_eq!(
        edited.content,
        crate::settings::ContentId::of(written.as_bytes())
    );

    // A second edit keeps the instance.
    edit_config(&path, Precondition::None, |file| {
        file.set(&id("gitlab/oss"), GITLAB_USER, Some("you"))
    })
    .unwrap();
    assert!(read(&path).contains(&format!("instance = {instance}")));
}

#[test]
fn a_malformed_or_repeated_instance_is_replaced_when_its_connection_is_edited() {
    let (_dir, path) = config(
        "[[gitlab]]\nname = \"a\"\ninstance = \"0123456789abcdef\"\n\n[[gitlab]]\nname = \"b\"\ninstance = \"0123456789abcdef\"\n\n[[gitlab]]\nname = \"c\"\ninstance = \"bad\"\n",
    );

    let edited = edit_config(&path, Precondition::None, |file| {
        file.rename(&id("gitlab/b"), "B")?;
        file.rename(&id("gitlab/c"), "C")
    })
    .unwrap();

    assert!(edited.warnings.is_empty(), "{:?}", edited.warnings);
    let written = read(&path);
    let instances = written
        .lines()
        .filter_map(|line| line.strip_prefix("instance = "))
        .collect::<Vec<_>>();
    assert_eq!(instances.len(), 3);
    assert_eq!(instances[0], "\"0123456789abcdef\"");
    assert!(instances[1] != instances[0] && instances[2] != "\"bad\"");
}

#[test]
fn an_unchanged_edit_writes_nothing() {
    let (_dir, path) = config(WORK);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    let modified = fs::metadata(&path).unwrap().modified().unwrap();

    // The label is already "Work": setting it again changes nothing.
    let edited = edit_config(&path, Precondition::None, |file| {
        file.rename(&id("gitlab/work"), "Work")
    })
    .unwrap();

    assert_eq!(read(&path), WORK);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    assert_eq!(
        edited.content,
        crate::settings::ContentId::of(WORK.as_bytes())
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "an unchanged file keeps its permissions");
    }

    // The next real write replaces it with an owner-only file.
    edit_config(&path, Precondition::None, |file| {
        file.rename(&id("gitlab/work"), "Company")
    })
    .unwrap();
    assert!(!crate::platform::private_file::readable_by_others(&path).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn a_missing_file_and_folder_are_created_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.toml");

    edit_config(&path, Precondition::None, |file| {
        file.add(&Source::Gitlab, Some("Work"), ConnectionLocation::Local)
    })
    .unwrap();

    assert!(read(&path).contains("label = \"Work\""));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(mode(&path), 0o600);
    }
    let mut names = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["config.toml", "config.toml.lock"]);
}

#[cfg(unix)]
#[test]
fn a_linked_config_keeps_its_link_and_a_dangling_link_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real.toml");
    fs::write(&target, WORK).unwrap();
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    edit_config(&link, Precondition::None, |file| {
        file.rename(&id("gitlab/work"), "Company")
    })
    .unwrap();

    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
    assert!(read(&target).contains("Company"));

    let dangling = dir.path().join("dangling.toml");
    std::os::unix::fs::symlink(dir.path().join("missing.toml"), &dangling).unwrap();
    let failed = edit_config(&dangling, Precondition::None, |file| {
        file.add(&Source::Git, None, ConnectionLocation::Local)
    });
    match failed {
        Err(EditError::Io(err)) => assert!(
            err.to_string()
                .contains("links to a file that does not exist")
        ),
        Err(other) => panic!("expected an I/O error, got {other}"),
        Ok(_) => panic!("a dangling link was written"),
    }
    assert!(!dir.path().join("missing.toml").exists());
}

#[cfg(unix)]
#[test]
fn a_folder_others_can_write_to_cannot_keep_the_settings() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("shared");
    fs::create_dir(&shared).unwrap();
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o1777)).unwrap();
    let path = shared.join("config.toml");

    let refused = edit_config(&path, Precondition::None, |file| {
        file.add(&Source::Git, None, ConnectionLocation::Local)
    });

    let Err(err @ EditError::Unprotectable { .. }) = refused else {
        panic!("expected the location to be refused");
    };
    assert!(
        err.to_string().ends_with(
            "cannot keep the settings private; choose another location with ACTIVITY_LOG_CONFIG or --config"
        ),
        "{err}"
    );
    assert!(!path.exists());
}

mod preconditions {
    use super::*;

    #[test]
    fn the_first_conditional_save_of_a_legacy_table_proceeds_and_assigns_its_instance() {
        let (_dir, path) = config(WORK);
        let base = base(&path, "gitlab/work", &[FieldId::Label]);

        edit_config(&path, base, |file| {
            file.rename(&id("gitlab/work"), "Company")
        })
        .unwrap();

        let written = read(&path);
        assert!(
            written.contains("Company") && written.contains("instance = "),
            "{written}"
        );
    }

    /// The re-added table got an instance from the other writer; the legacy base cannot tell
    /// it from the original, so the save is refused once, and goes through after reloading.
    #[test]
    fn a_same_value_re_add_before_the_first_save_is_a_replacement() {
        let (_dir, path) = config(WORK);
        let stale = base(&path, "gitlab/work", &[FieldId::Label]);
        other_writer(&path, |file| {
            file.remove(&id("gitlab/work"))?;
            let added = file.add(&Source::Gitlab, Some("Work"), ConnectionLocation::Local)?;
            file.set(&added, GITLAB_URL, Some("https://gitlab.example.com"))?;
            file.set(&added, GITLAB_TOKEN, Some("one"))
        });

        let refused = edit_config(&path, stale, |file| {
            file.rename(&id("gitlab/work"), "Company")
        });
        assert_eq!(conflict(refused), (ConflictKind::Replaced, Vec::new()));
        assert!(!read(&path).contains("Company"));

        let fresh = base(&path, "gitlab/work", &[FieldId::Label]);
        edit_config(&path, fresh, |file| {
            file.rename(&id("gitlab/work"), "Company")
        })
        .unwrap();
        assert!(read(&path).contains("Company"));
    }

    #[test]
    fn a_legacy_base_whose_touched_field_was_changed_is_a_change_of_that_field() {
        let (_dir, path) = config(WORK);
        let stale = base(&path, "gitlab/work", &[FieldId::Value(GITLAB_USER.name)]);
        // Outside the protocol, so the table stays without an instance.
        fs::write(&path, format!("{WORK}user = \"someone\"\n")).unwrap();

        let refused = edit_config(&path, stale, |file| {
            file.set(&id("gitlab/work"), GITLAB_USER, Some("me"))
        });

        assert_eq!(
            conflict(refused),
            (ConflictKind::Changed, vec![FieldId::Value("GITLAB_USER")])
        );
    }

    #[test]
    fn a_marked_base_and_a_same_value_re_add_is_a_replacement() {
        let (_dir, path) = config(WORK);
        other_writer(&path, |file| file.rename(&id("gitlab/work"), "Work 1"));
        let stale = base(&path, "gitlab/work", &[FieldId::Label]);
        other_writer(&path, |file| {
            file.remove(&id("gitlab/work"))?;
            let added = file.add(&Source::Gitlab, Some("Work"), ConnectionLocation::Local)?;
            file.set(&added, GITLAB_URL, Some("https://gitlab.example.com"))?;
            file.set(&added, GITLAB_TOKEN, Some("one"))
        });

        let refused = edit_config(&path, stale, |file| {
            file.rename(&id("gitlab/work"), "Company")
        });

        assert_eq!(conflict(refused), (ConflictKind::Replaced, Vec::new()));
    }

    /// The token is an account field: another token means another account, even for an edit
    /// that does not touch it. An unchanged token is not reported.
    #[test]
    fn a_token_changed_to_another_token_is_a_change_of_the_token() {
        let (_dir, path) = config(WORK);
        other_writer(&path, |file| {
            file.set(&id("gitlab/work"), GITLAB_USER, Some("me"))
        });
        let stale = base(&path, "gitlab/work", &[FieldId::Label]);
        other_writer(&path, |file| {
            file.set(&id("gitlab/work"), GITLAB_TOKEN, Some("two"))
        });

        let refused = edit_config(&path, stale, |file| {
            file.rename(&id("gitlab/work"), "Company")
        });

        assert_eq!(
            conflict(refused),
            (ConflictKind::Changed, vec![FieldId::Value("GITLAB_TOKEN")])
        );
    }

    #[test]
    fn two_label_edits_from_two_bases_conflict_on_the_label() {
        let (_dir, path) = config(WORK);
        let first = base(&path, "gitlab/work", &[FieldId::Label]);
        let second = base(&path, "gitlab/work", &[FieldId::Label]);

        edit_config(&path, first, |file| {
            file.rename(&id("gitlab/work"), "First")
        })
        .unwrap();
        let refused = edit_config(&path, second, |file| {
            file.rename(&id("gitlab/work"), "Second")
        });

        let (kind, fields) = conflict(refused);
        // The first save gave the legacy table its instance.
        assert_eq!((kind, fields), (ConflictKind::Replaced, Vec::new()));
        assert!(read(&path).contains("First"));

        let third = base(&path, "gitlab/work", &[FieldId::Label]);
        let fourth = base(&path, "gitlab/work", &[FieldId::Label]);
        edit_config(&path, third, |file| {
            file.rename(&id("gitlab/work"), "Third")
        })
        .unwrap();
        let refused = edit_config(&path, fourth, |file| {
            file.rename(&id("gitlab/work"), "Fourth")
        });
        assert_eq!(
            conflict(refused),
            (ConflictKind::Changed, vec![FieldId::Label])
        );
        assert!(read(&path).contains("Third"));
    }

    #[test]
    fn a_stale_remove_after_a_re_add_removes_nothing() {
        let (_dir, path) = config(WORK);
        other_writer(&path, |file| file.rename(&id("gitlab/work"), "Work 1"));
        let stale = base(&path, "gitlab/work", &[]);
        other_writer(&path, |file| {
            file.remove(&id("gitlab/work"))?;
            file.add(&Source::Gitlab, Some("Work"), ConnectionLocation::Local)
                .map(drop)
        });

        let refused = edit_config(&path, stale, |file| file.remove(&id("gitlab/work")));

        assert_eq!(conflict(refused), (ConflictKind::Replaced, Vec::new()));
        assert_eq!(value(&path, "gitlab/work", "GITLAB_URL"), None);
        assert!(read(&path).contains("[[gitlab]]"));
    }

    #[test]
    fn a_removed_connection_is_reported_as_removed() {
        let (_dir, path) = config(WORK);
        let stale = base(&path, "gitlab/work", &[FieldId::Label]);
        other_writer(&path, |file| file.remove(&id("gitlab/work")));

        let refused = edit_config(&path, stale, |file| {
            file.rename(&id("gitlab/work"), "Company")
        });

        let (kind, _) = conflict(refused);
        assert_eq!(kind, ConflictKind::Removed);
    }

    #[test]
    fn an_untouched_field_that_is_no_account_field_merges() {
        let (_dir, path) = config(WORK);
        let stale = base(&path, "gitlab/work", &[FieldId::Label]);
        // Outside the protocol, so only the value differs.
        fs::write(&path, format!("{WORK}user = \"someone\"\n")).unwrap();

        edit_config(&path, stale, |file| {
            file.rename(&id("gitlab/work"), "Company")
        })
        .unwrap();

        assert_eq!(
            value(&path, "gitlab/work", "GITLAB_USER").as_deref(),
            Some("someone")
        );
        assert!(read(&path).contains("Company"));
    }

    #[test]
    fn a_git_connection_removed_and_added_again_is_a_replacement() {
        let (_dir, path) = config("[[git]]\n");
        other_writer(&path, |file| {
            file.rename(&id("git/this-computer"), "This computer")
        });
        let stale = base(&path, "git/this-computer", &[FieldId::Label]);
        other_writer(&path, |file| {
            file.remove(&id("git/this-computer"))?;
            file.add(&Source::Git, None, ConnectionLocation::Local)
                .map(drop)
        });

        let refused = edit_config(&path, stale, |file| {
            file.rename(&id("git/this-computer"), "Laptop")
        });

        assert_eq!(conflict(refused), (ConflictKind::Replaced, Vec::new()));
    }

    #[test]
    fn conflict_messages_name_the_connection_without_values() {
        let (_dir, path) = config(WORK);
        let stale = base(&path, "gitlab/work", &[FieldId::Label]);
        other_writer(&path, |file| {
            file.set(
                &id("gitlab/work"),
                GITLAB_URL,
                Some("https://other.example.com"),
            )
        });
        let Err(err) = edit_config(&path, stale, |file| file.rename(&id("gitlab/work"), "x"))
        else {
            panic!("expected a conflict");
        };
        // The edit gave the table its instance, so the legacy base no longer matches.
        assert_eq!(
            err.message(Some("Work")),
            "Work was removed or replaced by another program. Nothing was saved."
        );

        let stale = base(&path, "gitlab/work", &[FieldId::Label]);
        other_writer(&path, |file| {
            file.set(
                &id("gitlab/work"),
                GITLAB_URL,
                Some("https://third.example.com"),
            )
        });
        let Err(err) = edit_config(&path, stale, |file| file.rename(&id("gitlab/work"), "x"))
        else {
            panic!("expected a conflict");
        };
        let message = err.message(Some("Work"));
        assert_eq!(
            message,
            "Work was changed by another program while you were editing it. Nothing was saved."
        );
        assert!(!format!("{err:?}").contains("example.com"));
    }

    /// `edit_config_with` is what `edit_config` runs once it holds the lock.
    #[test]
    fn the_precondition_is_checked_on_the_file_read_under_the_lock() {
        let (_dir, path) = config(WORK);
        let stale = base(&path, "gitlab/work", &[FieldId::Value(GITLAB_URL.name)]);
        let locked = super::super::lock(&path).unwrap();
        fs::write(
            &path,
            WORK.replace("gitlab.example.com", "moved.example.com"),
        )
        .unwrap();

        let refused = edit_config_with(locked, stale, |file| {
            file.set(
                &id("gitlab/work"),
                GITLAB_URL,
                Some("https://mine.example.com"),
            )
        });

        assert_eq!(
            conflict(refused),
            (ConflictKind::Changed, vec![FieldId::Value("GITLAB_URL")])
        );
    }
}

mod review {
    use super::*;

    /// Review 5: the skipped second Outlook table becomes visible under the removed one's id;
    /// a stale base of the removed one must not edit it.
    #[test]
    fn a_stale_save_never_edits_the_table_that_took_a_removed_ones_place() {
        let (_dir, path) = config(
            "[[outlook]]\nname = \"outlook\"\nlabel = \"Mail\"\n\n[[outlook]]\nname = \"outlook\"\nlabel = \"Mail\"\n",
        );
        let stale = base(&path, "outlook/outlook", &[FieldId::Label]);
        other_writer(&path, |file| file.remove(&id("outlook/outlook")));
        assert!(load(&path).edit_base(&id("outlook/outlook"), &[]).is_some());

        let refused = edit_config(&path, stale, |file| {
            file.rename(&id("outlook/outlook"), "Other")
        });

        assert_eq!(conflict(refused), (ConflictKind::Replaced, Vec::new()));
        assert!(!read(&path).contains("Other"));
    }

    /// Review 5: a repeated instance becomes readable once its first holder is removed; it is
    /// the removed connection's lifetime, not the remaining one's.
    #[test]
    fn a_repeated_instance_that_becomes_unique_is_replaced() {
        let (_dir, path) = config(
            "[[outlook]]\nname = \"outlook\"\ninstance = \"00000000000000aa\"\n\n[[outlook]]\nname = \"outlook\"\ninstance = \"00000000000000aa\"\n",
        );
        let stale = base(&path, "outlook/outlook", &[FieldId::Label]);
        other_writer(&path, |file| file.remove(&id("outlook/outlook")));

        let refused = edit_config(&path, stale, |file| {
            file.rename(&id("outlook/outlook"), "Other")
        });

        assert_eq!(conflict(refused), (ConflictKind::Replaced, Vec::new()));
    }

    /// Review 7: a save that changes nothing keeps the file byte for byte, line endings,
    /// quoting and comments included.
    #[test]
    fn a_same_value_save_keeps_the_file_as_written() {
        for text in [
            "[[gitlab]]\r\nname = \"work\"\r\nlabel = \"Work\"\r\nuser = \"me\"\r\n",
            "[[gitlab]]\nname = \"work\"\nlabel = 'Work' # preserve\nuser = 'me' # mine\n",
        ] {
            let (_dir, path) = config(text);
            let modified = fs::metadata(&path).unwrap().modified().unwrap();

            edit_config(&path, Precondition::None, |file| {
                file.rename(&id("gitlab/work"), "Work")?;
                file.set(&id("gitlab/work"), GITLAB_USER, Some("me"))?;
                file.set(&id("gitlab/work"), GITLAB_TOKEN, None)
            })
            .unwrap();

            assert_eq!(read(&path), text);
            assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        }
    }

    /// Review 7: a real change keeps the comment of the value it replaces and the file's line
    /// endings.
    #[test]
    fn a_change_keeps_comments_and_line_endings() {
        let (_dir, path) = config("[[gitlab]]\r\nname = \"work\"\r\nlabel = 'Work' # preserve\r\n");

        edit_config(&path, Precondition::None, |file| {
            file.rename(&id("gitlab/work"), "Company")
        })
        .unwrap();

        let written = read(&path);
        assert!(
            written.contains("label = \"Company\" # preserve\r\n"),
            "{written:?}"
        );
        assert!(!written.replace("\r\n", "").contains('\n'), "{written:?}");
    }

    /// Review, other defects: removing one connection leaves an unambiguous sibling as written.
    #[test]
    fn a_remove_leaves_siblings_whose_ids_do_not_move() {
        let (_dir, path) = config("[[gitlab]]\nlabel = \"One\"\n\n[[gitlab]]\nlabel = \"Two\"\n");

        edit_config(&path, Precondition::None, |file| {
            file.remove(&id("gitlab/one"))
        })
        .unwrap();

        // Only the removed table's text goes; the blank line before the next one stays.
        assert_eq!(read(&path), "\n[[gitlab]]\nlabel = \"Two\"\n");
    }

    /// The narrower case: a sibling whose generated id would move keeps it.
    #[test]
    fn a_remove_keeps_a_sibling_id_that_would_move() {
        let (_dir, path) = config(
            "[[gitlab]]\nlabel = \"Work\"\n\n[[gitlab]]\nlabel = \"Work\"\n\n[[gitlab]]\nlabel = \"Work\"\n",
        );

        edit_config(&path, Precondition::None, |file| {
            file.remove(&id("gitlab/work"))
        })
        .unwrap();

        let ids = load(&path)
            .connections()
            .unwrap()
            .connections
            .into_iter()
            .map(|connection| connection.id.to_string())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["gitlab/work-2", "gitlab/work-3"]);
    }

    /// Review, other defects: duplicates are decided by the order of the tables in the file.
    #[test]
    fn a_repeated_instance_belongs_to_the_earlier_table_in_the_file() {
        let resolved = ConfigFile::parse(
            "config.toml",
            b"[[gitlab]]\nname = \"a\"\n\n[[jira]]\nname = \"b\"\ninstance = \"00000000000000aa\"\n\n[[gitlab]]\nname = \"c\"\ninstance = \"00000000000000aa\"\n",
        )
        .unwrap()
        .connections()
        .unwrap();

        assert_eq!(
            resolved.warnings,
            [
                "gitlab/c: `instance` repeats another connection's; it gets a new one when Activity Log next saves this connection"
            ]
        );
    }

    #[test]
    fn the_lock_wait_can_only_be_shortened() {
        assert_eq!(
            super::super::lock_wait_from(Some("200")),
            Duration::from_millis(200)
        );
        assert_eq!(
            super::super::lock_wait_from(Some("60000")),
            Duration::from_secs(5)
        );
        assert_eq!(
            super::super::lock_wait_from(Some("soon")),
            Duration::from_secs(5)
        );
        assert_eq!(super::super::lock_wait_from(None), Duration::from_secs(5));
    }

    /// Review 1: the folder is bound when the lock is taken; moving it away during the save
    /// does not send the new file to whatever is at its old path.
    #[cfg(unix)]
    #[test]
    fn a_save_publishes_into_the_folder_it_locked_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("settings");
        fs::create_dir(&first).unwrap();
        let path = first.join("config.toml");
        fs::write(&path, WORK).unwrap();
        let locked = super::super::lock(&path).unwrap();
        let moved = dir.path().join("moved");
        fs::rename(&first, &moved).unwrap();
        fs::create_dir(&first).unwrap();
        fs::write(&path, "[[git]]\n").unwrap();

        edit_config_with(locked, Precondition::None, |file| {
            file.rename(&id("gitlab/work"), "Company")
        })
        .unwrap();

        assert_eq!(read(&path), "[[git]]\n");
        assert!(read(&moved.join("config.toml")).contains("Company"));
    }
}

mod recheck {
    use super::*;

    /// Re-check 5: a same-value save on a table whose name is only inferred keeps the file.
    #[test]
    fn a_same_value_save_of_an_inferred_name_keeps_the_file() {
        let text = "[[gitlab]]\r\nlabel = 'Work' # mine\r\nuser = \"me\"\r\n\r\n[[git]]\r\n";
        let (_dir, path) = config(text);
        #[cfg(unix)]
        let inode = std::os::unix::fs::MetadataExt::ino(&fs::metadata(&path).unwrap());

        edit_config(&path, Precondition::None, |file| {
            file.rename(&id("gitlab/work"), "Work")?;
            file.set(&id("gitlab/work"), GITLAB_USER, Some("me"))?;
            file.set(&id("gitlab/work"), GITLAB_TOKEN, None)?;
            file.rename(&id("git/this-computer"), "This computer")
        })
        .unwrap();

        assert_eq!(read(&path), text);
        #[cfg(unix)]
        assert_eq!(
            std::os::unix::fs::MetadataExt::ino(&fs::metadata(&path).unwrap()),
            inode
        );
    }

    /// Re-check 4: an `instance` that is a table (dotted keys) is unreadable; editing the
    /// connection replaces it with a real one, so a base from before is stale.
    #[test]
    fn a_dotted_instance_is_replaced_by_an_edit() {
        let (_dir, path) = config("[[gitlab]]\nname = \"work\"\ninstance.key = 7\n");
        let resolved = load(&path).connections().unwrap();
        assert_eq!(
            resolved.warnings,
            [
                "gitlab/work: `instance` is not 16 lowercase hex digits; it gets a new one when Activity Log next saves this connection"
            ]
        );
        let stale = base(&path, "gitlab/work", &[FieldId::Label]);

        other_writer(&path, |file| {
            file.set(&id("gitlab/work"), GITLAB_USER, Some("me"))
        });

        let written = read(&path);
        assert!(!written.contains("key = 7"), "{written}");
        assert!(
            load(&path).connections().unwrap().warnings.is_empty(),
            "{written}"
        );
        let refused = edit_config(&path, stale, |file| file.rename(&id("gitlab/work"), "x"));
        assert_eq!(conflict(refused), (ConflictKind::Replaced, Vec::new()));
    }
}
