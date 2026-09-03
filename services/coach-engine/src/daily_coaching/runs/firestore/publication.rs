//! The transaction that publishes one Run window: its Coaching Digest, the Digested Game
//! cards the Digest names, the Daily Coaching state document, and the Run document itself.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use crate::daily_coaching::{
    digest::{CoachingDigest, DigestedGameCard},
    firestore::FirestoreDailyCoachingStore,
    runs::{
        map_state_error, DailyCoachingRunAddress, DailyCoachingRunDocument, DailyCoachingRunLease,
        DailyCoachingRunOutcome, DailyCoachingRunStoreError,
    },
    DailyCoachingDocument,
};
use crate::firestore::{FirestoreError, FirestoreTransaction, FirestoreWrite};

use super::{
    run_query_timestamps, validate_path, FirestoreDailyCoachingRunStore, MAX_TRANSACTION_ATTEMPTS,
};

enum PublicationPreparation {
    AlreadyPublished(DailyCoachingRunDocument),
    Commit {
        run: DailyCoachingRunDocument,
        writes: Vec<FirestoreWrite>,
    },
}

impl FirestoreDailyCoachingRunStore {
    pub(super) async fn publish_window(
        &self,
        address: &DailyCoachingRunAddress,
        lease: &DailyCoachingRunLease,
        now: DateTime<Utc>,
        retention_days: u32,
        email_delivery_eligible: bool,
    ) -> Result<DailyCoachingRunDocument, DailyCoachingRunStoreError> {
        for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
            let transaction = self.database.begin_transaction().await?;
            let prepared = self
                .prepare_publication(
                    address,
                    lease,
                    &transaction,
                    now,
                    retention_days,
                    email_delivery_eligible,
                )
                .await;
            let (run, writes) = match prepared {
                Ok(PublicationPreparation::AlreadyPublished(run)) => {
                    self.database.rollback_transaction(transaction).await?;
                    return Ok(run);
                }
                Ok(PublicationPreparation::Commit { run, writes }) => (run, writes),
                Err(error) => {
                    self.database.rollback_transaction(transaction).await?;
                    return Err(error);
                }
            };
            match self.database.commit_transaction(transaction, writes).await {
                Ok(()) => return Ok(run),
                Err(FirestoreError::Conflict) if attempt + 1 < MAX_TRANSACTION_ATTEMPTS => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(DailyCoachingRunStoreError::Conflict)
    }

    async fn prepare_publication(
        &self,
        address: &DailyCoachingRunAddress,
        lease: &DailyCoachingRunLease,
        transaction: &FirestoreTransaction,
        now: DateTime<Utc>,
        retention_days: u32,
        email_delivery_eligible: bool,
    ) -> Result<PublicationPreparation, DailyCoachingRunStoreError> {
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
        let Some(mut run) = self
            .database
            .get_document_in_transaction::<DailyCoachingRunDocument>(&run_path, transaction)
            .await?
        else {
            return Err(DailyCoachingRunStoreError::NotFound);
        };
        run.validate()?;
        validate_path(&run_path, &run)?;
        // A reopened window is expected to arrive already published.
        let rebuilding = run.regeneration_count() > 0;
        if !rebuilding && run.outcome() == Some(DailyCoachingRunOutcome::Published) {
            return Ok(PublicationPreparation::AlreadyPublished(run));
        }
        let mut state = match self
            .database
            .get_document_in_transaction::<DailyCoachingDocument>(&state_path, transaction)
            .await?
        {
            Some(state) => {
                state
                    .validate_for(&address.owner_key)
                    .map_err(map_state_error)?;
                state
            }
            None => DailyCoachingDocument::empty(address.owner_key.clone()),
        };
        let original_state = state.clone();
        let fenced = !state.is_enabled() || state.run_fence() != run.run_fence();
        if fenced {
            run.complete(lease, DailyCoachingRunOutcome::Fenced, now, retention_days)?;
            let write = self
                .database
                .update_write(&run_path, &run, &run_query_timestamps(&run))?;
            return Ok(PublicationPreparation::Commit {
                run,
                writes: vec![write],
            });
        }
        run.bind_player(state.player_id())?;
        run.require_lease(lease)?;
        let successful = run.reviewed_games_with_provenance();
        let settled_backfill_sources = run.settled_initial_backfill_sources();
        state
            .reconcile_initial_backfills(&settled_backfill_sources)
            .map_err(map_state_error)?;
        state.validate().map_err(map_state_error)?;
        let state_changed = state != original_state;
        let mut reviewed = Vec::new();
        let mut replacing = BTreeSet::new();
        for candidate in successful {
            let owned_card_path =
                Self::card_path(&address.owner_key, &candidate.selected.source_identity);
            let card_path = owned_card_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let existing = self
                .database
                .get_document_in_transaction::<DigestedGameCard>(&card_path, transaction)
                .await?;
            if let Some(card) = existing {
                card.validate()
                    .map_err(|_| DailyCoachingRunStoreError::InvalidRecord)?;
                if card.source_identity != candidate.selected.source_identity {
                    return Err(DailyCoachingRunStoreError::InvalidRecord);
                }
                // A card this same window wrote is ours to replace; any other
                // window's card still means the Game is digested elsewhere.
                if card.digest_id == address.run_id {
                    replacing.insert(candidate.selected.source_identity.clone());
                    reviewed.push(candidate);
                }
            } else {
                reviewed.push(candidate);
            }
        }
        if reviewed.is_empty() {
            run.complete(
                lease,
                DailyCoachingRunOutcome::NoDigest,
                now,
                retention_days,
            )?;
            let mut writes = Vec::with_capacity(2);
            if state_changed {
                writes.push(self.database.update_write(&state_path, &state, &[])?);
            }
            writes.push(self.database.update_write(
                &run_path,
                &run,
                &run_query_timestamps(&run),
            )?);
            return Ok(PublicationPreparation::Commit { run, writes });
        }
        let owned_digest_path = Self::digest_path(&address.owner_key, &address.run_id);
        let digest_path = owned_digest_path
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let superseded = self
            .database
            .get_document_in_transaction::<CoachingDigest>(&digest_path, transaction)
            .await?;
        if superseded.is_some() && !rebuilding {
            return Err(DailyCoachingRunStoreError::InvalidRecord);
        }
        // This digest's card keys become delete paths below, so it earns the same
        // validation as every other document this transaction reads.
        if let Some(superseded) = superseded.as_ref() {
            superseded
                .validate_summary()
                .map_err(|_| DailyCoachingRunStoreError::InvalidRecord)?;
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
        run.mark_published(lease, now, retention_days)?;
        let rebuilt = cards
            .iter()
            .map(|card| card.digested_game_key.as_str())
            .collect::<BTreeSet<_>>();
        let orphaned = superseded
            .as_ref()
            .map(|digest| {
                digest
                    .ordered_card_keys
                    .iter()
                    .filter(|key| !rebuilt.contains(key.as_str()))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut writes = Vec::with_capacity(cards.len() + orphaned.len() + 3);
        writes.push(if superseded.is_some() {
            self.database.update_write(
                &digest_path,
                &digest,
                &[("publishedAt", digest.published_at)],
            )?
        } else {
            self.database.create_write(
                &digest_path,
                &digest,
                &[("publishedAt", digest.published_at)],
            )?
        });
        for card in &cards {
            let owned_card_path = Self::card_path(&address.owner_key, &card.source_identity);
            let card_path = owned_card_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            // `create_write` carries the exists=false precondition that keeps two
            // overlapping Runs from both writing one Game's card. Only a card this
            // window already owns may be updated in place.
            writes.push(if replacing.contains(&card.source_identity) {
                self.database.update_write(
                    &card_path,
                    card,
                    &[("digestedAt", card.digested_at), ("endedAt", card.ended_at)],
                )?
            } else {
                self.database.create_write(
                    &card_path,
                    card,
                    &[("digestedAt", card.digested_at), ("endedAt", card.ended_at)],
                )?
            });
        }
        // Games the superseded digest carried and the rebuild dropped.
        for key in &orphaned {
            let owned_card_path = Self::card_path_from_key(&address.owner_key, key);
            let card_path = owned_card_path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            writes.push(self.database.delete_write(&card_path)?);
        }
        if state_changed {
            writes.push(self.database.update_write(&state_path, &state, &[])?);
        }
        writes.push(
            self.database
                .update_write(&run_path, &run, &run_query_timestamps(&run))?,
        );
        if writes.len() > 23 {
            return Err(DailyCoachingRunStoreError::InvalidRecord);
        }
        Ok(PublicationPreparation::Commit { run, writes })
    }
}
