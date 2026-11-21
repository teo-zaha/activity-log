use super::{Editor, command_line_words};

#[test]
fn a_blank_variable_does_not_choose_the_editor() {
    let some = |value: &str| Some(value.to_string());
    assert_eq!(
        Editor::chosen(some("code --wait"), some("vim")).name(),
        "code --wait"
    );
    assert_eq!(Editor::chosen(some("  "), some("vim")).name(), "vim");
    assert_eq!(Editor::chosen(None, some("")), Editor::chosen(None, None));
    assert!(!Editor::chosen(None, None).name().is_empty());
}

#[test]
fn a_windows_command_line_keeps_backslashes_and_quoted_spaces() {
    assert_eq!(command_line_words("code --wait"), ["code", "--wait"]);
    assert_eq!(
        command_line_words(r"C:\Users\me\vim.exe"),
        [r"C:\Users\me\vim.exe"]
    );
    assert_eq!(
        command_line_words(r#""C:\Program Files\Notepad++\notepad++.exe" -multiInst  -nosession"#),
        [
            r"C:\Program Files\Notepad++\notepad++.exe",
            "-multiInst",
            "-nosession"
        ]
    );
    assert_eq!(command_line_words(r#"edit "" x"#), ["edit", "", "x"]);
}

/// The editor's own arguments come before the file, which arrives as one argument even with
/// spaces in its path or the editor's.
#[cfg(unix)]
#[test]
fn an_editor_with_arguments_gets_the_file_last() {
    use std::os::unix::fs::PermissionsExt;

    let folder = tempfile::tempdir().unwrap();
    let bin = folder.path().join("my editor");
    std::fs::create_dir(&bin).unwrap();
    let script = bin.join("edit.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/args\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = folder.path().join("a config.toml");

    let editor = format!("'{}' --wait -n", script.display());
    let status = Editor(editor).command().arg(&file).status().unwrap();

    assert!(status.success());
    assert_eq!(
        std::fs::read_to_string(bin.join("args")).unwrap(),
        format!("--wait\n-n\n{}\n", file.display())
    );
}
