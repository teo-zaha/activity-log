//! The one boundary remote failures pass on their way to the Logs pane, the settings and the
//! CLI: short messages, never a URL, a secret or raw control text.

use reqwest::{
    StatusCode, Url,
    header::{HeaderMap, WWW_AUTHENTICATE},
};
use serde_json::Value;

use crate::model::ProviderProblem;

/// How long a cleaned remote message may be, in characters.
const MESSAGE_LIMIT: usize = 300;
const REDACTED: &str = "[redacted]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// The server answered, but not with success. `message` is the server's own explanation,
    /// already cleaned by `remote_message` (empty when it gave none), for people only;
    /// `retry_after` is its `Retry-After` in seconds; `refusal` the OAuth error code it named.
    Status {
        status: StatusCode,
        message: String,
        retry_after: Option<u64>,
        refusal: Option<OAuthRefusal>,
    },
    Timeout,
    Unreachable,
    Decode,
}

impl ApiError {
    /// A request or its body that did not get through. reqwest's own text names the URL with
    /// its query, so only the kind of failure is kept; a body that breaks off is never `Decode`
    /// (reqwest reports it as a decode error).
    pub(crate) fn from_transport(err: &reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout
        } else {
            Self::Unreachable
        }
    }

    /// The problem this is for its connection, when it is more than a failure: a refusal of
    /// the credentials, a timeout, a server out of reach.
    pub fn problem(&self) -> Option<ProviderProblem> {
        match self {
            Self::Status { status, .. }
                if matches!(*status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) =>
            {
                Some(ProviderProblem::Refused)
            }
            Self::Timeout => Some(ProviderProblem::TimedOut),
            Self::Unreachable => Some(ProviderProblem::Unreachable),
            Self::Status { .. } | Self::Decode => None,
        }
    }

    /// The message for a person; `service` names the remote ("Jira").
    pub fn message(&self, service: &str) -> String {
        match self {
            Self::Status { status, .. } if status.is_redirection() => {
                format!("{service} redirected to another address; set the URL it redirects to")
            }
            Self::Status {
                status, message, ..
            } if message.is_empty() => format!("{service} answered {status}"),
            Self::Status {
                status, message, ..
            } => format!("{service} answered {status}: {message}"),
            Self::Timeout => format!("{service} did not answer within 30 s"),
            Self::Unreachable => format!("Could not reach {service}"),
            Self::Decode => format!("{service} sent an answer Activity Log could not read"),
        }
    }
}

/// The error code of an OAuth 2.0 bearer token refusal (RFC 6750 §3.1), a protocol code like a
/// status: GitLab names it in the body's `error` field and in
/// `WWW-Authenticate: Bearer error="…"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthRefusal {
    /// The request was malformed.
    InvalidRequest,
    /// The token is expired, revoked or otherwise not valid.
    InvalidToken,
    /// The token lacks the scope the request needs (a GitLab token with `read_user` only).
    InsufficientScope,
}

impl OAuthRefusal {
    /// The code an error answer names: in its `WWW-Authenticate` bearer challenge, else in its
    /// body's `error` field. A code RFC 6750 does not define names none.
    pub(crate) fn of_answer(headers: &HeaderMap, body: &str) -> Option<Self> {
        headers
            .get_all(WWW_AUTHENTICATE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find_map(bearer_error)
            .or_else(|| {
                serde_json::from_str::<Value>(body)
                    .ok()?
                    .get("error")?
                    .as_str()
                    .and_then(Self::of_code)
            })
    }

    fn of_code(code: &str) -> Option<Self> {
        match code {
            "invalid_request" => Some(Self::InvalidRequest),
            "invalid_token" => Some(Self::InvalidToken),
            "insufficient_scope" => Some(Self::InsufficientScope),
            _ => None,
        }
    }
}

/// The `error` parameter of a `Bearer` challenge (`Bearer realm="x", error="invalid_token"`),
/// as a code RFC 6750 defines.
fn bearer_error(challenge: &str) -> Option<OAuthRefusal> {
    let challenge = challenge.trim_start();
    let (scheme, params) = challenge.split_at(challenge.find(' ').unwrap_or(challenge.len()));
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    auth_params(params)
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("error"))
        .and_then(|(_, value)| OAuthRefusal::of_code(&value))
}

