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
fn only_a_long_run_that_finished_or_failed_deserves_a_notice() {
    let run = a_run();
    let long = THRESHOLD + Duration::from_secs(1);
    let ready = RunOutcome::Ready { entries: 1234 };

    assert_eq!(
        notice_for(&ended(run, long, ready)),
        Some(Notice {
            title: "Activity collected",
            body: "1234 entries".to_string(),
        })
    );
    assert!(notice_for(&ended(run, long, RunOutcome::Failed)).is_some());
    assert_eq!(
        notice_for(&ended(run, THRESHOLD - Duration::from_millis(1), ready)),
        None,
        "waited for"
    );
    assert_eq!(
        notice_for(&ended(run, long, RunOutcome::Stopped)),
        None,
        "replaced"
    );
}

/// What `follow` shows for each of `ends`, the window's focus read as `focus` says, and how often
/// it asked for the focus.
fn shown_for(
    ends: impl IntoIterator<Item = RunEnd>,
    focus: impl Fn() -> tauri::Result<bool>,
) -> (Vec<&'static str>, usize) {
    let (sender, ended_runs) = unbounded_channel();
    for end in ends {
        sender.send(end).unwrap();
    }
    drop(sender);
    let asked = Arc::new(Mutex::new(0));
    let shown = Arc::new(Mutex::new(Vec::new()));
    tauri::async_runtime::block_on(follow(
        ended_runs,
        {
            let asked = asked.clone();
            move || {
                *asked.lock().unwrap() += 1;
                focus()
            }
        },
        |_| true,
        {
            let shown = shown.clone();
            move |notice: Notice| shown.lock().unwrap().push(notice.title)
        },
    ));
    let shown = shown.lock().unwrap().clone();
    let asked = *asked.lock().unwrap();
    (shown, asked)
}

/// The window's focus is asked as a run that deserves a notice ends, never for one that does
/// not: after shutdown every run ends stopped, and the window may be gone.
#[test]
fn a_run_the_focused_window_watched_shows_nothing() {
    let run = a_run();
    let long = THRESHOLD * 2;
    let ends = || {
        [
            ended(run, long, RunOutcome::Ready { entries: 3 }),
            ended(run, long, RunOutcome::Stopped),
            ended(run, THRESHOLD / 2, RunOutcome::Failed),
        ]
    };

    assert_eq!(shown_for(ends(), || Ok(true)), (vec![], 1), "watched");
    assert_eq!(
        shown_for(ends(), || Ok(false)),
        (vec!["Activity collected"], 1),
        "in the background"
    );
}

/// A window whose focus cannot be read counts as in the background.
#[test]
fn a_window_whose_focus_cannot_be_read_is_told() {
    let run = a_run();
    let failed = ended(run, THRESHOLD * 2, RunOutcome::Failed);

    assert_eq!(
        shown_for([failed], || Err(tauri::Error::WindowNotFound)),
        (vec!["Collection failed"], 1)
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
        || Ok(false),
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
