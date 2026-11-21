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

/// What a key holds: one value, or a list of values of one type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderConfigShape {
    Single(ProviderConfigValueType),
    /// Several values, such as folders. Stored as a JSON array of strings; a plain value is read
    /// as a comma-separated list, so `GIT_DIRS=a,b` works from the environment.
    List(ProviderConfigValueType),
}

impl ProviderConfigShape {
    /// The type of the value, or of every item of a list.
    pub const fn value_type(self) -> ProviderConfigValueType {
        match self {
            Self::Single(value_type) | Self::List(value_type) => value_type,
        }
    }
}

impl std::fmt::Display for ProviderConfigShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Single(value_type) => write!(f, "{value_type}"),
            Self::List(value_type) => write!(f, "list of {value_type}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderConfigKey {
    /// The wire name, as `config.toml` and the settings save spell it: `GITLAB_URL`.
    pub name: &'static str,
    /// The field's name for people, as the settings show it: `GitLab URL`.
    pub label: &'static str,
    pub required: bool,
    pub shape: ProviderConfigShape,
    /// Says which account a connection uses (a server address, a token): a settings save based
    /// on an older value of it is a conflict even when it does not touch it.
    pub account: bool,
}

impl ProviderConfigKey {
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The type of the value, or of every item of a list.
    pub const fn value_type(self) -> ProviderConfigValueType {
        self.shape.value_type()
    }

    /// A required key labelled by its name; [`Self::labelled`] names it for people.
    pub const fn required(name: &'static str, value_type: ProviderConfigValueType) -> Self {
        Self {
            name,
            label: name,
            required: true,
            shape: ProviderConfigShape::Single(value_type),
            account: false,
        }
    }

    /// An optional key labelled by its name; [`Self::labelled`] names it for people.
    pub const fn optional(name: &'static str, value_type: ProviderConfigValueType) -> Self {
        Self {
            name,
            label: name,
            required: false,
            shape: ProviderConfigShape::Single(value_type),
            account: false,
        }
    }

    /// This key with `label` for people: `optional("GIT_DIRS", String).labelled("Git Dirs")`.
    pub const fn labelled(self, label: &'static str) -> Self {
        Self { label, ..self }
    }

    /// This key as a list of its value type: `optional("GIT_DIRS", String).list_of()`.
    pub const fn list_of(self) -> Self {
        Self {
            shape: ProviderConfigShape::List(self.value_type()),
            ..self
        }
    }
}
