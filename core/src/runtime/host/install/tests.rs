use std::collections::BTreeSet;

use super::super::tests::{connection, march};
use crate::{
    model::{ConnectionId, Source},
    runtime::report::{Outcome, RunReport, Slot, Stored},
};

/// Every Git connection has its outcome and none changed: only the order and the list tell
/// whether Git is affected.
#[test]
fn reordering_or_removing_connections_affects_their_kind() {
    let git = |name: &str| ConnectionId::new(Source::Git, name);
    let mut keys = crate::runtime::keys::KeyTable::default();
    keys.install(&[connection(&git("a"), &[]), connection(&git("b"), &[])]);
    let outcome = |id: ConnectionId| {
        let key = keys.key(&id).unwrap();
        (
            id,
            Slot::Settled(Outcome::Collected(Stored {
                key,
                entries: Vec::new().into(),
            })),
        )
    };
    let store = super::ResultStore::new(
        march(),
        RunReport {
            slots: vec![outcome(git("a")), outcome(git("b"))],
        },
    );
    let none = BTreeSet::new();
    let affected = |old: &[ConnectionId], new: &[ConnectionId]| {
        super::affected_kinds(old, new, &none, Some(&store))
    };

    assert!(affected(&[git("a"), git("b")], &[git("a"), git("b")]).is_empty());
    assert_eq!(
        affected(&[git("a"), git("b")], &[git("b"), git("a")]),
        BTreeSet::from([Source::Git])
    );
    assert_eq!(
        affected(&[git("a"), git("b")], &[git("a")]),
        BTreeSet::from([Source::Git])
    );
}
