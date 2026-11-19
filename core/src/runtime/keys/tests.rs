use std::collections::BTreeMap;

use crate::{
    model::{ConnectionId, IdentityEvidence, RemoteIdentity, Source},
    provider::connection::{Connection, ConnectionLocation, ConnectionValues},
};

use super::KeyTable;

fn jira(label: &str, token: &str) -> Connection {
    Connection::new(
        ConnectionId::new(Source::Jira, "work"),
        label,
        ConnectionLocation::Local,
        ConnectionValues::new(BTreeMap::from([
            (
                "JIRA_URL".to_string(),
                Some("https://jira.example.org".to_string()),
            ),
            ("JIRA_TOKEN".to_string(), Some(token.to_string())),
        ])),
    )
}

fn id() -> ConnectionId {
    ConnectionId::new(Source::Jira, "work")
}

fn verified(account: &str) -> IdentityEvidence {
    IdentityEvidence::Verified(RemoteIdentity {
        id: account.to_string(),
        username: account.to_string(),
    })
}

fn installed(connection: Connection) -> KeyTable {
    let mut keys = KeyTable::default();
    keys.install(&[connection]);
    keys
}

#[test]
fn a_token_edit_changes_the_key_and_a_label_edit_does_not() {
    let mut keys = installed(jira("Work", "token-a"));
    let first = keys.key(&id()).unwrap();

    assert!(keys.install(&[jira("Renamed", "token-a")]).is_empty());
    assert_eq!(keys.key(&id()), Some(first));

    let changed = keys.install(&[jira("Renamed", "token-b")]);
    assert_eq!(changed.into_iter().collect::<Vec<_>>(), [id()]);
    assert_ne!(keys.key(&id()), Some(first));
}

#[test]
fn a_location_change_changes_the_key_and_a_removed_connection_has_none() {
    let mut keys = installed(jira("Work", "token-a"));
    let first = keys.key(&id()).unwrap();
    let mut moved = jira("Work", "token-a");
    moved.location = ConnectionLocation::Wsl("Ubuntu".to_string());

    keys.install(&[moved]);
    assert_ne!(keys.key(&id()), Some(first));

    keys.install(&[]);
    assert_eq!(keys.key(&id()), None);
}

#[test]
fn the_same_principal_found_again_keeps_the_key_and_another_one_changes_it() {
    let mut keys = installed(jira("Work", "token-a"));
    let unknown = keys.key(&id()).unwrap();

    let (a, changed) = keys.record(&id(), &verified("a")).unwrap();
    assert!(changed && a != unknown, "unknown to verified changes it");
    assert_eq!(keys.record(&id(), &verified("a")), Some((a, false)));

    let (b, changed) = keys.record(&id(), &verified("b")).unwrap();
    assert!(changed && b != a);
}

#[test]
fn every_unnamed_check_gives_a_new_key() {
    let mut keys = installed(jira("Work", "token-a"));
    let (a, _) = keys.record(&id(), &verified("a")).unwrap();

    let (first, changed) = keys.record(&id(), &IdentityEvidence::Unnamed).unwrap();
    assert!(changed && first != a);
    let (second, changed) = keys.record(&id(), &IdentityEvidence::Unnamed).unwrap();
    assert!(changed && second != first);
}

#[test]
fn a_refusal_changes_the_key_and_no_answer_keeps_it() {
    let mut keys = installed(jira("Work", "token-a"));
    let (a, _) = keys.record(&id(), &verified("a")).unwrap();

    assert_eq!(
        keys.record(&id(), &IdentityEvidence::None),
        Some((a, false))
    );
    let (denied, changed) = keys.record(&id(), &IdentityEvidence::Denied).unwrap();
    assert!(changed && denied != a);
    assert_eq!(
        keys.record(&ConnectionId::new(Source::Jira, "other"), &verified("a")),
        None
    );
}

#[test]
fn an_edit_forgets_the_principal_of_the_old_values() {
    let mut keys = installed(jira("Work", "token-a"));
    keys.record(&id(), &verified("a")).unwrap();

    keys.install(&[jira("Work", "token-b")]);
    let edited = keys.key(&id()).unwrap();

    // The same account behind the new token is still a change from "nothing said yet".
    let (found, changed) = keys.record(&id(), &verified("a")).unwrap();
    assert!(changed && found != edited);
}

#[test]
fn a_connection_with_another_instance_is_a_new_one_and_a_first_instance_is_not() {
    let mut keys = installed(jira("Work", "token-a"));
    let legacy = keys.key(&id()).unwrap();

    // A legacy table gets its first instance when it is first written: the same connection.
    let first = jira("Work", "token-a").with_instance(Some("0000000000000001".to_string()));
    assert!(keys.install(std::slice::from_ref(&first)).is_empty());
    assert_eq!(keys.key(&id()), Some(legacy));
    assert!(keys.install(&[first]).is_empty());

    // Removed and added again with the same values: another instance, another connection.
    let readded = jira("Work", "token-a").with_instance(Some("0000000000000002".to_string()));
    assert_eq!(keys.install(&[readded]).len(), 1);
    assert_ne!(keys.key(&id()), Some(legacy));
}
