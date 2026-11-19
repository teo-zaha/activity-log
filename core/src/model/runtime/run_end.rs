use super::{run_id::RunId, run_outcome::RunOutcome};

/// What a collection run's work resolves to, once its terminal state is shown.
#[derive(Debug, Clone, Copy)]
pub struct RunEnd {
    pub run: RunId,
    /// From the moment the run's future was first polled.
    pub elapsed: std::time::Duration,
    pub outcome: RunOutcome,
}
