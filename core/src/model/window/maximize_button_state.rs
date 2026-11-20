use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The desktop's `maximize-button` event on Windows: where the pointer is over the invisible
/// window that stands over the dashboard's maximize button, which the button shows as its own
/// hover and press.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MaximizeButtonState {
    pub hovered: bool,
    pub pressed: bool,
}
