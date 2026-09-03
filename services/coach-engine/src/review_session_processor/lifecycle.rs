use std::{collections::btree_map::Entry, sync::Arc, time::Instant};

use chrono::Utc;

use crate::{
    game_analysis_store::GameAnalysisRecord,
    game_import::ReviewImport,
    game_import_store::{GameImportLookup, GameImportRecord, GameImportStoreError},
    imported_games::ImportedGameCard,
    lichess::LichessExportClient,
    profile_game_feed::DailyGameReviewRequest,
    review_durability::game_import_id,
    review_facts::{ReviewFactsError, ReviewFactsInput, ReviewFactsService, ReviewProviderTimings},
    review_session_contract::*,
    review_session_game_identity::ReviewSessionGameIdentity,
    types::{EloProfile, ReviewSide as PipelineReviewSide},
};

use super::{
    admission::{EngineLease, EngineWorkload},
    events::{EventEmitter, OperationProgressEmitter},
    readiness::surface_requires_complete_authoring,
    residency::emit_session_failure,
    session::{ProcessorReviewMoment, ProcessorSession, SessionInspectionError},
    ProcessorPrincipal, ReviewSessionProcessor, SessionStartSignal,
};

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub(super) async fn open_game_review(
        &self,
        principal: ProcessorPrincipal,
        game_import_id: GameImportId,
        emitter: Arc<EventEmitter>,
    ) {
        match self.game_imports.find(&principal, &game_import_id).await {
            Ok(GameImportLookup::Found(record)) => {
                emitter.completed(OperationCompletion::GameReviewOpened {
                    game_import_id: record.game_import_id,
                    review: Box::new(record.frozen_review),
                });
            }
            Ok(GameImportLookup::NotFound | GameImportLookup::OwnerMismatch) => emitter.rejected(
                OperationKind::GameReviewOpen,
                CommandRejectionReason::UnknownGameImport,
                RejectionRecovery::CorrectInput,
            ),
            Err(error) => {
                tracing::error!(
                    firestore_operation = "game_review_open",
                    category = error.diagnostic_category(),
                    error = %error,
                    "Game Review persistence failed"
                );
                emitter.unavailable(
                    OperationKind::GameReviewOpen,
                    ProviderUnavailableReason::Persistence,
                    RetryDirective::RetryAllowed,
                );
            }
        }
    }

    pub(super) async fn open_game_review_by_identity(
        &self,
        principal: ProcessorPrincipal,
        source: GameInputSource,
        review_side: ReviewSide,
        elo_rating: EloRating,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(identity) =
            ReviewSessionGameIdentity::from_request(&source, review_side, elo_rating)
        else {
            emitter.rejected(
                OperationKind::GameReviewOpen,
                match source {
                    GameInputSource::LichessUrl { .. } => CommandRejectionReason::InvalidLichessUrl,
                    GameInputSource::ChessComUrl { .. } => {
                        CommandRejectionReason::InvalidChessComUrl
                    }
                    _ => CommandRejectionReason::InvalidCommand,
                },
                RejectionRecovery::CorrectInput,
            );
            return;
        };
        self.open_game_review(
            principal.clone(),
            game_import_id(&principal, &identity),
            emitter,
        )
        .await;
    }

    pub(super) async fn import_game(
        &self,
        principal: ProcessorPrincipal,
        source: GameInputSource,
        review_side: RequestedReviewSide,
        elo_profile: RequestedEloProfile,
        emitter: Arc<EventEmitter>,
    ) {
        let pipeline_started = Instant::now();
        let progress = OperationProgressEmitter::new(
            emitter.clone(),
            OperationProgress::Import {
                stage: ImportProgressStage::ValidatingSource,
            },
        );
        let import_progress = progress.clone();
        let imported = progress
            .run(self.importer.import_review_with_progress(
                &source,
                review_side,
                &elo_profile,
                move |stage| {
                    import_progress.set(OperationProgress::Import { stage });
                },
            ))
            .await;
        let imported = match imported {
            Ok(imported) => imported,
            Err(error) => {
                emitter.event(error.terminal().event().clone());
                return;
            }
        };
        self.finish_game_import(
            pipeline_started,
            principal,
            imported,
            true,
            progress,
            emitter,
        )
        .await;
    }

    pub(super) async fn import_daily_game(
        &self,
        principal: ProcessorPrincipal,
        request: DailyGameReviewRequest,
        emitter: Arc<EventEmitter>,
    ) {
        let pipeline_started = Instant::now();
        let progress = OperationProgressEmitter::new(
            emitter.clone(),
            OperationProgress::Import {
                stage: ImportProgressStage::ValidatingSource,
            },
        );
        let import_progress = progress.clone();
        let imported = progress
            .run(
                self.importer
                    .import_daily_review_with_progress(&request, move |stage| {
                        import_progress.set(OperationProgress::Import { stage });
                    }),
            )
            .await;
        let imported = match imported {
            Ok(imported) => imported,
            Err(error) => {
                emitter.event(error.terminal().event().clone());
                return;
            }
        };
        self.finish_game_import(
            pipeline_started,
            principal,
            imported,
            true,
            progress,
            emitter,
        )
        .await;
    }

    async fn finish_game_import(
        &self,
        pipeline_started: Instant,
        principal: ProcessorPrincipal,
        imported: ReviewImport,
        record_imported_game: bool,
        progress: Arc<OperationProgressEmitter>,
        emitter: Arc<EventEmitter>,
    ) {
        progress.set(OperationProgress::Import {
            stage: ImportProgressStage::RunningGameReview,
        });
        // Resolved values, so the fingerprint matches the one a later
        // handle-less resume derives from what the Player typed.
        let game_identity = ReviewSessionGameIdentity::from_import(&imported.imported_game);
        let now = Utc::now();
        let game_import_id = game_import_id(&principal, &game_identity);
        match self.game_imports.find(&principal, &game_import_id).await {
            Ok(GameImportLookup::Found(record)) => {
                let card = match imported_game_card(
                    &principal,
                    record_imported_game,
                    &record.game_import_id,
                    &imported,
                    &record.frozen_review,
                    now,
                ) {
                    Ok(card) => card,
                    Err(error) => {
                        self.emit_game_import_persistence_failure(&emitter, &error);
                        return;
                    }
                };
                if let Some(card) = card {
                    if let Err(error) = self
                        .game_imports
                        .upsert_imported_game_card(&principal, card)
                        .await
                    {
                        self.emit_game_import_persistence_failure(&emitter, &error);
                        return;
                    }
                }
                self.complete_game_import(
                    pipeline_started,
                    &emitter,
                    *record,
                    ReviewProviderTimings {
                        engine_analysis: Vec::new(),
                        human_move_model: Vec::new(),
                    },
                );
                return;
            }
            Ok(GameImportLookup::NotFound) => {}
            Ok(GameImportLookup::OwnerMismatch) => {
                self.emit_game_import_persistence_failure(
                    &emitter,
                    &GameImportStoreError::InvalidRecord,
                );
                return;
            }
            Err(error) => {
                self.emit_game_import_persistence_failure(&emitter, &error);
                return;
            }
        }
        let reviewed = match self
            .cached_analysis(&game_identity, &imported.imported_game, now)
            .await
        {
            // Analysis is a pure function of the Game, the side, and the Elo,
            // so another Player having already paid for them is a complete
            // answer: no engine admission, no provider calls.
            Some(cached) => {
                let (review, player_selected_moments, engine_provenance) = cached.into_review();
                ReviewedGame {
                    review,
                    player_selected_moments,
                    engine_provenance,
                    provider_timings: ReviewProviderTimings {
                        engine_analysis: Vec::new(),
                        human_move_model: Vec::new(),
                    },
                }
            }
            None => {
                let Some(engine_lease) = resolve_engine_admission(
                    self.engine_admission
                        .acquire(EngineWorkload::Batch, &principal)
                        .await,
                    OperationKind::GameImport,
                    &emitter,
                ) else {
                    return;
                };
                let pipeline_elo =
                    EloProfile::try_from(imported.imported_game.elo_profile.rating.value())
                        .expect("a resolved Review Session Elo is a valid pipeline Elo");
                let pipeline_side = match imported.imported_game.review_side {
                    ReviewSide::White => PipelineReviewSide::White,
                    ReviewSide::Black => PipelineReviewSide::Black,
                    ReviewSide::Both => PipelineReviewSide::Both,
                };
                let review = progress
                    .run(
                        ReviewFactsService::new(self.engine.clone(), self.human.clone())
                            .review_session_game(
                                ReviewFactsInput {
                                    pgn: &imported.pgn,
                                    player_elo: pipeline_elo,
                                    review_side: pipeline_side,
                                    opening_identification: &imported.imported_game.game.opening,
                                },
                                &imported.imported_game.game.game_ref,
                            ),
                    )
                    .await;
                drop(engine_lease);
                let crate::review_facts::TimedGameReview {
                    review,
                    player_selected_moments,
                    engine_provenance,
                    provider_timings,
                } = match review {
                    Ok(timed_review) => timed_review,
                    Err(error) => {
                        emit_game_review_error(&emitter, error);
                        return;
                    }
                };
                self.remember_analysis(
                    &game_identity,
                    GameAnalysisRecord::new(
                        &imported.imported_game,
                        review.clone(),
                        player_selected_moments.clone(),
                        engine_provenance.clone(),
                        now,
                    ),
                )
                .await;
                ReviewedGame {
                    review,
                    player_selected_moments,
                    engine_provenance,
                    provider_timings,
                }
            }
        };
        let ReviewedGame {
            review,
            player_selected_moments,
            engine_provenance,
            provider_timings,
        } = reviewed;
        // The stable ID keeps this Player's frozen Game Review load-bearing.
        // Shared analysis only supplies the first record; subsequent imports
        // return that exact record.
        let record = GameImportRecord::new(
            game_import_id.clone(),
            principal.clone(),
            imported.imported_game.clone(),
            review.clone(),
            player_selected_moments,
            engine_provenance,
            Utc::now(),
        );
        let imported_game_card = match imported_game_card(
            &principal,
            record_imported_game,
            &game_import_id,
            &imported,
            &record.frozen_review,
            record.created_at,
        ) {
            Ok(card) => card,
            Err(error) => {
                self.emit_game_import_persistence_failure(&emitter, &error);
                return;
            }
        };
        let created = match imported_game_card.clone() {
            Some(card) => {
                self.game_imports
                    .create_with_imported_game_card(record.clone(), card)
                    .await
            }
            None => self.game_imports.create(record.clone()).await,
        };
        match created {
            Ok(()) => {}
            Err(GameImportStoreError::Conflict) => {
                match self.game_imports.find(&principal, &game_import_id).await {
                    Ok(GameImportLookup::Found(winner)) => {
                        if let Some(card) = imported_game_card {
                            if let Err(error) = self
                                .game_imports
                                .upsert_imported_game_card(&principal, card)
                                .await
                            {
                                self.emit_game_import_persistence_failure(&emitter, &error);
                                return;
                            }
                        }
                        self.complete_game_import(
                            pipeline_started,
                            &emitter,
                            *winner,
                            provider_timings,
                        );
                        return;
                    }
                    Ok(_) => {
                        self.emit_game_import_persistence_failure(
                            &emitter,
                            &GameImportStoreError::Conflict,
                        );
                        return;
                    }
                    Err(error) => {
                        self.emit_game_import_persistence_failure(&emitter, &error);
                        return;
                    }
                }
            }
            Err(error) => {
                self.emit_game_import_persistence_failure(&emitter, &error);
                return;
            }
        }
        self.complete_game_import(pipeline_started, &emitter, record, provider_timings);
    }

    fn complete_game_import(
        &self,
        pipeline_started: Instant,
        emitter: &EventEmitter,
        record: GameImportRecord,
        provider_timings: ReviewProviderTimings,
    ) {
        let timing = GameImportTiming {
            runtime_startup_milliseconds: self.runtime_startup.map(duration_milliseconds),
            total_pipeline_milliseconds: duration_milliseconds(pipeline_started.elapsed()),
            engine_analysis: ProviderTimingSummary::from_durations(
                self.engine.provider_name(),
                &provider_timings.engine_analysis,
            ),
            human_move_model: ProviderTimingSummary::from_durations(
                self.human.provider_name(),
                &provider_timings.human_move_model,
            ),
        };
        emit_game_import_timing_diagnostic(emitter, &timing);
        emitter.completed(OperationCompletion::GameImported {
            game_import_id: record.game_import_id,
            review: Box::new(record.frozen_review),
            timing: Some(timing),
            imported_game: Some(Box::new(record.imported_game)),
        });
    }

    fn emit_game_import_persistence_failure(
        &self,
        emitter: &EventEmitter,
        error: &GameImportStoreError,
    ) {
        tracing::error!(
            firestore_operation = "game_import_reuse",
            category = error.diagnostic_category(),
            error = %error,
            "Game Import persistence failed"
        );
        let retry = match error {
            GameImportStoreError::Configuration(_) | GameImportStoreError::InvalidRecord => {
                RetryDirective::NotRetryable
            }
            GameImportStoreError::Transport
            | GameImportStoreError::Unavailable
            | GameImportStoreError::Conflict => RetryDirective::RetryAllowed,
        };
        emitter.unavailable(
            OperationKind::GameImport,
            ProviderUnavailableReason::Persistence,
            retry,
        );
    }

    /// The cache is never load-bearing: a failure here costs latency, and
    /// failing the import over it would trade a slow review for no review.
    async fn cached_analysis(
        &self,
        identity: &ReviewSessionGameIdentity,
        imported: &ImportedGame,
        now: chrono::DateTime<Utc>,
    ) -> Option<Box<GameAnalysisRecord>> {
        match self.game_analysis.find(identity, imported, now).await {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(
                    firestore_operation = "game_analysis_find",
                    category = error.diagnostic_category(),
                    error = %error,
                    "shared Game Analysis lookup failed; reviewing the Game again"
                );
                None
            }
        }
    }

    /// Losing a write only means the next Player pays for the same analysis.
    async fn remember_analysis(
        &self,
        identity: &ReviewSessionGameIdentity,
        record: GameAnalysisRecord,
    ) {
        if let Err(error) = self.game_analysis.put(identity, record).await {
            tracing::warn!(
                firestore_operation = "game_analysis_put",
                category = error.diagnostic_category(),
                error = %error,
                "shared Game Analysis was not stored"
            );
        }
    }

    /// Makes one addressed Game Review interactive.
    ///
    /// The address is the whole input: there is no session identity to mint, so
    /// starting twice returns the same session and a reopened conversation
    /// starts rather than resumes. Whatever prepared analysis the shared cache
    /// holds arrives with it, which is why a Game a second Player already
    /// reviewed opens without recomputing anything.
    pub(super) async fn start_session(
        self: &Arc<Self>,
        principal: ProcessorPrincipal,
        surface: DeliverySurface,
        game_import_id: GameImportId,
        emitter: Arc<EventEmitter>,
    ) {
        let active_start = {
            self.starting_sessions
                .lock()
                .await
                .get(&game_import_id)
                .cloned()
        };
        if let Some(active_start) = active_start {
            active_start.wait().await;
            Box::pin(self.start_session(principal, surface, game_import_id, emitter)).await;
            return;
        }
        let (start_signal, joins_existing_start) = {
            let mut starting = self.starting_sessions.lock().await;
            match starting.entry(game_import_id.clone()) {
                Entry::Occupied(entry) => (entry.get().clone(), true),
                Entry::Vacant(entry) => {
                    let signal = Arc::new(SessionStartSignal::new());
                    entry.insert(signal.clone());
                    (signal, false)
                }
            }
        };
        if joins_existing_start {
            start_signal.wait().await;
            Box::pin(self.start_session(principal, surface, game_import_id, emitter)).await;
            return;
        }
        emitter.event(ReviewSessionEvent::Progress {
            stage: OperationProgress::ReviewSession {
                stage: ReviewSessionProgressStage::BuildingPosition,
            },
        });
        let session = match self.load_session(&principal, &game_import_id).await {
            Ok(session) => session,
            Err(error) => {
                self.finish_session_start(&game_import_id, &start_signal)
                    .await;
                emit_session_failure(&emitter, OperationKind::ReviewSessionStart, error);
                return;
            }
        };
        self.finish_session_start(&game_import_id, &start_signal)
            .await;
        if surface_requires_complete_authoring(surface)
            && !self
                .prepare_complete_authoring_batch(
                    &principal,
                    &session,
                    &emitter,
                    OperationKind::ReviewSessionStart,
                )
                .await
        {
            return;
        }
        let game = session.game_import();
        emitter.completed(OperationCompletion::ReviewSessionStarted {
            game_import_id: game_import_id.clone(),
            session_revision: session.checkpoint_revision().await,
            review: Box::new(game.review().clone()),
            imported_game: Box::new(game.imported_game.clone()),
            review_moments: session.review_session_moments().await,
        });
        if matches!(surface, DeliverySurface::CoachApp) {
            self.schedule_initial_review_moment_prefetch(game_import_id, session)
                .await;
        } else if matches!(surface, DeliverySurface::Web) {
            /* The web routing of a Review Session is the lazy trigger for its
            stored coaching artifacts: an import alone spends no Language
            Layer budget and adds nothing to the importing surface's call. */
            self.schedule_web_artifact_authoring(principal, game_import_id, &emitter);
        }
    }

    pub(super) async fn inspect_position(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        target: PositionInspectionTarget,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(session) = self
            .session(
                &game_import_id,
                principal,
                &emitter,
                OperationKind::PositionInspection,
            )
            .await
        else {
            return;
        };
        let Some(review_moment) = self
            .review_moment(
                &session,
                &review_moment_id,
                &emitter,
                OperationKind::PositionInspection,
            )
            .await
        else {
            return;
        };
        match review_moment.inspect(target).await {
            Ok(inspection) => {
                emitter.completed(OperationCompletion::PositionInspected {
                    inspection: Box::new(inspection),
                });
            }
            Err(SessionInspectionError::UnknownTarget) => emitter.rejected(
                OperationKind::PositionInspection,
                CommandRejectionReason::UnknownTarget,
                RejectionRecovery::CorrectInput,
            ),
        }
    }

    async fn finish_session_start(
        &self,
        game_import_id: &GameImportId,
        signal: &Arc<SessionStartSignal>,
    ) {
        self.starting_sessions.lock().await.remove(game_import_id);
        signal.finish();
    }

    pub(super) async fn review_moment(
        &self,
        session: &ProcessorSession,
        review_moment_id: &CriticalMomentId,
        emitter: &EventEmitter,
        operation: OperationKind,
    ) -> Option<Arc<ProcessorReviewMoment>> {
        let review_moment = session.review_moment(review_moment_id).await;
        if review_moment.is_none() {
            emitter.rejected(
                operation,
                CommandRejectionReason::UnknownMoment,
                RejectionRecovery::CorrectInput,
            );
        }
        review_moment
    }

    pub(super) async fn session(
        &self,
        game_import_id: &GameImportId,
        principal: &ProcessorPrincipal,
        emitter: &EventEmitter,
        operation: OperationKind,
    ) -> Option<Arc<ProcessorSession>> {
        match self.load_session(principal, game_import_id).await {
            Ok(session) => Some(session),
            Err(error) => {
                emit_session_failure(emitter, operation, error);
                None
            }
        }
    }
}

