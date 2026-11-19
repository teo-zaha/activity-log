use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A run of a meeting description: text, bold or italic, or a web link. The dashboard renders
/// the runs as text nodes and links, never as markup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[ts(export, tag = "kind", rename_all = "lowercase")]
pub enum TextSegment {
    /// `\n` in `text` is a line break.
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "is_false")]
        #[ts(as = "Option<bool>", optional)]
        strong: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        #[ts(as = "Option<bool>", optional)]
        emphasis: bool,
    },
    /// An http(s) link with a host; `text` is its label, or the link as written.
    Link { text: String, href: String },
}

impl TextSegment {
    /// Plain text.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            strong: false,
            emphasis: false,
        }
    }

    pub fn link(text: impl Into<String>, href: impl Into<String>) -> Self {
        Self::Link {
            text: text.into(),
            href: href.into(),
        }
    }
}

fn is_false(value: &bool) -> bool {
    !value
}
