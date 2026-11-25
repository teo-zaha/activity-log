//! In-memory diagnostic log shown in the dashboard settings.
//!
//! The desktop app has no console on Windows, so anything only written to stderr is invisible
//! there. Entries are kept in a bounded ring buffer; warnings and errors are still mirrored to
//! stderr for the CLI and API.

use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    sync::{LazyLock, Mutex},
};

use crate::{
    model::{ConnectionId, LogEntry, LogLevel},
    utils::now_iso,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use local_log::LocalLog;

const CAPACITY: usize = 1000;

static LOG: LazyLock<Mutex<LogBuffer>> = LazyLock::new(|| Mutex::new(LogBuffer::new(CAPACITY)));

pub(crate) struct LogBuffer {
    capacity: usize,
    next_id: u32,
    entries: VecDeque<LogEntry>,
}

impl LogBuffer {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            next_id: 1,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    pub(crate) fn push(&mut self, level: LogLevel, scope: String, message: String) -> &LogEntry {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.entries.push_back(LogEntry {
            id,
            timestamp: now_iso(),
            level,
            scope,
            message,
        });
        self.entries.back().expect("entry was just pushed")
    }

    pub(crate) fn entries(&self) -> Vec<LogEntry> {
        self.entries.iter().cloned().collect()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

tokio::task_local! {
    static CONNECTION: ConnectionId;
}

thread_local! {
    static THREAD_CONNECTION: RefCell<Option<ConnectionId>> = const { RefCell::new(None) };
}

/// Runs `future` for one connection: lines it logs under the connection's kind (`git`) are
/// shown under the connection (`git/wsl-nixos`), so collectors need not know about connections.
pub async fn in_connection<F: Future>(connection: ConnectionId, future: F) -> F::Output {
    CONNECTION.scope(connection, future).await
}

/// The connection being collected on this task or thread, to hand to blocking work.
pub fn current_connection() -> Option<ConnectionId> {
    THREAD_CONNECTION
        .with(|current| current.borrow().clone())
        .or_else(|| CONNECTION.try_with(Clone::clone).ok())
}

/// Like [`in_connection`], for a blocking closure such as one in `spawn_blocking`.
pub fn in_connection_sync<R>(connection: Option<ConnectionId>, f: impl FnOnce() -> R) -> R {
    /// Puts the previous connection back, also when `f` panics: blocking threads are reused.
    struct Restore(Option<ConnectionId>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            THREAD_CONNECTION.with(|current| current.replace(previous));
        }
    }
    let _restore = Restore(THREAD_CONNECTION.with(|current| current.replace(connection)));
    f()
}

fn connection_scope(scope: String) -> String {
    match current_connection() {
        Some(connection) if scope == connection.source.as_ref() => connection.to_string(),
        _ => scope,
    }
}

fn with_log<R>(f: impl FnOnce(&mut LogBuffer) -> R) -> R {
    // A panic while holding the lock must not take diagnostics down with it.
    let mut guard = LOG.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

pub fn record(level: LogLevel, scope: impl Into<String>, message: impl Into<String>) {
    let (scope, message) = (connection_scope(scope.into()), message.into());
    // Writing to stderr can block; the host's state lock must never wait for it.
    #[cfg(test)]
    assert!(
        !crate::runtime::host::state_lock_held(),
        "logged under the host state lock: [{scope}] {message}"
    );
    if level >= LogLevel::Warn {
        // With standard error gone (a closed pipe) the line is in the log all the same.
        let _ = std::io::Write::write_fmt(
            &mut std::io::stderr(),
            format_args!("  [{scope}] {message}\n"),
        );
    }
    #[cfg(test)]
    local_log::record(level, &scope, &message);
    with_log(|log| {
        log.push(level, scope, message);
    });
}

/// Prints developer diagnostics in debug builds without adding them to the settings log.
pub fn debug(scope: &str, message: &str) {
    if cfg!(debug_assertions) {
        let _ = std::io::Write::write_fmt(
            &mut std::io::stderr(),
            format_args!("  [{scope}] DEBUG {message}\n"),
        );
    }
}

pub fn info(scope: impl Into<String>, message: impl Into<String>) {
    record(LogLevel::Info, scope, message);
}

pub fn warn(scope: impl Into<String>, message: impl Into<String>) {
    record(LogLevel::Warn, scope, message);
}

pub fn error(scope: impl Into<String>, message: impl Into<String>) {
    record(LogLevel::Error, scope, message);
}

pub fn entries() -> Vec<LogEntry> {
    with_log(|log| log.entries())
}

pub fn clear() {
    with_log(LogBuffer::clear);
}

#[cfg(test)]
mod local_log {
    use std::rc::Rc;

    use super::*;

    thread_local! {
        static ACTIVE: RefCell<Option<Rc<RefCell<LogBuffer>>>> = const { RefCell::new(None) };
    }

    /// Captures logs on this thread, including tasks on a current-thread test runtime.
    pub(crate) struct LocalLog(Rc<RefCell<LogBuffer>>);

    impl LocalLog {
        pub(crate) fn start() -> Self {
            let log = Rc::new(RefCell::new(LogBuffer::new(CAPACITY)));
            ACTIVE.with(|active| {
                assert!(active.borrow().is_none());
                active.replace(Some(log.clone()));
            });
            Self(log)
        }

        pub(crate) fn entries(&self) -> Vec<LogEntry> {
            self.0.borrow().entries()
        }
    }

    impl Drop for LocalLog {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.replace(None));
        }
    }

    pub(super) fn record(level: LogLevel, scope: &str, message: &str) {
        ACTIVE.with(|active| {
            if let Some(log) = active.borrow().as_ref() {
                log.borrow_mut().push(level, scope.into(), message.into());
            }
        });
    }
}
