//! The collection state shared by the API server and the desktop app: one [`Host`] per process
//! owns the installed config's connections and their checks, the collection on screen with
//! its per-connection results, config reloads and edits, and what it publishes.

use std::{
    any::Any,
    sync::{Mutex, MutexGuard},
};

use chrono::{DateTime, Duration, Utc};

use crate::{
    backend_error::refused_recheck,
    model::{
        BackendError, CollectAnswer, CollectOptions, CollectRequest, ConnectionId, RangeError,
        RecheckAnswer, ShownActivity,
    },
};

pub mod forward;
pub mod host;
pub(crate) mod keys;
pub mod report;
pub(crate) mod result_store;
#[cfg(test)]
mod tests;

pub use forward::{Due, Forwarder, PING_INTERVAL, RuntimeMessage};
pub use host::{
    Answered, BadConfig, Edit, Feed, Host, HostWork, Part, Published, RecheckRefused, RunSpawner,
};
pub use report::{
    Checked, CollectFailure, CollectionKey, Collector, Outcome, Plan, Progress, RunReport, Slot,
    Stored, Superseded,
};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn panic_message(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("panic")
}

pub fn resolve_collect_options(request: &CollectRequest) -> Result<CollectOptions, RangeError> {
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .map(|instant| instant.with_timezone(&Utc))
            .map_err(|_| RangeError::NotRfc3339)
    };
    let options = CollectOptions {
        start: parse(&request.start)?,
        end: parse(&request.end)?,
    };
    options.check()?;
    Ok(options)
}

/// What the API and the desktop app answer a collect request with: the activity the host
/// installed or shows, with its stamp. A range the host would not collect is refused; a host that
/// shows nothing yet (it is quitting) has no data.
pub fn answer_collect(
    host: &Host,
    request: &CollectRequest,
) -> Result<CollectAnswer, BackendError> {
    let answered = host.collect(resolve_collect_options(request)?);
    let activity = ShownActivity::of(answered.value).ok_or_else(BackendError::no_data)?;
    Ok(CollectAnswer {
        activity,
        stamp: answered.stamp,
    })
}

/// What the API and the desktop app answer a recheck of `id` with: the providers, `id`
/// checking, and their stamp; a connection the host does not have is not found.
pub fn answer_recheck(host: &Host, id: &ConnectionId) -> Result<RecheckAnswer, BackendError> {
    let answered = host
        .recheck(id)
        .map_err(|refused| refused_recheck(refused, id))?;
    Ok(RecheckAnswer {
        providers: answered.value,
        stamp: answered.stamp,
    })
}

/// The last 30 days up to now; the dashboard re-collects whole local days when asked.
fn default_collect_options() -> CollectOptions {
    let end = Utc::now();
    CollectOptions {
        start: end - Duration::days(30),
        end,
    }
}
