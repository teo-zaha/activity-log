use std::time::{Duration, Instant};

use super::{AppleScriptError, run_apple_script, script_error};

/// Its error lines are parsed, so it runs in the C locale.
#[tokio::test]
async fn runs_in_the_c_locale() {
    let locale = run_apple_script("return system attribute \"LC_ALL\"", &[])
        .await
        .unwrap();

    assert_eq!(locale, "C");
}

#[tokio::test]
async fn passes_arguments_and_returns_the_text_result() {
    let result = run_apple_script(
        "on run argv\nreturn \"from \" & item 1 of argv & \" to \" & item 2 of argv\nend run",
        &["2026-03-02T08:00:00Z", "2026-03-03T08:00:00Z"],
    )
    .await
    .unwrap();

    assert_eq!(result, "from 2026-03-02T08:00:00Z to 2026-03-03T08:00:00Z");
}

#[tokio::test]
async fn a_script_error_is_its_own_message() {
    let err = run_apple_script("error \"Outlook not signed in\"", &[])
        .await
        .unwrap_err();

    assert_eq!(
        err,
        AppleScriptError {
            message: "Outlook not signed in".to_string(),
            code: Some(-2700),
        }
    );
}

#[tokio::test]
async fn a_script_that_does_not_compile_is_a_syntax_error() {
    let err = run_apple_script("set x to", &[]).await.unwrap_err();

    assert!(err.is_syntax(), "{err:?}");
}

/// A slow app must not hold the collection: the wait gives up its thread, and a timeout ends
/// it at once and kills the process.
#[tokio::test(flavor = "current_thread")]
async fn a_timeout_stops_a_running_script() {
    let started = Instant::now();
    let other_work = tokio::spawn(async { "other connections keep going" });

    let result = tokio::time::timeout(
        Duration::from_millis(300),
        run_apple_script("delay 30", &[]),
    )
    .await;

    assert!(result.is_err(), "the timeout fired");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(other_work.await.unwrap(), "other connections keep going");
}

/// macOS refusing Automation and an app that is not running are told apart by their numbers,
/// whatever language the message is in.
#[test]
fn an_error_number_names_the_problem() {
    use crate::provider::source_failure::SourceProblem;
    let denied = script_error("0:9: execution error: Nicht berechtigt. (-1743)");
    assert_eq!(denied.problem(), Some(SourceProblem::PermissionDenied));
    let closed = script_error(
        "0:9: execution error: Microsoft Outlook got an error: Application isn’t running. (-600)",
    );
    assert_eq!(closed.problem(), Some(SourceProblem::NotRunning));
    assert_eq!(
        script_error("0:9: execution error: Busy (-1712)").problem(),
        None
    );
    assert!(!denied.is_syntax());
}

#[test]
fn reads_osascript_error_lines() {
    assert_eq!(
        script_error("0:57: execution error: Outlook not signed in (-2700)\n"),
        AppleScriptError {
            message: "Outlook not signed in".to_string(),
            code: Some(-2700),
        }
    );
    assert_eq!(
        script_error("9:10: syntax error: Ausdruck erwartet, aber Zeilenende gefunden. (-2741)"),
        AppleScriptError {
            message: "Ausdruck erwartet, aber Zeilenende gefunden.".to_string(),
            code: Some(-2741),
        }
    );
    // A message with a colon of its own keeps it.
    assert_eq!(
        script_error("1:2: execution error: Outlook: busy (-1712)").message,
        "Outlook: busy"
    );
    assert_eq!(
        script_error("").message,
        "AppleScript failed without a message"
    );
}

/// Dropping the wait kills the process itself, not only the wait.
#[tokio::test]
async fn a_dropped_script_does_not_keep_running() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    let script = format!(
        "do shell script \"echo $PPID > '{}'\"\ndelay 30",
        pid_file.display()
    );

    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let run = run_apple_script(&script, &[]);
        tokio::pin!(run);
        // Until the script has written its process id, then drop it.
        loop {
            tokio::select! {
                _ = &mut run => panic!("the script ended by itself"),
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if std::fs::read_to_string(&pid_file).is_ok_and(|pid| !pid.trim().is_empty()) {
                        break;
                    }
                }
            }
        }
    })
    .await;
    assert!(result.is_ok(), "the script never started");
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    let started = Instant::now();
    // `kill -0` fails once the process is gone.
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "osascript {pid} still runs"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
