//! `source` of an entry: one value per kind, so an entry's struct can only name its own kind.

/// A field that is always `$wire` on the wire, and refused as anything else.
macro_rules! source_tag {
    ($(#[$doc:meta])* $name:ident = $wire:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name;

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str($wire)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let found = String::deserialize(deserializer)?;
                if found == $wire {
                    Ok(Self)
                } else {
                    Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Str(&found),
                        &$wire,
                    ))
                }
            }
        }
    };
}

source_tag!(
    /// `"git"`.
    GitTag = "git"
);
source_tag!(
    /// `"gitlab"`.
    GitLabTag = "gitlab"
);
source_tag!(
    /// `"github"`.
    GitHubTag = "github"
);
source_tag!(
    /// `"jira"`.
    JiraTag = "jira"
);
source_tag!(
    /// `"outlook"`.
    OutlookTag = "outlook"
);
source_tag!(
    /// `"system"`.
    SystemTag = "system"
);
source_tag!(
    /// `"calendar"`.
    CalendarTag = "calendar"
);
source_tag!(
    /// `"figma"`.
    FigmaTag = "figma"
);
