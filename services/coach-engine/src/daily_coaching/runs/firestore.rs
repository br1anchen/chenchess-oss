use std::{collections::BTreeSet, time::Duration};

use chrono::{DateTime, Utc};

use crate::firestore::{FirestoreDatabase, FirestoreError};
use crate::{
    profile_game_feed::ProfileGameSourceIdentity, review_durability::path::hashed_path_segment,
};

use super::super::selection::SelectedDailyCoachingGame;
use super::super::{
    digest::{CoachingDigest, DigestedGameCard},
    firestore::FirestoreDailyCoachingStore,
    state::InitialBackfillMutation,
    DailyCoachingDocument,
};
use super::{
    map_state_error, DailyCoachingGameResult, DailyCoachingOwnerKey, DailyCoachingRunAddress,
    DailyCoachingRunClaim, DailyCoachingRunConnection, DailyCoachingRunDocument,
    DailyCoachingRunLease, DailyCoachingRunOutcome, DailyCoachingRunStore,
    DailyCoachingRunStoreError, RunStoreFuture,
};

pub(crate) const RUNS_COLLECTION: &str = "dailyCoachingRuns";
const DIGESTS_COLLECTION: &str = "coachingDigests";
const DIGESTED_GAMES_COLLECTION: &str = "digestedGames";
const ACTIVE_STATUS: &str = "active";
const COMPLETED_STATUS: &str = "completed";
const EXPIRED_BATCH_SIZE: u16 = 100;
const MAX_TRANSACTION_ATTEMPTS: usize = 4;

mod publication;

pub(crate) struct FirestoreDailyCoachingRunStore {
    database: FirestoreDatabase,
}

impl FirestoreDailyCoachingRunStore {
    pub(crate) fn new(database: FirestoreDatabase) -> Self {
        Self { database }
    }

    fn collection_path(address: &DailyCoachingRunAddress) -> [String; 3] {
        [
            "users".to_string(),
            address.owner_key.as_str().to_string(),
            RUNS_COLLECTION.to_string(),
        ]
    }

    fn document_path(address: &DailyCoachingRunAddress) -> [String; 4] {
        let [users, player, runs] = Self::collection_path(address);
        [users, player, runs, address.run_id.clone()]
    }

    fn digest_path(owner_key: &DailyCoachingOwnerKey, digest_id: &str) -> [String; 4] {
        [
            "users".to_string(),
            owner_key.as_str().to_string(),
            DIGESTS_COLLECTION.to_string(),
            digest_id.to_string(),
        ]
    }

    fn card_path(
        owner_key: &DailyCoachingOwnerKey,
        identity: &ProfileGameSourceIdentity,
    ) -> [String; 4] {
        [
            "users".to_string(),
            owner_key.as_str().to_string(),
            DIGESTED_GAMES_COLLECTION.to_string(),
            hashed_path_segment(identity.canonical_key()),
        ]
    }

    /// Card path for a `digestedGameKey` already hashed by a published Coaching Digest.
    fn card_path_from_key(
        owner_key: &DailyCoachingOwnerKey,
        digested_game_key: &str,
    ) -> [String; 4] {
        [
            "users".to_string(),
            owner_key.as_str().to_string(),
            DIGESTED_GAMES_COLLECTION.to_string(),
            digested_game_key.to_string(),
        ]
    }

    async fn mutate(
        &self,
        address: &DailyCoachingRunAddress,
        mutation: impl Fn(&mut DailyCoachingRunDocument) -> Result<(), DailyCoachingRunStoreError>,
    ) -> Result<DailyCoachingRunDocument, DailyCoachingRunStoreError> {
        let owned_path = Self::document_path(address);
        let path = owned_path.iter().map(String::as_str).collect::<Vec<_>>();
        for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
            let transaction = self.database.begin_transaction().await?;
            let prepared = async {
                let Some(mut run) = self
                    .database
                    .get_document_in_transaction::<DailyCoachingRunDocument>(&path, &transaction)
                    .await?
                else {
                    return Err(DailyCoachingRunStoreError::NotFound);
                };
                run.validate()?;
                validate_path(&path, &run)?;
                mutation(&mut run)?;
                run.validate()?;
                let write = self
                    .database
                    .update_write(&path, &run, &run_query_timestamps(&run))?;
                Ok::<_, DailyCoachingRunStoreError>((run, write))
            };
            let (run, write) = match prepared.await {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.database.rollback_transaction(transaction).await?;
                    return Err(error);
                }
            };
            match self
                .database
                .commit_transaction(transaction, vec![write])
                .await
            {
                Ok(()) => return Ok(run),
                Err(FirestoreError::Conflict) if attempt + 1 < MAX_TRANSACTION_ATTEMPTS => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(DailyCoachingRunStoreError::Conflict)
    }

