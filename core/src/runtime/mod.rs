//! The collection state shared by the API server and the desktop app: one [`Host`] per process
//! owns the installed config's connections and their checks, the collection on screen with
//! its per-connection results, config reloads and edits, and what it publishes.

use std::{
    any::Any,
    sync::{Mutex, MutexGuard},
};

use chrono::{DateTime, Duration, Utc};

use crate::model::{CollectOptions, CollectRequest};

pub mod host;
pub(crate) mod keys;
pub mod report;
pub(crate) mod result_store;
#[cfg(test)]
mod tests;

pub use host::{BadConfig, Edit, Host, HostWork, Part, Published, RunSpawner};
pub use report::{
    Checked, CollectionKey, Collector, Outcome, Plan, Progress, RunReport, Slot, Stored, Superseded,
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

pub fn resolve_collect_options(
    request: &CollectRequest,
) -> std::result::Result<CollectOptions, &'static str> {
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .map(|instant| instant.with_timezone(&Utc))
            .map_err(|_| "start and end must be RFC 3339 timestamps")
    };
    let options = CollectOptions {
        start: parse(&request.start)?,
        end: parse(&request.end)?,
    };
    options.check()?;
    Ok(options)
}

/// The last 30 days up to now; the dashboard re-collects whole local days when asked.
fn default_collect_options() -> CollectOptions {
    let end = Utc::now();
    CollectOptions {
        start: end - Duration::days(30),
        end,
    }
}
