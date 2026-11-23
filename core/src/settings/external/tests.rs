use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{ExternalEdit, ExternalEditError, edit_externally};
use crate::settings::{ConfigFile, ConflictKind, EditError, FieldId, Precondition, edit_config};

fn folder_names(folder: &Path) -> Vec<String> {
    let mut names = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// An editor that replaces the copy's text with `text`.
fn writes(text: &'static str) -> impl FnOnce(&Path) -> anyhow::Result<()> {
    move |copy| Ok(fs::write(copy, text)?)
}

fn instance_lines(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("instance = "))
        .map(str::to_string)
        .collect()
}

#[test]
fn closing_the_editor_without_a_change_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[[git]]\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let mut seen = PathBuf::new();

    let outcome = edit_externally(
        &path,
        |copy| {
            seen = copy.to_path_buf();
            assert_eq!(fs::read_to_string(copy).unwrap(), "[[git]]\n");
            Ok(())
        },
        |_: &crate::settings::ConfigFile| {
            Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                "no store in tests"
            )))
        },
    )
    .unwrap();

    assert!(matches!(outcome, ExternalEdit::Unchanged));
    assert_ne!(seen, path, "the editor never opens the real file");
    assert_eq!(fs::read_to_string(&path).unwrap(), "[[git]]\n");
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }
    assert_eq!(folder_names(dir.path()), ["config.toml"]);

    let missing = dir.path().join("missing.toml");
    let outcome = edit_externally(
        &missing,
        |_| Ok(()),
        |_: &crate::settings::ConfigFile| {
            Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                "no store in tests"
            )))
        },
    )
    .unwrap();
    assert!(matches!(outcome, ExternalEdit::Unchanged));
    assert!(!missing.exists());
    assert_eq!(folder_names(dir.path()), ["config.toml"]);
}

#[test]
fn instances_follow_the_tables_the_user_changed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let original = "# mine\n[[gitlab]]\nname = \"marked\"\ninstance = \"00000000000000aa\"\n\n[[gitlab]]\nname = \"legacy\"\nlabel = \"Legacy\"\n\n[[gitlab]]\nname = \"edited\"\nlabel = \"Edited\"\n";
    fs::write(&path, original).unwrap();

    edit_externally(
        &path,
        writes(
            "# mine\n[[gitlab]]\nname = \"marked\"\ninstance = \"00000000000000aa\"\n\n[[gitlab]]\nname = \"legacy\"\nlabel   =   \"Legacy\" # same value\n\n[[gitlab]]\nname = \"edited\"\nlabel = \"Edited again\"\n\n[[gitlab]]\nname = \"pasted\"\ninstance = \"00000000000000aa\"\n",
        ), |_: &crate::settings::ConfigFile| Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!("no store in tests"))),
    )
    .unwrap();

    let written = fs::read_to_string(&path).unwrap();
    let lines = instance_lines(&written);
    // `marked` and `pasted` both carry aa, so neither keeps it; `edited` gets its first one.
    assert_eq!(lines.len(), 3, "{written}");
    assert!(
        lines.iter().all(|line| line != "\"00000000000000aa\""),
        "{written}"
    );
    assert!(
        written.contains(
            "name = \"legacy\"\nlabel   =   \"Legacy\" # same value\n\n[[gitlab]]\ninstance = "
        ),
        "an unchanged legacy table stays without one, as written: {written}"
    );
    assert!(written.starts_with("# mine\n[[gitlab]]\nname = \"marked\"\ninstance = "));
}

#[test]
fn a_deleted_instance_line_makes_a_new_lifetime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\ninstance = \"00000000000000aa\"\n",
    )
    .unwrap();
    let work = "gitlab/work".parse().unwrap();
    let old = ConfigFile::load(&path)
        .unwrap()
        .edit_base(&work, &[FieldId::Label])
        .unwrap();

    edit_externally(
        &path,
        writes("[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\n"),
        |_: &crate::settings::ConfigFile| {
            Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                "no store in tests"
            )))
        },
    )
    .unwrap();

    let written = fs::read_to_string(&path).unwrap();
    let lines = instance_lines(&written);
    assert_eq!(lines.len(), 1);
    assert_ne!(lines[0], "\"00000000000000aa\"");
    let refused = edit_config(&path, Precondition::Unchanged(old), None, |file| {
        file.rename(&work, "Company")
    });
    assert!(matches!(
        refused,
        Err(EditError::Conflict(conflict)) if conflict.kind == ConflictKind::Replaced
    ));
}

