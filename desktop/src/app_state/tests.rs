use activity_log_core::diagnostics;

use super::AppState;

fn logged(needle: &str) -> bool {
    diagnostics::entries()
        .iter()
        .any(|entry| entry.scope == "config" && entry.message.contains(needle))
}

#[test]
fn config_problems_at_startup_reach_the_logs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[gitlab]]\nlabel = \"Startup\"\ntokn = \"x\"\n").unwrap();

    let state = AppState::start(Some(path));

    assert_eq!(state.host.providers_response().providers.len(), 1);
    assert!(logged("gitlab/startup: unknown setting `tokn`"));
}

/// B2: the error names the line and column, never the text on it.
#[test]
fn a_broken_config_at_startup_is_logged_without_its_text_and_starts_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[gitlab]]\ntoken = \"glpat-STARTUP-SECRET\n").unwrap();

    let state = AppState::start(Some(path));
    assert!(state.host.providers_response().providers.is_empty());
    let entries = diagnostics::entries();
    let logged = entries
        .iter()
        .find(|entry| {
            entry.scope == "settings" && entry.message.contains("Reading settings failed")
        })
        .expect("the error is logged");
    assert!(
        logged.message.contains("line 2, column"),
        "{}",
        logged.message
    );
    assert!(
        entries
            .iter()
            .all(|entry| !entry.message.contains("STARTUP-SECRET"))
    );
}
