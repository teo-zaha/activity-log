use super::remote_identity::RemoteIdentity;

/// What a startup check's identity request said about the token's owner. For GitLab that is
/// the token's owner, not the configured user whose events are collected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum IdentityEvidence {
    /// The identity request succeeded and named the principal (non-empty id).
    Verified(RemoteIdentity),
    /// The identity request was refused with 401 or a 403 without signs of throttling.
    Denied,
    /// No usable answer, including a successful response without an account ID, a request
    /// failure without denial evidence, or a source without an identity request.
    #[default]
    None,
}