#[test]
fn a_file_changed_during_the_edit_keeps_the_copy_and_publishes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[[git]]\n").unwrap();

    let kept = edit_externally(
        &path,
        |copy| {
            fs::write(&path, "[[system]]\n")?;
            Ok(fs::write(copy, "[[jira]]\n")?)
        },
        |_: &crate::settings::ConfigFile| {
            Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                "no store in tests"
            )))
        },
    )
    .unwrap_err();

    let message = kept.to_string();
    let source =
        std::error::Error::source(&kept).and_then(|cause| cause.downcast_ref::<EditError>());
    assert!(
        matches!(source, Some(EditError::FileChanged { .. })),
        "{kept:?}"
    );
    let ExternalEditError::Kept { copy, cause } = kept else {
        panic!("expected the copy to be kept");
    };
    assert!(matches!(cause, EditError::FileChanged { .. }), "{cause:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "[[system]]\n");
    assert_eq!(fs::read_to_string(&copy).unwrap(), "[[jira]]\n");
    assert_eq!(
        message,
        format!(
            "config.toml changed while you were editing; your version is in {}",
            copy.display()
        )
    );
}

#[test]
fn an_invalid_copy_is_kept_with_a_redacted_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[[git]]\n").unwrap();

    let kept = edit_externally(
        &path,
        writes("[[gitlab]]\ntoken = \"glpat-SECRET\n"),
        |_: &crate::settings::ConfigFile| {
            Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                "no store in tests"
            )))
        },
    )
    .unwrap_err();

    let message = kept.to_string();
    let ExternalEditError::Kept { copy, cause } = kept else {
        panic!("expected the copy to be kept");
    };
    assert!(matches!(cause, EditError::Invalid(_)), "{cause:?}");
    // What the terminal shows names the place and never the file's text.
    assert!(message.contains("line 2, column"), "{message}");
    assert!(!message.contains("SECRET"), "{message}");
    assert!(copy.exists());
    assert_eq!(fs::read_to_string(&path).unwrap(), "[[git]]\n");
}

/// Editors such as vim may save by writing a new file and renaming it over the copy; the
/// published file is still a new owner-only one.
#[test]
fn an_editor_that_saves_by_rename_does_not_change_the_protection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    edit_externally(
        &path,
        |copy| {
            let fresh = copy.with_file_name("4913");
            fs::write(&fresh, "[[git]]\n")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&fresh, fs::Permissions::from_mode(0o644))?;
            }
            Ok(fs::rename(&fresh, copy)?)
        },
        |_: &crate::settings::ConfigFile| {
            Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                "no store in tests"
            )))
        },
    )
    .unwrap();

    // A table new in the edited text gets an instance.
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.starts_with("[[git]]\ninstance = "), "{written}");
    assert!(!crate::platform::private_file::readable_by_others(&path).unwrap());
    assert_eq!(
        folder_names(dir.path()),
        ["config.toml", "config.toml.lock"]
    );
}

/// Review 6: `config edit` repairs a file that does not parse.
#[test]
fn a_broken_file_can_be_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[[gitlab]\nlabel = \"Work\"\n").unwrap();

    edit_externally(
        &path,
        writes("[[gitlab]]\nlabel = \"Work\"\n"),
        |_: &crate::settings::ConfigFile| {
            Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                "no store in tests"
            )))
        },
    )
    .unwrap();

    let written = fs::read_to_string(&path).unwrap();
    assert!(written.starts_with("[[gitlab]]\ninstance = "), "{written}");
    assert!(written.ends_with("\nlabel = \"Work\"\n"), "{written}");
}

/// Review 7: an edit that needs no instance change is published as the user wrote it.
#[test]
fn a_comment_only_edit_is_published_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "[[gitlab]]\r\nname = \"work\"\r\ninstance = \"00000000000000aa\"\r\n",
    )
    .unwrap();
    let edited = "# mine\r\n[[gitlab]]\r\nname = \"work\"\r\ninstance = \"00000000000000aa\"\r\n";

    edit_externally(
        &path,
        move |copy| Ok(fs::write(copy, edited)?),
        |_: &crate::settings::ConfigFile| {
            Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                "no store in tests"
            )))
        },
    )
    .unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), edited);
}

/// Review 7: instance changes are spliced into the user's text; nothing else moves.
#[test]
fn instance_changes_keep_the_rest_of_the_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[[git]]\r\n").unwrap();

    edit_externally(
        &path,
        writes("[[git]]\r\n\r\n[[gitlab]] # new\r\nlabel = 'Work'   # mine\r\n"),
        |_: &crate::settings::ConfigFile| {
            Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                "no store in tests"
            )))
        },
    )
    .unwrap();

    let written = fs::read_to_string(&path).unwrap();
    let instance = instance_lines(&written).pop().unwrap();
    assert_eq!(
        written,
        format!(
            "[[git]]\r\n\r\n[[gitlab]] # new\r\ninstance = {}\r\nlabel = 'Work'   # mine\r\n",
            instance.trim_end_matches('\r')
        )
    );
}

