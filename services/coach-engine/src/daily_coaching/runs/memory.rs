use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, Utc};

use crate::profile_game_feed::ProfileGameSourceIdentity;

use super::super::digest::{CoachingDigest, DigestedGameCard};
use super::super::selection::SelectedDailyCoachingGame;
use super::{
    map_state_error, DailyCoachingGameResult, DailyCoachingOwnerKey, DailyCoachingRunAddress,
    DailyCoachingRunClaim, DailyCoachingRunConnection, DailyCoachingRunDocument,
    DailyCoachingRunLease, DailyCoachingRunOutcome, DailyCoachingRunStore,
    DailyCoachingRunStoreError, RunStoreFuture,
};
use crate::daily_coaching::state::{InMemoryDailyCoachingStore, InitialBackfillMutation};

#[derive(Default)]
struct InMemoryDailyCoachingPersistence {
    runs: BTreeMap<(String, String), DailyCoachingRunDocument>,
    digests: BTreeMap<(String, String), CoachingDigest>,
    cards: BTreeMap<(String, String), DigestedGameCard>,
}

pub(crate) struct InMemoryDailyCoachingRunStore {
    pub(super) state_store: Arc<InMemoryDailyCoachingStore>,
    persistence: Mutex<InMemoryDailyCoachingPersistence>,
}

impl InMemoryDailyCoachingRunStore {
    pub(crate) fn new(state_store: Arc<InMemoryDailyCoachingStore>) -> Self {
        Self {
            state_store,
            persistence: Mutex::new(InMemoryDailyCoachingPersistence::default()),
        }
    }

    fn mutate(
        &self,
        address: &DailyCoachingRunAddress,
        mutation: impl FnOnce(&mut DailyCoachingRunDocument) -> Result<(), DailyCoachingRunStoreError>,
    ) -> Result<DailyCoachingRunDocument, DailyCoachingRunStoreError> {
        let mut persistence = self
            .persistence
            .lock()
            .expect("in-memory Daily Coaching persistence is not poisoned");
        let key = address.key();
        let mut run = persistence
            .runs
            .get(&key)
            .cloned()
            .ok_or(DailyCoachingRunStoreError::NotFound)?;
        mutation(&mut run)?;
        run.validate()?;
        persistence.runs.insert(key, run.clone());
        Ok(run)
    }

    fn mutate_at_state_fence(
        &self,
        address: &DailyCoachingRunAddress,
        mutation: impl FnOnce(
            &mut DailyCoachingRunDocument,
            bool,
        ) -> Result<(), DailyCoachingRunStoreError>,
    ) -> Result<DailyCoachingRunDocument, DailyCoachingRunStoreError> {
        self.state_store.at_run_fence(
            &address.owner_key,
            self.run_fence(address)?,
            |fenced, player_id| {
                self.mutate(address, |run| {
                    if !fenced {
                        run.bind_player(player_id.as_ref())?;
                    }
                    mutation(run, fenced)
                })
            },
        )
    }

    fn run_fence(
        &self,
        address: &DailyCoachingRunAddress,
    ) -> Result<u64, DailyCoachingRunStoreError> {
        self.persistence
            .lock()
            .expect("in-memory Daily Coaching persistence is not poisoned")
            .runs
            .get(&address.key())
            .map(DailyCoachingRunDocument::run_fence)
            .ok_or(DailyCoachingRunStoreError::NotFound)
    }
}

impl DailyCoachingRunStore for InMemoryDailyCoachingRunStore {
    fn list_digested_game_cards<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Vec<DigestedGameCard>> {
        Box::pin(async move {
            let persistence = self
                .persistence
                .lock()
                .expect("in-memory Daily Coaching persistence is not poisoned");
            Ok(persistence
                .cards
                .iter()
                .filter(|((owner, _), card)| owner == owner_key.as_str() && card.validate().is_ok())
                .map(|(_, card)| card.clone())
                .collect())
        })
    }