    async fn mutate_at_state_fence(
        &self,
        address: &DailyCoachingRunAddress,
        mutation: impl Fn(
            &mut DailyCoachingRunDocument,
            bool,
        ) -> Result<bool, DailyCoachingRunStoreError>,
    ) -> Result<DailyCoachingRunDocument, DailyCoachingRunStoreError> {
        let owned_path = Self::document_path(address);
        let path = owned_path.iter().map(String::as_str).collect::<Vec<_>>();
        let owned_state_path = FirestoreDailyCoachingStore::document_path(&address.owner_key);
        let state_path = owned_state_path
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
            let transaction = self.database.begin_transaction().await?;
            let prepared = async {
                let Some(mut run) = self
                    .database
                    .get_document_in_transaction::<DailyCoachingRunDocument>(&path, &transaction)
                    .await?
                else {
                    return Err(DailyCoachingRunStoreError::NotFound);
                };
                let state = self
                    .database
                    .get_document_in_transaction::<DailyCoachingDocument>(&state_path, &transaction)
                    .await?;
                let state = match state {
                    Some(state) => {
                        state
                            .validate_for(&address.owner_key)
                            .map_err(map_state_error)?;
                        state
                    }
                    None => DailyCoachingDocument::empty(address.owner_key.clone()),
                };
                run.validate()?;
                validate_path(&path, &run)?;
                let fenced = !state.is_enabled() || state.run_fence() != run.run_fence();
                let identity_bound = if fenced {
                    false
                } else {
                    run.bind_player(state.player_id())?
                };
                let mutated = mutation(&mut run, fenced)?;
                let changed = identity_bound || mutated;
                run.validate()?;
                let write = if changed {
                    Some(
                        self.database
                            .update_write(&path, &run, &run_query_timestamps(&run))?,
                    )
                } else {
                    None
                };
                Ok::<_, DailyCoachingRunStoreError>((run, write))
            };
            let (run, write) = match prepared.await {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.database.rollback_transaction(transaction).await?;
                    return Err(error);
                }
            };
            let Some(write) = write else {
                self.database.rollback_transaction(transaction).await?;
                return Ok(run);
            };
            match self
                .database
                .commit_transaction(transaction, vec![write])
                .await
            {
                Ok(()) => return Ok(run),
                Err(FirestoreError::Conflict) if attempt + 1 < MAX_TRANSACTION_ATTEMPTS => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(DailyCoachingRunStoreError::Conflict)
    }
}

pub(super) fn run_query_timestamps(
    run: &DailyCoachingRunDocument,
) -> Vec<(&'static str, DateTime<Utc>)> {
    let mut timestamps = vec![
        ("nextAttemptAt", run.next_attempt_at()),
        ("purgeAt", run.purge_at),
    ];
    if let Some(finished_at) = run.finished_at() {
        timestamps.push(("finishedAt", finished_at));
    }
    timestamps
}

impl DailyCoachingRunStore for FirestoreDailyCoachingRunStore {
    fn list_digested_game_cards<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Vec<DigestedGameCard>> {
        Box::pin(async move {
            let owned_collection_path = [
                "users".to_string(),
                owner_key.as_str().to_string(),
                DIGESTED_GAMES_COLLECTION.to_string(),
            ];
            let collection_path = owned_collection_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            Ok(self
                .database
                .list_valid_documents::<DigestedGameCard>(&collection_path)
                .await?
                .into_iter()
                .filter_map(|(document_id, card)| {
                    (document_id == card.digested_game_key && card.validate().is_ok())
                        .then_some(card)
                })
                .collect())
        })
    }

