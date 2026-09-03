use std::{collections::BTreeSet, time::Duration};

use chrono::{DateTime, NaiveDate, TimeDelta, Timelike, Utc};
use serde::{Deserialize, Serialize};

use super::{
    configuration::DailyCoachingConfiguration,
    digest::{CoachingWindowKind, FrozenDailyGameReview, FrozenDigestGame},
    schedule::DailyWindow,
    selection::SelectedDailyCoachingGame,
    state::{
        DailyCoachingDocument, DailyCoachingOwnerKey, DailyCoachingStoreError,
        StoredPlayingProfileConnection,
    },
    DailyCoachingProvider,
};
use crate::{
    profile_game_feed::{ProfileGameSourceIdentity, ProfileGameWindowEntry, PublicChessProfile},
    review_session_contract::PlayerId,
};

pub(super) mod firestore;
mod lease;
mod memory;
mod model;
mod progress;
mod store;
use lease::expires_at;
pub(crate) use memory::InMemoryDailyCoachingRunStore;
use model::StoredDailyCoachingRunDocument;
pub(crate) use store::{DailyCoachingRunStore, RunStoreFuture};

pub(crate) const RUN_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DailyCoachingRunClaim {
    Created(Box<DailyCoachingRunDocument>),
    Existing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DailyCoachingRunAddress {
    pub(crate) owner_key: DailyCoachingOwnerKey,
    pub(crate) run_id: String,
}

impl DailyCoachingRunAddress {
    fn key(&self) -> (String, String) {
        (self.owner_key.as_str().to_string(), self.run_id.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DailyCoachingRunStatus {
    Active,
    PendingSelection,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DailyCoachingRunOutcome {
    Published,
    NoDigest,
    Fenced,
    Abandoned,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DailyCoachingRunLease {
    holder_id: String,
    fencing_token: u64,
    heartbeat_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DailyCoachingRunConnection {
    provider: DailyCoachingProvider,
    identity_username: String,
    username: String,
    canonical_url: String,
}

impl From<&StoredPlayingProfileConnection> for DailyCoachingRunConnection {
    fn from(connection: &StoredPlayingProfileConnection) -> Self {
        Self {
            provider: connection.provider(),
            identity_username: connection.identity_username().to_string(),
            username: connection.username().to_string(),
            canonical_url: connection.canonical_url().to_string(),
        }
    }
}

impl DailyCoachingRunConnection {
    pub(crate) fn provider(&self) -> DailyCoachingProvider {
        self.provider
    }

    pub(crate) fn identity_username(&self) -> &str {
        &self.identity_username
    }

    pub(crate) fn canonical_url(&self) -> &str {
        &self.canonical_url
    }

    fn is_valid(&self) -> bool {
        !self.identity_username.is_empty()
            && self.identity_username == self.username.to_ascii_lowercase()
            && PublicChessProfile::parse(&self.canonical_url).is_ok_and(|profile| {
                DailyCoachingProvider::from(profile.provider()) == self.provider
                    && profile.identity_username() == self.identity_username
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum DailyCoachingRunState {
    Active {
        lease: DailyCoachingRunLease,
        next_attempt_at: DateTime<Utc>,
    },
    PendingSelection {
        next_attempt_at: DateTime<Utc>,
    },
    Completed {
        outcome: DailyCoachingRunOutcome,
        finished_at: DateTime<Utc>,
        next_attempt_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DailyCoachingRunGame {
    pub(crate) selected: ProfileGameWindowEntry,
    #[serde(default)]
    window_kind: CoachingWindowKind,
    attempts: u8,
    progress: DailyCoachingGameProgress,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "gameStatus",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum DailyCoachingGameProgress {
    Pending,
    Reviewed { review: FrozenDailyGameReview },
    TerminalUnreviewed,
    RetryExhaustedUnreviewed,
    DeadlineUnreviewed,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DailyCoachingGameResult {
    Reviewed(FrozenDailyGameReview),
    Retryable,
    Terminal,
    RetryExhausted { attempted: bool },
    UnfinishedAtDeadline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "StoredDailyCoachingRunDocument")]
pub(crate) struct DailyCoachingRunDocument {
    schema_version: u16,
    owner_key: DailyCoachingOwnerKey,
    player_id: Option<PlayerId>,
    run_id: String,
    coverage_date: NaiveDate,
    timezone: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    due_at: DateTime<Utc>,
    deadline: DateTime<Utc>,
    claimed_at: DateTime<Utc>,
    run_fence: u64,
    takeover_count: u32,
    connections: Vec<DailyCoachingRunConnection>,
    selection: Option<Vec<DailyCoachingRunGame>>,
    /// How many times an Administrator has reopened this window to rebuild its Coaching Digest.
    #[serde(default, skip_serializing_if = "is_zero")]
    regeneration_count: u32,
    #[serde(flatten)]
    state: DailyCoachingRunState,
    purge_at: DateTime<Utc>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

/// Firestore stores the queryable mirror timestamps (`nextAttemptAt`,
/// `purgeAt`, `finishedAt`) at microsecond precision, while the instants
/// serialized inside the document keep nanoseconds. `validate()` compares the
/// two, so every instant must be born microsecond-aligned or a run becomes
/// unreadable after one round trip.
fn stored_instant(value: DateTime<Utc>) -> DateTime<Utc> {
    value
        .with_nanosecond(value.nanosecond() / 1_000 * 1_000)
        .expect("truncated nanoseconds stay in range")
}

impl DailyCoachingRunDocument {
    pub(crate) fn claimed(
        state: &DailyCoachingDocument,
        window: &DailyWindow,
        holder_id: &str,
        now: DateTime<Utc>,
        configuration: &DailyCoachingConfiguration,
    ) -> Result<Self, DailyCoachingRunStoreError> {
        let now = stored_instant(now);
        let deadline = stored_instant(window.deadline);
        let mut lease = DailyCoachingRunLease::first(holder_id, now, configuration.lease_ttl)?;
        lease.expires_at = lease.expires_at.min(deadline);
        let run = Self {
            schema_version: RUN_SCHEMA_VERSION,
            owner_key: state.owner_key().clone(),
            player_id: Some(
                state
                    .player_id()
                    .ok_or(DailyCoachingRunStoreError::InvalidRecord)?
                    .clone(),
            ),
            run_id: window.run_id(),
            coverage_date: window.coverage_date,
            timezone: state
                .timezone()
                .ok_or(DailyCoachingRunStoreError::InvalidRecord)?
                .to_string(),
            starts_at: stored_instant(window.starts_at),
            ends_at: stored_instant(window.ends_at),
            due_at: stored_instant(window.due_at),
            deadline,
            claimed_at: now,
            run_fence: state.run_fence(),
            takeover_count: 0,
            connections: state
                .connections()
                .iter()
                .map(DailyCoachingRunConnection::from)
                .collect(),
            selection: None,
            regeneration_count: 0,
            state: DailyCoachingRunState::Active {
                next_attempt_at: lease.expires_at,
                lease,
            },
            purge_at: retention_at(now, configuration.run_retention_days)?,
        };
        run.validate()?;
        Ok(run)
    }

    pub(crate) fn skipped(
        state: &DailyCoachingDocument,
        window: &DailyWindow,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> Result<Self, DailyCoachingRunStoreError> {
        let now = stored_instant(now);
        let purge_at = retention_at(now, retention_days)?;
        let run = Self {
            schema_version: RUN_SCHEMA_VERSION,
            owner_key: state.owner_key().clone(),
            player_id: state.player_id().cloned(),
            run_id: window.run_id(),
            coverage_date: window.coverage_date,
            timezone: state
                .timezone()
                .ok_or(DailyCoachingRunStoreError::InvalidRecord)?
                .to_string(),
            starts_at: stored_instant(window.starts_at),
            ends_at: stored_instant(window.ends_at),
            due_at: stored_instant(window.due_at),
            deadline: stored_instant(window.deadline),
            claimed_at: now,
            run_fence: state.run_fence(),
            takeover_count: 0,
            connections: state
                .connections()
                .iter()
                .map(DailyCoachingRunConnection::from)
                .collect(),
            selection: None,
            regeneration_count: 0,
            state: DailyCoachingRunState::Completed {
                outcome: DailyCoachingRunOutcome::Skipped,
                finished_at: now,
                next_attempt_at: purge_at,
            },
            purge_at,
        };
        run.validate()?;
        Ok(run)
    }

    pub(crate) fn address(&self) -> DailyCoachingRunAddress {
        DailyCoachingRunAddress {
            owner_key: self.owner_key.clone(),
            run_id: self.run_id.clone(),
        }
    }

    pub(crate) fn lease(&self) -> Result<&DailyCoachingRunLease, DailyCoachingRunStoreError> {
        match &self.state {
            DailyCoachingRunState::Active { lease, .. } => Ok(lease),
            DailyCoachingRunState::PendingSelection { .. }
            | DailyCoachingRunState::Completed { .. } => Err(DailyCoachingRunStoreError::Fenced),
        }
    }

    pub(crate) fn run_fence(&self) -> u64 {
        self.run_fence
    }

    pub(crate) fn takeover_count(&self) -> u32 {
        self.takeover_count
    }

    pub(crate) fn finished_at(&self) -> Option<DateTime<Utc>> {
        match &self.state {
            DailyCoachingRunState::Completed { finished_at, .. } => Some(*finished_at),
            DailyCoachingRunState::Active { .. }
            | DailyCoachingRunState::PendingSelection { .. } => None,
        }
    }

    pub(crate) fn operational_counts(&self) -> DailyCoachingRunOperationalCounts {
        let mut counts = DailyCoachingRunOperationalCounts::default();
        for game in self.selection.iter().flatten() {
            if game.attempts > 0 {
                counts.attempted_games += 1;
            }
            match &game.progress {
                DailyCoachingGameProgress::Reviewed { review } => {
                    counts.game_import_ids.push(review.game_import_id.clone());
                }
                DailyCoachingGameProgress::TerminalUnreviewed => {
                    counts.permanent_game_failures += 1;
                }
                DailyCoachingGameProgress::RetryExhaustedUnreviewed => {
                    counts.retry_exhausted += 1;
                }
                DailyCoachingGameProgress::Pending
                | DailyCoachingGameProgress::DeadlineUnreviewed => {}
            }
        }
        counts
    }

    pub(crate) fn player_id(&self) -> Result<&PlayerId, DailyCoachingRunStoreError> {
        self.player_id
            .as_ref()
            .ok_or(DailyCoachingRunStoreError::InvalidRecord)
    }

    pub(super) fn bind_player(
        &mut self,
        player_id: Option<&PlayerId>,
    ) -> Result<bool, DailyCoachingRunStoreError> {
        let Some(player_id) = player_id else {
            return Ok(false);
        };
        if DailyCoachingOwnerKey::for_player(player_id) != self.owner_key {
            return Err(DailyCoachingRunStoreError::InvalidRecord);
        }
        match &self.player_id {
            Some(stored) if stored != player_id => Err(DailyCoachingRunStoreError::InvalidRecord),
            Some(_) => Ok(false),
            None => {
                self.player_id = Some(player_id.clone());
                Ok(true)
            }
        }
    }

    pub(crate) fn regeneration_count(&self) -> u32 {
        self.regeneration_count
    }

    pub(crate) fn connections(&self) -> &[DailyCoachingRunConnection] {
        &self.connections
    }

    pub(crate) fn starts_at(&self) -> DateTime<Utc> {
        self.starts_at
    }

    pub(crate) fn ends_at(&self) -> DateTime<Utc> {
        self.ends_at
    }

    pub(crate) fn deadline(&self) -> DateTime<Utc> {
        self.deadline
    }

    pub(crate) fn coverage_date(&self) -> NaiveDate {
        self.coverage_date
    }

    pub(crate) fn timezone(&self) -> &str {
        &self.timezone
    }

    pub(crate) fn selection(&self) -> Option<&[DailyCoachingRunGame]> {
        self.selection.as_deref()
    }

    pub(crate) fn next_pending_game(&self) -> Option<(usize, &DailyCoachingRunGame)> {
        self.selection
            .as_ref()?
            .iter()
            .enumerate()
            .find(|(_, game)| matches!(game.progress, DailyCoachingGameProgress::Pending))
    }

    pub(crate) fn reviewed_games(&self) -> Vec<(ProfileGameWindowEntry, FrozenDailyGameReview)> {
        self.selection
            .iter()
            .flatten()
            .filter_map(|game| match &game.progress {
                DailyCoachingGameProgress::Reviewed { review } => {
                    Some((game.selected.clone(), review.clone()))
                }
                DailyCoachingGameProgress::Pending
                | DailyCoachingGameProgress::TerminalUnreviewed
                | DailyCoachingGameProgress::RetryExhaustedUnreviewed
                | DailyCoachingGameProgress::DeadlineUnreviewed => None,
            })
            .collect()
    }

    pub(crate) fn reviewed_games_with_provenance(&self) -> Vec<FrozenDigestGame> {
        self.selection
            .iter()
            .flatten()
            .filter_map(|game| match &game.progress {
                DailyCoachingGameProgress::Reviewed { review } => Some(FrozenDigestGame {
                    selected: game.selected.clone(),
                    review: review.clone(),
                    window_kind: game.window_kind,
                }),
                DailyCoachingGameProgress::Pending
                | DailyCoachingGameProgress::TerminalUnreviewed
                | DailyCoachingGameProgress::RetryExhaustedUnreviewed
                | DailyCoachingGameProgress::DeadlineUnreviewed => None,
            })
            .collect()
    }

    fn settled_initial_backfill_sources(&self) -> BTreeSet<ProfileGameSourceIdentity> {
        self.selection
            .iter()
            .flatten()
            .filter(|game| game.window_kind == CoachingWindowKind::InitialBackfill)
            .filter_map(|game| match game.progress {
                DailyCoachingGameProgress::Reviewed { .. }
                | DailyCoachingGameProgress::TerminalUnreviewed
                | DailyCoachingGameProgress::RetryExhaustedUnreviewed => {
                    Some(game.selected.source_identity.clone())
                }
                DailyCoachingGameProgress::Pending
                | DailyCoachingGameProgress::DeadlineUnreviewed => None,
            })
            .collect()
    }

    fn has_pending_games(&self) -> bool {
        self.selection
            .iter()
            .flatten()
            .any(|game| matches!(game.progress, DailyCoachingGameProgress::Pending))
    }

    pub(crate) fn next_attempt_at(&self) -> DateTime<Utc> {
        match &self.state {
            DailyCoachingRunState::Active {
                next_attempt_at, ..
            }
            | DailyCoachingRunState::PendingSelection { next_attempt_at }
            | DailyCoachingRunState::Completed {
                next_attempt_at, ..
            } => *next_attempt_at,
        }
    }

    pub(crate) fn status(&self) -> DailyCoachingRunStatus {
        match &self.state {
            DailyCoachingRunState::Active { .. } => DailyCoachingRunStatus::Active,
            DailyCoachingRunState::PendingSelection { .. } => {
                DailyCoachingRunStatus::PendingSelection
            }
            DailyCoachingRunState::Completed { .. } => DailyCoachingRunStatus::Completed,
        }
    }

    pub(crate) fn outcome(&self) -> Option<DailyCoachingRunOutcome> {
        match &self.state {
            DailyCoachingRunState::Completed { outcome, .. } => Some(*outcome),
            DailyCoachingRunState::Active { .. }
            | DailyCoachingRunState::PendingSelection { .. } => None,
        }
    }

    fn key(&self) -> (String, String) {
        self.address().key()
    }

    fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        matches!(
            &self.state,
            DailyCoachingRunState::Active {
                next_attempt_at,
                ..
            } if *next_attempt_at <= now
        )
    }

    fn take_over(
        &mut self,
        holder_id: &str,
        now: DateTime<Utc>,
        lease_ttl: Duration,
    ) -> Result<(), DailyCoachingRunStoreError> {
        let now = stored_instant(now);
        if !self.is_expired_at(now) {
            return Err(DailyCoachingRunStoreError::Fenced);
        }
        let DailyCoachingRunState::Active { lease, .. } = &self.state else {
            return Err(DailyCoachingRunStoreError::Fenced);
        };
        let mut replacement =
            DailyCoachingRunLease::takeover(holder_id, lease.fencing_token, now, lease_ttl)?;
        if now < self.deadline {
            replacement.expires_at = replacement.expires_at.min(self.deadline);
        }
        self.state = DailyCoachingRunState::Active {
            next_attempt_at: replacement.expires_at,
            lease: replacement,
        };
        self.takeover_count = self
            .takeover_count
            .checked_add(1)
            .ok_or(DailyCoachingRunStoreError::InvalidRecord)?;
        Ok(())
    }

    /// Re-opens a terminal window so an Administrator can rebuild its Coaching Digest.
    /// The selection is cleared so the rebuild re-selects and re-reviews rather than
    /// republishing frozen reviews, and the deadline is extended to bound the new work.
    #[cfg_attr(not(test), allow(dead_code))]
    fn reopen_for_regeneration(
        &mut self,
        holder_id: &str,
        now: DateTime<Utc>,
        lease_ttl: Duration,
        deadline: DateTime<Utc>,
    ) -> Result<(), DailyCoachingRunStoreError> {
        let now = stored_instant(now);
        let deadline = stored_instant(deadline);
        // A window that published nothing is the one most worth rebuilding after a provider fix,
        // so both terminal digest outcomes reopen. An active or abandoned Run does not.
        if !matches!(
            self.outcome(),
            Some(DailyCoachingRunOutcome::Published | DailyCoachingRunOutcome::NoDigest)
        ) || deadline <= now
        {
            return Err(DailyCoachingRunStoreError::Fenced);
        }
        // The completed Run kept no lease to continue, and no holder can be outstanding on a
        // published window; a stale lease still fails `require_lease` on the whole record.
        let lease = DailyCoachingRunLease::first(holder_id, now, lease_ttl)?;
        // Incremented here, not at publication: the rebuild pass itself must be able to tell it
        // is rebuilding, so selection can treat this window's own Games as selectable again.
        self.regeneration_count = self
            .regeneration_count
            .checked_add(1)
            .ok_or(DailyCoachingRunStoreError::InvalidRecord)?;
        self.deadline = deadline;
        self.selection = None;
        self.state = DailyCoachingRunState::Active {
            next_attempt_at: lease.expires_at,
            lease,
        };
        Ok(())
    }

    fn heartbeat(
        &mut self,
        lease: &DailyCoachingRunLease,
        now: DateTime<Utc>,
        lease_ttl: Duration,
    ) -> Result<(), DailyCoachingRunStoreError> {
        self.require_lease(lease)?;
        let now = stored_instant(now);
        if now >= self.deadline {
            return Err(DailyCoachingRunStoreError::Fenced);
        }
        let expires_at = expires_at(now, lease_ttl)?.min(self.deadline);
        let DailyCoachingRunState::Active {
            lease: current,
            next_attempt_at,
        } = &mut self.state
        else {
            return Err(DailyCoachingRunStoreError::Fenced);
        };
        current.heartbeat_at = now;
        current.expires_at = expires_at;
        *next_attempt_at = expires_at;
        Ok(())
    }

    fn freeze_selection(
        &mut self,
        lease: &DailyCoachingRunLease,
        selection: Vec<SelectedDailyCoachingGame>,
    ) -> Result<(), DailyCoachingRunStoreError> {
        self.require_lease(lease)?;
        if selection.is_empty() || selection.len() > 10 {
            return Err(DailyCoachingRunStoreError::InvalidRecord);
        }
        if let Some(existing) = &self.selection {
            let existing = existing
                .iter()
                .map(|game| (&game.selected, game.window_kind))
                .collect::<Vec<_>>();
            let requested = selection
                .iter()
                .map(|game| (&game.selected, game.window_kind))
                .collect::<Vec<_>>();
            return if existing == requested {
                Ok(())
            } else {
                Err(DailyCoachingRunStoreError::Conflict)
            };
        }
        self.selection = Some(
            selection
                .into_iter()
                .map(|selection| DailyCoachingRunGame {
                    selected: selection.selected,
                    window_kind: selection.window_kind,
                    attempts: 0,
                    progress: DailyCoachingGameProgress::Pending,
                })
                .collect(),
        );
        Ok(())
    }

    fn record_game(
        &mut self,
        lease: &DailyCoachingRunLease,
        index: usize,
        result: DailyCoachingGameResult,
        now: DateTime<Utc>,
        retry_at: Option<DateTime<Utc>>,
    ) -> Result<(), DailyCoachingRunStoreError> {
        self.require_lease(lease)?;
        let now = stored_instant(now);
        let retry_at = retry_at.map(stored_instant);
        let game = self
            .selection
            .as_mut()
            .and_then(|games| games.get_mut(index))
            .ok_or(DailyCoachingRunStoreError::InvalidRecord)?;
        if !matches!(game.progress, DailyCoachingGameProgress::Pending) {
            return Err(DailyCoachingRunStoreError::Conflict);
        }
        if !matches!(
            result,
            DailyCoachingGameResult::UnfinishedAtDeadline
                | DailyCoachingGameResult::RetryExhausted { attempted: false }
        ) {
            game.attempts = game
                .attempts
                .checked_add(1)
                .ok_or(DailyCoachingRunStoreError::InvalidRecord)?;
        }
        match result {
            DailyCoachingGameResult::Reviewed(review) => {
                if review.played_plies != game.selected.played_plies {
                    return Err(DailyCoachingRunStoreError::InvalidRecord);
                }
                game.progress = DailyCoachingGameProgress::Reviewed { review };
            }
            DailyCoachingGameResult::Terminal => {
                game.progress = DailyCoachingGameProgress::TerminalUnreviewed;
            }
            DailyCoachingGameResult::RetryExhausted { .. } => {
                game.progress = DailyCoachingGameProgress::RetryExhaustedUnreviewed;
            }
            DailyCoachingGameResult::UnfinishedAtDeadline => {
                game.progress = DailyCoachingGameProgress::DeadlineUnreviewed;
            }
            DailyCoachingGameResult::Retryable => {
                let retry_at = retry_at
                    .filter(|retry_at| *retry_at > now && *retry_at <= self.deadline)
                    .ok_or(DailyCoachingRunStoreError::InvalidRecord)?;
                let DailyCoachingRunState::Active {
                    lease: current,
                    next_attempt_at,
                } = &mut self.state
                else {
                    return Err(DailyCoachingRunStoreError::Fenced);
                };
                current.heartbeat_at = now;
                current.expires_at = retry_at;
                *next_attempt_at = retry_at;
            }
        }
        Ok(())
    }

    fn complete(
        &mut self,
        lease: &DailyCoachingRunLease,
        outcome: DailyCoachingRunOutcome,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> Result<(), DailyCoachingRunStoreError> {
        self.require_lease(lease)?;
        if matches!(
            outcome,
            DailyCoachingRunOutcome::Published | DailyCoachingRunOutcome::Skipped
        ) {
            return Err(DailyCoachingRunStoreError::InvalidRecord);
        }
        let now = stored_instant(now);
        self.purge_at = retention_at(now, retention_days)?;
        self.state = DailyCoachingRunState::Completed {
            outcome,
            finished_at: now,
            next_attempt_at: self.purge_at,
        };
        Ok(())
    }

    fn mark_published(
        &mut self,
        lease: &DailyCoachingRunLease,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> Result<(), DailyCoachingRunStoreError> {
        self.require_lease(lease)?;
        if self.reviewed_games().is_empty() || self.has_pending_games() {
            return Err(DailyCoachingRunStoreError::InvalidRecord);
        }
        let now = stored_instant(now);
        self.purge_at = retention_at(now, retention_days)?;
        self.state = DailyCoachingRunState::Completed {
            outcome: DailyCoachingRunOutcome::Published,
            finished_at: now,
            next_attempt_at: self.purge_at,
        };
        Ok(())
    }

    fn require_lease(
        &self,
        expected: &DailyCoachingRunLease,
    ) -> Result<(), DailyCoachingRunStoreError> {
        match &self.state {
            DailyCoachingRunState::Active { lease, .. }
                if lease.holder_id == expected.holder_id
                    && lease.fencing_token == expected.fencing_token =>
            {
                Ok(())
            }
            DailyCoachingRunState::Active { .. }
            | DailyCoachingRunState::PendingSelection { .. }
            | DailyCoachingRunState::Completed { .. } => Err(DailyCoachingRunStoreError::Fenced),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), DailyCoachingRunStoreError> {
        let completed = matches!(&self.state, DailyCoachingRunState::Completed { .. });
        let providers = self
            .connections
            .iter()
            .map(DailyCoachingRunConnection::provider)
            .collect::<BTreeSet<_>>();
        if self.schema_version != RUN_SCHEMA_VERSION
            || self.run_id != format!("daily-{}", self.coverage_date)
            || self.player_id.as_ref().is_some_and(|player_id| {
                DailyCoachingOwnerKey::for_player(player_id) != self.owner_key
            })
            || self.timezone.parse::<chrono_tz::Tz>().is_err()
            || self.connections.is_empty()
            || self.starts_at >= self.ends_at
            || self.ends_at > self.due_at
            || self.due_at >= self.deadline
            || self.claimed_at < self.due_at
            || self.claimed_at >= self.deadline && !completed
            || providers.len() != self.connections.len()
            || self
                .connections
                .iter()
                .any(|connection| !connection.is_valid())
            || self.purge_at <= self.claimed_at
            || !self.selection_is_valid()
            || !self.state_is_valid()
        {
            Err(DailyCoachingRunStoreError::InvalidRecord)
        } else {
            Ok(())
        }
    }

    fn selection_is_valid(&self) -> bool {
        let Some(selection) = &self.selection else {
            return !matches!(self.outcome(), Some(DailyCoachingRunOutcome::Published));
        };
        let identities = selection
            .iter()
            .map(|game| game.selected.source_identity.clone())
            .collect::<BTreeSet<_>>();
        let import_ids = selection
            .iter()
            .filter_map(|game| match &game.progress {
                DailyCoachingGameProgress::Reviewed { review } => {
                    Some(review.game_import_id.clone())
                }
                DailyCoachingGameProgress::Pending
                | DailyCoachingGameProgress::TerminalUnreviewed
                | DailyCoachingGameProgress::RetryExhaustedUnreviewed
                | DailyCoachingGameProgress::DeadlineUnreviewed => None,
            })
            .collect::<BTreeSet<_>>();
        !selection.is_empty()
            && selection.len() <= 10
            && identities.len() == selection.len()
            && import_ids.len()
                == selection
                    .iter()
                    .filter(|game| {
                        matches!(game.progress, DailyCoachingGameProgress::Reviewed { .. })
                    })
                    .count()
            && selection.iter().all(DailyCoachingRunGame::is_valid)
            && !matches!(self.state, DailyCoachingRunState::PendingSelection { .. })
            && match self.outcome() {
                Some(DailyCoachingRunOutcome::Published) => {
                    !self.reviewed_games().is_empty() && !self.has_pending_games()
                }
                Some(DailyCoachingRunOutcome::NoDigest) => !self.has_pending_games(),
                Some(
                    DailyCoachingRunOutcome::Fenced
                    | DailyCoachingRunOutcome::Abandoned
                    | DailyCoachingRunOutcome::Skipped,
                )
                | None => true,
            }
    }

    fn state_is_valid(&self) -> bool {
        match &self.state {
            DailyCoachingRunState::Active {
                lease,
                next_attempt_at,
            } => {
                !lease.holder_id.trim().is_empty()
                    && lease.fencing_token > 0
                    && lease.heartbeat_at >= self.claimed_at
                    && lease.heartbeat_at <= lease.expires_at
                    && (lease.heartbeat_at >= self.deadline || lease.expires_at <= self.deadline)
                    && *next_attempt_at == lease.expires_at
            }
            DailyCoachingRunState::PendingSelection { next_attempt_at } => {
                *next_attempt_at == self.deadline
            }
            DailyCoachingRunState::Completed {
                finished_at,
                next_attempt_at,
                ..
            } => {
                *finished_at >= self.claimed_at
                    && *finished_at < self.purge_at
                    && *next_attempt_at == self.purge_at
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DailyCoachingRunOperationalCounts {
    pub(crate) attempted_games: u32,
    pub(crate) permanent_game_failures: u32,
    pub(crate) retry_exhausted: u32,
    pub(crate) game_import_ids: Vec<crate::review_session_contract::GameImportId>,
}

fn retention_at(
    now: DateTime<Utc>,
    retention_days: u32,
) -> Result<DateTime<Utc>, DailyCoachingRunStoreError> {
    now.checked_add_signed(TimeDelta::days(i64::from(retention_days)))
        .ok_or(DailyCoachingRunStoreError::InvalidRecord)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
/// Durable Daily Coaching Run-store failure.
pub enum DailyCoachingRunStoreError {
    /// The Run store is not configured correctly.
    #[error("Daily Coaching Run persistence is misconfigured")]
    Configuration,
    /// The Run-store request failed in transit.
    #[error("Daily Coaching Run persistence transport failed")]
    Transport,
    /// The Run store is temporarily unavailable.
    #[error("Daily Coaching Run persistence is unavailable")]
    Unavailable,
    /// A concurrent write prevented the operation from committing.
    #[error("Daily Coaching Run persistence conflicted")]
    Conflict,
    /// The addressed Run does not exist.
    #[error("Daily Coaching Run does not exist")]
    NotFound,
    /// The caller no longer owns the Run lease.
    #[error("Daily Coaching Run holder was fenced")]
    Fenced,
    /// Persisted Run fields violate the Run contract.
    #[error("Daily Coaching Run persistence returned an invalid record")]
    InvalidRecord,
}

fn map_state_error(error: DailyCoachingStoreError) -> DailyCoachingRunStoreError {
    match error {
        DailyCoachingStoreError::Configuration(_) => DailyCoachingRunStoreError::Configuration,
        DailyCoachingStoreError::Transport => DailyCoachingRunStoreError::Transport,
        DailyCoachingStoreError::Unavailable => DailyCoachingRunStoreError::Unavailable,
        DailyCoachingStoreError::Conflict => DailyCoachingRunStoreError::Conflict,
        DailyCoachingStoreError::Fenced => DailyCoachingRunStoreError::Fenced,
        DailyCoachingStoreError::Domain(_) | DailyCoachingStoreError::InvalidRecord => {
            DailyCoachingRunStoreError::InvalidRecord
        }
    }
}

#[cfg(test)]
mod tests;
