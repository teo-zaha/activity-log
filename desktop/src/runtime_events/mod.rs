//! The host's state as Tauri events to the main window (`docs/plans/shared-contracts.md` §5):
//! `state` with both parts, then `activity` and `providers` as each changes, the same JSON as
//! `GET /api/events`. The dashboard listens first and then reads `get_runtime_state`, so what
//! is emitted before it listens is not missed: Tauri evaluates an event only in a webview that
//! has a listener for it.
//!
//! Each event becomes a script the window's thread runs, and the host's `watch` bounds only
//! its own values, not those scripts. So `activity`, the large one, is sent at most every
//! [`ACTIVITY_SPACING`]; once that quiet period ends the then-current value decides what is
//! sent. `state` and `providers` go at once. A failed emit resets the forwarder, so a full
//! pair is sent once [`RETRY_AFTER`] passed.

use std::{fmt::Display, sync::Arc};

use activity_log_core::{
    diagnostics,
    runtime::{Due, Forwarder, Published, RuntimeMessage},
};
use futures::{Stream, StreamExt};
use tokio::time::{Duration, Instant, sleep_until};

#[cfg(test)]
mod tests;

/// The shortest time between two `activity` events (or a `state` and an `activity`).
pub(crate) const ACTIVITY_SPACING: Duration = Duration::from_millis(250);
/// How long a failed emit waits before the pair is sent again.
pub(crate) const RETRY_AFTER: Duration = Duration::from_secs(1);

/// What the next wait ends with.
enum Woken {
    Value(Arc<Published>),
    Timer,
    Ended,
}

/// Forwards each value of `values` through `emit` (an event's name and its JSON) until
/// `values` ends, which the host's feed does at shutdown.
pub(crate) async fn forward<E: Display>(
    mut values: impl Stream<Item = Arc<Published>> + Unpin,
    mut emit: impl FnMut(&'static str, String) -> Result<(), E>,
) {
    let mut forwarder = Forwarder::default();
    let mut latest: Option<Arc<Published>> = None;
    // When the last `activity` or `state` went out.
    let mut activity_sent: Option<Instant> = None;
    // When to look again without a new value: the end of a quiet period, or a retry.
    let mut wake: Option<Instant> = None;
    // A failed emit: nothing is sent before this.
    let mut retry_at: Option<Instant> = None;
    let mut failing = false;
    loop {
        let woken = match wake {
            // A value ready when the timer ends goes first: the timer would act on the value
            // before it, which a new authority may have replaced.
            Some(at) => tokio::select! {
                biased;
                value = values.next() => value.map_or(Woken::Ended, Woken::Value),
                () = sleep_until(at) => Woken::Timer,
            },
            None => values.next().await.map_or(Woken::Ended, Woken::Value),
        };
        match woken {
            Woken::Ended => return,
            Woken::Value(value) => latest = Some(value),
            Woken::Timer => wake = None,
        }
        let now = Instant::now();
        if retry_at.is_some_and(|at| now < at) {
            continue;
        }
        retry_at = None;
        let Some(value) = &latest else { continue };
        let mut send = |message: RuntimeMessage| emit(message.event(), message.to_json());
        let sent = match forwarder.due(value) {
            Due::Nothing => Ok(()),
            Due::Pair => send(RuntimeMessage::pair(value)).map(|()| {
                forwarder.delivered_pair(value);
                activity_sent = Some(now);
                // The pair carries the activity a quiet period held back.
                wake = None;
            }),
            Due::Parts {
                activity,
                providers,
            } => {
                let providers_sent = if providers {
                    send(RuntimeMessage::providers(value))
                        .map(|()| forwarder.delivered_providers(value))
                } else {
                    Ok(())
                };
                providers_sent.and_then(|()| {
                    let quiet_until = activity_sent.map(|at| at + ACTIVITY_SPACING);
                    match quiet_until {
                        Some(until) if activity && now < until => {
                            wake = Some(wake.map_or(until, |at| at.min(until)));
                            Ok(())
                        }
                        _ if activity => send(RuntimeMessage::activity(value)).map(|()| {
                            forwarder.delivered_activity(value);
                            activity_sent = Some(now);
                        }),
                        _ => Ok(()),
                    }
                })
            }
        };
        match sent {
            Ok(()) => failing = false,
            Err(err) => {
                forwarder.reset();
                if !failing {
                    diagnostics::warn(
                        "desktop",
                        format!("Sending the activity to the window failed: {err}"),
                    );
                    failing = true;
                }
                let at = now + RETRY_AFTER;
                retry_at = Some(at);
                wake = Some(at);
            }
        }
    }
}