    fn create<'a>(
        &'a self,
        run: DailyCoachingRunDocument,
    ) -> RunStoreFuture<'a, DailyCoachingRunClaim> {
        Box::pin(async move {
            run.validate()?;
            let address = run.address();
            let owned_path = Self::collection_path(&address);
            let path = owned_path.iter().map(String::as_str).collect::<Vec<_>>();
            match self
                .database
                .create_document(&path, &address.run_id, &run, &run_query_timestamps(&run))
                .await
            {
                Ok(()) => Ok(DailyCoachingRunClaim::Created(Box::new(run))),
                Err(FirestoreError::Conflict) => Ok(DailyCoachingRunClaim::Existing),
                Err(error) => Err(error.into()),
            }
        })
    }

    fn expired<'a>(
        &'a self,
        now: DateTime<Utc>,
    ) -> RunStoreFuture<'a, Vec<DailyCoachingRunDocument>> {
        Box::pin(async move {
            self.database
                .query_due_collection_group::<DailyCoachingRunDocument>(
                    RUNS_COLLECTION,
                    ACTIVE_STATUS,
                    now,
                    EXPIRED_BATCH_SIZE,
                )
                .await?
                .into_iter()
                .map(|stored| {
                    stored.value.validate()?;
                    let path = stored.path.iter().map(String::as_str).collect::<Vec<_>>();
                    validate_path(&path, &stored.value)?;
                    Ok(stored.value)
                })
                .collect()
        })
    }

    fn finished_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> RunStoreFuture<'a, Vec<DailyCoachingRunDocument>> {
        Box::pin(async move {
            self.database
                .query_collection_group_timestamp_range::<DailyCoachingRunDocument>(
                    RUNS_COLLECTION,
                    COMPLETED_STATUS,
                    "finishedAt",
                    starts_at,
                    ends_at,
                )
                .await?
                .into_iter()
                .map(|stored| {
                    stored.value.validate()?;
                    let path = stored.path.iter().map(String::as_str).collect::<Vec<_>>();
                    validate_path(&path, &stored.value)?;
                    Ok(stored.value)
                })
                .collect()
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
                    run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)?;
                    Ok(true)
                } else {
                    run.require_lease(lease)?;
                    Ok(false)
                }
            })
            .await
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
            match self
                .mutate(address, |run| {
                    if !run.is_expired_at(now) {
                        return Err(DailyCoachingRunStoreError::Fenced);
                    }
                    run.take_over(holder_id, now, lease_ttl)
                })
                .await
            {
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
                    run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)?;
                } else {
                    run.heartbeat(lease, now, lease_ttl)?;
                }
                Ok(true)
            })
            .await
        })
    }

    fn digested_sources<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        candidates: &'a [ProfileGameSourceIdentity],
        rebuilding: Option<&'a str>,
    ) -> RunStoreFuture<'a, BTreeSet<ProfileGameSourceIdentity>> {
        Box::pin(async move {
            let mut digested = BTreeSet::new();
            for identity in candidates {
                let owned_path = Self::card_path(owner_key, identity);
                let path = owned_path.iter().map(String::as_str).collect::<Vec<_>>();
                if let Some(card) = self
                    .database
                    .get_document::<DigestedGameCard>(&path)
                    .await?
                {
                    card.validate()
                        .map_err(|_| DailyCoachingRunStoreError::InvalidRecord)?;
                    if &card.source_identity != identity {
                        return Err(DailyCoachingRunStoreError::InvalidRecord);
                    }
                    // A Game the digest under rebuild carries stays selectable for that rebuild.
                    if rebuilding != Some(card.digest_id.as_str()) {
                        digested.insert(identity.clone());
                    }
                }
            }
            Ok(digested)
        })
    }

    fn update_initial_backfill<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        connection: &'a DailyCoachingRunConnection,
        mutation: InitialBackfillMutation,
    ) -> RunStoreFuture<'a, DailyCoachingDocument> {
        Box::pin(async move {
            let owned_run_path = Self::document_path(address);
            let run_path = owned_run_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let owned_state_path = FirestoreDailyCoachingStore::document_path(&address.owner_key);
            let state_path = owned_state_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                let prepared = async {
                    let Some(mut run) = self
                        .database
                        .get_document_in_transaction::<DailyCoachingRunDocument>(
                            &run_path,
                            &transaction,
                        )
                        .await?
                    else {
                        return Err(DailyCoachingRunStoreError::NotFound);
                    };
                    let Some(mut state) = self
                        .database
                        .get_document_in_transaction::<DailyCoachingDocument>(
                            &state_path,
                            &transaction,
                        )
                        .await?
                    else {
                        return Err(DailyCoachingRunStoreError::Fenced);
                    };
                    state
                        .validate_for(&address.owner_key)
                        .map_err(map_state_error)?;
                    let original_state = state.clone();
                    run.validate()?;
                    validate_path(&run_path, &run)?;
                    if !state.is_enabled() || state.run_fence() != run.run_fence() {
                        return Err(DailyCoachingRunStoreError::Fenced);
                    }
                    let identity_bound = run.bind_player(state.player_id())?;
                    run.require_lease(lease)?;
                    state
                        .mutate_initial_backfill(
                            run.run_fence(),
                            connection.provider(),
                            connection.identity_username(),
                            mutation.clone(),
                        )
                        .map_err(map_state_error)?;
                    state.validate().map_err(map_state_error)?;
                    run.validate()?;
                    let mut writes = Vec::with_capacity(2);
                    if state != original_state {
                        writes.push(self.database.update_write(&state_path, &state, &[])?);
                    }
                    if identity_bound {
                        writes.push(self.database.update_write(
                            &run_path,
                            &run,
                            &run_query_timestamps(&run),
                        )?);
                    }
                    Ok::<_, DailyCoachingRunStoreError>((state, writes))
                }
                .await;
                let (state, writes) = match prepared {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        self.database.rollback_transaction(transaction).await?;
                        return Err(error);
                    }
                };
                if writes.is_empty() {
                    self.database.rollback_transaction(transaction).await?;
                    return Ok(state);
                }
                match self.database.commit_transaction(transaction, writes).await {
                    Ok(()) => return Ok(state),
                    Err(FirestoreError::Conflict) if attempt + 1 < MAX_TRANSACTION_ATTEMPTS => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(DailyCoachingRunStoreError::Conflict)
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
                    run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)?;
                } else {
                    run.freeze_selection(lease, selection.clone())?;
                }
                Ok(true)
            })
            .await
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
                    run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)?;
                } else {
                    run.record_game(lease, index, result.clone(), now, retry_at)?;
                }
                Ok(true)
            })
            .await
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
        Box::pin(self.publish_window(address, lease, now, retention_days, email_delivery_eligible))
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
            .await
        })
    }

    fn archive<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Vec<CoachingDigest>> {
        Box::pin(async move {
            let owned_collection_path = [
                "users".to_string(),
                owner_key.as_str().to_string(),
                DIGESTS_COLLECTION.to_string(),
            ];
            let collection_path = owned_collection_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let mut digests = self
                .database
                .list_valid_documents::<CoachingDigest>(&collection_path)
                .await?
                .into_iter()
                .filter_map(|(document_id, digest)| {
                    (document_id == digest.digest_id
                        && &digest.owner_key == owner_key
                        && digest.validate_summary().is_ok())
                    .then_some(digest)
                })
                .collect::<Vec<_>>();
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
            let owned_collection_path = [
                "users".to_string(),
                owner_key.as_str().to_string(),
                RUNS_COLLECTION.to_string(),
            ];
            let collection_path = owned_collection_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let mut latest = None;
            for (document_id, run) in self
                .database
                .list_documents::<DailyCoachingRunDocument>(&collection_path)
                .await?
            {
                if document_id != run.address().run_id
                    || run.address().owner_key != *owner_key
                    || run.validate().is_err()
                {
                    return Err(DailyCoachingRunStoreError::InvalidRecord);
                }
                if !matches!(
                    run.outcome(),
                    Some(DailyCoachingRunOutcome::Published | DailyCoachingRunOutcome::NoDigest)
                ) {
                    continue;
                }
                if latest
                    .as_ref()
                    .is_none_or(|current: &DailyCoachingRunDocument| {
                        (run.coverage_date(), run.address().run_id)
                            > (current.coverage_date(), current.address().run_id)
                    })
                {
                    latest = Some(run);
                }
            }
            Ok(latest)
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
                )?;
                Ok(true)
            })
            .await
        })
    }

    #[cfg(test)]
    fn read<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>> {
        Box::pin(async move {
            let owned_path = Self::document_path(address);
            let path = owned_path.iter().map(String::as_str).collect::<Vec<_>>();
            let Some(run) = self
                .database
                .get_document::<DailyCoachingRunDocument>(&path)
                .await?
            else {
                return Ok(None);
            };
            run.validate()?;
            validate_path(&path, &run)?;
            Ok(Some(run))
        })
    }

    fn read_digest<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
    ) -> RunStoreFuture<'a, Option<(CoachingDigest, Vec<DigestedGameCard>)>> {
        Box::pin(async move {
            let owned_digest_path = Self::digest_path(owner_key, digest_id);
            let digest_path = owned_digest_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let Some(digest) = self
                .database
                .get_document::<CoachingDigest>(&digest_path)
                .await?
            else {
                return Ok(None);
            };
            if digest.digest_id != digest_id || &digest.owner_key != owner_key {
                return Err(DailyCoachingRunStoreError::InvalidRecord);
            }
            let mut cards = Vec::with_capacity(digest.ordered_card_keys.len());
            for key in &digest.ordered_card_keys {
                let owned_path = [
                    "users".to_string(),
                    owner_key.as_str().to_string(),
                    DIGESTED_GAMES_COLLECTION.to_string(),
                    key.clone(),
                ];
                let path = owned_path.iter().map(String::as_str).collect::<Vec<_>>();
                let card = self
                    .database
                    .get_document::<DigestedGameCard>(&path)
                    .await?
                    .ok_or(DailyCoachingRunStoreError::InvalidRecord)?;
                cards.push(card);
            }
            digest
                .validate(&cards)
                .map_err(|_| DailyCoachingRunStoreError::InvalidRecord)?;
            Ok(Some((digest, cards)))
        })
    }
}

