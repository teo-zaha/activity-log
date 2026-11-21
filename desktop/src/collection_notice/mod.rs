//! A system notification when a long collection ends while the window is in the background
//! (native-feel decision 8). The host's spawner only sends each run's end here; `follow` decides
//! with the pure `notice_for`, and asks whether the run is still the one on screen last, just
//! before it shows anything. Rust decides, not the dashboard: a hidden web view's timers are
//! held back, which is when this matters.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use activity_log_core::model::{RunEnd, RunId, RunOutcome};
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

/// The notice for a run that ended so, while the window had focus or not; none for a short
/// run, one the user watched, or one that was stopped (a newer run or shutdown replaced it).
pub(crate) fn notice_for(end: &RunEnd, focused: bool) -> Option<Notice> {
    if focused || end.elapsed < THRESHOLD {
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

/// Whether the main window has focus, as its `Focused` events said; it starts without.
#[derive(Default)]
pub(crate) struct WindowFocus(AtomicBool);

impl WindowFocus {
    pub(crate) fn set(&self, focused: bool) {
        self.0.store(focused, Ordering::Relaxed);
    }

    pub(crate) fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Shows a notice for each run end that deserves one, until the spawner is gone. `is_current`
/// is asked last, so a run that another replaced meanwhile shows nothing.
pub(crate) async fn follow(
    mut ends: UnboundedReceiver<RunEnd>,
    focused: impl Fn() -> bool,
    is_current: impl Fn(RunId) -> bool,
    mut notify: impl FnMut(Notice),
) {
    while let Some(end) = ends.recv().await {
        if let Some(notice) = notice_for(&end, focused())
            && is_current(end.run)
        {
            notify(notice);
        }
    }
}
