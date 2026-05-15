use activity_log_core::{
    model::{
        ProviderConfigSaveRequestDto, ProviderConfigSaveValueDto, ProviderConfigValueTypeDto,
        Source,
    },
    provider::{git::default_git_dirs_value, provider_config::ProviderConfigValueType},
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

    assert!(token.secret);
    assert!(token.is_set);
    assert_eq!(token.value, None);
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
            value: Some("basic".to_string()),
        }],
    };

    let result = apply_provider_config_updates(&mut store_values, &request);
    assert!(result.is_err());
}

#[test]
fn snapshot_shows_default_git_dirs_value_when_unset() {
    let Some(default_value) = default_git_dirs_value() else {
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
    assert_eq!(dirs.value.as_deref(), Some(default_value.as_str()));
}
