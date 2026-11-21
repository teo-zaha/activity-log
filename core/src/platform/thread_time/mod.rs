//! The processor time the current thread has used, for tests that measure how work grows:
//! unlike the clock, other processes competing for the processor do not add to it.

use std::time::Duration;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use self::unix as imp;
#[cfg(target_os = "windows")]
use self::windows as imp;

/// The processor time the calling thread has used so far.
pub(crate) fn thread_time() -> Duration {
    imp::thread_time()
}
