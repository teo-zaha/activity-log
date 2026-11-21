use std::time::Duration;

use windows::Win32::{
    Foundation::FILETIME,
    System::Threading::{GetCurrentThread, GetThreadTimes},
};

pub(super) fn thread_time() -> Duration {
    let (mut created, mut exited, mut kernel, mut user) = Default::default();
    // SAFETY: the pseudo handle of the current thread is always valid, and each pointer is to
    // a writable `FILETIME`.
    unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    }
    .expect("the thread's processor times are unavailable");
    // FILETIME counts in units of 100 ns.
    Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}

fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}
