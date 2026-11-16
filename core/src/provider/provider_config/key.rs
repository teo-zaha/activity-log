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

/// What a key holds: one value, a list of values, or lists of values by name, all of one type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderConfigShape {
    Single(ProviderConfigValueType),
    /// Several values, such as folders. Stored as a JSON array of strings; a plain value is read
    /// as a comma-separated list, so `GIT_DIRS=a,b` works from the environment.
    List(ProviderConfigValueType),
    /// A list of values under each name, such as folders per WSL distribution. Stored as a JSON
    /// object of string arrays: `{"Ubuntu":["~/work"]}`.
    Map(ProviderConfigValueType),
}

impl ProviderConfigShape {
    /// The type of the value, or of every item of a list or map.
    pub const fn value_type(self) -> ProviderConfigValueType {
        match self {
            Self::Single(value_type) | Self::List(value_type) | Self::Map(value_type) => value_type,
        }
    }
}

impl std::fmt::Display for ProviderConfigShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Single(value_type) => write!(f, "{value_type}"),
            Self::List(value_type) => write!(f, "list of {value_type}"),
            Self::Map(value_type) => write!(f, "lists of {value_type} by name"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderConfigKey {
    pub name: &'static str,
    pub required: bool,
    pub shape: ProviderConfigShape,
}

impl ProviderConfigKey {
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The type of the value, or of every item of a list or map.
    pub const fn value_type(self) -> ProviderConfigValueType {
        self.shape.value_type()
    }

    pub const fn required(name: &'static str, value_type: ProviderConfigValueType) -> Self {
        Self {
            name,
            required: true,
            shape: ProviderConfigShape::Single(value_type),
        }
    }

    pub const fn optional(name: &'static str, value_type: ProviderConfigValueType) -> Self {
        Self {
            name,
            required: false,
            shape: ProviderConfigShape::Single(value_type),
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

    /// This key as a list of its value type: `optional_string("GIT_DIRS").list_of()`.
    pub const fn list_of(self) -> Self {
        Self {
            shape: ProviderConfigShape::List(self.value_type()),
            ..self
        }
    }

    /// This key as lists of its value type by name: `optional_string("WSL_GIT_DIRS").map_of()`.
    pub const fn map_of(self) -> Self {
        Self {
            shape: ProviderConfigShape::Map(self.value_type()),
            ..self
        }
    }
}
