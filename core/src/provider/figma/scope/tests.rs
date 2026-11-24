use super::*;
use crate::provider::provider_config::{ProviderConfigShape, ProviderConfigValueType};

fn config(items: &[&str]) -> FigmaConfig {
    FigmaConfig {
        token: "test-secret".to_owned(),
        files: Some(items.iter().map(|item| (*item).to_owned()).collect()),
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
    assert_eq!(FigmaScope::of(&config(&[])).unwrap_err(), ScopeError::Empty);
    let mut unset = config(&[]);
    unset.files = None;
    assert_eq!(FigmaScope::of(&unset).unwrap_err(), ScopeError::Empty);
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
fn file_list_is_optional_and_does_not_bind_the_token() {
    let [token, files] = FigmaConfig::binding().requirements else {
        panic!("expected a token and a file list");
    };
    assert!(token.is_secret() && token.account && !token.binds);
    assert!(!files.required && !files.binds && !files.account);
    assert_eq!(
        files.shape,
        ProviderConfigShape::List(ProviderConfigValueType::String)
    );
}