/// Analysis plus what it cost to obtain. A cache hit reports zero provider
/// calls rather than borrowing the original review's timings, so the import
/// diagnostic keeps telling the truth about this pipeline run.
struct ReviewedGame {
    review: GameReview,
    player_selected_moments: Vec<GameReviewCriticalMoment>,
    engine_provenance: Option<crate::engine_analysis::EngineProvenance>,
    provider_timings: ReviewProviderTimings,
}

fn imported_game_card(
    principal: &ProcessorPrincipal,
    record_imported_game: bool,
    game_import_id: &GameImportId,
    imported: &ReviewImport,
    review: &GameReview,
    imported_at: chrono::DateTime<Utc>,
) -> Result<Option<ImportedGameCard>, GameImportStoreError> {
    if !record_imported_game || !matches!(principal, ProcessorPrincipal::Player(_)) {
        return Ok(None);
    }
    if matches!(
        imported.imported_game.provenance,
        ImportProvenance::LocalPgn { .. }
    ) {
        return Ok(None);
    }
    let learning_path_count = review
        .learning_plan
        .tracks
        .iter()
        .try_fold(0_u16, |count, track| {
            count.checked_add(u16::try_from(track.support.len()).ok()?)
        })
        .ok_or(GameImportStoreError::InvalidRecord)?;
    ImportedGameCard::new(
        game_import_id.clone(),
        &imported.imported_game,
        &imported.pgn,
        learning_path_count,
        imported_at,
    )
    .map(Some)
    .map_err(|_| GameImportStoreError::InvalidRecord)
}

