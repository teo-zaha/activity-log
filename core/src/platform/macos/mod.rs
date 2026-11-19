use std::{fmt, process::Stdio};

use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSString;
use tokio::{io::AsyncWriteExt, process::Command};

#[cfg(test)]
mod tests;

const OUTLOOK_BUNDLE_ID: &str = "com.microsoft.Outlook";

pub fn outlook_is_installed() -> bool {
    let workspace = NSWorkspace::sharedWorkspace();
    let bundle_id = NSString::from_str(OUTLOOK_BUNDLE_ID);
    workspace
        .URLForApplicationWithBundleIdentifier(&bundle_id)
        .is_some()
}

/// Why a script did not run, in the script's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleScriptError {
    pub message: String,
    /// The script did not compile, for example because the app's dictionary lacks a term.
    pub syntax: bool,
}

impl fmt::Display for AppleScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppleScriptError {}

/// Runs `source` in its own `osascript` process, `args` passed to its `run` handler. The
/// collection waits without holding a runtime thread, and dropping the future (a timeout, a
/// stopped collection) kills the process. AppleScript run in-process would block the thread
/// it runs on until the app it talks to answers.
pub async fn run_apple_script(source: &str, args: &[&str]) -> Result<String, AppleScriptError> {
    let failed = |what: &str, err: std::io::Error| AppleScriptError {
        message: format!("{what}: {err}"),
        syntax: false,
    };
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| failed("Cannot start osascript", err))?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    // osascript reads the whole script before it runs it, so the output is read only after.
    stdin
        .write_all(source.as_bytes())
        .await
        .map_err(|err| failed("Cannot pass the script to osascript", err))?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .await
        .map_err(|err| failed("osascript failed", err))?;
    if output.status.success() {
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        // osascript ends its result with a line break.
        if text.ends_with('\n') {
            text.pop();
        }
        Ok(text)
    } else {
        Err(script_error(&String::from_utf8_lossy(&output.stderr)))
    }
}

/// osascript's error line, `12:40: execution error: Outlook not signed in (-2700)`, as the
/// message alone. Syntax errors are told apart by their codes (-2740, -2741), since their text
/// is in the user's language.
pub(crate) fn script_error(stderr: &str) -> AppleScriptError {
    let line = stderr.trim();
    let line = line
        .split_once(": ")
        .filter(|(range, _)| {
            range
                .split(':')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        })
        .map_or(line, |(_, rest)| rest);
    let (code, message) = match line.rsplit_once(" (") {
        Some((message, code)) if code.ends_with(')') => (Some(&code[..code.len() - 1]), message),
        _ => (None, line),
    };
    let message = ["execution error: ", "syntax error: "]
        .iter()
        .find_map(|prefix| message.strip_prefix(prefix))
        .unwrap_or(message)
        .trim();
    AppleScriptError {
        message: if message.is_empty() {
            "AppleScript failed without a message".to_string()
        } else {
            message.to_string()
        },
        syntax: matches!(code, Some("-2740" | "-2741")),
    }
}
