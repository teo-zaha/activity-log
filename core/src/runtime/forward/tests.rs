use std::sync::Arc;

use chrono::{TimeZone, Utc};

use super::{Due, Forwarder, RuntimeMessage};
use crate::{
    model::{
        ActivityLog, ActivityLogMeta, ActivityState, CollectedActivitySummary,
        CollectionFailedResponse, ProvidersResponse, SupportedOs,
    },
    runtime::host::{Part, Published},
};

fn providers() -> ProvidersResponse {
    ProvidersResponse {
        current_os: SupportedOs::Linux,
        providers: Vec::new(),
    }
}

fn log() -> ActivityState {
    ActivityState::Ready {
        log: Arc::new(ActivityLog {
            meta: ActivityLogMeta {
                generated: "2026-11-19T08:00:01Z".to_string(),
                start: "2026-10-20T00:00:00Z".to_string(),
                end: "2026-11-19T08:00:00Z".to_string(),
                pending: Vec::new(),
            },
            summary: CollectedActivitySummary {
                connections: Vec::new(),
                merged_count: 0,
            },
            entries: Vec::new(),
        }),
    }
}

fn failed() -> ActivityState {
    let start = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();
    ActivityState::Failed {
        failure: CollectionFailedResponse::new(
            &crate::model::CollectOptions { start, end },
            "boom".to_string(),
        ),
    }
}

/// A value of host `a` under `authority`, each part at its revision.
fn value(authority: u64, activity: u64, providers_at: u64) -> Published {
    Published {
        epoch: "a".into(),
        authority,
        activity: Part {
            revision: activity,
            value: Arc::new(log()),
        },
        providers: Part {
            revision: providers_at,
            value: Arc::new(providers()),
        },
    }
}

fn names(messages: &[RuntimeMessage]) -> Vec<&'static str> {
    messages.iter().map(RuntimeMessage::event).collect()
}

#[test]
fn the_first_value_is_due_as_a_pair_and_then_nothing_until_a_part_rises() {
    let mut forwarder = Forwarder::default();
    let first = value(1, 3, 3);

    assert_eq!(forwarder.due(&first), Due::Pair);
    assert_eq!(names(&forwarder.take(&first)), ["state"]);
    assert_eq!(forwarder.due(&first), Due::Nothing);

    let next = value(1, 4, 3);
    assert_eq!(
        forwarder.due(&next),
        Due::Parts {
            activity: true,
            providers: false
        }
    );
}

#[test]
fn an_authority_not_delivered_makes_a_pair_due_however_many_values_were_skipped() {
    let mut forwarder = Forwarder::default();
    forwarder.take(&value(1, 3, 3));

    // The subscriber lagged: an install (authority 2) and later parts came meanwhile.
    let latest = value(2, 9, 7);

    assert_eq!(names(&forwarder.take(&latest)), ["state"]);
    assert_eq!(forwarder.due(&latest), Due::Nothing);
}

#[test]
fn a_reset_makes_a_pair_due() {
    let mut forwarder = Forwarder::default();
    let shown = value(1, 3, 3);
    forwarder.take(&shown);

    forwarder.reset();

    assert_eq!(forwarder.due(&shown), Due::Pair);
}

#[test]
fn acknowledging_one_part_leaves_the_other_due() {
    let mut forwarder = Forwarder::default();
    forwarder.delivered_pair(&value(1, 3, 3));
    let next = value(1, 5, 4);

    forwarder.delivered_providers(&next);

    assert_eq!(
        forwarder.due(&next),
        Due::Parts {
            activity: true,
            providers: false
        }
    );
    forwarder.delivered_activity(&next);
    assert_eq!(forwarder.due(&next), Due::Nothing);
}

#[test]
fn a_part_acknowledged_before_any_pair_leaves_the_pair_due() {
    let mut forwarder = Forwarder::default();
    let shown = value(1, 3, 3);

    forwarder.delivered_activity(&shown);

    assert_eq!(forwarder.due(&shown), Due::Pair);
}

#[test]
fn parts_are_taken_in_revision_order() {
    let mut forwarder = Forwarder::default();
    forwarder.take(&value(1, 3, 3));

    assert_eq!(
        names(&forwarder.take(&value(1, 6, 5))),
        ["providers", "activity"]
    );
    assert_eq!(
        names(&forwarder.take(&value(1, 7, 8))),
        ["activity", "providers"]
    );
    // One transition changed both: activity first.
    assert_eq!(
        names(&forwarder.take(&value(1, 9, 9))),
        ["activity", "providers"]
    );
}

#[test]
fn each_message_is_its_dto_as_compact_json() {
    let mut shown = value(1, 3, 2);
    shown.activity.value = Arc::new(failed());

    let pair = RuntimeMessage::pair(&shown);
    let activity = RuntimeMessage::activity(&shown);
    let providers = RuntimeMessage::providers(&shown);

    assert_eq!(
        pair.to_json(),
        serde_json::to_string(&shown.runtime_state()).unwrap()
    );
    assert_eq!(
        activity.to_json(),
        serde_json::to_string(&shown.activity_update()).unwrap()
    );
    assert_eq!(
        providers.to_json(),
        serde_json::to_string(&shown.providers_update()).unwrap()
    );
    assert!(!pair.to_json().contains('\n'));
    let parsed: serde_json::Value = serde_json::from_str(&activity.to_json()).unwrap();
    assert_eq!(parsed["revision"], 3);
    assert_eq!(parsed["authority"], "1");
    assert_eq!(parsed["state"]["kind"], "failed");
    let parsed: serde_json::Value = serde_json::from_str(&pair.to_json()).unwrap();
    assert_eq!(parsed["providers"]["revision"], 2);
    assert_eq!(parsed["providers"]["epoch"], parsed["activity"]["epoch"]);
}

#[test]
fn a_ping_names_the_host_and_is_never_due() {
    let shown = value(1, 3, 3);
    let mut forwarder = Forwarder::default();
    forwarder.take(&shown);

    let ping = RuntimeMessage::ping(&shown);

    assert_eq!(ping.event(), "ping");
    assert_eq!(ping.to_json(), r#"{"epoch":"a"}"#);
    assert_eq!(forwarder.due(&shown), Due::Nothing);
}
