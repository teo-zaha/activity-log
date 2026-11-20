use crate::model::common::collect_options::CollectOptions;

/// Why a `[start, end)` range cannot be collected or exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeError {
    /// `start` or `end` is not an RFC 3339 instant.
    NotRfc3339,
    /// `end` is not after `start`.
    Empty,
    /// Longer than `CollectOptions::MAX_DAYS`.
    TooLong,
}

impl std::fmt::Display for RangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRfc3339 => write!(f, "start and end must be RFC 3339 timestamps"),
            Self::Empty => write!(f, "end must be after start"),
            Self::TooLong => write!(
                f,
                "A range can span at most 10 years ({} days)",
                CollectOptions::MAX_DAYS
            ),
        }
    }
}

impl std::error::Error for RangeError {}
