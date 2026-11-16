use std::collections::BTreeMap;

use super::{WSL_GIT_DIRS, WslConfig, home_dirs, parse_wsl_dir, user_wsl_distros};
use crate::provider::provider_config::{ConfigSource, ProviderConfig};

struct MapSource(Vec<(&'static str, &'static str)>);

impl ConfigSource for MapSource {
    fn get(&self, key: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    }
}

fn wsl_config(value: &'static str) -> anyhow::Result<WslConfig> {
    WslConfig::from_config(&ProviderConfig::resolve(
        &MapSource(vec![("WSL_GIT_DIRS", value)]),
        &[WSL_GIT_DIRS],
    ))
}

fn dirs(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    entries
        .iter()
        .map(|(distro, dirs)| {
            (
                distro.to_string(),
                dirs.iter().map(|dir| dir.to_string()).collect(),
            )
        })
        .collect()
}

#[test]
fn tells_unset_folders_from_an_empty_map() {
    assert_eq!(wsl_config(" ").unwrap().dirs, None);
    assert_eq!(wsl_config("{}").unwrap().dirs, Some(BTreeMap::new()));
}

#[test]
fn reads_folders_per_distribution() {
    let config =
        wsl_config(r#"{"NixOS": ["~", "/home/nixos/trn/atlas/", " ~/projects "], "Debian": []}"#)
            .unwrap();

    assert_eq!(
        config.dirs,
        Some(dirs(&[
            ("Debian", &[]),
            ("NixOS", &["~", "/home/nixos/trn/atlas", "~/projects"]),
        ]))
    );
}

#[test]
fn rejects_folders_that_are_not_linux_paths() {
    assert!(wsl_config(r#"{"NixOS": ["C:/work/repo"]}"#).is_err());
    assert!(parse_wsl_dir("NixOS", "relative/path").is_err());
    assert!(parse_wsl_dir("NixOS", "~user").is_err());
    assert_eq!(parse_wsl_dir("NixOS", "/").unwrap(), "/");
}

#[test]
fn default_folders_are_the_home_of_every_user_distribution() {
    let names = [
        "Ubuntu",
        "docker-desktop",
        "docker-desktop-data",
        "NixOS",
        "Rancher-Desktop",
    ]
    .map(str::to_string)
    .to_vec();

    assert_eq!(
        home_dirs(user_wsl_distros(names)),
        dirs(&[("NixOS", &["~"]), ("Ubuntu", &["~"])])
    );
}
