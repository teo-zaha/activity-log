use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The dashboard's name for one of its menu items: what a click on it reports back and what an
/// update addresses. Unique within one menu; it means nothing outside the menu it was sent in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MenuItemKey(pub u32);

impl std::fmt::Display for MenuItemKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
