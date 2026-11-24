use serde::Deserialize;

/// The stable id used to match a version or comment to the token's owner.
#[derive(Debug, Deserialize)]
pub(crate) struct FigmaUser {
    pub(crate) id: String,
}