fn validate_path(
    path: &[&str],
    run: &DailyCoachingRunDocument,
) -> Result<(), DailyCoachingRunStoreError> {
    let expected = FirestoreDailyCoachingRunStore::document_path(&run.address());
    if path == expected.iter().map(String::as_str).collect::<Vec<_>>() {
        Ok(())
    } else {
        Err(DailyCoachingRunStoreError::InvalidRecord)
    }
}

impl From<FirestoreError> for DailyCoachingRunStoreError {
    fn from(error: FirestoreError) -> Self {
        match error {
            FirestoreError::Configuration(_) => Self::Configuration,
            FirestoreError::Transport => Self::Transport,
            FirestoreError::Unavailable => Self::Unavailable,
            FirestoreError::Conflict => Self::Conflict,
            FirestoreError::InvalidDocument => Self::InvalidRecord,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        daily_coaching::DailyCoachingOwnerKey, review_durability::path::hashed_path_segment,
        review_session_contract::PlayerId,
    };

    #[test]
    fn run_is_stored_under_the_hashed_player_subtree() {
        let address = DailyCoachingRunAddress {
            owner_key: DailyCoachingOwnerKey::for_player(
                &PlayerId::try_from("firebase-player".to_string()).unwrap(),
            ),
            run_id: "daily-2026-08-09".to_string(),
        };

        assert_eq!(
            FirestoreDailyCoachingRunStore::document_path(&address).join("/"),
            format!(
                "users/{}/dailyCoachingRuns/daily-2026-08-09",
                hashed_path_segment("firebase-player")
            )
        );
    }
}

#[cfg(test)]
#[path = "firestore_tests.rs"]
mod firestore_tests;
