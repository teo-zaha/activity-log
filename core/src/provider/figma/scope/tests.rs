use super::*;

#[test]
fn team_scope_reads_numeric_ids_and_both_link_shapes_in_list_order() {
    let mut config = config(&[]);
    config.teams = Some(
        [
            "100001",
            "https://www.figma.com/files/team/200002/Design",
            "https://figma.com/files/300003/team/100001/Design?fuid=8",
            "https://www.figma.com/files/300003/team/400004",
        ]
        .map(str::to_owned)
        .to_vec(),
    );
    let scope = FigmaScope::of(&config).unwrap();
    let teams: Vec<_> = scope
        .teams
        .iter()
        .map(|(item, id)| (*item, id.as_str()))
        .collect();
    assert_eq!(teams, [(1, "100001"), (2, "200002"), (4, "400004")]);
}

#[test]
fn invalid_team_links_and_ids_report_the_original_list_position() {
    for input in [
        "",
        "Team31",
        "１２３",
        "https://www.figma.com/files/team/%31",
        "https://www.figma.com/files/team/31/..",
        "https://www.figma.com/files/31",
        "https://www.figma.com/files/31/team/",
        "https://www.figma.com/files/team/31\\Design",
        "https://www.figma.com.example.org/files/team/31",
        "https://user@www.figma.com/files/team/31",
        "https://www.figma.com:444/files/team/31",
        "http://www.figma.com/files/team/31",
    ] {
        let mut config = config(&[]);
        config.teams = Some(vec!["21".into(), "21".into(), input.into()]);
        assert_eq!(
            FigmaScope::of(&config).unwrap_err(),
            ScopeError::NotATeam { item: 3 },
            "{input}"
        );
    }
}
use crate::provider::provider_config::{ProviderConfigShape, ProviderConfigValueType};

fn config(items: &[&str]) -> FigmaConfig {
    FigmaConfig {
        token: "test-secret".to_owned(),
        files: Some(items.iter().map(|item| (*item).to_owned()).collect()),
        teams: None,
    }
}

#[test]
fn reads_keys_from_pasted_links_and_tabs() {
    for editor in ["file", "design", "board", "slides", "proto", "make", "site"] {
        for (suffix, expected) in [
            ("/FILEkey0001/Checkout?node-id=1%3A2#view", "FILEkey0001"),
            ("/FILEkey0001/branch/Branch0002/Checkout", "Branch0002"),
        ] {
            let path = format!("/{editor}{suffix}");
            assert_eq!(FigmaFileKey::from_path(&path).unwrap().as_str(), expected);
            for host in ["figma.com", "www.figma.com"] {
                let link = format!("https://{host}{path}");
                assert_eq!(
                    FigmaScope::of(&config(&[&link])).unwrap().files[0]
                        .1
                        .as_str(),
                    expected
                );
            }
        }
    }
}

#[test]
fn keeps_the_first_position_for_each_distinct_key() {
    let scope = FigmaScope::of(&config(&[
        "FILEkey0001",
        "https://www.figma.com/design/FILEkey0001/Checkout",
        "OTHERfile0002",
    ]))
    .unwrap();
    assert_eq!(scope.duplicates, 1);
    assert_eq!(
        scope
            .files
            .iter()
            .map(|(position, key)| (*position, key.as_str()))
            .collect::<Vec<_>>(),
        [(1, "FILEkey0001"), (3, "OTHERfile0002")]
    );
}

#[test]
fn rejects_invalid_keys_and_reports_the_original_list_position() {
    assert_eq!(FigmaScope::of(&config(&[])).unwrap().files.len(), 0);
    let mut unset = config(&[]);
    unset.files = None;
    assert_eq!(FigmaScope::of(&unset).unwrap().files.len(), 0);
    for input in [
        "",
        ".",
        "..",
        "a/b",
        "key\t",
        "https://www.figma.com/design/",
        "https://www.figma.com/design/FILEkey0001/branch/",
        "https://www.figma.com/design/FILEkey0001/branch/..",
        "https://www.figma.com/design/FILEkey0001/branch/%2e%2e",
        "https://www.figma.com/design/%2F",
        "https://www.figma.com/design/FILEkey0001/../OTHERfile0002",
        "https://www.figma.com/design/FILE\tkey0001",
        "https://www.figma.com?fake=/design/FILEkey0001",
        "https://www.figma.com:8443/design/FILEkey0001",
        "https://person@www.figma.com/design/FILEkey0001",
        "https://www.figma.com\\extra/design/FILEkey0001",
        "http://www.figma.com/design/FILEkey0001",
        &"a".repeat(129),
    ] {
        assert_eq!(
            FigmaScope::of(&config(&["FILEkey0001", input])).unwrap_err(),
            ScopeError::NotAFile { item: 2 },
            "{input:?}"
        );
    }
    assert_eq!(
        FigmaScope::of(&config(&[
            "FILEkey0001",
            "https://figma.example.org/design/FILEkey0001",
        ]))
        .unwrap_err(),
        ScopeError::OtherHost { item: 2 }
    );
}

#[test]
fn file_and_team_lists_are_optional_and_do_not_bind_the_token() {
    let [token, files, teams] = FigmaConfig::binding().requirements else {
        panic!("expected a token, a file list and a team list");
    };
    assert!(token.is_secret() && token.account && !token.binds);
    for list in [files, teams] {
        assert!(!list.required && !list.binds && !list.account);
        assert_eq!(
            list.shape,
            ProviderConfigShape::List(ProviderConfigValueType::String)
        );
    }
}
