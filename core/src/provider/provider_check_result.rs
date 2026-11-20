use crate::model::{IdentityEvidence, ProviderInfo, ProviderProblem, ProviderStatus};

/// How a connection's startup check ended. A problem always comes with its reason, and only an
/// unusable or degraded connection has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    Available,
    /// Usable, but not everything may be collected.
    Degraded {
        problem: ProviderProblem,
        reason: String,
    },
    Unavailable {
        problem: ProviderProblem,
        reason: String,
    },
}

impl CheckOutcome {
    pub fn status(&self) -> ProviderStatus {
        match self {
            Self::Available => ProviderStatus::Available,
            Self::Degraded { .. } => ProviderStatus::Degraded,
            Self::Unavailable { .. } => ProviderStatus::Unavailable,
        }
    }

    pub fn problem(&self) -> Option<ProviderProblem> {
        match self {
            Self::Available => None,
            Self::Degraded { problem, .. } | Self::Unavailable { problem, .. } => Some(*problem),
        }
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Available => None,
            Self::Degraded { reason, .. } | Self::Unavailable { reason, .. } => Some(reason),
        }
    }

    /// Whether the UI offers to check the connection again: any problem but another OS, which
    /// no check can fix.
    pub fn recheckable(&self) -> bool {
        self.problem()
            .is_some_and(|problem| problem != ProviderProblem::Unsupported)
    }

    /// Writes this outcome into `info`: its status, problem, reason, whether it is usable and
    /// whether it can be checked again, which must agree.
    pub fn apply_to(&self, info: &mut ProviderInfo) {
        info.status = self.status();
        info.usable = !matches!(self, Self::Unavailable { .. });
        info.problem = self.problem();
        info.reason = self.reason().map(str::to_string);
        info.recheckable = self.recheckable();
    }
}

#[derive(Debug, Clone)]
pub struct ProviderCheckResult {
    pub outcome: CheckOutcome,
    /// Whose token the check saw; only remote kinds with an identity request set it.
    pub identity: IdentityEvidence,
}

impl ProviderCheckResult {
    pub fn available() -> Self {
        Self::of(CheckOutcome::Available)
    }

    pub fn degraded(reason: impl Into<String>) -> Self {
        Self::of(CheckOutcome::Degraded {
            problem: ProviderProblem::Failed,
            reason: reason.into(),
        })
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::unavailable_as(ProviderProblem::Failed, reason)
    }

    /// A check that failed with `err`: unavailable, with the problem a typed cause in its chain
    /// names (`source_failure::problem_of`), else `failed`.
    pub fn unavailable_from(err: &anyhow::Error) -> Self {
        Self::unavailable_as(
            crate::provider::source_failure::problem_of(err).unwrap_or(ProviderProblem::Failed),
            format!("{err:#}"),
        )
    }

    pub fn unavailable_as(problem: ProviderProblem, reason: impl Into<String>) -> Self {
        Self::of(CheckOutcome::Unavailable {
            problem,
            reason: reason.into(),
        })
    }

    fn of(outcome: CheckOutcome) -> Self {
        Self {
            outcome,
            identity: IdentityEvidence::None,
        }
    }

    pub fn with_identity(mut self, identity: IdentityEvidence) -> Self {
        self.identity = identity;
        self
    }
}
