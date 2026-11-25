use serde::Deserialize;

/// The fields Activity Log takes from GitLab's single-project endpoint.
#[derive(Deserialize)]
pub(crate) struct GitLabProject {
    pub(crate) id: u64,
    pub(crate) path_with_namespace: String,
    pub(crate) web_url: Option<String>,
}
