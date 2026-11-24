use serde::{Deserialize, de::DeserializeOwned};
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
    errors: Vec<QueryError>,
    message: Option<String>,
}

impl QueryRefusal {
    pub fn errors(&self) -> &[QueryError] {
        &self.errors
    }

    pub fn kinds(&self) -> impl Iterator<Item = QueryErrorKind> + '_ {
        self.errors.iter().map(|error| error.kind)
    }

    pub(super) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    pub kind: QueryErrorKind,
    /// Missing or malformed paths supply no evidence about a particular field.
    pub path: Option<Vec<QueryPathSegment>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum QueryPathSegment {
    Field(String),
    Index(usize),
}

/// Inspect the envelope before decoding its data, since partial data may be missing fields.
pub struct QueryAnswer {
    data: Option<Value>,
    refusal: Option<QueryRefusal>,
}

impl QueryAnswer {
    pub(crate) fn read(mut response: Value, secrets: &[&str]) -> Result<Self, ApiError> {
        let fields = response.as_object_mut().ok_or(ApiError::Decode)?;
        let data = fields.remove("data");
        let refusal = fields
            .remove("errors")
            .map(|errors| read_refusal(&errors, secrets))
            .transpose()?;
        Ok(Self { data, refusal })
    }

    pub(crate) fn complete<T: DeserializeOwned>(self) -> Result<T, ApiError> {
        if let Some(refusal) = self.refusal {
            return Err(ApiError::Query(refusal));
        }
        let data = self.data.filter(Value::is_object).ok_or(ApiError::Decode)?;
        serde_json::from_value(data).map_err(|_| ApiError::Decode)
    }

    /// Gives a provider's partial-answer policy the data and the typed refusal together.
    pub fn into_parts(self) -> (Option<Value>, Option<QueryRefusal>) {
        (self.data, self.refusal)
    }
}

fn read_refusal(errors: &Value, secrets: &[&str]) -> Result<QueryRefusal, ApiError> {
    let rows = errors
        .as_array()
        .filter(|errors| !errors.is_empty())
        .ok_or(ApiError::Decode)?;
    let errors = rows
        .iter()
        .map(|row| QueryError {
            kind: QueryErrorKind::of(row),
            path: row.get("path").and_then(|path| Vec::deserialize(path).ok()),
        })
        .collect();
    let messages: Vec<_> = rows
        .iter()
        .filter_map(|error| error.get("message")?.as_str())
        .filter(|message| !message.trim().is_empty())
        .collect();
    let message = (!messages.is_empty()).then(|| clean_message(messages.join("; "), secrets));
    Ok(QueryRefusal { errors, message })
}

pub(crate) fn query_data<T: DeserializeOwned>(
    response: Value,
    secrets: &[&str],
) -> Result<T, ApiError> {
    QueryAnswer::read(response, secrets)?.complete()
}

#[cfg(test)]
mod tests;
