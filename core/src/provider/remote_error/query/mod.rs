use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{ApiError, clean_message};

/// Machine-readable GraphQL error classes used by GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryErrorKind {
    RateLimited,
    InsufficientScopes,
    NotFound,
    Forbidden,
    UndefinedField,
    Other,
}

impl QueryErrorKind {
    fn of(error: &Value) -> Self {
        [
            error.get("type"),
            error.get("extensions").and_then(|value| value.get("code")),
        ]
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find_map(|code| match code {
            "RATE_LIMITED" => Some(Self::RateLimited),
            "INSUFFICIENT_SCOPES" => Some(Self::InsufficientScopes),
            "NOT_FOUND" => Some(Self::NotFound),
            "FORBIDDEN" => Some(Self::Forbidden),
            "undefinedField" => Some(Self::UndefinedField),
            _ => None,
        })
        .unwrap_or(Self::Other)
    }
}

/// A GraphQL answer with errors, even if it also carried data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryRefusal {
    kinds: Vec<QueryErrorKind>,
    message: Option<String>,
}

impl QueryRefusal {
    pub fn kinds(&self) -> &[QueryErrorKind] {
        &self.kinds
    }

    pub(super) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

/// Inspect the envelope before decoding its data, since partial data may be missing fields.
pub(crate) fn query_data<T: DeserializeOwned>(
    mut response: Value,
    secrets: &[&str],
) -> Result<T, ApiError> {
    let fields = response.as_object_mut().ok_or(ApiError::Decode)?;
    if let Some(errors) = fields.remove("errors") {
        let errors = errors
            .as_array()
            .filter(|errors| !errors.is_empty())
            .ok_or(ApiError::Decode)?;
        let kinds = errors.iter().map(QueryErrorKind::of).collect();
        let messages: Vec<_> = errors
            .iter()
            .filter_map(|error| error.get("message")?.as_str())
            .filter(|message| !message.trim().is_empty())
            .collect();
        let message = (!messages.is_empty()).then(|| clean_message(messages.join("; "), secrets));
        return Err(ApiError::Query(QueryRefusal { kinds, message }));
    }
    let data = fields
        .remove("data")
        .filter(Value::is_object)
        .ok_or(ApiError::Decode)?;
    serde_json::from_value(data).map_err(|_| ApiError::Decode)
}

#[cfg(test)]
mod tests;
