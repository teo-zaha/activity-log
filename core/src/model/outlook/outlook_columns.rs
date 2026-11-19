use serde::Deserialize;
use serde_json::Value;

/// What the macOS script answers: one list per property, item `i` of each about the same event.
/// A property Outlook would not give for the range has no list.
#[derive(Debug, Deserialize)]
pub(crate) struct OutlookColumns {
    /// Numbers on macOS; kept as JSON so text ids would read too.
    #[serde(default)]
    pub id: Option<Vec<Value>>,
    pub subject: Vec<Option<String>>,
    /// Seconds since 1970.
    pub start: Vec<f64>,
    pub end: Vec<f64>,
    #[serde(default)]
    pub location: Option<Vec<Option<String>>>,
    #[serde(default)]
    pub organizer: Option<Vec<Option<String>>>,
    #[serde(default)]
    pub is_organizer: Option<Vec<Option<bool>>>,
    /// The dictionary's names: `free`, `tentative`, `busy`, `out of office`.
    #[serde(default)]
    pub busy_status: Option<Vec<Option<String>>>,
    #[serde(default)]
    pub body: Option<Vec<Option<String>>>,
    /// Properties Outlook failed to give for the range: `busy_status`, `body`, `organizer`, ….
    #[serde(default)]
    pub unread: Vec<String>,
}
