//! The user's editor (`$VISUAL`, `$EDITOR`) as a command. The value may carry arguments
//! (`code --wait`); it is read as Git reads it: by `sh` on Unix, and on Windows as a program
//! path when it names a file, else as a command line split at spaces outside double quotes.

use std::process::Command;

use crate::platform::private_file::private_command;

#[cfg(test)]
mod tests;

/// The editor `config edit` runs: `$VISUAL`, else `$EDITOR`, else the system's own (`vi`,
/// `notepad`); a variable that is set but blank does not count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor(String);

impl Editor {
    pub fn from_env() -> Self {
        let chosen = |name| std::env::var(name).ok();
        Self::chosen(chosen("VISUAL"), chosen("EDITOR"))
    }

    fn chosen(visual: Option<String>, editor: Option<String>) -> Self {
        let fallback = if cfg!(target_os = "windows") {
            "notepad"
        } else {
            "vi"
        };
        Self(
            [visual, editor]
                .into_iter()
                .flatten()
                .find(|value| !value.trim().is_empty())
                .unwrap_or_else(|| fallback.to_string()),
        )
    }

    /// The value, for messages.
    pub fn name(&self) -> &str {
        &self.0
    }

    /// The command that runs the editor on the paths its caller adds as arguments; whatever
    /// the editor creates beside the file (backups, swap files) is owner-only too.
    pub fn command(&self) -> Command {
        command_impl(&self.0)
    }
}

#[cfg(unix)]
fn command_impl(editor: &str) -> Command {
    // `sh -c '<editor> "$@"' <editor> <file>`: the editor's words as the shell reads them, and
    // the file as one argument whatever it holds.
    let mut command = private_command(std::ffi::OsStr::new("sh"));
    command
        .arg("-c")
        .arg(format!("{editor} \"$@\""))
        .arg(editor);
    command
}

#[cfg(windows)]
fn command_impl(editor: &str) -> Command {
    // `C:\Program Files\Sublime Text\subl.exe` names a file as it is, spaces and all.
    if std::path::Path::new(editor).is_file() {
        return private_command(std::ffi::OsStr::new(editor));
    }
    let words = command_line_words(editor);
    let (program, arguments) = words
        .split_first()
        .expect("an editor is never blank, so it has a word");
    let mut command = private_command(std::ffi::OsStr::new(program));
    command.args(arguments);
    command
}

/// The words of a Windows command line: split at whitespace outside double quotes, the quotes
/// dropped, backslashes kept as they are (they separate folders).
#[cfg(any(windows, test))]
fn command_line_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                word.get_or_insert_default();
            }
            space if space.is_whitespace() && !quoted => words.extend(word.take()),
            other => word.get_or_insert_default().push(other),
        }
    }
    words.extend(word);
    words
}
