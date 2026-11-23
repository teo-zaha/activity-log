use std::sync::{Arc, Mutex};

use activity_log_core::{
    model::{ActivityState, ProvidersResponse, SupportedOs},
    runtime::{Part, Published, RuntimeMessage},
};
use futures::channel::mpsc;
use tokio::time::{Duration, advance};

use super::{ACTIVITY_SPACING, RETRY_AFTER, forward};

/// A value of host `a` under `authority`, each part at its revision; `providers` names the
/// providers list's OS, so a changed list can be told apart.
fn value(authority: u64, activity: u64, providers: u64) -> Arc<Published> {
    Arc::new(Published {
        epoch: "a".into(),
        authority,
        activity: Part {
            revision: activity,
            value: Arc::new(ActivityState::Empty),
        },
        providers: Part {
            revision: providers,
            value: Arc::new(ProvidersResponse {
                current_os: SupportedOs::Linux,
                providers: Vec::new(),
                secret_store: activity_log_core::model::SecretStoreInfo::Os {
                    state: activity_log_core::model::OsStoreState::Ready,
                },
            }),
        },
    })
}

/// The emitter the forwarder is given: it records each event and fails while `failing`.
#[derive(Clone, Default)]
struct Window {
    events: Arc<Mutex<Vec<(&'static str, String)>>>,
    failing: Arc<Mutex<bool>>,
}

impl Window {
    fn emit(&self) -> impl FnMut(&'static str, String) -> Result<(), &'static str> + use<> {
        let window = self.clone();
        move |event, json| {
            if *window.failing.lock().unwrap() {
                return Err("the window is gone");
            }
            window.events.lock().unwrap().push((event, json));
            Ok(())
        }
    }

    /// The events since the last call, by name.
    fn take(&self) -> Vec<(&'static str, String)> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }

    fn names(&self) -> Vec<&'static str> {
        self.take().into_iter().map(|(name, _)| name).collect()
    }
}

/// A running forwarder and the sender of its values.
fn start(window: &Window) -> mpsc::UnboundedSender<Arc<Published>> {
    let (values, received) = mpsc::unbounded();
    tokio::spawn(forward(received, window.emit()));
    values
}

/// Lets the forwarder run up to its next wait, without moving the clock.
async fn settle() {
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn each_event_is_the_message_of_the_value() {
    let window = Window::default();
    let values = start(&window);
    let first = value(1, 3, 2);

    values.unbounded_send(first.clone()).unwrap();
    settle().await;
    advance(ACTIVITY_SPACING).await;
    let next = value(1, 4, 2);
    values.unbounded_send(next.clone()).unwrap();
    settle().await;

    assert_eq!(
        window.take(),
        [
            ("state", RuntimeMessage::pair(&first).to_json()),
            ("activity", RuntimeMessage::activity(&next).to_json()),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_emit_sends_a_full_pair_after_a_second() {
    let window = Window::default();
    let values = start(&window);
    values.unbounded_send(value(1, 3, 3)).unwrap();
    settle().await;
    window.take();

    *window.failing.lock().unwrap() = true;
    advance(ACTIVITY_SPACING).await;
    values.unbounded_send(value(1, 4, 3)).unwrap();
    settle().await;
    *window.failing.lock().unwrap() = false;
    // A value that comes during the wait sends nothing yet.
    values.unbounded_send(value(1, 5, 3)).unwrap();
    settle().await;
    assert!(window.take().is_empty());

    advance(RETRY_AFTER).await;
    settle().await;

    assert_eq!(
        window.take(),
        [("state", RuntimeMessage::pair(&value(1, 5, 3)).to_json())]
    );
}

#[tokio::test(start_paused = true)]
async fn providers_go_at_once_while_activity_waits_for_the_quiet_period_to_end() {
    let window = Window::default();
    let values = start(&window);
    values.unbounded_send(value(1, 3, 3)).unwrap();
    settle().await;
    window.take();

    // Within the quiet period after the pair: activity waits, providers do not.
    values.unbounded_send(value(1, 4, 3)).unwrap();
    settle().await;
    values.unbounded_send(value(1, 4, 5)).unwrap();
    settle().await;
    assert_eq!(window.names(), ["providers"]);

    advance(ACTIVITY_SPACING).await;
    settle().await;
    let sent = window.take();
    assert_eq!(
        sent,
        [(
            "activity",
            RuntimeMessage::activity(&value(1, 4, 5)).to_json()
        )]
    );
}

#[tokio::test(start_paused = true)]
async fn the_last_activity_is_sent_when_its_quiet_period_ends_without_a_later_value() {
    let window = Window::default();
    let values = start(&window);
    values.unbounded_send(value(1, 3, 3)).unwrap();
    settle().await;
    advance(ACTIVITY_SPACING).await;
    values.unbounded_send(value(1, 4, 3)).unwrap();
    settle().await;
    assert_eq!(window.names(), ["state", "activity"]);

    // The collection ends right after: nothing is published later.
    advance(Duration::from_millis(10)).await;
    values.unbounded_send(value(1, 5, 3)).unwrap();
    settle().await;
    assert!(window.take().is_empty());
    advance(ACTIVITY_SPACING).await;
    settle().await;

    assert_eq!(
        window.take(),
        [(
            "activity",
            RuntimeMessage::activity(&value(1, 5, 3)).to_json()
        )]
    );
}

#[tokio::test(start_paused = true)]
async fn an_authority_change_during_the_quiet_period_sends_one_pair_and_no_held_activity() {
    let window = Window::default();
    let values = start(&window);
    values.unbounded_send(value(1, 3, 3)).unwrap();
    settle().await;
    window.take();
    values.unbounded_send(value(1, 4, 3)).unwrap();
    settle().await;
    assert!(window.take().is_empty(), "the activity waits");

    values.unbounded_send(value(2, 5, 5)).unwrap();
    settle().await;
    advance(ACTIVITY_SPACING * 4).await;
    settle().await;

    assert_eq!(
        window.take(),
        [("state", RuntimeMessage::pair(&value(2, 5, 5)).to_json())]
    );
}

#[tokio::test(start_paused = true)]
async fn the_forwarder_ends_with_its_values() {
    let window = Window::default();
    let (values, received) = mpsc::unbounded();
    let forwarding = tokio::spawn(forward(received, window.emit()));

    drop(values);

    forwarding.await.unwrap();
}

/// Values that become ready at their time: one due exactly when a quiet period ends is ready
/// together with the forwarder's timer, which is what wakes it.
struct Scripted {
    values: std::collections::VecDeque<(tokio::time::Instant, Arc<Published>)>,
}

impl futures::Stream for Scripted {
    type Item = Arc<Published>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.values.front() {
            Some((at, _)) if *at <= tokio::time::Instant::now() => {
                std::task::Poll::Ready(self.values.pop_front().map(|(_, value)| value))
            }
            // The forwarder's own timer wakes it at the next value's time.
            _ => std::task::Poll::Pending,
        }
    }
}

/// A new authority and the end of a quiet period at the same moment: the value wins, so the
/// held activity of the old authority is never sent before the new pair.
#[tokio::test(start_paused = true)]
async fn a_ready_value_goes_before_a_quiet_period_that_ended_with_it() {
    for _ in 0..100 {
        let window = Window::default();
        let now = tokio::time::Instant::now();
        let values = Scripted {
            values: [
                (now, value(1, 3, 3)),
                (now, value(1, 4, 3)),
                (now + ACTIVITY_SPACING, value(2, 5, 5)),
            ]
            .into(),
        };
        let forwarding = tokio::spawn(forward(values, window.emit()));
        settle().await;
        assert_eq!(window.names(), ["state"], "the activity waits");

        advance(ACTIVITY_SPACING).await;
        settle().await;
        forwarding.abort();

        assert_eq!(
            window.take(),
            [("state", RuntimeMessage::pair(&value(2, 5, 5)).to_json())]
        );
    }
}
