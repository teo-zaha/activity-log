use crate::model::{ProviderProblem, ProviderStatus};

#[derive(Debug, Clone)]
pub struct ProviderCheckResult {
    pub status: ProviderStatus,
    /// Set together with `reason`.
    pub problem: Option<ProviderProblem>,
    pub reason: Option<String>,
}

impl ProviderCheckResult {
    pub fn available() -> Self {
        Self {
            status: ProviderStatus::Available,
            problem: None,
            reason: None,
        }
    }

    pub fn degraded(reason: impl Into<String>) -> Self {
        Self::problem(ProviderStatus::Degraded, ProviderProblem::Failed, reason)
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::problem(ProviderStatus::Unavailable, ProviderProblem::Failed, reason)
    }

    pub fn problem(
        status: ProviderStatus,
        problem: ProviderProblem,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            status,
            problem: Some(problem),
            reason: Some(reason.into()),
        }
    }
}