    fn create<'a>(
        &'a self,
        run: DailyCoachingRunDocument,
    ) -> RunStoreFuture<'a, DailyCoachingRunClaim> {
        Box::pin(async move {
            run.validate()?;
            let key = run.key();
            let mut persistence = self
                .persistence
                .lock()
                .expect("in-memory Daily Coaching persistence is not poisoned");
            if let std::collections::btree_map::Entry::Vacant(entry) = persistence.runs.entry(key) {
                entry.insert(run.clone());
                Ok(DailyCoachingRunClaim::Created(Box::new(run)))
            } else {
                Ok(DailyCoachingRunClaim::Existing)
            }
        })
    }

    fn expired<'a>(
        &'a self,
        now: DateTime<Utc>,
    ) -> RunStoreFuture<'a, Vec<DailyCoachingRunDocument>> {
        Box::pin(async move {
            let persistence = self
                .persistence
                .lock()
                .expect("in-memory Daily Coaching persistence is not poisoned");
            let mut expired = persistence
                .runs
                .values()
                .filter(|run| run.is_expired_at(now))
                .cloned()
                .collect::<Vec<_>>();
            expired.sort_by_key(DailyCoachingRunDocument::next_attempt_at);
            Ok(expired)
        })
    }

    fn finished_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> RunStoreFuture<'a, Vec<DailyCoachingRunDocument>> {
        Box::pin(async move {
            let persistence = self
                .persistence
                .lock()
                .expect("in-memory Daily Coaching persistence is not poisoned");
            let mut completed = persistence
                .runs
                .values()
                .filter(|run| {
                    run.finished_at()
                        .is_some_and(|finished| finished >= starts_at && finished < ends_at)
                })
                .cloned()
                .collect::<Vec<_>>();
            completed.sort_by_key(|run| (run.finished_at(), run.address().run_id));
            Ok(completed)
        })
    }

    fn check_fence<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        Box::pin(async move {
            self.mutate_at_state_fence(address, |run, fenced| {
                if fenced {
                    run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)
                } else {
                    run.require_lease(lease)
                }
            })
        })
    }

    fn take_over<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        holder_id: &'a str,
        now: DateTime<Utc>,
        lease_ttl: Duration,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>> {
        Box::pin(async move {
            match self.mutate(address, |run| run.take_over(holder_id, now, lease_ttl)) {
                Ok(run) => Ok(Some(run)),
                Err(DailyCoachingRunStoreError::Fenced | DailyCoachingRunStoreError::NotFound) => {
                    Ok(None)
                }
                Err(error) => Err(error),
            }
        })
    }

    fn heartbeat<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        now: DateTime<Utc>,
        lease_ttl: Duration,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        Box::pin(async move {
            self.mutate_at_state_fence(address, |run, fenced| {
                if fenced {
                    run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)
                } else {
                    run.heartbeat(lease, now, lease_ttl)
                }
            })
        })
    }

    fn digested_sources<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        candidates: &'a [ProfileGameSourceIdentity],
        rebuilding: Option<&'a str>,
    ) -> RunStoreFuture<'a, BTreeSet<ProfileGameSourceIdentity>> {
        Box::pin(async move {
            let persistence = self
                .persistence
                .lock()
                .expect("in-memory Daily Coaching persistence is not poisoned");
            Ok(candidates
                .iter()
                .filter(|identity| {
                    let key = (
                        owner_key.as_str().to_string(),
                        crate::review_durability::path::hashed_path_segment(
                            identity.canonical_key(),
                        ),
                    );
                    match persistence.cards.get(&key) {
                        None => false,
                        Some(card) => rebuilding != Some(card.digest_id.as_str()),
                    }
                })
                .cloned()
                .collect())
        })
    }

    fn update_initial_backfill<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        connection: &'a DailyCoachingRunConnection,
        mutation: InitialBackfillMutation,
    ) -> RunStoreFuture<'a, crate::daily_coaching::DailyCoachingDocument> {
        Box::pin(async move {
            self.state_store.with_document(&address.owner_key, |state| {
                let current_state = state.ok_or(DailyCoachingRunStoreError::Fenced)?;
                let mut next_state = current_state.clone();
                let mut persistence = self
                    .persistence
                    .lock()
                    .expect("in-memory Daily Coaching persistence is not poisoned");
                let mut run = persistence
                    .runs
                    .get(&address.key())
                    .cloned()
                    .ok_or(DailyCoachingRunStoreError::NotFound)?;
                run.validate()?;
                if !next_state.is_enabled() || next_state.run_fence() != run.run_fence() {
                    return Err(DailyCoachingRunStoreError::Fenced);
                }
                run.bind_player(next_state.player_id())?;
                run.require_lease(lease)?;
                next_state
                    .mutate_initial_backfill(
                        run.run_fence(),
                        connection.provider(),
                        connection.identity_username(),
                        mutation,
                    )
                    .map_err(map_state_error)?;
                next_state.validate().map_err(map_state_error)?;
                run.validate()?;
                *current_state = next_state.clone();
                persistence.runs.insert(address.key(), run);
                Ok(next_state)
            })
        })
    }

    fn freeze_selection<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        selection: Vec<SelectedDailyCoachingGame>,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        Box::pin(async move {
            self.mutate_at_state_fence(address, |run, fenced| {
                if fenced {
                    run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)
                } else {
                    run.freeze_selection(lease, selection)
                }
            })
        })
    }

    fn record_game<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        index: usize,
        result: DailyCoachingGameResult,
        now: DateTime<Utc>,
        retry_at: Option<DateTime<Utc>>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        Box::pin(async move {
            self.mutate_at_state_fence(address, |run, fenced| {
                if fenced {
                    run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)
                } else {
                    run.record_game(lease, index, result, now, retry_at)
                }
            })
        })
    }

    fn publish<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        now: DateTime<Utc>,
        retention_days: u32,
        email_delivery_eligible: bool,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        Box::pin(async move {
            self.state_store.with_document(&address.owner_key, |state| {
                let mut persistence = self
                    .persistence
                    .lock()
                    .expect("in-memory Daily Coaching persistence is not poisoned");
                let mut run = persistence
                    .runs
                    .get(&address.key())
                    .cloned()
                    .ok_or(DailyCoachingRunStoreError::NotFound)?;
                run.validate()?;
                // A reopened window is expected to arrive already published.
                let rebuilding = run.regeneration_count() > 0;
                if !rebuilding && run.outcome() == Some(DailyCoachingRunOutcome::Published) {
                    return Ok(run);
                }
                let current_state = state.ok_or(DailyCoachingRunStoreError::Fenced)?;
                let mut next_state = current_state.clone();
                let fenced = !next_state.is_enabled() || next_state.run_fence() != run.run_fence();
                if fenced {
                    run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)?;
                    persistence.runs.insert(address.key(), run.clone());
                    return Ok(run);
                }
                run.bind_player(next_state.player_id())?;
                run.require_lease(lease)?;
                let successful = run.reviewed_games_with_provenance();
                let settled_backfill_sources = run.settled_initial_backfill_sources();
                next_state
                    .reconcile_initial_backfills(&settled_backfill_sources)
                    .map_err(map_state_error)?;
                next_state.validate().map_err(map_state_error)?;
                let reviewed = successful
                    .into_iter()
                    .filter(|game| {
                        let key = (
                            address.owner_key.as_str().to_string(),
                            crate::review_durability::path::hashed_path_segment(
                                game.selected.source_identity.canonical_key(),
                            ),
                        );
                        // A card this same window wrote is ours to replace; any other window's
                        // card still means the Game is already digested elsewhere.
                        match persistence.cards.get(&key) {
                            None => true,
                            Some(card) => card.digest_id == address.run_id,
                        }
                    })
                    .collect::<Vec<_>>();
                if reviewed.is_empty() {
                    run.complete(
                        lease,
                        DailyCoachingRunOutcome::NoDigest,
                        now,
                        retention_days,
                    )?;
                    *current_state = next_state;
                    persistence.runs.insert(address.key(), run.clone());
                    return Ok(run);
                }
                let (digest, cards) = CoachingDigest::daily(
                    address.owner_key.clone(),
                    address.run_id.clone(),
                    run.coverage_date(),
                    now,
                    run.regeneration_count(),
                    email_delivery_eligible,
                    run.timezone().to_string(),
                    &reviewed,
                )
                .map_err(|_| DailyCoachingRunStoreError::InvalidRecord)?;
                let digest_key = (
                    address.owner_key.as_str().to_string(),
                    address.run_id.clone(),
                );
                let superseded = persistence.digests.get(&digest_key);
                if superseded.is_some() && !rebuilding {
                    return Err(DailyCoachingRunStoreError::Conflict);
                }
                // Cards the superseded digest owned but the rebuild no longer selects.
                let orphaned = superseded
                    .map(|digest| {
                        let rebuilt = cards
                            .iter()
                            .map(|card| card.digested_game_key.as_str())
                            .collect::<BTreeSet<_>>();
                        digest
                            .ordered_card_keys
                            .iter()
                            .filter(|key| !rebuilt.contains(key.as_str()))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                run.mark_published(lease, now, retention_days)?;
                run.validate()?;
                *current_state = next_state;
                for key in orphaned {
                    persistence
                        .cards
                        .remove(&(address.owner_key.as_str().to_string(), key));
                }
                for card in cards {
                    persistence.cards.insert(
                        (
                            address.owner_key.as_str().to_string(),
                            card.digested_game_key.clone(),
                        ),
                        card,
                    );
                }
                persistence.digests.insert(digest_key, digest);
                persistence.runs.insert(address.key(), run.clone());
                Ok(run)
            })
        })
    }

    fn complete<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        outcome: DailyCoachingRunOutcome,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        Box::pin(async move {
            self.mutate_at_state_fence(address, |run, fenced| {
                run.complete(
                    lease,
                    if fenced {
                        DailyCoachingRunOutcome::Fenced
                    } else {
                        outcome
                    },
                    now,
                    retention_days,
                )
            })
        })
    }

    fn reopen_for_regeneration<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        holder_id: &'a str,
        now: DateTime<Utc>,
        lease_ttl: Duration,
        deadline: DateTime<Utc>,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        Box::pin(async move {
            self.mutate(address, |run| {
                run.reopen_for_regeneration(holder_id, now, lease_ttl, deadline)
            })
        })
    }

    fn archive<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Vec<CoachingDigest>> {
        Box::pin(async move {
            let persistence = self
                .persistence
                .lock()
                .expect("in-memory Daily Coaching persistence is not poisoned");
            let mut digests = persistence
                .digests
                .iter()
                .filter(|((owner, _), _)| owner == owner_key.as_str())
                .map(|(_, digest)| {
                    digest
                        .validate_summary()
                        .map(|()| digest.clone())
                        .map_err(|_| DailyCoachingRunStoreError::InvalidRecord)
                })
                .collect::<Result<Vec<_>, _>>()?;
            digests.sort_by(|left, right| {
                right
                    .published_at
                    .cmp(&left.published_at)
                    .then_with(|| right.digest_id.cmp(&left.digest_id))
            });
            Ok(digests)
        })
    }

    fn latest_visible<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>> {
        Box::pin(async move {
            let persistence = self
                .persistence
                .lock()
                .expect("in-memory Daily Coaching persistence is not poisoned");
            Ok(persistence
                .runs
                .values()
                .filter(|run| {
                    run.address().owner_key == *owner_key
                        && matches!(
                            run.outcome(),
                            Some(
                                DailyCoachingRunOutcome::Published
                                    | DailyCoachingRunOutcome::NoDigest
                            )
                        )
                })
                .max_by_key(|run| (run.coverage_date(), run.address().run_id))
                .cloned())
        })
    }

    #[cfg(test)]
    fn read<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>> {
        Box::pin(async move {
            Ok(self
                .persistence
                .lock()
                .expect("in-memory Daily Coaching persistence is not poisoned")
                .runs
                .get(&address.key())
                .cloned())
        })
    }

    fn read_digest<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
    ) -> RunStoreFuture<'a, Option<(CoachingDigest, Vec<DigestedGameCard>)>> {
        Box::pin(async move {
            let persistence = self
                .persistence
                .lock()
                .expect("in-memory Daily Coaching persistence is not poisoned");
            let Some(digest) = persistence
                .digests
                .get(&(owner_key.as_str().to_string(), digest_id.to_string()))
                .cloned()
            else {
                return Ok(None);
            };
            if digest.digest_id != digest_id || &digest.owner_key != owner_key {
                return Err(DailyCoachingRunStoreError::InvalidRecord);
            }
            let cards = digest
                .ordered_card_keys
                .iter()
                .map(|key| {
                    persistence
                        .cards
                        .get(&(owner_key.as_str().to_string(), key.clone()))
                        .cloned()
                        .ok_or(DailyCoachingRunStoreError::InvalidRecord)
                })
                .collect::<Result<Vec<_>, _>>()?;
            digest
                .validate(&cards)
                .map_err(|_| DailyCoachingRunStoreError::InvalidRecord)?;
            Ok(Some((digest, cards)))
        })
    }
}
