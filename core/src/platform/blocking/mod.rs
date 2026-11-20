//! Blocking OS work (a folder walk, COM, the event log, `wsl.exe`) on a thread of its own. A
//! stopped or timed-out collection drops the future that waits for it, but cannot stop the
//! thread, so the work gets a flag that says nobody waits any more, ends at its next check, and
//! kills the process it waits for. A call that cannot be interrupted (one into Outlook) holds
//! its resource until it returns: the next job for it waits, and gives up when it is stopped.

use std::{
    collections::HashMap,
    io::{Read, Write},
    process::{Command, Output, Stdio},
    sync::{
        Arc, Mutex, TryLockError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, anyhow};

use crate::diagnostics;

#[cfg(test)]
mod tests;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

/// How often waiting work looks at its flag.
const POLL: Duration = Duration::from_millis(25);

/// Work that ended because nobody waited for it any more: the collection was replaced or timed
/// out. Told apart from a failure by its type (`err.is::<Stopped>()`), never by its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stopped;

impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Stopped: the collection was replaced or timed out")
    }
}

impl std::error::Error for Stopped {}

/// Sets its flag when dropped: the future waiting for the work is gone.
struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Runs `work` on a blocking thread with a flag that is set once nobody waits for it: the
/// collection was stopped (another range, a settings change) or timed out. `work` checks the
/// flag and ends early. Its log lines go to the connection being collected.
pub(crate) async fn blocking_until_dropped<T: Send + 'static>(
    what: &'static str,
    work: impl FnOnce(&AtomicBool) -> T + Send + 'static,
) -> Result<T> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel_on_drop = CancelOnDrop(cancelled.clone());
    let connection = diagnostics::current_connection();
    tokio::task::spawn_blocking(move || {
        diagnostics::in_connection_sync(connection, || work(&cancelled))
    })
    .await
    .map_err(|err| anyhow!("{what} failed: {err}"))
}

/// One job at a time for an OS resource.
// Used by the Windows integrations; tested everywhere.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct Exclusive(Mutex<()>);

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl Exclusive {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(()))
    }

    /// Runs `work` once no other job holds the resource; gives up with [`Stopped`] when
    /// `cancelled` is set while it waits.
    pub(crate) fn run<T>(&self, cancelled: &AtomicBool, work: impl FnOnce() -> T) -> Result<T> {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err(anyhow::Error::new(Stopped));
            }
            match self.0.try_lock() {
                // A job that panicked left nothing to repair.
                Ok(_held) => return Ok(work()),
                Err(TryLockError::Poisoned(poisoned)) => {
                    let _held = poisoned.into_inner();
                    return Ok(work());
                }
                Err(TryLockError::WouldBlock) => std::thread::sleep(POLL),
            }
        }
    }
}

/// [`Exclusive`] per name, such as one per WSL distribution.
// Used by the Windows integrations; tested everywhere.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct ExclusiveByName(Mutex<HashMap<String, Arc<Exclusive>>>);

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl ExclusiveByName {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    pub(crate) fn get(&self, name: &str) -> Arc<Exclusive> {
        let mut by_name = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        by_name
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Exclusive::new()))
            .clone()
    }
}

/// How long the threads that write and read a finished or stopped child's pipes are waited
/// for once its processes are killed.
const DRAIN: Duration = Duration::from_secs(2);

/// Runs `command` with `input` on its standard input and its output read, all while watching
/// `cancelled`. The child and every process it starts are one tree (a process group, a job
/// object on Windows): once `cancelled` is set the tree is killed and the wait ends, and once
/// the child ends what it left running is killed too, so nothing keeps its pipes open. A child
/// that ends without reading all of its input failed.
// Used by the Windows integrations; tested everywhere.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn run_cancellable(
    command: &mut Command,
    input: Option<Vec<u8>>,
    cancelled: &AtomicBool,
) -> Result<Output> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    imp::prepare(command);
    let mut child = command.spawn()?;
    let tree = match imp::adopt(&child) {
        Ok(tree) => tree,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!(
                "Cannot keep track of the program's processes: {err}"
            ));
        }
    };
    let writer = input
        .zip(child.stdin.take())
        .map(|(input, mut stdin)| std::thread::spawn(move || stdin.write_all(&input)));
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if cancelled.load(Ordering::Relaxed) {
            break None;
        }
        std::thread::sleep(POLL);
    };
    tree.kill();
    let _ = child.kill();
    let _ = child.wait();

    // With the tree gone the pipes close and the threads end.
    let started = std::time::Instant::now();
    let drained = || {
        stdout.is_finished()
            && stderr.is_finished()
            && writer.as_ref().is_none_or(|writer| writer.is_finished())
    };
    while !drained() && started.elapsed() < DRAIN {
        std::thread::sleep(POLL);
    }
    let Some(status) = status else {
        return Err(anyhow::Error::new(Stopped));
    };
    if !drained() {
        return Err(anyhow!(
            "The program's output stayed open after it ended; it was left running"
        ));
    }
    if let Some(Err(err)) = writer.map(|writer| writer.join().unwrap_or(Ok(()))) {
        return Err(anyhow!(
            "The program ended without reading all of its input: {err}"
        ));
    }
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}
