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
    /// The identity request was refused with 401 or a 403 without signs of throttling.
    Denied,
    /// No usable answer because the server was busy or unreachable, the request timed out,
    /// the server failed in another way, or the source has no identity request.
    #[default]
    None,
}