/// `name=value` and `name="quoted \" value"` pairs separated by commas (RFC 9110 §11.2); a
/// pair that does not fit ends the list.
fn auth_params(text: &str) -> Vec<(String, String)> {
    let mut params = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars
            .next_if(|ch| ch.is_whitespace() || *ch == ',')
            .is_some()
        {}
        let name: String = std::iter::from_fn(|| {
            chars.next_if(|ch| *ch != '=' && *ch != ',' && !ch.is_whitespace())
        })
        .collect();
        while chars.next_if(|ch| ch.is_whitespace()).is_some() {}
        if name.is_empty() || chars.next() != Some('=') {
            return params;
        }
        while chars.next_if(|ch| ch.is_whitespace()).is_some() {}
        let mut value = String::new();
        if chars.next_if_eq(&'"').is_some() {
            loop {
                match chars.next() {
                    Some('"') => break,
                    Some('\\') => value.extend(chars.next()),
                    Some(ch) => value.push(ch),
                    None => return params,
                }
            }
        } else {
            value.extend(std::iter::from_fn(|| {
                chars.next_if(|ch| *ch != ',' && !ch.is_whitespace())
            }));
        }
        params.push((name, value));
    }
}

/// A collector's request to `service` that failed, kept whole in its `anyhow` chain so the
/// runtime can tell a refusal, a timeout and an unreachable server apart; it reads as
/// [`ApiError::message`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFailure {
    /// Who answered: a kind (`Jira`) or a host (a calendar feed's).
    pub service: std::borrow::Cow<'static, str>,
    pub error: ApiError,
}

impl std::fmt::Display for RemoteFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.error.message(&self.service))
    }
}

impl std::error::Error for RemoteFailure {}

/// The explanation in an error body, cleaned for display: only Jira's `errorMessages` and
/// `errors` or GitLab's `message` and `error` are read (an HTML page gives nothing), each of
/// `secrets` becomes `[redacted]`, addresses lose their user info, query and fragment (one
/// that does not parse becomes `[redacted]`), control and bidirectional characters become
/// visible `\u{…}` escapes, and the result is at most 300 characters.
pub fn remote_message(body: &str, secrets: &[&str]) -> String {
    let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(body) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for name in ["errorMessages", "errors", "message", "error"] {
        if let Some(value) = fields.get(name) {
            collect_texts(value, None, &mut parts);
        }
    }
    let mut text = parts.join("; ");
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        text = text.replace(secret, REDACTED);
    }
    cap(&escape_controls(&strip_urls(&text)))
}

/// Every `scheme://…` in `text` reduced to its scheme, host, port and path.
fn strip_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let start = rest[..at]
            .char_indices()
            .rev()
            .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
            .last()
            .map_or(at, |(index, _)| index);
        if start == at {
            out.push_str(&rest[..at + 3]);
            rest = &rest[at + 3..];
            continue;
        }
        // An address runs to the next space, double quote, angle bracket or backtick: all of
        // RFC 3986's sub-delimiters (`'`, `(`, `)`, `*`, `!`, `;`, `,`, `$`, `@`, …) belong
        // to it, so none of them can cut off a query. Punctuation at its very end is the
        // sentence's.
        let end = rest[at..]
            .find(|ch: char| {
                ch.is_whitespace() || ch.is_control() || matches!(ch, '"' | '<' | '>' | '`')
            })
            .map_or(rest.len(), |offset| at + offset);
        let address =
            rest[start..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'']);
        out.push_str(&rest[..start]);
        out.push_str(&match Url::parse(address) {
            Ok(mut url) if url.has_host() => {
                // Setting user info fails only for URLs without a host, excluded above.
                let _ = url.set_username("");
                let _ = url.set_password(None);
                url.set_query(None);
                url.set_fragment(None);
                url.to_string()
            }
            _ => REDACTED.to_string(),
        });
        rest = &rest[start + address.len()..];
    }
    out.push_str(rest);
    out
}

/// Strings in `value`, an object's entries as `key: text`.
fn collect_texts(value: &Value, key: Option<&str>, out: &mut Vec<String>) {
    match value {
        Value::String(text) if !text.trim().is_empty() => out.push(match key {
            Some(key) => format!("{key}: {text}"),
            None => text.clone(),
        }),
        Value::Array(items) => items.iter().for_each(|item| collect_texts(item, key, out)),
        Value::Object(entries) => entries
            .iter()
            .for_each(|(key, item)| collect_texts(item, Some(key), out)),
        _ => {}
    }
}

fn escape_controls(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_control() || is_bidi_control(ch) {
            escaped.push_str(&format!("\\u{{{:x}}}", u32::from(ch)));
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

fn is_bidi_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

fn cap(text: &str) -> String {
    if text.chars().count() <= MESSAGE_LIMIT {
        return text.to_string();
    }
    let mut capped: String = text.chars().take(MESSAGE_LIMIT - 1).collect();
    capped.push('…');
    capped
}

#[cfg(test)]
mod tests;
