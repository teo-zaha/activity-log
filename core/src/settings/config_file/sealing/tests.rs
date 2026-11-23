use super::Sealed;
use crate::{provider::gitlab::GITLAB_TOKEN, secrets::sealed::SealError, settings::EditError};

/// A secret the writer brought that the cipher refuses fails the edit with the `SealError`
/// itself in its chain; one the file held already only stays as it was.
#[test]
fn a_refused_seal_keeps_its_error() {
    let id = "gitlab/work".parse::<crate::model::ConnectionId>().unwrap();
    let failed = || Sealed {
        count: 0,
        unbound: Vec::new(),
        failed: vec![(id.clone(), GITLAB_TOKEN, SealError::Empty)],
    };
    let Err(EditError::Internal(err)) = failed().report(&[(id.clone(), GITLAB_TOKEN)]) else {
        panic!("the edit stands");
    };
    assert_eq!(err.downcast_ref::<SealError>(), Some(&SealError::Empty));
    assert!(failed().report(&[]).is_ok());
}
