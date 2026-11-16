use super::{WSL_DISTROS, WSL_GIT_DIRS, WslConfig, WslRepo, parse_wsl_repo, user_wsl_distros};
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

fn wsl_config(values: Vec<(&'static str, &'static str)>) -> anyhow::Result<WslConfig> {
    WslConfig::from_config(&ProviderConfig::resolve(
        &MapSource(values),
        &[WSL_DISTROS, WSL_GIT_DIRS],
    ))
}

#[test]
fn tells_unset_distros_from_an_empty_list() {
    assert_eq!(
        wsl_config(vec![("WSL_DISTROS", " ")]).unwrap().distros,
        None
    );
    assert_eq!(
        wsl_config(vec![("WSL_DISTROS", "[]")]).unwrap().distros,
        Some(Vec::new())
    );
}

#[test]
fn reads_wsl_repositories() {
    let config = wsl_config(vec![(
        "WSL_GIT_DIRS",
        "NixOS:/home/nixos/trn/atlas, NixOS:/home/nixos/projects/stitch-lab/",
    )])
    .unwrap();

    assert_eq!(
        config.repos,
        vec![
            WslRepo {
                distro: "NixOS".to_string(),
                path: "/home/nixos/trn/atlas".to_string(),
            },
            WslRepo {
                distro: "NixOS".to_string(),
                path: "/home/nixos/projects/stitch-lab".to_string(),
            },
        ]
    );
    assert!(wsl_config(vec![("WSL_GIT_DIRS", "C:/work/repo")]).is_err());
    assert!(parse_wsl_repo("NixOS:relative/path").is_err());
}

#[test]
fn default_distros_skip_tool_managed_ones() {
    let names = [
        "Ubuntu",
        "docker-desktop",
        "docker-desktop-data",
        "NixOS",
        "Rancher-Desktop",
    ]
    .map(str::to_string)
    .to_vec();

    assert_eq!(user_wsl_distros(names), ["Ubuntu", "NixOS"]);
}
