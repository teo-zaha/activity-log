use std::collections::BTreeMap;

use super::{ConnectionValues, slug};
use crate::{
    model::{ConnectionId, Source},
    provider::provider_config::ConfigSource,
};

#[test]
fn slugs_labels_for_ids() {
    assert_eq!(slug("Work GitLab"), "work-gitlab");
    assert_eq!(slug("  WSL · NixOS "), "wsl-nixos");
    assert_eq!(slug("ÄÖÜ"), "connection");
}

#[test]
fn ids_read_and_print_as_kind_slash_name() {
    let id = "gitlab/oss".parse::<ConnectionId>().unwrap();
    assert_eq!(id, ConnectionId::new(Source::Gitlab, "oss"));
    assert_eq!(id.to_string(), "gitlab/oss");
    assert!("nope/oss".parse::<ConnectionId>().is_err());
    assert!("gitlab/".parse::<ConnectionId>().is_err());
    assert!("gitlab".parse::<ConnectionId>().is_err());
}

#[test]
fn a_cleared_value_reads_as_unset() {
    let values = ConnectionValues::new(BTreeMap::from([
        ("GITLAB_USER".to_string(), Some("me".to_string())),
        ("GITLAB_TOKEN".to_string(), None),
    ]));

    assert_eq!(values.get("GITLAB_USER").as_deref(), Some("me"));
    assert_eq!(values.get("GITLAB_TOKEN"), None);
    assert_eq!(values.get("GITLAB_URL"), None);
}
