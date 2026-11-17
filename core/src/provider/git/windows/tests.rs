use super::user_wsl_distros;

#[test]
fn default_distributions_skip_tool_managed_ones() {
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
