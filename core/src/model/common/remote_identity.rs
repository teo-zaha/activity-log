/// Who a remote token belongs to, as the server named it in the startup check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteIdentity {
    /// Stable account id: GitLab's numeric `id` as text, Jira Data Center's `key`. Never empty.
    pub id: String,
    /// GitLab `username`, Jira `name`.
    pub username: String,
}
