use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{DailyCoachingProvider, DailyCoachingStoreError, StoredPlayingProfileConnection};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum StoredPlayingProfileHealth {
    Reachable {
        epoch: u64,
    },
    ProfileUnavailable {
        epoch: u64,
        entered_at: DateTime<Utc>,
    },
}

impl Default for StoredPlayingProfileHealth {
    fn default() -> Self {
        Self::Reachable { epoch: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileHealthObservation {
    Reachable,
    ProfileUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileUnavailableNotice {
    pub(crate) provider: DailyCoachingProvider,
    pub(crate) identity_username: String,
    pub(crate) epoch: u64,
    pub(crate) entered_at: DateTime<Utc>,
}

impl StoredPlayingProfileConnection {
    pub(crate) fn is_profile_unavailable(&self) -> bool {
        matches!(
            &self.health,
            StoredPlayingProfileHealth::ProfileUnavailable { .. }
        )
    }

    pub(super) fn profile_unavailable_notice(&self) -> Option<ProfileUnavailableNotice> {
        let StoredPlayingProfileHealth::ProfileUnavailable { epoch, entered_at } = &self.health
        else {
            return None;
        };
        Some(ProfileUnavailableNotice {
            provider: self.provider,
            identity_username: self.identity_username.clone(),
            epoch: *epoch,
            entered_at: *entered_at,
        })
    }

    pub(super) fn observe_health(
        &mut self,
        observation: ProfileHealthObservation,
        now: DateTime<Utc>,
    ) -> Result<bool, DailyCoachingStoreError> {
        let next = match (&self.health, observation) {
            (StoredPlayingProfileHealth::Reachable { .. }, ProfileHealthObservation::Reachable)
            | (
                StoredPlayingProfileHealth::ProfileUnavailable { .. },
                ProfileHealthObservation::ProfileUnavailable,
            ) => return Ok(false),
            (
                StoredPlayingProfileHealth::Reachable { epoch },
                ProfileHealthObservation::ProfileUnavailable,
            ) => StoredPlayingProfileHealth::ProfileUnavailable {
                epoch: epoch
                    .checked_add(1)
                    .ok_or(DailyCoachingStoreError::InvalidRecord)?,
                entered_at: now,
            },
            (
                StoredPlayingProfileHealth::ProfileUnavailable { epoch, .. },
                ProfileHealthObservation::Reachable,
            ) => StoredPlayingProfileHealth::Reachable { epoch: *epoch },
        };
        self.health = next;
        Ok(true)
    }

    pub(super) fn health_is_valid(&self) -> bool {
        match &self.health {
            StoredPlayingProfileHealth::Reachable { .. } => true,
            StoredPlayingProfileHealth::ProfileUnavailable { epoch, entered_at } => {
                *epoch > 0 && entered_at.timestamp_millis() > 0
            }
        }
    }
}
