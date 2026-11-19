/// How a collection run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// `entries` is the length of the run's final log `entries`.
    Ready { entries: usize },
    /// The run failed as a whole; the message stays in the shown state and the Logs pane.
    Failed,
    /// Stopped by a newer run, a runtime replacement or shutdown.
    Stopped,
}