/// Review, other defects: a value's type counts, its formatting does not, and an untouched
/// table keeps even a malformed instance line.
#[test]
fn legacy_tables_are_compared_by_typed_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "[[jira]]\nname = \"a\"\nextra = 1\n\n[[gitlab]]\nname = \"b\"\nextra = [ 1,2 ]\ninstance = \"bad\"\n",
    )
    .unwrap();

    edit_externally(
        &path,
        writes("[[jira]]\nname = \"a\"\nextra = '1'\n\n[[gitlab]]\nname = \"b\"\nextra = [1, 2]\ninstance = \"bad\"\n"), |_: &crate::settings::ConfigFile| Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!("no store in tests"))),
    )
    .unwrap();

    let written = fs::read_to_string(&path).unwrap();
    assert!(written.contains("instance = \"bad\""), "{written}");
    assert_eq!(instance_lines(&written).len(), 2, "{written}");
}

/// Re-check 4: a dotted or quoted-dotted `instance` is never spliced as if it were a value;
/// the table gets a real instance, which a base from before no longer matches.
#[test]
fn a_dotted_instance_is_replaced_not_rewritten_into_a_key() {
    for marker in ["instance.key = 7", "\"instance\".y = 'x'"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = format!("[[gitlab]]\nname = \"work\"\n{marker}\n");
        fs::write(&path, &original).unwrap();
        let work = "gitlab/work".parse().unwrap();
        let stale = ConfigFile::load(&path)
            .unwrap()
            .edit_base(&work, &[FieldId::Label])
            .unwrap();
        let edited = format!("[[gitlab]]\nname = \"work\"\nuser = \"me\"\n{marker}\n");

        edit_externally(
            &path,
            move |copy| Ok(fs::write(copy, edited)?),
            |_: &crate::settings::ConfigFile| {
                Err(crate::secrets::StoreError::unavailable(anyhow::anyhow!(
                    "no store in tests"
                )))
            },
        )
        .unwrap();

        let written = fs::read_to_string(&path).unwrap();
        let file = ConfigFile::load(&path).unwrap();
        assert!(file.connections().warnings.is_empty(), "{written}");
        assert_eq!(instance_lines(&written).len(), 1, "{written}");
        let refused = edit_config(&path, Precondition::Unchanged(stale), None, |file| {
            file.rename(&work, "Company")
        });
        assert!(
            matches!(&refused, Err(EditError::Conflict(conflict)) if conflict.kind == ConflictKind::Replaced),
            "{marker}: {written}"
        );
    }
}

const TYPED_TOKEN: &str = "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\nurl = \"https://gitlab.example.test\"\ntoken = \"glpat-test-0000\"\n";

/// A token typed into the file is sealed with the key the sealer gives, asked only then.
#[test]
fn a_typed_token_is_sealed_with_the_key() {
    use crate::secrets::{DataKey, KEY_LEN, Sealer, StoreKind};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "").unwrap();
    let mut asked = 0;
    let outcome = edit_externally(&path, writes(TYPED_TOKEN), |_| {
        asked += 1;
        Ok(Sealer::for_tests(
            StoreKind::Os,
            DataKey::from_slice(&[1; KEY_LEN]).unwrap(),
        ))
    })
    .unwrap();
    assert!(matches!(outcome, ExternalEdit::Saved { .. }));
    assert_eq!(asked, 1);
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("glpat-test-0000"), "{text}");
    assert!(text.contains("token = { sealed = \"v1:os:"), "{text}");
}

/// A store that gives no key never fails the edit: the token is saved as typed, and a warning
/// says so.
#[test]
fn a_typed_token_without_a_key_is_saved_as_typed_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "").unwrap();
    let outcome = edit_externally(&path, writes(TYPED_TOKEN), |_| {
        Err(crate::secrets::StoreError::refused(anyhow::anyhow!(
            "interaction not allowed"
        )))
    })
    .unwrap();
    let ExternalEdit::Saved { warnings } = outcome else {
        panic!("not saved");
    };
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("glpat-test-0000")
    );
}

/// An edit that types no new secret asks for no key.
#[test]
fn an_edit_without_a_new_secret_asks_no_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, TYPED_TOKEN).unwrap();
    let relabelled = "[[gitlab]]\nname = \"work\"\nlabel = \"Company\"\nurl = \"https://gitlab.example.test\"\ntoken = \"glpat-test-0000\"\n";
    edit_externally(&path, writes(relabelled), |_| {
        panic!("no key is needed for a label")
    })
    .unwrap();
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("glpat-test-0000")
    );
}
