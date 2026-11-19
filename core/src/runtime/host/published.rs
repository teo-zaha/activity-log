use std::sync::Arc;

use crate::model::{ActivityState, ProvidersResponse};

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
#[derive(Debug, Clone)]
pub struct Published {
    pub activity: Part<ActivityState>,
    pub providers: Part<ProvidersResponse>,
}

/// The highest revision a JSON number holds exactly.
const MAX_REVISION: u64 = (1 << 53) - 1;

impl Published {
    pub(super) fn initial(providers: ProvidersResponse) -> Self {
        Self {
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
    /// changed neither. Each changed part takes `revision + 1`.
    pub(super) fn next(
        &self,
        revision: u64,
        activity: &Arc<ActivityState>,
        providers: ProvidersResponse,
    ) -> Option<Self> {
        let activity_changed = !Arc::ptr_eq(&self.activity.value, activity);
        let providers_changed = *self.providers.value != providers;
        if !activity_changed && !providers_changed {
            return None;
        }
        let revision = revision + 1;
        assert!(revision <= MAX_REVISION, "revision past 2^53 - 1");
        Some(Self {
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
}
