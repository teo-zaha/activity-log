use activity_log_core::{
    model::{
        ProviderConfigFieldValueDto, ProviderConfigSaveRequestDto, ProviderConfigSaveValueDto,
        ProviderConfigValueDto, ProviderConfigValueTypeDto, Source,
    },
    provider::{git::default_git_root, provider_config::ProviderConfigValueType},
};

use super::{StoreEnvConfigSource, apply_provider_config_updates, build_provider_config_snapshot};

#[test]
fn maps_provider_config_value_types_to_public_dto() {
    assert_eq!(
        ProviderConfigValueTypeDto::from(ProviderConfigValueType::String),
        ProviderConfigValueTypeDto::String
    );
    assert_eq!(
        ProviderConfigValueTypeDto::from(ProviderConfigValueType::Bool),
        ProviderConfigValueTypeDto::Bool
    );
    assert_eq!(
        ProviderConfigValueTypeDto::from(ProviderConfigValueType::Int),
        ProviderConfigValueTypeDto::Int
    );
    assert_eq!(
        ProviderConfigValueTypeDto::from(ProviderConfigValueType::Password),
        ProviderConfigValueTypeDto::Password
    );
    assert_eq!(
        ProviderConfigValueTypeDto::from(ProviderConfigValueType::Email),
        ProviderConfigValueTypeDto::Email
    );
    assert_eq!(
        ProviderConfigValueTypeDto::from(ProviderConfigValueType::Url),
        ProviderConfigValueTypeDto::Url
    );
}

#[test]
fn save_updates_clear_values_with_null_tombstone() {
    let mut store_values = std::collections::BTreeMap::new();
    store_values.insert(
        "JIRA_URL".to_string(),
        Some("https://jira.example".to_string()),
    );

    let request = ProviderConfigSaveRequestDto {
        source: Source::Jira,
        values: vec![ProviderConfigSaveValueDto {
            key: "JIRA_URL".to_string(),
            value: None,
        }],
    };

    apply_provider_config_updates(&mut store_values, &request).expect("valid request");

    assert_eq!(store_values.get("JIRA_URL"), Some(&None));
}

#[test]
fn snapshot_masks_password_values() {
    let mut store_values = std::collections::BTreeMap::new();
    store_values.insert(
        "JIRA_TOKEN".to_string(),
        Some("super-secret-token".to_string()),
    );

    let snapshot = build_provider_config_snapshot(&StoreEnvConfigSource { store_values });
    let jira = snapshot
        .providers
        .iter()
        .find(|provider| provider.source == Source::Jira)
        .expect("jira provider");
    let token = jira
        .fields
        .iter()
        .find(|field| field.key == "JIRA_TOKEN")
        .expect("jira token field");

    assert!(token.is_set);
    assert_eq!(
        token.value,
        ProviderConfigFieldValueDto::Single {
            value_type: ProviderConfigValueTypeDto::Password,
            secret: true,
            value: None,
        }
    );
    assert_eq!(token.label, "Jira Token");
}

#[test]
fn label_formatter_keeps_common_acronyms_readable() {
    assert_eq!(super::provider_config_key_label("JIRA_URL"), "Jira URL");
    assert_eq!(
        super::provider_config_key_label("GITLAB_TOKEN"),
        "GitLab Token"
    );
    assert_eq!(
        super::provider_config_key_label("JIRA_INSECURE"),
        "Jira Insecure"
    );
}

#[test]
fn snapshot_only_exposes_current_jira_fields() {
    let snapshot = build_provider_config_snapshot(&StoreEnvConfigSource::default());
    let jira = snapshot
        .providers
        .iter()
        .find(|provider| provider.source == Source::Jira)
        .expect("jira provider");

    assert_eq!(jira.fields.len(), 3);
    assert!(jira.fields.iter().all(|field| field.key != "JIRA_EMAIL"));
    assert!(
        jira.fields
            .iter()
            .all(|field| field.key != "JIRA_AUTH_TYPE")
    );
    assert!(jira.fields.iter().any(|field| field.key == "JIRA_URL"));
    assert!(jira.fields.iter().any(|field| field.key == "JIRA_TOKEN"));
    assert!(jira.fields.iter().any(|field| field.key == "JIRA_INSECURE"));
}

#[test]
fn save_rejects_unknown_legacy_jira_fields() {
    let mut store_values = std::collections::BTreeMap::new();
    let request = ProviderConfigSaveRequestDto {
        source: Source::Jira,
        values: vec![ProviderConfigSaveValueDto {
            key: "JIRA_AUTH_TYPE".to_string(),
            value: Some(ProviderConfigValueDto::Text("basic".to_string())),
        }],
    };

    let result = apply_provider_config_updates(&mut store_values, &request);
    assert!(result.is_err());
}

#[test]
fn snapshot_shows_default_git_dirs_value_when_unset() {
    let Some(default_root) = default_git_root() else {
        return;
    };

    let snapshot = build_provider_config_snapshot(&StoreEnvConfigSource::default());
    let git = snapshot
        .providers
        .iter()
        .find(|provider| provider.source == Source::Git)
        .expect("git provider");
    let dirs = git
        .fields
        .iter()
        .find(|field| field.key == "GIT_DIRS")
        .expect("git dirs field");

    assert!(!dirs.is_set);
    assert_eq!(
        dirs.value,
        ProviderConfigFieldValueDto::List {
            item_type: ProviderConfigValueTypeDto::String,
            items: vec![default_root.display().to_string()],
        }
    );
}