pub(super) fn resolve_engine_admission(
    admission: Result<EngineLease, ProviderUnavailableReason>,
    operation: OperationKind,
    emitter: &EventEmitter,
) -> Option<EngineLease> {
    match admission {
        Ok(lease) => Some(lease),
        Err(reason) => {
            emitter.unavailable(operation, reason, RetryDirective::StartNewOperation);
            None
        }
    }
}

fn emit_game_review_error(emitter: &EventEmitter, error: ReviewFactsError) {
    let provider = match error {
        ReviewFactsError::Engine(_) => Some(ProviderUnavailableReason::StockfishProcess),
        ReviewFactsError::Human(_) => Some(ProviderUnavailableReason::MaiaTransport),
        _ => None,
    };
    if let Some(reason) = provider {
        emitter.unavailable(
            OperationKind::GameImport,
            reason,
            RetryDirective::RetryAllowed,
        );
    } else {
        emitter.rejected(
            OperationKind::GameImport,
            CommandRejectionReason::MissingEvidence,
            RejectionRecovery::None,
        );
    }
}

fn emit_game_import_timing_diagnostic(emitter: &EventEmitter, timing: &GameImportTiming) {
    tracing::info!(
        operation_id = emitter.operation_id().as_str(),
        runtime_startup_milliseconds = ?timing.runtime_startup_milliseconds,
        total_pipeline_milliseconds = timing.total_pipeline_milliseconds,
        engine_provider = timing.engine_analysis.provider,
        engine_call_count = timing.engine_analysis.call_count,
        engine_total_milliseconds = timing.engine_analysis.total_milliseconds,
        engine_median_milliseconds = timing.engine_analysis.median_milliseconds,
        engine_maximum_milliseconds = timing.engine_analysis.maximum_milliseconds,
        human_provider = timing.human_move_model.provider,
        human_call_count = timing.human_move_model.call_count,
        human_total_milliseconds = timing.human_move_model.total_milliseconds,
        human_median_milliseconds = timing.human_move_model.median_milliseconds,
        human_maximum_milliseconds = timing.human_move_model.maximum_milliseconds,
        "review-session game import timing"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_game_cards_project_only_for_player_when_recording() {
        let (imported, review, id, player, now) = imported_game_card_policy_fixture();

        assert!(
            imported_game_card(&player, false, &id, &imported, &review, now)
                .unwrap()
                .is_none()
        );
        assert!(imported_game_card(
            &ProcessorPrincipal::LocalCoach,
            true,
            &id,
            &imported,
            &review,
            now,
        )
        .unwrap()
        .is_none());
        assert!(
            imported_game_card(&player, true, &id, &imported, &review, now)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn daily_coaching_player_imports_project_imported_game_cards() {
        let (imported, review, id, player, now) = imported_game_card_policy_fixture();
        // import_daily_game passes record_imported_game: true with the Player.
        assert!(
            imported_game_card(&player, true, &id, &imported, &review, now)
                .unwrap()
                .is_some()
        );
    }

    fn imported_game_card_policy_fixture() -> (
        ReviewImport,
        GameReview,
        GameImportId,
        ProcessorPrincipal,
        chrono::DateTime<Utc>,
    ) {
        let imported_game = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
        )))
        .unwrap();
        let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/events.json"
        )))
        .unwrap();
        let review = events
            .into_iter()
            .find_map(|event| match event.event {
                ReviewSessionEvent::Completed { result } => match *result {
                    OperationCompletion::GameImported { review, .. } => Some(*review),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        (
            ReviewImport {
                imported_game,
                pgn: "1. e4 e5 *".to_string(),
            },
            review,
            GameImportId::try_from("game-import:fixture:card-policy".to_string()).unwrap(),
            ProcessorPrincipal::Player(
                PlayerId::try_from("player:card-policy".to_string()).unwrap(),
            ),
            "2026-08-12T10:00:00Z".parse().unwrap(),
        )
    }
}
