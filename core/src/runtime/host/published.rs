use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    model::{
        ActivityState, ActivityUpdate, InstallReceipt, PartStamp, ProvidersResponse,
        ProvidersUpdate, RuntimeState,
    },
    utils::fnv1a,
};

/// One part of the published state with the revision at which it last changed.
#[derive(Debug)]
pub struct Part<T> {
    pub revision: u64,
    pub value: Arc<T>,
}

impl<T> Clone for Part<T> {
    fn clone(&self) -> Self {
        Self {
            revision: self.revision,
            value: self.value.clone(),
        }
    }
}

/// What a host shows, as one coherent value: replaced whole, under the host's state lock, at
/// every transition that changes a part. Revisions rise in the order transitions are installed.
/// `epoch` names the host; `authority` rises at a config install or an identity change that
/// invalidates a collection key, which stamps both parts.
#[derive(Debug, Clone)]
pub struct Published {
    pub epoch: Arc<str>,
    pub authority: u64,
    pub activity: Part<ActivityState>,
    pub providers: Part<ProvidersResponse>,
}

/// The highest revision a JSON number holds exactly.
const MAX_REVISION: u64 = (1 << 53) - 1;

/// Hosts made in this process, so two of them never share an epoch.
static HOSTS: AtomicU64 = AtomicU64::new(0);

/// 16 hex digits of FNV-1a over the start time, the process id and this process's host count.
fn new_epoch() -> Arc<str> {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let host = HOSTS.fetch_add(1, Ordering::Relaxed);
    let bytes = started
        .to_le_bytes()
        .into_iter()
        .chain(std::process::id().to_le_bytes())
        .chain(host.to_le_bytes());
    format!("{:016x}", fnv1a(bytes)).into()
}

impl Published {
    pub(super) fn initial(providers: ProvidersResponse) -> Self {
        Self {
            epoch: new_epoch(),
            authority: 1,
            activity: Part {
                revision: 0,
                value: Arc::new(ActivityState::Empty),
            },
            providers: Part {
                revision: 0,
                value: Arc::new(providers),
            },
        }
    }

    /// The value after a transition that showed `activity` and `providers`, or `None` when it
    /// changed neither and is no authority boundary. Each changed part takes `revision + 1`; a
    /// boundary (`new_authority`) raises the authority and stamps both parts.
    pub(super) fn next(
        &self,
        revision: u64,
        activity: &Arc<ActivityState>,
        providers: ProvidersResponse,
        new_authority: bool,
    ) -> Option<Self> {
        let activity_changed = new_authority || !Arc::ptr_eq(&self.activity.value, activity);
        let providers_changed = new_authority || *self.providers.value != providers;
        if !activity_changed && !providers_changed {
            return None;
        }
        let revision = revision + 1;
        assert!(revision <= MAX_REVISION, "revision past 2^53 - 1");
        Some(Self {
            epoch: self.epoch.clone(),
            authority: self.authority + u64::from(new_authority),
            activity: if activity_changed {
                Part {
                    revision,
                    value: activity.clone(),
                }
            } else {
                self.activity.clone()
            },
            providers: if providers_changed {
                Part {
                    revision,
                    value: Arc::new(providers),
                }
            } else {
                self.providers.clone()
            },
        })
    }

    pub fn revision(&self) -> u64 {
        self.activity.revision.max(self.providers.revision)
    }

    pub fn activity_stamp(&self) -> PartStamp {
        self.stamp(self.activity.revision)
    }

    pub fn providers_stamp(&self) -> PartStamp {
        self.stamp(self.providers.revision)
    }

    /// Where a settings change installed now shows: the providers part.
    pub fn receipt(&self) -> InstallReceipt {
        InstallReceipt {
            epoch: self.epoch.to_string(),
            revision: self.providers.revision,
        }
    }

    /// Both parts on the wire.
    pub fn runtime_state(&self) -> RuntimeState {
        RuntimeState {
            activity: self.activity_update(),
            providers: self.providers_update(),
        }
    }

    /// The activity part on the wire.
    pub fn activity_update(&self) -> ActivityUpdate {
        let stamp = self.activity_stamp();
        ActivityUpdate {
            epoch: stamp.epoch,
            authority: stamp.authority,
            revision: stamp.revision,
            state: self.activity.value.clone(),
        }
    }

    /// The providers part on the wire.
    pub fn providers_update(&self) -> ProvidersUpdate {
        let stamp = self.providers_stamp();
        ProvidersUpdate {
            epoch: stamp.epoch,
            authority: stamp.authority,
            revision: stamp.revision,
            providers: self.providers.value.clone(),
        }
    }

    fn stamp(&self, revision: u64) -> PartStamp {
        PartStamp {
            epoch: self.epoch.to_string(),
            authority: self.authority.to_string(),
            revision,
        }
    }
}