fn git_dirs_request(value: Option<ProviderConfigValueDto>) -> ProviderConfigSaveRequestDto {
    ProviderConfigSaveRequestDto {
        source: Source::Git,
        values: vec![ProviderConfigSaveValueDto {
            key: "GIT_DIRS".to_string(),
            value,
        }],
    }
}

#[test]
fn saves_lists_cleaned_and_keeps_an_empty_list_apart_from_a_cleared_one() {
    let mut store_values = std::collections::BTreeMap::new();

    let items = vec![
        " C:\\work ".to_string(),
        String::new(),
        "\"D:\\repos\"".to_string(),
    ];
    apply_provider_config_updates(
        &mut store_values,
        &git_dirs_request(Some(ProviderConfigValueDto::List(items))),
    )
    .expect("valid list");
    assert_eq!(
        store_values.get("GIT_DIRS"),
        Some(&Some(r#"["C:\\work","D:\\repos"]"#.to_string()))
    );

    apply_provider_config_updates(
        &mut store_values,
        &git_dirs_request(Some(ProviderConfigValueDto::List(Vec::new()))),
    )
    .expect("empty list");
    assert_eq!(store_values.get("GIT_DIRS"), Some(&Some("[]".to_string())));

    apply_provider_config_updates(&mut store_values, &git_dirs_request(None)).expect("clear");
    assert_eq!(store_values.get("GIT_DIRS"), Some(&None));
}

#[test]
fn snapshot_sends_git_dirs_as_a_list_of_strings() {
    let snapshot = build_provider_config_snapshot(&StoreEnvConfigSource::default());
    let dirs = snapshot
        .providers
        .iter()
        .find(|provider| provider.source == Source::Git)
        .and_then(|git| git.fields.iter().find(|field| field.key == "GIT_DIRS"))
        .expect("git dirs field");

    assert!(matches!(
        dirs.value,
        ProviderConfigFieldValueDto::List {
            item_type: ProviderConfigValueTypeDto::String,
            ..
        }
    ));
}

#[test]
fn rejects_a_list_for_a_single_value_key() {
    let mut store_values = std::collections::BTreeMap::new();
    let request = ProviderConfigSaveRequestDto {
        source: Source::Jira,
        values: vec![ProviderConfigSaveValueDto {
            key: "JIRA_URL".to_string(),
            value: Some(ProviderConfigValueDto::List(vec!["https://a".to_string()])),
        }],
    };

    assert!(apply_provider_config_updates(&mut store_values, &request).is_err());
}

#[test]
fn snapshot_shows_a_configured_empty_list_as_set_and_empty() {
    let mut store_values = std::collections::BTreeMap::new();
    store_values.insert("GIT_DIRS".to_string(), Some("[]".to_string()));

    let snapshot = build_provider_config_snapshot(&StoreEnvConfigSource { store_values });
    let dirs = snapshot
        .providers
        .iter()
        .find(|provider| provider.source == Source::Git)
        .and_then(|git| git.fields.iter().find(|field| field.key == "GIT_DIRS"))
        .expect("git dirs field");

    assert!(dirs.is_set);
    assert_eq!(
        dirs.value,
        ProviderConfigFieldValueDto::List {
            item_type: ProviderConfigValueTypeDto::String,
            items: Vec::new(),
        }
    );
}

#[test]
fn rejects_lists_by_name_for_a_list_key() {
    let mut store_values = std::collections::BTreeMap::new();
    let map = [("NixOS".to_string(), vec!["~".to_string()])].into();

    let error = apply_provider_config_updates(
        &mut store_values,
        &git_dirs_request(Some(ProviderConfigValueDto::Map(map))),
    )
    .expect_err("a map is not a list");
    assert!(error.contains("not lists by name"), "{error}");
}

#[cfg(target_os = "windows")]
#[test]
fn saves_wsl_folders_by_distribution_and_keeps_an_empty_map() {
    let mut store_values = std::collections::BTreeMap::new();
    let request =
        |map: std::collections::BTreeMap<String, Vec<String>>| ProviderConfigSaveRequestDto {
            source: Source::Git,
            values: vec![ProviderConfigSaveValueDto {
                key: "WSL_GIT_DIRS".to_string(),
                value: Some(ProviderConfigValueDto::Map(map)),
            }],
        };

    let map = [(
        " NixOS ".to_string(),
        vec![" ~/work ".to_string(), String::new()],
    )]
    .into();
    apply_provider_config_updates(&mut store_values, &request(map)).expect("valid map");
    assert_eq!(
        store_values.get("WSL_GIT_DIRS"),
        Some(&Some(r#"{"NixOS":["~/work"]}"#.to_string()))
    );

    apply_provider_config_updates(&mut store_values, &request(Default::default()))
        .expect("empty map");
    assert_eq!(
        store_values.get("WSL_GIT_DIRS"),
        Some(&Some("{}".to_string()))
    );
}
