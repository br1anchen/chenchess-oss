use std::{collections::BTreeSet, future::Future, pin::Pin, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::profile_game_feed::{ProfileGameWindowEntry, RecentProfileGameCursor};
use crate::{
    profile_game_feed::{
        ProfileGameSourceIdentity, PublicChessProfile, ValidatedPublicChessProfile,
    },
    review_session_contract::PlayerId,
};

use super::schedule::local_date;
use super::{
    canonical_timezone, ConnectPlayingProfileOutcome, DailyCoachingMutationRejectionReason,
    DailyCoachingProvider, DailyCoachingSetupState, PlayingProfileConnection,
    PlayingProfileConnectionStatus,
};

pub(crate) const STATE_SCHEMA_VERSION: u16 = 1;

mod health;
mod initial_backfill;
mod memory;
mod owner;

use health::StoredPlayingProfileHealth;
pub(crate) use health::{ProfileHealthObservation, ProfileUnavailableNotice};
use initial_backfill::InitialBackfill;
pub(crate) use initial_backfill::{
    InitialBackfillMutation, InitialBackfillSnapshot, InitialBackfillUnavailableReason,
};
pub(crate) use memory::InMemoryDailyCoachingStore;
pub(crate) use owner::DailyCoachingOwnerKey;

pub(crate) type StoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, DailyCoachingStoreError>> + Send + 'a>>;

pub(crate) trait DailyCoachingStore: Send + Sync {
    #[cfg(test)]
    fn read<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> StoreFuture<'a, DailyCoachingDocument>;

    fn list(&self) -> StoreFuture<'_, Vec<DailyCoachingDocument>>;

    fn bind_player<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
    ) -> StoreFuture<'a, DailyCoachingDocument>;

    fn connect_profile<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
        connection: StoredPlayingProfileConnection,
        timezone: String,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, StoredPlayingProfileConnection>;

    fn replace_profile<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        connection: StoredPlayingProfileConnection,
        expected_identity_username: String,
    ) -> StoreFuture<'a, DailyCoachingDocument>;

    fn remove_profile<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        provider: DailyCoachingProvider,
        expected_identity_username: String,
    ) -> StoreFuture<'a, DailyCoachingDocument>;

    fn set_enabled<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, DailyCoachingDocument>;

    fn advance_daily_window<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        expected: NaiveDate,
        next: NaiveDate,
    ) -> StoreFuture<'a, DailyCoachingDocument>;

    #[cfg(test)]
    fn resolve_initial_backfill<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        expected_run_fence: u64,
        provider: DailyCoachingProvider,
        expected_identity_username: String,
        games: Vec<ProfileGameWindowEntry>,
    ) -> StoreFuture<'a, DailyCoachingDocument>;

    fn accept_nudge<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        now: DateTime<Utc>,
        minimum_interval: Duration,
    ) -> StoreFuture<'a, NudgeAdmission>;

    fn observe_profile_health<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        provider: DailyCoachingProvider,
        expected_identity_username: &'a str,
        observation: ProfileHealthObservation,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, Option<DailyCoachingDocument>>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DailyCoachingDocument {
    #[serde(deserialize_with = "deserialize_current_schema_version")]
    schema_version: u16,
    owner_key: DailyCoachingOwnerKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    player_id: Option<PlayerId>,
    revision: u64,
    enabled: bool,
    timezone: Option<String>,
    connections: Vec<StoredPlayingProfileConnection>,
    next_daily_window: Option<NaiveDate>,
    run_fence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_nudge_at: Option<DateTime<Utc>>,
}

fn deserialize_current_schema_version<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let actual = u16::deserialize(deserializer)?;
    if actual == STATE_SCHEMA_VERSION {
        Ok(actual)
    } else {
        Err(serde::de::Error::custom(
            "unexpected Daily Coaching schema version",
        ))
    }
}

