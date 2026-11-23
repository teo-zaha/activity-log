use activity_log_core::settings::{ConfigFile, view::SaveBase};

use super::{HELD, SnapshotRegistry};

fn file() -> ConfigFile {
    ConfigFile::parse("config.toml", b"[[gitlab]]\nlabel = \"Work\"\n").unwrap()
}

#[test]
fn a_request_is_based_on_the_snapshot_it_names_while_it_is_held() {
    let registry = SnapshotRegistry::default();
    let id = "gitlab/work".parse().unwrap();
    let first = registry.register(file());

    assert!(matches!(
        registry.base(&first, &id, &[]),
        SaveBase::Known(_)
    ));
    assert!(matches!(
        registry.base(&first, &"gitlab/other".parse().unwrap(), &[]),
        SaveBase::Unknown(_)
    ));

    for _ in 0..HELD {
        registry.register(file());
    }
    assert!(matches!(
        registry.base(&first, &id, &[]),
        SaveBase::Unknown(_)
    ));
}
