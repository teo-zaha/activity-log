//! Where a change (a GitLab merge request, later a GitHub pull request) stands among its
//! repository's open changes: one wire type every code host's entries carry. `ChangeStack` is
//! here, its parts one per file.

pub mod change_stack_above;
pub mod change_stack_bottom;
pub mod change_stack_end;
pub mod change_stack_node;
pub mod stack_cut;
pub mod stacked_change;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use self::{
    change_stack_above::ChangeStackAbove, change_stack_bottom::ChangeStackBottom,
    change_stack_end::ChangeStackEnd, stacked_change::StackedChange,
};

/// Where a change (a merge or pull request) stands among its repository's open changes, as the
/// host had it when this collection ran, not on the entry's day. Absent when the change is not
/// open, is in no stack, or the lookup could not tell whether it is open.
///
/// Made only by [`ChangeStack::new`], which reading one checks too: a stack has something
/// below or above its change, no blank title or branch, and only http(s) links without user
/// info.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", try_from = "RawChangeStack")]
#[ts(export, rename_all = "camelCase")]
pub struct ChangeStack {
    /// The entry's own change as it is now.
    current: StackedChange,
    /// What it builds on, from the stack's base up to the change whose source branch it
    /// targets; empty when it is the base.
    below: Vec<StackedChange>,
    bottom: ChangeStackBottom,
    above: ChangeStackAbove,
}

/// A stack as it is read, before [`ChangeStack::new`] checks it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawChangeStack {
    current: StackedChange,
    below: Vec<StackedChange>,
    bottom: ChangeStackBottom,
    above: ChangeStackAbove,
}

/// What [`ChangeStack::new`] refuses, when a stack is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InvalidChangeStack;

impl std::fmt::Display for InvalidChangeStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "a change stack must hold more than its change, without blank titles or branches and with http(s) links only",
        )
    }
}

impl std::error::Error for InvalidChangeStack {}

impl TryFrom<RawChangeStack> for ChangeStack {
    type Error = InvalidChangeStack;

    fn try_from(raw: RawChangeStack) -> Result<Self, Self::Error> {
        Self::new(raw.current, raw.below, raw.bottom, raw.above).ok_or(InvalidChangeStack)
    }
}

impl ChangeStack {
    /// The stack, or `None` for one that holds nothing but its change (nothing below, the
    /// base reached, nothing above and that complete), a blank title or branch, or a link that
    /// is not http(s) with a host and without user info.
    pub fn new(
        current: StackedChange,
        below: Vec<StackedChange>,
        bottom: ChangeStackBottom,
        above: ChangeStackAbove,
    ) -> Option<Self> {
        let lonely = below.is_empty()
            && matches!(bottom, ChangeStackBottom::Reached { .. })
            && above.changes.is_empty()
            && above.end == ChangeStackEnd::Complete;
        let bottom_valid = match &bottom {
            ChangeStackBottom::Reached { branch } => !branch.trim().is_empty(),
            ChangeStackBottom::Ambiguous | ChangeStackBottom::Cut { .. } => true,
        };
        let valid = !lonely
            && bottom_valid
            && valid_change(&current)
            && below.iter().all(valid_change)
            && valid_above(&above);
        valid.then_some(Self {
            current,
            below,
            bottom,
            above,
        })
    }

    pub fn current(&self) -> &StackedChange {
        &self.current
    }

    pub fn below(&self) -> &[StackedChange] {
        &self.below
    }

    pub fn bottom(&self) -> &ChangeStackBottom {
        &self.bottom
    }

    pub fn above(&self) -> &ChangeStackAbove {
        &self.above
    }
}

fn valid_above(above: &ChangeStackAbove) -> bool {
    above
        .changes
        .iter()
        .all(|node| valid_change(&node.change) && valid_above(&node.above))
}

fn valid_change(change: &StackedChange) -> bool {
    !change.title.trim().is_empty() && change.web_url.as_deref().is_none_or(is_web_link)
}

/// An http(s) address with a host and no user info. Which origin it may have is known only
/// where the stack is collected (`collect::change_stack::web_link`).
fn is_web_link(text: &str) -> bool {
    Url::parse(text).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.has_host()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[cfg(test)]
mod tests;
