use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct JiraNamedField {
    pub name: String,
}
