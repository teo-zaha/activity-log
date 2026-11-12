//! In-memory diagnostic log shown in the dashboard settings.
//!
//! The desktop app has no console on Windows, so anything only written to stderr is invisible
//! there. Entries are kept in a bounded ring buffer; warnings and errors are still mirrored to
//! stderr for the CLI and API.

use std::{
    collections::VecDeque,
    sync::{LazyLock, Mutex},
};

use crate::{
    model::{LogEntry, LogLevel},
    utils::now_iso,
};

#[cfg(test)]
mod tests;

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

fn with_log<R>(f: impl FnOnce(&mut LogBuffer) -> R) -> R {
    // A panic while holding the lock must not take diagnostics down with it.
    let mut guard = LOG.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

pub fn record(level: LogLevel, scope: impl Into<String>, message: impl Into<String>) {
    let (scope, message) = (scope.into(), message.into());
    if level >= LogLevel::Warn {
        eprintln!("  [{scope}] {message}");
    }
    with_log(|log| {
        log.push(level, scope, message);
    });
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
