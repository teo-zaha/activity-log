use crate::{
    model::Source,
    provider::{adapter as adapter_of, provider_config::ProviderConfigKey},
};

/// The file's name for a config key: `GITLAB_URL` → `url`, `GIT_DIRS` → `dirs`.
pub fn field_name(source: &Source, key: ProviderConfigKey) -> String {
    let prefix = format!("{}_", source.as_ref().to_ascii_uppercase());
    key.name
        .strip_prefix(&prefix)
        .unwrap_or(key.name)
        .to_ascii_lowercase()
}

/// The config keys of `source`, as its provider declares them.
pub fn config_keys(source: &Source) -> &'static [ProviderConfigKey] {
    adapter_of(source).config_requirements()
}

/// The config key a file field names for `source`, as the CLI accepts it.
pub fn key_for_field(source: &Source, field: &str) -> Option<ProviderConfigKey> {
    config_keys(source)
        .iter()
        .copied()
        .find(|key| field_name(source, *key) == field.to_ascii_lowercase())
}
