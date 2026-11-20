use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use super::{Exclusive, Stopped, blocking_until_dropped};

// Several threads: the test waits on the blocking work while a worker runs and drops it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_wait_stops_the_blocking_work() {
    let (started, has_started) = mpsc::channel();
    let (ended, has_ended) = mpsc::channel();
    let work = tokio::spawn(blocking_until_dropped("Test", move |cancelled| {
        started.send(()).unwrap();
        let mut steps = 0_u64;
        while !cancelled.load(Ordering::Relaxed) {
            steps += 1;
            std::thread::sleep(Duration::from_millis(1));
        }
        ended.send(steps).unwrap();
    }));
    has_started.recv_timeout(Duration::from_secs(5)).unwrap();

    // What `Abortable` and the collection timeout do to the run waiting for it.
    work.abort();

    let steps = has_ended
        .recv_timeout(Duration::from_secs(5))
        .expect("the blocking work saw it was no longer awaited");
    assert!(steps < 5_000);
}

#[test]
fn a_second_job_waits_for_the_first_and_gives_up_when_stopped() {
    let resource = Arc::new(Exclusive::new());
    let (holding, is_holding) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let first = {
        let resource = resource.clone();
        std::thread::spawn(move || {
            resource
                .run(&AtomicBool::new(false), || {
                    holding.send(()).unwrap();
                    released.recv().unwrap();
                })
                .unwrap();
        })
    };
    is_holding.recv().unwrap();

    let stopped = AtomicBool::new(true);
    let err = resource.run(&stopped, || ()).unwrap_err();
    assert!(err.is::<Stopped>(), "{err:#}");

    release.send(()).unwrap();
    first.join().unwrap();
    assert_eq!(resource.run(&AtomicBool::new(false), || 7).unwrap(), 7);
}

#[test]
fn a_stopped_job_does_not_start_on_a_free_resource() {
    let mut ran = false;
    let err = Exclusive::new()
        .run(&AtomicBool::new(true), || ran = true)
        .unwrap_err();
    assert!(err.is::<Stopped>(), "{err:#}");
    assert!(!ran);
}

/// Sets `cancelled` after `after`.
#[cfg(unix)]
fn cancel_after(after: Duration) -> Arc<AtomicBool> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    std::thread::spawn(move || {
        std::thread::sleep(after);
        flag.store(true, Ordering::Relaxed);
    });
    cancelled
}

/// Process tests: they run `sh`, `sleep` and `true`.
#[cfg(unix)]
mod processes {
    use std::{
        process::Command,
        sync::atomic::AtomicBool,
        time::{Duration, Instant},
    };

    use super::{super::run_cancellable, Stopped, cancel_after};

    fn alive(pid: i32) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// Until `pid` is gone, at most 2 s.
    fn ends(pid: i32) -> bool {
        let started = Instant::now();
        while alive(pid) {
            if started.elapsed() > Duration::from_secs(2) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        true
    }

    #[test]
    fn a_finished_child_gives_its_output() {
        let output = run_cancellable(
            Command::new("sh").args(["-c", "cat; echo err >&2"]),
            Some(b"in".to_vec()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"in");
        assert_eq!(output.stderr, b"err\n");
    }

    /// Stopping kills the child and every process it started, which then can no longer hold
    /// its output open; the wait ends at once.
    #[test]
    fn a_stop_kills_the_child_and_what_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
        let cancelled = cancel_after(Duration::from_millis(300));
        let started = Instant::now();

        let err = run_cancellable(Command::new("sh").args(["-c", &script]), None, &cancelled)
            .unwrap_err();

        assert!(err.is::<Stopped>(), "{err:#}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        let pid = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(ends(pid), "sleep {pid} still runs");
    }

    /// The child ended and left a process holding its output open: that process is killed,
    /// and the output comes at once.
    #[test]
    fn what_a_finished_child_left_running_is_killed() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let script = format!(
            "sleep 30 & echo $! > '{}'; echo started",
            pid_file.display()
        );
        let started = Instant::now();

        let output = run_cancellable(
            Command::new("sh").args(["-c", &script]),
            None,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(output.stdout, b"started\n");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        let pid = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(ends(pid), "sleep {pid} still runs");
    }

    /// Input the child does not read cannot hold the wait either.
    #[test]
    fn a_stop_ends_the_wait_while_the_input_is_still_being_written() {
        let cancelled = cancel_after(Duration::from_millis(200));
        let started = Instant::now();

        let err = run_cancellable(
            Command::new("sleep").arg("30"),
            Some(vec![b'x'; 4 << 20]),
            &cancelled,
        )
        .unwrap_err();

        assert!(err.is::<Stopped>(), "{err:#}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    /// A child that ends without reading its input did not do its work, even when it says so.
    #[test]
    fn a_child_that_leaves_its_input_unread_failed() {
        let err = run_cancellable(
            &mut Command::new("true"),
            Some(vec![b'x'; 4 << 20]),
            &AtomicBool::new(false),
        )
        .unwrap_err();

        assert!(err.to_string().contains("input"), "{err}");
    }
}

/// Process tests on Windows (a device check: run there with `cargo test -p core`).
#[cfg(windows)]
mod windows_processes {
    use std::{
        process::Command,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };

    use super::{super::run_cancellable, Stopped};

    #[test]
    fn a_finished_child_gives_its_output() {
        let output = run_cancellable(
            Command::new("cmd").args(["/c", "echo out"]),
            None,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "out");
    }

    /// The child starts another process holding its output; stopping ends both at once.
    #[test]
    fn a_stop_kills_the_child_and_what_it_started() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();

        let err = run_cancellable(
            Command::new("cmd").args(["/c", "ping -n 30 127.0.0.1 & ping -n 30 127.0.0.1"]),
            None,
            &cancelled,
        )
        .unwrap_err();

        assert!(err.is::<Stopped>(), "{err:#}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }
}
