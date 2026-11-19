use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{list_item::ListItem, text_segment::TextSegment};

/// A block of a meeting description as its organizer laid it out, read from HTML, Markdown or
/// the plain text Outlook writes for a formatted text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[ts(export, tag = "kind", rename_all = "lowercase")]
pub enum DescriptionBlock {
    /// Lines that belong together.
    Paragraph {
        segments: Vec<TextSegment>,
    },
    Heading {
        segments: Vec<TextSegment>,
    },
    List {
        ordered: bool,
        items: Vec<ListItem>,
    },
    /// An image on a line of its own. `src` is an https link to it; without one the source
    /// named the image but its data is not in the feed (an attachment), and `alt` is all there
    /// is to show. An empty `alt` marks an image the author made decorative; it always has a
    /// `src`.
    Image {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        src: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        alt: Option<String>,
    },
}