impl DailyCoachingDocument {
    pub(crate) fn empty(owner_key: DailyCoachingOwnerKey) -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            owner_key,
            player_id: None,
            revision: 0,
            enabled: false,
            timezone: None,
            connections: Vec::new(),
            next_daily_window: None,
            run_fence: 0,
            last_nudge_at: None,
        }
    }

    pub(crate) fn owner_key(&self) -> &DailyCoachingOwnerKey {
        &self.owner_key
    }

    pub(crate) fn player_id(&self) -> Option<&PlayerId> {
        self.player_id.as_ref()
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn timezone(&self) -> Option<&str> {
        self.timezone.as_deref()
    }

    pub(crate) fn connections(&self) -> &[StoredPlayingProfileConnection] {
        &self.connections
    }

    pub(crate) fn next_daily_window(&self) -> Option<NaiveDate> {
        self.next_daily_window
    }

    pub(crate) fn run_fence(&self) -> u64 {
        self.run_fence
    }

    pub(crate) fn connection(
        &self,
        provider: DailyCoachingProvider,
    ) -> Option<&StoredPlayingProfileConnection> {
        self.connections
            .iter()
            .find(|connection| connection.provider == provider)
    }

    pub(crate) fn connection_for_identity(
        &self,
        provider: DailyCoachingProvider,
        identity_username: &str,
    ) -> Option<&StoredPlayingProfileConnection> {
        self.connection(provider)
            .filter(|connection| connection.identity_username == identity_username)
    }

    pub(crate) fn has_unresolved_initial_backfill(&self) -> bool {
        self.connections
            .iter()
            .any(StoredPlayingProfileConnection::has_unresolved_initial_backfill)
    }

    pub(crate) fn has_only_empty_completed_backfills(&self) -> bool {
        !self.connections.is_empty()
            && self
                .connections
                .iter()
                .all(StoredPlayingProfileConnection::has_empty_completed_backfill)
    }

    pub(crate) fn has_unavailable_initial_backfill(&self) -> bool {
        self.connections
            .iter()
            .any(StoredPlayingProfileConnection::has_unavailable_initial_backfill)
    }

    pub(crate) fn all_profiles_unavailable(&self) -> bool {
        !self.connections.is_empty()
            && self
                .connections
                .iter()
                .all(StoredPlayingProfileConnection::is_profile_unavailable)
    }

    pub(crate) fn profile_unavailable_notices(&self) -> Vec<ProfileUnavailableNotice> {
        if !self.all_profiles_unavailable() {
            return Vec::new();
        }
        self.connections
            .iter()
            .filter_map(StoredPlayingProfileConnection::profile_unavailable_notice)
            .collect()
    }

    pub(crate) fn profile_unavailable_notice(
        &self,
        provider: DailyCoachingProvider,
        identity_username: &str,
    ) -> Option<ProfileUnavailableNotice> {
        self.connection_for_identity(provider, identity_username)
            .and_then(StoredPlayingProfileConnection::profile_unavailable_notice)
    }

    pub(crate) fn all_profile_unavailable_notices(&self) -> Vec<ProfileUnavailableNotice> {
        self.connections
            .iter()
            .filter_map(StoredPlayingProfileConnection::profile_unavailable_notice)
            .collect()
    }

    pub(super) fn connect(
        &mut self,
        player_id: &PlayerId,
        connection: StoredPlayingProfileConnection,
        timezone: String,
        now: DateTime<Utc>,
    ) -> Result<StoredPlayingProfileConnection, DailyCoachingStoreError> {
        self.validate_player_id(player_id)?;
        if let Some(existing) = self.connection(connection.provider) {
            return if existing.identity_username == connection.identity_username {
                let existing = existing.clone();
                self.player_id.get_or_insert_with(|| player_id.clone());
                Ok(existing)
            } else {
                Err(DailyCoachingDomainError::ProviderAlreadyConnected.into())
            };
        }
        let was_empty = self.connections.is_empty();
        self.connections.push(connection.clone());
        self.connections.sort_by_key(|entry| entry.provider);
        if was_empty {
            self.enabled = true;
            let timezone = self.timezone.get_or_insert(timezone);
            self.next_daily_window = Some(local_date(now, timezone)?);
        }
        self.player_id.get_or_insert_with(|| player_id.clone());
        self.advance_revision()?;
        Ok(connection)
    }

    pub(super) fn bind_player(
        &mut self,
        player_id: &PlayerId,
    ) -> Result<(), DailyCoachingStoreError> {
        self.validate_player_id(player_id)?;
        if !self.connections.is_empty() {
            self.player_id.get_or_insert_with(|| player_id.clone());
        }
        Ok(())
    }

    pub(super) fn replace(
        &mut self,
        connection: StoredPlayingProfileConnection,
        expected_identity_username: &str,
    ) -> Result<Self, DailyCoachingStoreError> {
        let Some(index) = self
            .connections
            .iter()
            .position(|existing| existing.provider == connection.provider)
        else {
            return Err(DailyCoachingDomainError::StalePlayingProfile.into());
        };
        if self.connections[index].identity_username == connection.identity_username {
            return Ok(self.clone());
        }
        if self.connections[index].identity_username != expected_identity_username {
            return Err(DailyCoachingDomainError::StalePlayingProfile.into());
        }
        self.connections[index] = connection;
        if self.enabled {
            self.advance_run_fence()?;
        }
        self.advance_revision()?;
        Ok(self.clone())
    }

    pub(super) fn remove(
        &mut self,
        provider: DailyCoachingProvider,
        expected_identity_username: &str,
    ) -> Result<Self, DailyCoachingStoreError> {
        let Some(index) = self
            .connections
            .iter()
            .position(|connection| connection.provider == provider)
        else {
            return Ok(self.clone());
        };
        if self.connections[index].identity_username != expected_identity_username {
            return Err(DailyCoachingDomainError::StalePlayingProfile.into());
        }
        self.connections.remove(index);
        if self.enabled {
            self.advance_run_fence()?;
        }
        if self.connections.is_empty() {
            self.enabled = false;
            self.next_daily_window = None;
        }
        self.advance_revision()?;
        Ok(self.clone())
    }

    pub(super) fn set_enabled(
        &mut self,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> Result<Self, DailyCoachingStoreError> {
        if enabled && self.connections.is_empty() {
            return Err(DailyCoachingDomainError::NoPlayingProfile.into());
        }
        if self.enabled != enabled {
            self.enabled = enabled;
            if enabled {
                self.next_daily_window = Some(local_date(
                    now,
                    self.timezone
                        .as_deref()
                        .ok_or(DailyCoachingStoreError::InvalidRecord)?,
                )?);
            } else {
                self.next_daily_window = None;
                self.advance_run_fence()?;
            }
            self.advance_revision()?;
        }
        Ok(self.clone())
    }

    pub(super) fn advance_daily_window(
        &mut self,
        expected: NaiveDate,
        next: NaiveDate,
    ) -> Result<Self, DailyCoachingStoreError> {
        if self.next_daily_window == Some(next) {
            return Ok(self.clone());
        }
        if !self.enabled || self.next_daily_window != Some(expected) || next <= expected {
            return Err(DailyCoachingStoreError::Conflict);
        }
        self.next_daily_window = Some(next);
        self.advance_revision()?;
        Ok(self.clone())
    }

    #[cfg(test)]
    pub(super) fn resolve_initial_backfill(
        &mut self,
        expected_run_fence: u64,
        provider: DailyCoachingProvider,
        expected_identity_username: &str,
        games: Vec<ProfileGameWindowEntry>,
    ) -> Result<Self, DailyCoachingStoreError> {
        self.mutate_initial_backfill(
            expected_run_fence,
            provider,
            expected_identity_username,
            InitialBackfillMutation::Resolve(games),
        )
    }

    #[cfg(test)]
    pub(super) fn checkpoint_initial_backfill(
        &mut self,
        expected_run_fence: u64,
        provider: DailyCoachingProvider,
        expected_identity_username: &str,
        games: Vec<ProfileGameWindowEntry>,
        cursor: RecentProfileGameCursor,
    ) -> Result<Self, DailyCoachingStoreError> {
        self.mutate_initial_backfill(
            expected_run_fence,
            provider,
            expected_identity_username,
            InitialBackfillMutation::Checkpoint { games, cursor },
        )
    }

    pub(crate) fn mutate_initial_backfill(
        &mut self,
        expected_run_fence: u64,
        provider: DailyCoachingProvider,
        expected_identity_username: &str,
        mutation: InitialBackfillMutation,
    ) -> Result<Self, DailyCoachingStoreError> {
        self.require_initial_backfill_fence(expected_run_fence)?;
        let Some(connection_index) = self.connections.iter().position(|connection| {
            connection.provider == provider
                && connection.identity_username == expected_identity_username
        }) else {
            return Err(DailyCoachingStoreError::Fenced);
        };
        if !matches!(&mutation, InitialBackfillMutation::Reconcile(_))
            && !matches!(
                self.connections[connection_index].initial_backfill,
                InitialBackfill::Pending { .. }
            )
        {
            return Ok(self.clone());
        }
        match mutation {
            InitialBackfillMutation::Reconcile(digested_games) => {
                if !self.connections[connection_index]
                    .initial_backfill
                    .reconcile(&digested_games)
                {
                    return Ok(self.clone());
                }
            }
            InitialBackfillMutation::Resolve(games) => {
                self.connections[connection_index].initial_backfill =
                    InitialBackfill::resolved(games)?;
            }
            InitialBackfillMutation::ResolveStalled(games) => {
                self.connections[connection_index].initial_backfill =
                    InitialBackfill::resolved_stalled(games)?;
            }
            InitialBackfillMutation::Checkpoint { games, cursor } => {
                self.connections[connection_index].initial_backfill =
                    InitialBackfill::checkpointed(games, cursor)?;
            }
            InitialBackfillMutation::Unavailable(reason) => {
                self.connections[connection_index].initial_backfill = InitialBackfill::Completed {
                    had_eligible_games: false,
                    unavailable_reason: Some(reason),
                };
            }
        }
        self.advance_revision()?;
        Ok(self.clone())
    }

    fn require_initial_backfill_fence(
        &self,
        expected_run_fence: u64,
    ) -> Result<(), DailyCoachingStoreError> {
        if self.enabled && self.run_fence == expected_run_fence {
            Ok(())
        } else {
            Err(DailyCoachingStoreError::Fenced)
        }
    }

    pub(super) fn reconcile_initial_backfills(
        &mut self,
        digested_games: &BTreeSet<ProfileGameSourceIdentity>,
    ) -> Result<Self, DailyCoachingStoreError> {
        let mut changed = false;
        for connection in &mut self.connections {
            changed |= connection.initial_backfill.reconcile(digested_games);
        }
        if changed {
            self.advance_revision()?;
        }
        Ok(self.clone())
    }

    pub(super) fn accept_nudge(
        &mut self,
        now: DateTime<Utc>,
        minimum_interval: Duration,
    ) -> Result<NudgeAdmission, DailyCoachingStoreError> {
        let accepted = self.enabled
            && self.last_nudge_at.is_none_or(|last| {
                chrono::TimeDelta::from_std(minimum_interval)
                    .is_ok_and(|interval| now.signed_duration_since(last) >= interval)
            });
        if accepted {
            self.last_nudge_at = Some(now);
            self.advance_revision()?;
        }
        Ok(NudgeAdmission {
            state: self.clone(),
            accepted,
        })
    }

    pub(super) fn observe_profile_health(
        &mut self,
        provider: DailyCoachingProvider,
        expected_identity_username: &str,
        observation: ProfileHealthObservation,
        now: DateTime<Utc>,
    ) -> Result<Option<Self>, DailyCoachingStoreError> {
        let Some(connection) = self.connections.iter_mut().find(|connection| {
            connection.provider == provider
                && connection.identity_username == expected_identity_username
        }) else {
            return Ok(None);
        };
        if connection.observe_health(observation, now)? {
            self.advance_revision()?;
        }
        Ok(Some(self.clone()))
    }

    pub(crate) fn validate(&self) -> Result<(), DailyCoachingStoreError> {
        validate_setup_fields(self.enabled, self.timezone.as_deref(), &self.connections)?;
        if self.schema_version != STATE_SCHEMA_VERSION
            || self.player_id.as_ref().is_some_and(|player_id| {
                DailyCoachingOwnerKey::for_player(player_id) != self.owner_key
            })
            || (self.enabled != self.next_daily_window.is_some())
        {
            Err(DailyCoachingStoreError::InvalidRecord)
        } else {
            Ok(())
        }
    }

    pub(crate) fn validate_for(
        &self,
        owner_key: &DailyCoachingOwnerKey,
    ) -> Result<(), DailyCoachingStoreError> {
        self.validate()?;
        if &self.owner_key == owner_key {
            Ok(())
        } else {
            Err(DailyCoachingStoreError::InvalidRecord)
        }
    }

    fn validate_player_id(&self, player_id: &PlayerId) -> Result<(), DailyCoachingStoreError> {
        if DailyCoachingOwnerKey::for_player(player_id) != self.owner_key
            || self
                .player_id
                .as_ref()
                .is_some_and(|stored| stored != player_id)
        {
            Err(DailyCoachingStoreError::InvalidRecord)
        } else {
            Ok(())
        }
    }

    fn advance_revision(&mut self) -> Result<(), DailyCoachingStoreError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(DailyCoachingStoreError::InvalidRecord)?;
        Ok(())
    }

    fn advance_run_fence(&mut self) -> Result<(), DailyCoachingStoreError> {
        self.run_fence = self
            .run_fence
            .checked_add(1)
            .ok_or(DailyCoachingStoreError::InvalidRecord)?;
        Ok(())
    }

    pub(crate) fn project(&self) -> DailyCoachingSetupState {
        if self.connections.is_empty() {
            DailyCoachingSetupState::NotConnected
        } else {
            DailyCoachingSetupState::Connected {
                enabled: self.enabled,
                timezone: self
                    .timezone
                    .clone()
                    .expect("validated connected state has a timezone"),
                connections: self
                    .connections
                    .iter()
                    .map(StoredPlayingProfileConnection::project)
                    .collect(),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredPlayingProfileConnection {
    provider: DailyCoachingProvider,
    identity_username: String,
    username: String,
    canonical_url: String,
    #[serde(default)]
    health: StoredPlayingProfileHealth,
    #[serde(default)]
    initial_backfill: InitialBackfill,
}

impl StoredPlayingProfileConnection {
    pub(crate) fn from_validated(profile: ValidatedPublicChessProfile) -> Self {
        Self {
            provider: profile.provider().into(),
            identity_username: profile.identity_username().to_string(),
            username: profile.username().to_string(),
            canonical_url: profile.canonical_url().to_string(),
            health: StoredPlayingProfileHealth::default(),
            initial_backfill: InitialBackfill::default(),
        }
    }

    pub(crate) fn identity_username(&self) -> &str {
        &self.identity_username
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn canonical_url(&self) -> &str {
        &self.canonical_url
    }

    pub(crate) fn provider(&self) -> DailyCoachingProvider {
        self.provider
    }

    pub(crate) fn initial_backfill(&self) -> InitialBackfillSnapshot {
        match &self.initial_backfill {
            InitialBackfill::Pending { games, cursor } => InitialBackfillSnapshot::Pending {
                games: games.clone(),
                cursor: cursor.clone(),
            },
            InitialBackfill::Owed { games, .. } => InitialBackfillSnapshot::Owed(games.clone()),
            InitialBackfill::Completed { .. } => InitialBackfillSnapshot::Completed,
        }
    }

    fn has_unresolved_initial_backfill(&self) -> bool {
        matches!(
            self.initial_backfill,
            InitialBackfill::Pending { .. } | InitialBackfill::Owed { .. }
        )
    }

    fn has_empty_completed_backfill(&self) -> bool {
        matches!(
            self.initial_backfill,
            InitialBackfill::Completed {
                had_eligible_games: false,
                unavailable_reason: None,
            }
        )
    }

    fn has_unavailable_initial_backfill(&self) -> bool {
        matches!(
            self.initial_backfill,
            InitialBackfill::Completed {
                unavailable_reason: Some(_),
                ..
            }
        )
    }

    pub(crate) fn is_valid(&self) -> bool {
        !self.identity_username.is_empty()
            && self.identity_username == self.username.to_ascii_lowercase()
            && PublicChessProfile::parse(&self.canonical_url).is_ok_and(|profile| {
                DailyCoachingProvider::from(profile.provider()) == self.provider
                    && profile.identity_username() == self.identity_username
            })
            && self.health_is_valid()
            && self.initial_backfill.is_valid_for(self)
    }

    fn project(&self) -> PlayingProfileConnection {
        PlayingProfileConnection {
            provider: self.provider,
            username: self.username.clone(),
            canonical_url: self.canonical_url.clone(),
            status: if self.is_profile_unavailable() {
                PlayingProfileConnectionStatus::ProfileUnavailable
            } else {
                PlayingProfileConnectionStatus::Connected
            },
        }
    }

    pub(crate) fn completed_outcome(&self) -> ConnectPlayingProfileOutcome {
        let projected = self.project();
        ConnectPlayingProfileOutcome::Completed {
            provider: projected.provider,
            username: projected.username,
            canonical_url: projected.canonical_url,
            status: projected.status,
        }
    }

    #[cfg(test)]
    pub(crate) fn test(provider: DailyCoachingProvider, username: &str) -> Self {
        let canonical_url = match provider {
            DailyCoachingProvider::Lichess => format!("https://lichess.org/@/{username}"),
            DailyCoachingProvider::ChessCom => {
                format!("https://www.chess.com/member/{username}")
            }
        };
        Self {
            provider,
            identity_username: username.to_ascii_lowercase(),
            username: username.to_string(),
            canonical_url,
            health: StoredPlayingProfileHealth::default(),
            initial_backfill: InitialBackfill::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NudgeAdmission {
    pub(crate) state: DailyCoachingDocument,
    pub(crate) accepted: bool,
}

fn validate_setup_fields(
    enabled: bool,
    timezone: Option<&str>,
    connections: &[StoredPlayingProfileConnection],
) -> Result<(), DailyCoachingStoreError> {
    let providers = connections
        .iter()
        .map(|connection| connection.provider)
        .collect::<BTreeSet<_>>();
    if providers.len() != connections.len()
        || (connections.is_empty() && enabled)
        || (!connections.is_empty() && timezone.is_none())
        || timezone.is_some_and(|timezone| canonical_timezone(timezone).is_none())
        || connections.iter().any(|connection| !connection.is_valid())
    {
        Err(DailyCoachingStoreError::InvalidRecord)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DailyCoachingDomainError {
    #[error("the provider already has a different Playing Profile Connection")]
    ProviderAlreadyConnected,
    #[error("the Playing Profile Connection changed after it was read")]
    StalePlayingProfile,
    #[error("Daily Coaching cannot be enabled without a Playing Profile Connection")]
    NoPlayingProfile,
}

impl DailyCoachingDomainError {
    pub(crate) fn rejection_reason(self) -> DailyCoachingMutationRejectionReason {
        match self {
            Self::NoPlayingProfile => DailyCoachingMutationRejectionReason::NoPlayingProfile,
            Self::ProviderAlreadyConnected | Self::StalePlayingProfile => {
                DailyCoachingMutationRejectionReason::StalePlayingProfile
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DailyCoachingStoreError {
    #[error(transparent)]
    Domain(#[from] DailyCoachingDomainError),
    #[error("Daily Coaching persistence is misconfigured: {0}")]
    Configuration(String),
    #[error("Daily Coaching persistence transport failed")]
    Transport,
    #[error("Daily Coaching persistence is unavailable")]
    Unavailable,
    #[error("Daily Coaching persistence conflicted")]
    Conflict,
    #[error("Daily Coaching state changed while work was in flight")]
    Fenced,
    #[error("Daily Coaching persistence returned an invalid record")]
    InvalidRecord,
}

impl From<super::schedule::DailyWindowError> for DailyCoachingStoreError {
    fn from(_error: super::schedule::DailyWindowError) -> Self {
        Self::InvalidRecord
    }
}

#[cfg(test)]
mod tests;
