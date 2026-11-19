use super::remote_identity::RemoteIdentity;

/// What a startup check's identity request said about the token's owner. For GitLab that is
/// the token's owner, not the configured user whose events are collected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum IdentityEvidence {
    /// The identity request succeeded and named the principal (non-empty id).
    Verified(RemoteIdentity),
    /// The identity request succeeded (2xx) but named no one: the token works, whose it is is
    /// unknown.
    Unnamed,
    /// The identity request was refused with 401 or 403.
    Denied,
    /// No usable answer: timeout, unreachable, 5xx, or a kind without an identity request.
    #[default]
    None,
}
