use std::collections::HashSet;

use serde::Deserialize;

use crate::{
    collect::change_stack::{FailureKind, LookupCause, LookupFailure, StackSeed},
    model::{ChangeStack, ChangeStackAbove, ChangeStackBottom, ChangeStackEnd, StackCut},
    provider::remote_error::{ApiError, QueryErrorKind, QueryPathSegment},
};

use super::lookup::{GitHubChanges, PullRow, State, ignored_filter, request_failure};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativePull {
    #[serde(flatten)]
    pull: PullRow,
    #[serde(deserialize_with = "Option::deserialize")]
    stack_entry: Option<Membership>,
}

#[derive(Deserialize)]
struct Membership {
    position: u32,
    #[serde(deserialize_with = "Option::deserialize")]
    stack: Option<NativeStack>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeStack {
    base_ref_name: String,
    entries: Entries,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entries {
    page_info: PageInfo,
    nodes: Vec<PositionedPull>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionedPull {
    position: u32,
    pull_request: Option<PullRow>,
}

pub(super) enum NativeResult {
    /// The pull request is open and GitHub reports no native stack.
    Walk,
    /// The native positions are known and the shared walk can extend them at either end.
    Seed(StackSeed),
    /// The pull request is closed, or its native membership already answers the question.
    Resolved(Option<ChangeStack>),
}

pub(super) fn unsupported(failure: &LookupFailure) -> bool {
    let Some(LookupCause::Request(ApiError::Query(refusal))) = failure.cause() else {
        return false;
    };
    refusal.errors().iter().all(|error| {
        error.kind == QueryErrorKind::UndefinedField
            && error.path.as_ref().is_none_or(|path| {
                matches!(path.last(), Some(QueryPathSegment::Field(field)) if field == "stackEntry")
            })
    })
}

impl NativePull {
    pub(super) fn stack(
        self,
        wanted: u64,
        lookup: &GitHubChanges<'_>,
    ) -> Result<NativeResult, LookupFailure> {
        if self.pull.summary.number != wanted {
            return Err(ignored_filter());
        }
        if self.pull.summary.state != State::Open {
            lookup.not_open();
            return Ok(NativeResult::Resolved(None));
        }
        let Some(Membership {
            position,
            stack: Some(stack),
        }) = self.stack_entry
        else {
            return Ok(NativeResult::Walk);
        };
        let current = lookup
            .change(self.pull)
            .ok_or_else(|| request_failure(ApiError::Decode))?;
        if stack.base_ref_name.trim().is_empty() || position == 0 {
            return Err(request_failure(ApiError::Decode));
        }
        let mut rows = stack.entries.nodes;
        rows.sort_unstable_by_key(|row| row.position);
        let more = stack.entries.page_info.has_next_page;
        let mut numbers = HashSet::new();
        let mut positions = HashSet::new();
        let mut below = Vec::new();
        let mut above = Vec::new();
        let mut found = false;
        for row in rows {
            let pull = row.pull_request.ok_or(FailureKind::Unreadable)?;
            if row.position == 0
                || pull.summary.number == 0
                || !positions.insert(row.position)
                || !numbers.insert(pull.summary.number)
            {
                return Err(ignored_filter());
            }
            if row.position == position || pull.summary.number == wanted {
                if row.position != position
                    || pull.summary.number != wanted
                    || pull.summary.state != State::Open
                {
                    return Err(ignored_filter());
                }
                found = true;
                continue;
            }
            if pull.summary.state == State::Open {
                let change = lookup
                    .change(pull)
                    .ok_or_else(|| request_failure(ApiError::Decode))?;
                if row.position < position {
                    below.push(change);
                } else {
                    above.push(change);
                }
            }
        }
        if !found {
            // A capped first page cannot establish this pull request's immediate neighbours.
            if more && positions.iter().all(|&seen| seen < position) {
                return Ok(NativeResult::Resolved(ChangeStack::new(
                    current.stacked(),
                    Vec::new(),
                    ChangeStackBottom::Cut {
                        reason: StackCut::Limit,
                    },
                    ChangeStackAbove {
                        changes: Vec::new(),
                        end: ChangeStackEnd::Cut {
                            reason: StackCut::Limit,
                        },
                    },
                )));
            }
            return Err(ignored_filter());
        }
        Ok(NativeResult::Seed(StackSeed {
            current,
            below,
            base_branch: stack.base_ref_name,
            above,
            more_above: more,
        }))
    }
}
