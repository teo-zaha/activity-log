use std::{env, fs};

use super::{get_cached, set_cached};

#[test]
fn cache_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let prev = env::current_dir().unwrap();
    env::set_current_dir(tmp.path()).unwrap();

    set_cached("foo/bar", &vec!["a", "b"]).unwrap();
    let cached = get_cached::<Vec<String>>("foo/bar").unwrap().unwrap();
    assert_eq!(cached, vec!["a".to_string(), "b".to_string()]);

    env::set_current_dir(prev).unwrap();
    fs::remove_dir_all(tmp.path()).ok();
}
