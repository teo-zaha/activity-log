use strum::{AsRefStr, Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, AsRefStr, EnumString)]
#[strum(serialize_all = "lowercase", ascii_case_insensitive)]
pub enum ProviderConfigValueType {
    String,
    Bool,
    Int,
    Password,
    Email,
    Url,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderConfigKey {
    pub name: &'static str,
    pub required: bool,
    pub value_type: ProviderConfigValueType,
}

impl ProviderConfigKey {
    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn required(name: &'static str, value_type: ProviderConfigValueType) -> Self {
        Self {
            name,
            required: true,
            value_type,
        }
    }

    pub const fn optional(name: &'static str, value_type: ProviderConfigValueType) -> Self {
        Self {
            name,
            required: false,
            value_type,
        }
    }

    pub const fn required_string(name: &'static str) -> Self {
        Self::required(name, ProviderConfigValueType::String)
    }

    pub const fn required_bool(name: &'static str) -> Self {
        Self::required(name, ProviderConfigValueType::Bool)
    }

    pub const fn required_int(name: &'static str) -> Self {
        Self::required(name, ProviderConfigValueType::Int)
    }

    pub const fn required_password(name: &'static str) -> Self {
        Self::required(name, ProviderConfigValueType::Password)
    }

    pub const fn required_email(name: &'static str) -> Self {
        Self::required(name, ProviderConfigValueType::Email)
    }

    pub const fn required_url(name: &'static str) -> Self {
        Self::required(name, ProviderConfigValueType::Url)
    }

    pub const fn optional_string(name: &'static str) -> Self {
        Self::optional(name, ProviderConfigValueType::String)
    }

    pub const fn optional_bool(name: &'static str) -> Self {
        Self::optional(name, ProviderConfigValueType::Bool)
    }

    pub const fn optional_int(name: &'static str) -> Self {
        Self::optional(name, ProviderConfigValueType::Int)
    }

    pub const fn optional_password(name: &'static str) -> Self {
        Self::optional(name, ProviderConfigValueType::Password)
    }

    pub const fn optional_email(name: &'static str) -> Self {
        Self::optional(name, ProviderConfigValueType::Email)
    }

    pub const fn optional_url(name: &'static str) -> Self {
        Self::optional(name, ProviderConfigValueType::Url)
    }
}
