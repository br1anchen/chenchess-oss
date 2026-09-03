use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};

use crate::{
    firestore::{FirestoreDatabase, FirestoreError},
    game_import_store::{GameImportStore, GameImportStoreError, ReviewSessionGame},
    quality_capture::QualityCaptureAppender,
    review_annotation_store::{
        annotation_create_write, read_annotation_document, ReviewAnnotationAddress,
        ReviewAnnotationStoreError, ReviewMomentAnnotation,
    },
    review_session_contract::GameImportId,
};

use super::{
    entry::{
        ReviewAnalysisCacheError, ReviewAnalysisCacheFuture, ReviewAnalysisCacheStore,
        ReviewAnalysisEntries, ReviewAnalysisEntry, ReviewAnalysisMutation,
    },
    moment_document_id, moment_document_path, moments_collection_path, InvalidEntryReason,
    ReviewAnalysisMomentDocument, ReviewKey,
};

pub(super) fn encoded_moment_bytes(
    moment: &ReviewAnalysisEntry,
    game: &ReviewSessionGame,
) -> Result<usize, ReviewAnalysisCacheError> {
    serialized_bytes(&ReviewAnalysisMomentDocument::from_moment(moment, game)?)
}

#[cfg(test)]
pub(super) fn encoded_moment_payload(
    moment: &ReviewAnalysisEntry,
    game: &ReviewSessionGame,
) -> Result<String, ReviewAnalysisCacheError> {
    ReviewAnalysisMomentDocument::from_moment(moment, game)?.into_payload_json()
}

/// The cache address a Game Import ID names.
fn cache_address(game_import_id: &GameImportId) -> Result<ReviewKey, ReviewAnalysisCacheError> {
    ReviewKey::from_game_import_id(game_import_id).ok_or(ReviewAnalysisCacheError::InvalidEntry(
        InvalidEntryReason::GameImport,
    ))
}

pub(crate) fn review_analysis_cache_store(
    database: FirestoreDatabase,
    game_imports: Arc<dyn GameImportStore>,
    quality_capture: QualityCaptureAppender,
) -> Arc<dyn ReviewAnalysisCacheStore> {
    Arc::new(FirestoreReviewAnalysisCache {
        database,
        game_imports,
        quality_capture,
    })
}

struct FirestoreReviewAnalysisCache {
    database: FirestoreDatabase,
    #[allow(dead_code)]
    game_imports: Arc<dyn GameImportStore>,
    quality_capture: QualityCaptureAppender,
}

