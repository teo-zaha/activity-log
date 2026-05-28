use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct RawOutlookEvent {
    pub subject: Option<String>,
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub organizer: Option<String>,
    pub is_organizer: Option<bool>,
    #[serde(default)]
    pub is_default_calendar: Option<bool>,
}
