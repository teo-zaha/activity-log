use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct JiraHistoryItem {
    pub field: String,
    #[serde(rename = "fromString")]
    pub from_string: Option<String>,
    #[serde(rename = "toString")]
    pub to_string: Option<String>,
}