impl ReviewAnalysisCacheStore for FirestoreReviewAnalysisCache {
    /// Writes this review's analysis to the shared cache, first-writer-wins.
    ///
    /// The batched attempt is one commit; a conflict means someone else got
    /// there first, and only then does it fall back to per-entry commits to find
    /// out which ones. Each of those swallows its own conflict, so the common
    /// contended case — a whole review already cached — costs one extra commit
    /// per Review Moment and never fails.
    fn seed<'a>(&'a self, entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            let started_at = Instant::now();
            let review_key = cache_address(&entries.game_import_id)?;
            let mut documents = Vec::with_capacity(entries.entries.len());
            for entry in &entries.entries {
                documents.push((
                    moment_document_id(&entry.moment_id),
                    ReviewAnalysisMomentDocument::from_moment(entry, &entries.game)?,
                    entry.purge_at,
                ));
            }
            if documents.is_empty() {
                return Ok(());
            }
            let entry_write = |entry: &(String, ReviewAnalysisMomentDocument, DateTime<Utc>)| {
                self.database.create_write(
                    &moment_document_path(&review_key, &entry.0),
                    &entry.1,
                    &[("purgeAt", entry.2)],
                )
            };
            let batched = documents
                .iter()
                .map(entry_write)
                .collect::<Result<Vec<_>, _>>()?;
            let mut written = batched.len();
            match self.database.commit(batched).await {
                Ok(()) => {}
                Err(FirestoreError::Conflict) => {
                    written = 0;
                    for entry in &documents {
                        match self.database.commit(vec![entry_write(entry)?]).await {
                            Ok(()) => written += 1,
                            Err(FirestoreError::Conflict) => {}
                            Err(error) => return Err(error.into()),
                        }
                    }
                }
                Err(error) => return Err(error.into()),
            }
            tracing::info!(
                event = "review_analysis_cache_seed_completion",
                firestore_operation = "review_analysis_cache_seed",
                write_count = written,
                entry_count = documents.len(),
                evidence_entry_count = evidence_entry_count(&entries.entries),
                wall_milliseconds = started_at.elapsed().as_millis(),
                "review-analysis cache persistence metrics"
            );
            Ok(())
        })
    }

    fn load<'a>(
        &'a self,
        game_import_id: &'a GameImportId,
        game: &'a ReviewSessionGame,
        now: DateTime<Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>> {
        Box::pin(async move {
            let started_at = Instant::now();
            let review_key = cache_address(game_import_id)?;
            let documents = self
                .database
                .list_documents::<ReviewAnalysisMomentDocument>(&moments_collection_path(
                    &review_key,
                ))
                .await?;
            let read_document_count = documents.len();
            let read_bytes = serialized_bytes(&documents)?;
            // Firestore TTL deletion is best-effort and can lag its `purgeAt` by
            // days, so retention is enforced on read as well: an entry past its
            // purge time is a miss, not stale analysis served to a Player.
            let mut entries = Vec::with_capacity(documents.len());
            for (document_id, document) in documents {
                if now >= document.purge_at {
                    continue;
                }
                match decode_entry(document_id, document, game_import_id, game) {
                    Ok(entry) => entries.push(entry),
                    // A single unreadable entry costs its Review Moment its
                    // cached analysis, not the whole review: the rest is still
                    // worth far more to a Player than nothing at all.
                    Err(error) => tracing::warn!(
                        firestore_operation = "review_analysis_cache_load",
                        category = error.diagnostic_category(),
                        reason = error.diagnostic_reason(),
                        "a cached Review Moment was skipped"
                    ),
                }
            }
            entries.sort_by_key(|entry| entry.core.review_moment.ply);
            tracing::info!(
                event = "review_analysis_cache_load_completion",
                firestore_operation = "review_analysis_cache_load",
                read_document_count,
                read_bytes,
                entry_count = entries.len(),
                wall_milliseconds = started_at.elapsed().as_millis(),
                "review-analysis cache persistence metrics"
            );
            Ok(entries)
        })
    }

    /// Upgrades one entry, unconditionally.
    ///
    /// This is the only write that may displace a stored entry: a prepared
    /// Review Moment carries analysis the stored one does not, whoever wrote it.
    /// Nothing is read first — the entry is addressed, and the write is the
    /// whole mutation.
    fn replace_moment<'a>(
        &'a self,
        mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            let started_at = Instant::now();
            let review_key = cache_address(&mutation.game_import_id)?;
            let moment_document_id = moment_document_id(mutation.moment_id());
            let document =
                ReviewAnalysisMomentDocument::from_moment(&mutation.entry, &mutation.game)?;
            let entry_write = || {
                self.database.upsert_write(
                    &moment_document_path(&review_key, &moment_document_id),
                    &document,
                    &[("purgeAt", mutation.entry.purge_at)],
                )
            };
            let mut writes = vec![entry_write()?];
            let quality_writes = self
                .quality_capture
                .prepare_firestore_writes(&mutation.owner, &mutation.quality_captures)
                .await;
            let capture_was_gated = !quality_writes.is_empty();
            writes.extend(quality_writes);
            let write_count = writes.len();
            let mutation_bytes = serialized_bytes(&mutation.entry)?;
            let mut status = "succeeded";
            match self.database.commit(writes).await {
                Ok(()) => {}
                // Only the quality-capture writes carry preconditions, so a
                // conflict here is the outbox losing a race, never the entry.
                // The Player's analysis still lands; the capture is dropped.
                Err(FirestoreError::Conflict) if capture_was_gated => {
                    status = "capture-dropped";
                    self.database.commit(vec![entry_write()?]).await?;
                }
                Err(error) => return Err(error.into()),
            }
            tracing::info!(
                event = "review_analysis_cache_replace_completion",
                firestore_operation = "review_analysis_cache_replace_moment",
                write_count,
                mutation_bytes,
                wall_milliseconds = started_at.elapsed().as_millis(),
                status,
                "review-analysis cache persistence metrics"
            );
            Ok(())
        })
    }
}

