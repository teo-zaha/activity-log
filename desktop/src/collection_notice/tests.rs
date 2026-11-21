use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use activity_log_core::model::{RunEnd, RunId, RunOutcome};
use tokio::sync::mpsc::unbounded_channel;

use super::{Notice, THRESHOLD, follow, notice_for};
use crate::app_state::AppState;

/// A real run's id, from a host's first collection.
fn a_run() -> RunId {
    let (state, mut ends) = AppState::start_following(None);
    state.host.start_initial_collection();
    tauri::async_runtime::block_on(ends.recv())
        .expect("the run ends")
        .run
}

fn ended(run: RunId, elapsed: Duration, outcome: RunOutcome) -> RunEnd {
    RunEnd {
        run,
        elapsed,
        outcome,
    }
}

#[test]
fn only_a_long_run_that_ended_in_the_background_is_told() {
    let run = a_run();
    let long = THRESHOLD + Duration::from_secs(1);
    let ready = RunOutcome::Ready { entries: 1234 };

    assert_eq!(
        notice_for(&ended(run, long, ready), false),
        Some(Notice {
            title: "Activity collected",
            body: "1234 entries".to_string(),
        })
    );
    assert!(notice_for(&ended(run, long, RunOutcome::Failed), false).is_some());
    assert_eq!(notice_for(&ended(run, long, ready), true), None, "watched");
    assert_eq!(
        notice_for(
            &ended(run, THRESHOLD - Duration::from_millis(1), ready),
            false
        ),
        None,
        "waited for"
    );
    assert_eq!(
        notice_for(&ended(run, long, RunOutcome::Stopped), false),
        None,
        "replaced"
    );
}

/// Whether the run is still the one on screen is asked last, only for a notice to show.
#[test]
fn a_run_another_replaced_meanwhile_shows_nothing() {
    let run = a_run();
    let long = THRESHOLD * 2;
    let (ends, ended_runs) = unbounded_channel();
    for outcome in [
        RunOutcome::Ready { entries: 3 },
        RunOutcome::Stopped,
        RunOutcome::Failed,
    ] {
        ends.send(ended(run, long, outcome)).unwrap();
    }
    drop(ends);
    let asked = Arc::new(Mutex::new(0));
    let shown = Arc::new(Mutex::new(Vec::new()));
    let current = Arc::new(Mutex::new(true));

    tauri::async_runtime::block_on(follow(
        ended_runs,
        || false,
        {
            let asked = asked.clone();
            let current = current.clone();
            move |_| {
                *asked.lock().unwrap() += 1;
                // The first run is current; by the third another run replaced it.
                std::mem::replace(&mut *current.lock().unwrap(), false)
            }
        },
        {
            let shown = shown.clone();
            move |notice: Notice| shown.lock().unwrap().push(notice.title)
        },
    ));

    assert_eq!(*shown.lock().unwrap(), ["Activity collected"]);
    assert_eq!(*asked.lock().unwrap(), 2, "never for the stopped run");
}
