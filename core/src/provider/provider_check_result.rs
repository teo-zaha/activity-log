use crate::model::ProviderStatus;

#[derive(Debug, Clone)]
pub struct ProviderCheckResult {
    pub status: ProviderStatus,
    pub reason: Option<String>,
}

impl ProviderCheckResult {
    pub fn available() -> Self {
        Self {
            status: ProviderStatus::Available,
            reason: None,
        }
    }

    pub fn degraded(reason: impl Into<String>) -> Self {
        Self {
            status: ProviderStatus::Degraded,
            reason: Some(reason.into()),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            status: ProviderStatus::Unavailable,
            reason: Some(reason.into()),
        }
    }
}
