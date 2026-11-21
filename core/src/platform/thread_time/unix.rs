use std::time::Duration;

pub(super) fn thread_time() -> Duration {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `time` is a valid, writable `timespec`; the clock exists on macOS and Linux.
    let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
    assert_eq!(status, 0, "the thread's processor clock is unavailable");
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}