/// First-open persist: annotation create + cache entry + quality outbox in one commit.
pub(crate) struct FirestoreFirstOpenPublication {
    database: FirestoreDatabase,
    quality_capture: QualityCaptureAppender,
}

impl FirestoreFirstOpenPublication {
    pub(crate) fn new(
        database: FirestoreDatabase,
        quality_capture: QualityCaptureAppender,
    ) -> Self {
        Self {
            database,
            quality_capture,
        }
    }

    pub(crate) async fn persist(
        &self,
        address: &ReviewAnnotationAddress,
        annotation: ReviewMomentAnnotation,
        mutation: ReviewAnalysisMutation,
    ) -> Result<ReviewMomentAnnotation, ReviewAnalysisCacheError> {
        let review_key = cache_address(&mutation.game_import_id)?;
        let moment_id = moment_document_id(mutation.moment_id());
        let document = ReviewAnalysisMomentDocument::from_moment(&mutation.entry, &mutation.game)?;
        let entry_write = || {
            self.database.upsert_write(
                &moment_document_path(&review_key, &moment_id),
                &document,
                &[("purgeAt", mutation.entry.purge_at)],
            )
        };
        let annotation_write = annotation_create_write(&self.database, address, &annotation)
            .map_err(annotation_persist_error)?;
        let mut writes = vec![annotation_write, entry_write()?];
        let quality_writes = self
            .quality_capture
            .prepare_firestore_writes(&mutation.owner, &mutation.quality_captures)
            .await;
        let capture_was_gated = !quality_writes.is_empty();
        writes.extend(quality_writes);
        match self.database.commit(writes).await {
            Ok(()) => Ok(annotation),
            Err(FirestoreError::Conflict) => {
                let existing = read_annotation_document(&self.database, address, &annotation)
                    .await
                    .map_err(annotation_persist_error)?;
                let mut fallback = vec![entry_write()?];
                if capture_was_gated {
                    fallback.extend(
                        self.quality_capture
                            .prepare_firestore_writes(&mutation.owner, &mutation.quality_captures)
                            .await,
                    );
                }
                match self.database.commit(fallback).await {
                    Ok(()) => {}
                    Err(FirestoreError::Conflict) if capture_was_gated => {
                        self.database.commit(vec![entry_write()?]).await?;
                    }
                    Err(error) => return Err(error.into()),
                }
                Ok(existing.unwrap_or(annotation))
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn annotation_persist_error(error: ReviewAnnotationStoreError) -> ReviewAnalysisCacheError {
    match error {
        ReviewAnnotationStoreError::Configuration(message) => {
            ReviewAnalysisCacheError::Configuration(message)
        }
        ReviewAnnotationStoreError::Unavailable => ReviewAnalysisCacheError::Unavailable,
        ReviewAnnotationStoreError::InvalidRecord => {
            ReviewAnalysisCacheError::InvalidEntry(InvalidEntryReason::DocumentDecode)
        }
    }
}

/// Decodes one stored document into the entry it describes.
fn decode_entry(
    document_id: String,
    document: ReviewAnalysisMomentDocument,
    game_import_id: &GameImportId,
    game: &ReviewSessionGame,
) -> Result<ReviewAnalysisEntry, ReviewAnalysisCacheError> {
    let moment_id = game
        .automatic_critical_moments()
        .into_iter()
        .map(|imported| imported.moment.critical_moment_id)
        .chain(
            game.player_selected_moments
                .values()
                .map(|moment| moment.critical_moment_id.clone()),
        )
        .find(|moment_id| moment_document_id(moment_id) == document_id)
        .ok_or(ReviewAnalysisCacheError::InvalidEntry(
            InvalidEntryReason::MomentDecode,
        ))?;
    let entry = document.into_moment(game_import_id, moment_id, game)?;
    entry
        .validate(game)
        .map_err(|_| ReviewAnalysisCacheError::InvalidEntry(InvalidEntryReason::EntryValidation))?;
    Ok(entry)
}

fn evidence_entry_count(entries: &[ReviewAnalysisEntry]) -> usize {
    entries.iter().map(|entry| entry.evidence.len()).sum()
}

fn serialized_bytes(value: &impl serde::Serialize) -> Result<usize, ReviewAnalysisCacheError> {
    serde_json::to_vec(value)
        .map(|encoded| encoded.len())
        .map_err(|_| ReviewAnalysisCacheError::InvalidEntry(InvalidEntryReason::Serialization))
}

impl From<GameImportStoreError> for ReviewAnalysisCacheError {
    fn from(error: GameImportStoreError) -> Self {
        match error {
            GameImportStoreError::Configuration(message) => Self::Configuration(message),
            GameImportStoreError::Transport => Self::Transport,
            GameImportStoreError::Unavailable => Self::Unavailable,
            GameImportStoreError::Conflict | GameImportStoreError::InvalidRecord => {
                Self::InvalidEntry(InvalidEntryReason::GameImport)
            }
        }
    }
}

impl From<FirestoreError> for ReviewAnalysisCacheError {
    fn from(error: FirestoreError) -> Self {
        match error {
            FirestoreError::Configuration(message) => Self::Configuration(message),
            FirestoreError::Transport => Self::Transport,
            FirestoreError::Unavailable => Self::Unavailable,
            FirestoreError::Conflict => Self::Conflict,
            FirestoreError::InvalidDocument => {
                Self::InvalidEntry(InvalidEntryReason::DocumentDecode)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use axum::{
        extract::{Path, State},
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };
    use serde_json::Value;
    use tokio::sync::Mutex;

    use super::*;
    use crate::{
        engine_analysis::EngineProvenance,
        game_import_store::{GameImportRecord, InMemoryGameImportStore},
        quality_capture::QualityCaptureDraft,
        review_analysis_cache::{
            durable_moment::DurableReviewMomentPayload,
            entry::{
                CheckpointReviewSessionMoment, PreparedReviewSessionMoment,
                RestoredReviewSessionMoment,
            },
            test_fixtures::{fixture_import_owned_by, fixture_player},
        },
        review_session_contract::{EvidenceId, IdempotencyKey, ReviewSessionCoreContract},
        review_session_processor::ProcessorPrincipal,
    };

    #[derive(Default)]
    struct FakeFirestoreState {
        commits: Vec<Value>,
        document_reads: Vec<String>,
        list_reads: usize,
    }

    #[tokio::test]
    async fn a_review_seeds_its_analysis_once_and_first_writer_wins() {
        let (state, store, server) = fixture_store().await;
        let created_at = "2026-07-26T10:00:00.123456789Z".parse().unwrap();
        let (entries, imported, _idempotency_key) = fixture_entries(created_at);
        let expected_write_count = entries.entries.len();

        let durable_documents = serde_json::to_string(&entries.entries).unwrap();
        for forbidden in [
            "hypothesis",
            "objectiveRefutation",
            "humanMoveModel",
            "originalWords",
            "committedAlternatives",
        ] {
            assert!(
                !durable_documents.contains(forbidden),
                "cache entries must not contain {forbidden}"
            );
        }
        store.seed(entries.clone()).await.unwrap();

        let commit = state.lock().await.commits.last().cloned().unwrap();
        let writes = commit["writes"].as_array().unwrap().clone();
        assert_eq!(writes.len(), expected_write_count);
        assert!(
            writes
                .iter()
                .all(|write| write["currentDocument"]["exists"] == false),
            "each entry claims the document does not exist: first writer wins"
        );
        let review_key = cache_address(&imported.game_import_id).unwrap();
        assert!(writes[0]["update"]["name"]
            .as_str()
            .unwrap()
            .contains(&format!("/reviewAnalysis/{}/moments/", review_key.as_str())));

        // Seeding again changes nothing: every create loses its precondition.
        let before = state.lock().await.commits.len();
        store.seed(entries).await.unwrap();
        assert_eq!(
            cached_moment_writes(&state.lock().await.commits, before),
            0,
            "a review whose analysis is already cached must not write it again"
        );

        server.abort();
    }

    #[tokio::test]
    async fn a_second_players_review_of_one_game_reads_the_first_players_analysis() {
        let (state, store, server) = fixture_store().await;
        let created_at = "2026-07-26T10:00:00.123456789Z".parse().unwrap();
        let (first, first_import, _) = fixture_entries(created_at);
        // The second Player has just started: nothing opened, nothing prepared.
        let (second, second_import) =
            fixture_pending_entries(fixture_player("firebase-player-b"), created_at);
        assert_ne!(first_import.game_import_id, second_import.game_import_id);
        assert_eq!(
            cache_address(&first_import.game_import_id).unwrap(),
            cache_address(&second_import.game_import_id).unwrap()
        );

        let before_first = state.lock().await.commits.len();
        store.seed(first).await.unwrap();
        let before_second = state.lock().await.commits.len();
        assert_eq!(
            cached_moment_writes(&state.lock().await.commits, before_first),
            1,
            "the first review pays for the analysis"
        );

        store.seed(second.clone()).await.unwrap();
        assert_eq!(
            cached_moment_writes(&state.lock().await.commits, before_second),
            0,
            "an unopened second review must not overwrite prepared analysis"
        );

        let loaded = store
            .load(&second_import.game_import_id, &second.game, created_at)
            .await
            .unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(
            matches!(
                loaded
                    .into_iter()
                    .next()
                    .unwrap()
                    .into_restored(&second.game)
                    .unwrap(),
                RestoredReviewSessionMoment::Prepared { .. }
            ),
            "the second Player reads prepared analysis they never computed"
        );

        server.abort();
    }

    #[tokio::test]
    async fn an_entry_past_its_retention_is_a_miss() {
        let (_state, store, server) = fixture_store().await;
        let created_at: DateTime<Utc> = "2026-07-26T10:00:00.123456789Z".parse().unwrap();
        let (entries, imported, _) = fixture_entries(created_at);
        let game = entries.game.clone();
        store.seed(entries).await.unwrap();

        let expired_at = created_at
            + chrono::TimeDelta::hours(super::super::REVIEW_ANALYSIS_CACHE_LIFETIME_HOURS + 1);
        assert!(store
            .load(&imported.game_import_id, &game, expired_at)
            .await
            .unwrap()
            .is_empty());

        server.abort();
    }

    /// The generation lives inside the review key, so a bump is a different
    /// address. Simulated by moving the address rather than by shipping a second
    /// live generation — `review_durability` owns proving the key itself moves.
    #[tokio::test]
    async fn analysis_written_at_another_generation_is_unreachable() {
        let (_state, store, server) = fixture_store().await;
        let created_at = "2026-07-26T10:00:00.123456789Z".parse().unwrap();
        let (entries, _imported, _) = fixture_entries(created_at);
        let game = entries.game.clone();
        store.seed(entries).await.unwrap();

        let regenerated =
            GameImportId::try_from(format!("game-import:{}:{}", "c".repeat(64), "d".repeat(32)))
                .unwrap();
        assert!(
            store
                .load(&regenerated, &game, created_at)
                .await
                .unwrap()
                .is_empty(),
            "a generation bump must miss rather than serve analysis from the old address"
        );

        server.abort();
    }

    #[tokio::test]
    async fn a_moment_mutation_writes_only_the_addressed_entry() {
        let (state, store, server) = fixture_store().await;
        let created_at = "2026-07-26T10:00:00.123456789Z".parse().unwrap();
        let mutation_at = created_at + chrono::TimeDelta::hours(1);
        let (entries, imported, _) = fixture_entries(created_at);
        let game = entries.game.clone();
        let mutation = fixture_moment_mutation(&entries, &imported, mutation_at);
        let changed_moment_document_id = moment_document_id(mutation.moment_id());
        store.seed(entries).await.unwrap();
        let (list_reads_before, document_reads_before) = {
            let state = state.lock().await;
            (state.list_reads, state.document_reads.len())
        };

        store.replace_moment(mutation).await.unwrap();

        {
            let state = state.lock().await;
            assert_eq!(
                state.list_reads, list_reads_before,
                "an addressed mutation must not list every Review Moment"
            );
            assert_eq!(
                state.document_reads.len(),
                document_reads_before,
                "an addressed mutation reads nothing at all"
            );
            let writes = state.commits.last().unwrap()["writes"].as_array().unwrap();
            assert_eq!(writes.len(), 1);
            assert!(writes[0]["update"]["name"]
                .as_str()
                .unwrap()
                .ends_with(&changed_moment_document_id));
            assert!(
                writes[0]["currentDocument"].is_null(),
                "the upgrade is unconditional, whoever seeded the entry"
            );
        }

        let loaded = store
            .load(&imported.game_import_id, &game, mutation_at)
            .await
            .unwrap();
        assert_eq!(loaded.len(), 1);
        server.abort();
    }

    #[tokio::test]
    async fn production_moment_mutation_commits_the_entry_and_quality_outbox_together() {
        let (state, store, server) = fixture_store_for(true).await;
        let created_at = "2026-07-26T10:00:00.123456789Z".parse().unwrap();
        let mutation_at = created_at + chrono::TimeDelta::hours(1);
        let (entries, mut imported, _) = fixture_entries(created_at);
        imported.engine_provenance = Some(EngineProvenance {
            version: "Stockfish 18".to_string(),
            binary_sha256: "a".repeat(64),
            depth: 16,
            threads: 1,
            hash_mib: 16,
        });
        let mut mutation = fixture_moment_mutation(&entries, &imported, mutation_at);
        mutation.quality_captures = vec![QualityCaptureDraft::game_analysis(&imported).unwrap()];
        store.seed(entries).await.unwrap();

        store.replace_moment(mutation).await.unwrap();

        let committed = state.lock().await.commits.last().cloned().unwrap();
        let writes = committed["writes"].as_array().unwrap();
        assert_eq!(
            writes.len(),
            3,
            "the entry, preference key, and outbox must share one commit"
        );
        assert!(writes.iter().any(|write| {
            write["update"]["name"]
                .as_str()
                .is_some_and(|name| name.contains("/qualityOutbox/"))
        }));
        server.abort();
    }

    #[test]
    fn corrupt_inline_evidence_fails_closed() {
        let created_at = "2026-07-26T10:00:00.123456789Z".parse().unwrap();
        let (entries, _imported, _) = fixture_entries(created_at);

        let mut missing = entries.clone();
        missing.entries[0].evidence.pop();
        assert!(missing.entries[0].validate(&missing.game).is_err());

        let mut corrupt = entries.clone();
        corrupt.entries[0].evidence[0].metadata_mut().evidence_id =
            EvidenceId::try_from(format!("sha256:{}", "0".repeat(64))).unwrap();
        assert!(corrupt.entries[0].validate(&corrupt.game).is_err());

        let entry = &entries.entries[0];
        let mut occupied_wire = serde_json::to_value(
            DurableReviewMomentPayload::from_moment(entry, &entries.game).unwrap(),
        )
        .unwrap();
        occupied_wire["occupied"] = Value::Bool(true);
        assert!(
            serde_json::from_value::<DurableReviewMomentPayload>(occupied_wire).is_err(),
            "an unknown durable Moment field must fail closed"
        );
    }

    fn cached_moment_writes(commits: &[Value], from: usize) -> usize {
        commits[from..]
            .iter()
            .filter_map(|commit| commit["writes"].as_array())
            .flatten()
            .filter(|write| {
                write["update"]["name"]
                    .as_str()
                    .is_some_and(|name| name.contains("/reviewAnalysis/"))
            })
            .count()
    }

    async fn fixture_store() -> (
        Arc<Mutex<FakeFirestoreState>>,
        FirestoreReviewAnalysisCache,
        tokio::task::JoinHandle<Result<(), std::io::Error>>,
    ) {
        fixture_store_for(false).await
    }

    async fn fixture_store_for(
        production: bool,
    ) -> (
        Arc<Mutex<FakeFirestoreState>>,
        FirestoreReviewAnalysisCache,
        tokio::task::JoinHandle<Result<(), std::io::Error>>,
    ) {
        let state = Arc::new(Mutex::new(FakeFirestoreState::default()));
        let application = Router::new()
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-staging/documents:commit",
                post(commit_documents),
            )
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-production/documents:commit",
                post(commit_documents),
            )
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-staging/documents/reviewAnalysis/:review_key/moments",
                get(list_cached_moments),
            )
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-production/documents/reviewAnalysis/:review_key/moments",
                get(list_cached_moments),
            )
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-staging/documents/reviewAnalysis/:review_key/moments/:moment_id",
                get(read_cached_moment),
            )
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-production/documents/reviewAnalysis/:review_key/moments/:moment_id",
                get(read_cached_moment),
            )
            .route(
                "/v1/projects/chenchess-test/databases/coach-app-production/documents/users/:owner_id",
                get(read_quality_preference),
            )
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, application).await });
        let database = if production {
            FirestoreDatabase::production_emulator("chenchess-test", address.to_string()).unwrap()
        } else {
            FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap()
        };
        let store = FirestoreReviewAnalysisCache {
            quality_capture: QualityCaptureAppender::for_application(database.clone()),
            database,
            game_imports: Arc::new(InMemoryGameImportStore::default()),
        };
        (state, store, server)
    }

    fn fixture_entries(
        created_at: DateTime<Utc>,
    ) -> (ReviewAnalysisEntries, GameImportRecord, IdempotencyKey) {
        let imported = fixture_import_owned_by(fixture_player("firebase-player-a"), created_at);
        let core: ReviewSessionCoreContract = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/core-contract.json"
        )))
        .unwrap();
        let idempotency_key =
            IdempotencyKey::try_from("idempotency-key:fixture:durable".to_string()).unwrap();
        let entries = ReviewAnalysisEntries::try_new(
            &imported,
            vec![CheckpointReviewSessionMoment::Prepared(Box::new(
                PreparedReviewSessionMoment {
                    core,
                    local_decision: None,
                    idempotency_keys: BTreeSet::from([idempotency_key.clone()]),
                    exploration: Default::default(),
                    comment_publication: Default::default(),
                },
            ))],
            created_at,
        )
        .unwrap();
        (entries, imported, idempotency_key)
    }

    /// A review at its start: every Review Moment listed, none opened.
    fn fixture_pending_entries(
        owner: ProcessorPrincipal,
        created_at: DateTime<Utc>,
    ) -> (ReviewAnalysisEntries, GameImportRecord) {
        let imported = fixture_import_owned_by(owner, created_at);
        let core: ReviewSessionCoreContract = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/core-contract.json"
        )))
        .unwrap();
        let entries = ReviewAnalysisEntries::try_new(
            &imported,
            vec![CheckpointReviewSessionMoment::Pending {
                core: Box::new(core),
            }],
            created_at,
        )
        .unwrap();
        (entries, imported)
    }

    fn fixture_moment_mutation(
        entries: &ReviewAnalysisEntries,
        imported: &GameImportRecord,
        mutation_at: DateTime<Utc>,
    ) -> ReviewAnalysisMutation {
        let replacement = entries
            .entries
            .iter()
            .cloned()
            .find_map(
                |entry| match entry.into_restored(&entries.game).ok().unwrap() {
                    RestoredReviewSessionMoment::Prepared { prepared, .. } => Some(*prepared),
                    RestoredReviewSessionMoment::Pending { .. } => None,
                },
            )
            .unwrap();
        ReviewAnalysisMutation::try_new(
            imported.game_import_id.clone(),
            imported.owner.clone(),
            entries.game.clone(),
            replacement,
            mutation_at,
            Vec::new(),
        )
        .unwrap()
    }

    /// Honours `currentDocument.exists: false`, because first-writer-wins on the
    /// analysis cache is a server-side precondition and a fake that ignored it
    /// would let a losing writer look like a winner.
    async fn commit_documents(
        State(state): State<Arc<Mutex<FakeFirestoreState>>>,
        Json(mut commit): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        truncate_firestore_timestamps(&mut commit);
        let existing = committed_updates(&state)
            .await
            .into_iter()
            .filter_map(|document| document["name"].as_str().map(str::to_owned))
            .collect::<BTreeSet<_>>();
        let precondition_failed = commit["writes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|write| {
                write["currentDocument"]["exists"] == false
                    && write["update"]["name"]
                        .as_str()
                        .is_some_and(|name| existing.contains(name))
            });
        if precondition_failed {
            return (StatusCode::CONFLICT, Json(serde_json::json!({})));
        }
        state.lock().await.commits.push(commit);
        (StatusCode::OK, Json(serde_json::json!({})))
    }

    fn truncate_firestore_timestamps(value: &mut Value) {
        match value {
            Value::Array(values) => {
                for value in values {
                    truncate_firestore_timestamps(value);
                }
            }
            Value::Object(fields) => {
                if let Some(raw) = fields
                    .get("timestampValue")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                {
                    let timestamp = DateTime::parse_from_rfc3339(&raw).unwrap().to_utc();
                    let rounded = DateTime::<Utc>::from_timestamp(
                        timestamp.timestamp(),
                        timestamp.timestamp_subsec_micros() * 1_000,
                    )
                    .unwrap();
                    fields.insert(
                        "timestampValue".to_string(),
                        Value::String(rounded.to_rfc3339()),
                    );
                }
                for value in fields.values_mut() {
                    truncate_firestore_timestamps(value);
                }
            }
            _ => {}
        }
    }

    async fn read_quality_preference(Path(owner_id): Path<String>) -> Json<Value> {
        Json(serde_json::json!({
            "name": format!(
                "projects/chenchess-test/databases/coach-app-production/documents/users/{owner_id}"
            ),
            "updateTime": "2026-07-26T10:00:00Z",
            "fields": {
                "schemaVersion": { "integerValue": "1" },
                "createdAt": { "timestampValue": "2026-07-26T09:00:00Z" },
                "updatedAt": { "timestampValue": "2026-07-26T09:00:00Z" },
                "captureEnabled": { "booleanValue": true },
                "acknowledgedDisclosureVersion": { "integerValue": "1" }
            }
        }))
    }

    async fn list_cached_moments(
        State(state): State<Arc<Mutex<FakeFirestoreState>>>,
        Path(review_key): Path<String>,
    ) -> Json<Value> {
        state.lock().await.list_reads += 1;
        let marker = format!("/reviewAnalysis/{review_key}/moments/");
        let documents = committed_updates(&state)
            .await
            .into_iter()
            .filter(|document| {
                document["name"]
                    .as_str()
                    .is_some_and(|name| name.contains(&marker))
            })
            .collect::<Vec<_>>();
        Json(serde_json::json!({ "documents": documents }))
    }

    async fn read_cached_moment(
        State(state): State<Arc<Mutex<FakeFirestoreState>>>,
        Path((review_key, moment_id)): Path<(String, String)>,
    ) -> Result<Json<Value>, StatusCode> {
        state
            .lock()
            .await
            .document_reads
            .push(format!("moments:{moment_id}"));
        committed_updates(&state)
            .await
            .into_iter()
            .find(|document| {
                document["name"].as_str().is_some_and(|name| {
                    name.ends_with(&format!("/reviewAnalysis/{review_key}/moments/{moment_id}"))
                })
            })
            .map(Json)
            .ok_or(StatusCode::NOT_FOUND)
    }

    async fn committed_updates(state: &Arc<Mutex<FakeFirestoreState>>) -> Vec<Value> {
        let state = state.lock().await;
        let mut documents = BTreeMap::new();
        for update in state
            .commits
            .iter()
            .filter_map(|commit| commit["writes"].as_array())
            .flatten()
            .map(|write| write["update"].clone())
        {
            let Some(name) = update["name"].as_str() else {
                continue;
            };
            documents.insert(name.to_string(), update);
        }
        documents.into_values().collect()
    }
}
