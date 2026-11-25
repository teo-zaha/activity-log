use std::collections::BTreeMap;

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::provider::figma::scope::FigmaTeamId;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct FileRecord {
    pub(super) last_tab: Option<DateTime<Utc>>,
    pub(super) last_contribution: Option<DateTime<Utc>>,
    #[serde(default)]
    pub(super) last_reappearance: Option<DateTime<Utc>>,
    pub(super) access: Option<AccessCheck>,
    pub(super) last_attempt: Option<DateTime<Utc>>,
    #[serde(default)]
    pub(super) team_sightings: BTreeMap<FigmaTeamId, TeamSighting>,
}

impl FileRecord {
    pub(super) fn merge(&mut self, other: Self) {
        self.last_tab = self.last_tab.max(other.last_tab);
        self.last_contribution = self.last_contribution.max(other.last_contribution);
        self.last_reappearance = self.last_reappearance.max(other.last_reappearance);
        self.access = self.access.max(other.access);
        self.last_attempt = self.last_attempt.max(other.last_attempt);
        for (team, sighting) in other.team_sightings {
            self.observe_team(team, sighting);
        }
    }

    pub(super) fn observe_team(&mut self, team: FigmaTeamId, sighting: TeamSighting) {
        let known = self.team_sightings.entry(team).or_insert(sighting);
        *known = (*known).max(sighting);
    }

    pub(super) fn keep_since(&self, cutoff: DateTime<Utc>) -> bool {
        self.last_tab
            .max(self.last_contribution)
            .is_some_and(|at| at >= cutoff)
            || self
                .team_sightings
                .values()
                .any(|sighting| sighting.seen_at >= cutoff)
    }

    pub(super) fn tab_activity_since(&self, cutoff: DateTime<Utc>) -> bool {
        self.last_tab.is_some()
            && self
                .last_tab
                .max(self.last_contribution)
                .is_some_and(|at| at >= cutoff)
    }

    /// Reserve a retry while the book is locked so two processes cannot both retry a lost file.
    pub(super) fn claim(&mut self, now: DateTime<Utc>) -> bool {
        let Some(AccessCheck {
            observed_at,
            state: Access::Lost,
        }) = self.access
        else {
            return true;
        };
        let attempted = self
            .last_attempt
            .map_or(observed_at, |at| at.max(observed_at));
        if self.last_reappearance.is_some_and(|seen| seen > attempted)
            || now.signed_duration_since(attempted) >= TimeDelta::days(1)
        {
            self.last_attempt = self.last_attempt.max(Some(now));
            return true;
        }
        false
    }
}

/// Keep the modification time from the newest sighting, breaking a tie by modification time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(super) struct TeamSighting {
    pub(super) seen_at: DateTime<Utc>,
    pub(super) last_modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(super) struct AccessCheck {
    pub(super) observed_at: DateTime<Utc>,
    pub(super) state: Access,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Access {
    Lost,
    Readable,
}
