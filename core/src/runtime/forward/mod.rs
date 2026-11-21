//! Forwarding the published state to one subscriber (`docs/plans/shared-contracts.md` §5):
//! the event stream (`GET /api/events`) and the desktop's events. A subscriber gets both parts
//! as one `state` first, again after an authority change it has not been sent and after a
//! failed delivery, and otherwise each part whose revision rose since it was last sent.

use std::time::Duration;

use serde::Serialize;

use super::host::Published;
use crate::model::{ActivityUpdate, ProvidersUpdate, RuntimePing, RuntimeState};

/// How often the event stream sends `ping` while nothing changes, so a reader behind any proxy
/// can tell a quiet stream from a dead one (the dashboard gives up after two missed).
pub const PING_INTERVAL: Duration = Duration::from_secs(15);

#[cfg(test)]
mod tests;

/// One message to a subscriber, named as its event.
#[derive(Debug, Clone)]
pub enum RuntimeMessage {
    State(RuntimeState),
    Activity(ActivityUpdate),
    Providers(ProvidersUpdate),
    /// Not state: the stream is alive. Never due from a [`Forwarder`].
    Ping(RuntimePing),
}

impl RuntimeMessage {
    pub fn pair(value: &Published) -> Self {
        Self::State(value.runtime_state())
    }

    pub fn activity(value: &Published) -> Self {
        Self::Activity(value.activity_update())
    }

    pub fn providers(value: &Published) -> Self {
        Self::Providers(value.providers_update())
    }

    pub fn ping(value: &Published) -> Self {
        Self::Ping(RuntimePing {
            epoch: value.epoch.to_string(),
        })
    }

    /// Every event's name, in the order of `event`'s variants.
    pub const EVENTS: [&'static str; 4] = ["state", "activity", "providers", "ping"];

    /// The event's name: `state`, `activity`, `providers` or `ping`.
    pub fn event(&self) -> &'static str {
        let [state, activity, providers, ping] = Self::EVENTS;
        match self {
            Self::State(_) => state,
            Self::Activity(_) => activity,
            Self::Providers(_) => providers,
            Self::Ping(_) => ping,
        }
    }

    /// The message as compact JSON, on one line.
    pub fn to_json(&self) -> String {
        match self {
            Self::State(state) => json(state),
            Self::Activity(update) => json(update),
            Self::Providers(update) => json(update),
            Self::Ping(ping) => json(ping),
        }
    }
}

fn json(value: &impl Serialize) -> String {
    // Our own DTOs: string keys only and no fallible serializers.
    serde_json::to_string(value).expect("the runtime DTOs serialize")
}

/// What a subscriber still needs of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// Both parts, as one `state`.
    Pair,
    /// The parts whose revision rose; at least one of them.
    Parts {
        activity: bool,
        providers: bool,
    },
    Nothing,
}

/// What was delivered to a subscriber: the authority of the last pair and each part's revision.
#[derive(Debug, Clone, Copy)]
struct Cursor {
    authority: u64,
    activity: u64,
    providers: u64,
}

/// Per-subscriber delivery accounting. Each accepted message is acknowledged on its own, so a
/// part sent while the other waits advances only its own cursor.
#[derive(Debug, Default)]
pub struct Forwarder {
    /// `None` before the first pair and after a failed delivery: a pair is due.
    cursor: Option<Cursor>,
}

impl Forwarder {
    pub fn due(&self, value: &Published) -> Due {
        let Some(cursor) = self.cursor else {
            return Due::Pair;
        };
        if cursor.authority != value.authority {
            return Due::Pair;
        }
        let activity = value.activity.revision > cursor.activity;
        let providers = value.providers.revision > cursor.providers;
        if activity || providers {
            Due::Parts {
                activity,
                providers,
            }
        } else {
            Due::Nothing
        }
    }

    /// After a `state` of `value` was accepted: both cursors and the authority advance.
    pub fn delivered_pair(&mut self, value: &Published) {
        self.cursor = Some(Cursor {
            authority: value.authority,
            activity: value.activity.revision,
            providers: value.providers.revision,
        });
    }

    /// After the activity part of `value` was accepted. Without a pair delivered, nothing.
    pub fn delivered_activity(&mut self, value: &Published) {
        if let Some(cursor) = &mut self.cursor {
            cursor.activity = cursor.activity.max(value.activity.revision);
        }
    }

    /// After the providers part of `value` was accepted. Without a pair delivered, nothing.
    pub fn delivered_providers(&mut self, value: &Published) {
        if let Some(cursor) = &mut self.cursor {
            cursor.providers = cursor.providers.max(value.providers.revision);
        }
    }

    /// After a failed delivery: the next message is a pair.
    pub fn reset(&mut self) {
        self.cursor = None;
    }

    /// Everything due of `value`, parts in revision order (activity first at the same one),
    /// each counted as delivered: for a subscriber that takes whatever it is handed, the event
    /// stream, whose connection ends on a failed send.
    pub fn take(&mut self, value: &Published) -> Vec<RuntimeMessage> {
        match self.due(value) {
            Due::Pair => {
                self.delivered_pair(value);
                vec![RuntimeMessage::pair(value)]
            }
            Due::Parts {
                activity,
                providers,
            } => {
                let mut parts = Vec::with_capacity(2);
                if activity {
                    parts.push((value.activity.revision, RuntimeMessage::activity(value)));
                }
                if providers {
                    parts.push((value.providers.revision, RuntimeMessage::providers(value)));
                }
                // Stable: at one revision activity stays first.
                parts.sort_by_key(|(revision, _)| *revision);
                self.delivered_activity(value);
                self.delivered_providers(value);
                parts.into_iter().map(|(_, message)| message).collect()
            }
            Due::Nothing => Vec::new(),
        }
    }
}
