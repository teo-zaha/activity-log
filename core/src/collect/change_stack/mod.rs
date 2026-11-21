//! Change stacks, whatever the code host: which open changes (merge or pull requests) a
//! change builds on and which build on it, by the branches they come from and go into.
//!
//! A host implements [`ChangeLookup`] over its repository key; [`change_stacks`] does the rest
//! for one connection: one page of a repository's open changes first, which answers every walk
//! there from memory when it holds them all; otherwise the host is asked per branch, each
//! question once (single-flight), within the connection's [`Budget`] of requests and time and
//! all inside the caller's future (no task is spawned), so dropping it stops every request.

mod budget;
mod lookup;
mod walk;
mod web_link;

pub(crate) use budget::{Budget, STACK_BUDGET};
pub(crate) use lookup::{
    ChangeLookup, ChangeRef, FailureKind, LookupCause, LookupFailure, OpenChange, OpenPage,
};
pub(crate) use walk::change_stacks;
pub(crate) use web_link::web_link;
