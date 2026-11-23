//! A system notification when a long collection ends while the window is in the background
//! (native-feel decision 8). The host's spawner only sends each run's end here; `follow` decides
//! with the pure `notice_for`, and asks whether the run is still the one on screen last, just
//! before it shows anything. Rust decides, not the dashboard: a hidden web view's timers are
//! held back, which is when this matters.

use std::time::Duration;

use activity_log_core::{
    diagnostics,
    model::{RunEnd, RunId, RunOutcome},
};
use tokio::sync::mpsc::UnboundedReceiver;

#[cfg(test)]
mod tests;

/// A run shorter than this was waited for, not left in the background.
pub(crate) const THRESHOLD: Duration = Duration::from_secs(20);

/// What the notification says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notice {
    pub(crate) title: &'static str,
    pub(crate) body: String,
}

/// The notice a run that ended so deserves if nobody watched it; none for a short run or one
/// that was stopped (a newer run or shutdown replaced it).
pub(crate) fn notice_for(end: &RunEnd) -> Option<Notice> {
    if end.elapsed < THRESHOLD {
        return None;
    }
    match end.outcome {
        RunOutcome::Ready { entries } => Some(Notice {
            title: "Activity collected",
            body: match entries {
                1 => "1 entry".to_string(),
                entries => format!("{entries} entries"),
            },
        }),
        RunOutcome::Failed => Some(Notice {
            title: "Collection failed",
            body: "Open Activity Log to see why.".to_string(),
        }),
        RunOutcome::Stopped => None,
    }
}

/// Shows a notice for each run end that deserves one, for the app's life: the host the spawner
/// lives in outlasts this task, so the ends never close. After shutdown every run ends stopped
/// and none is current, so it shows nothing more. `focused` (the main window's
/// `is_focused()`) is asked only for a run that deserves a notice, as it ends, and `is_current`
/// last, so a run that another replaced meanwhile shows nothing.
pub(crate) async fn follow(
    mut ends: UnboundedReceiver<RunEnd>,
    focused: impl Fn() -> tauri::Result<bool>,
    is_current: impl Fn(RunId) -> bool,
    mut notify: impl FnMut(Notice),
) {
    while let Some(end) = ends.recv().await {
        if let Some(notice) = notice_for(&end)
            && !has_focus(&focused)
            && is_current(end.run)
        {
            notify(notice);
        }
    }
}

/// A window whose focus cannot be read (it is gone, or the read failed) counts as in the
/// background: a notice nobody needed is better than a result nobody hears of.
fn has_focus(focused: impl Fn() -> tauri::Result<bool>) -> bool {
    focused().unwrap_or_else(|err| {
        diagnostics::warn(
            "desktop",
            format!("Reading the window's focus failed: {err}"),
        );
        false
    })
}
