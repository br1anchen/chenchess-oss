use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tracing::Instrument;

use crate::{
    lichess::LichessExportClient, operating_limits::REVIEW_MOMENT_PREPARATION_DEADLINE_SECONDS,
    review_session_cancellation::ReviewSessionCancellation, review_session_contract::*,
};

use super::{
    events::EventEmitter,
    session::{ProcessorReviewMoment, ProcessorReviewMomentEntry, ProcessorSession},
    LiveOperation, ProcessorPrincipal, ReviewSessionProcessor,
};

pub(super) struct ReviewMomentPrefetchContext<'a> {
    pub(super) principal: &'a ProcessorPrincipal,
    pub(super) operation_id: &'a OperationId,
    pub(super) game_import_id: &'a GameImportId,
    pub(super) review_moment_id: &'a CriticalMomentId,
    pub(super) idempotency_key: &'a IdempotencyKey,
    pub(super) emitter: &'a EventEmitter,
}

pub(super) enum ReviewMomentPrefetchReuse {
    Hit(Arc<ProcessorReviewMoment>),
    Miss,
    Terminal,
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub(super) async fn await_review_moment_prefetch(
        &self,
        context: ReviewMomentPrefetchContext<'_>,
        entry: &ProcessorReviewMomentEntry,
        base_revision: u64,
    ) -> ReviewMomentPrefetchReuse {
        let ReviewMomentPrefetchContext {
            principal,
            operation_id,
            game_import_id,
            review_moment_id,
            idempotency_key,
            emitter,
        } = context;
        let cancellation = ReviewSessionCancellation::default();
        if !self
            .register_live(
                operation_id.clone(),
                LiveOperation::ReviewMomentPreparation {
                    owner: principal.clone(),
                    game_import_id: game_import_id.clone(),
                    idempotency_key: idempotency_key.clone(),
                    cancellation: cancellation.clone(),
                },
            )
            .await
        {
            emitter.rejected(
                OperationKind::ReviewMomentOpen,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            );
            return ReviewMomentPrefetchReuse::Terminal;
        }
        emit_progress(
            emitter,
            game_import_id,
            review_moment_id,
            ReviewMomentPreparationProgressStage::WaitingForCapacity,
        );
        emit_progress(
            emitter,
            game_import_id,
            review_moment_id,
            ReviewMomentPreparationProgressStage::PreparingAuthoringContext,
        );
        let prepared = entry
            .await_prefetched_candidate(base_revision, &cancellation)
            .await;
        self.live.lock().await.remove(operation_id);
        if prepared.is_err() || cancellation.is_cancelled() {
            emitter.cancelled(OperationKind::ReviewMomentOpen);
            return ReviewMomentPrefetchReuse::Terminal;
        }
        match prepared.expect("a non-cancelled prefetch wait has a result") {
            Some(prepared) => {
                log_prefetch_lookup(review_moment_id, true, base_revision);
                emit_progress(
                    emitter,
                    game_import_id,
                    review_moment_id,
                    ReviewMomentPreparationProgressStage::CommittingAuthoringContext,
                );
                ReviewMomentPrefetchReuse::Hit(prepared)
            }
            None => {
                log_prefetch_lookup(review_moment_id, false, base_revision);
                ReviewMomentPrefetchReuse::Miss
            }
        }
    }

    pub(super) async fn schedule_initial_review_moment_prefetch(
        &self,
        game_import_id: GameImportId,
        session: Arc<ProcessorSession>,
    ) {
        let mut entries = session.review_moment_entries().await;
        entries.sort_by_key(|entry| entry.ply());
        let Some(entry) = entries.into_iter().next() else {
            return;
        };
        if entry.prepared_moment().await.is_some() {
            return;
        }
        let base_revision = session.checkpoint_revision().await;
        let Some(engine_lease) = self.engine_admission.try_acquire_prefetch() else {
            tracing::info!(
                event = "coach_review_moment_prefetch_completion",
                game_import_id = game_import_id.as_str(),
                review_moment_id = entry.moment_id().as_str(),
                base_revision,
                status = "skipped_busy",
                "optional Review Moment prefetch skipped without queueing"
            );
            return;
        };
        let cancellation = ReviewSessionCancellation::default();
        if !entry
            .begin_prefetch(base_revision, cancellation.clone())
            .await
        {
            return;
        }
        let recording = self.recording.clone();
        let engine = self.engine.clone();
        let human = self.human.clone();
        let annotations = session.annotations().clone();
        tokio::spawn(
            async move {
                let started_at = Instant::now();
                let deadline = Instant::now()
                    + Duration::from_secs(REVIEW_MOMENT_PREPARATION_DEADLINE_SECONDS);
                let prepared = tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => None,
                    result = tokio::time::timeout_at(
                        tokio::time::Instant::from_std(deadline),
                        entry.prepare_candidate(
                            recording.as_deref(),
                            engine,
                            human,
                            annotations,
                            deadline,
                        ),
                    ) => {
                        match result {
                            Ok(Ok(prepared)) => Some(prepared),
                            Ok(Err(error)) => {
                                tracing::warn!(
                                    event = "coach_review_moment_prefetch_degraded",
                                    game_import_id = game_import_id.as_str(),
                                    review_moment_id = entry.moment_id().as_str(),
                                    category = error.diagnostic_category(),
                                    "optional Review Moment prefetch failed"
                                );
                                None
                            }
                            Err(_) => {
                                tracing::warn!(
                                    event = "coach_review_moment_prefetch_degraded",
                                    game_import_id = game_import_id.as_str(),
                                    review_moment_id = entry.moment_id().as_str(),
                                    category = "timeout",
                                    "optional Review Moment prefetch timed out"
                                );
                                None
                            }
                        }
                    }
                };
                let status = if cancellation.is_cancelled() {
                    "cancelled"
                } else if prepared.is_some() {
                    "succeeded"
                } else {
                    "degraded"
                };
                entry
                    .finish_prefetch(base_revision, &cancellation, prepared)
                    .await;
                drop(engine_lease);
                tracing::info!(
                    event = "coach_review_moment_prefetch_completion",
                    game_import_id = game_import_id.as_str(),
                    review_moment_id = entry.moment_id().as_str(),
                    base_revision,
                    status,
                    wall_milliseconds = started_at.elapsed().as_millis(),
                    "optional Review Moment prefetch completed"
                );
            }
            .in_current_span(),
        );
    }
}

pub(super) fn log_prefetch_lookup(
    review_moment_id: &CriticalMomentId,
    cache_hit: bool,
    base_revision: u64,
) {
    tracing::info!(
        event = "coach_review_moment_prepared_cache_lookup",
        review_moment_id = review_moment_id.as_str(),
        base_revision,
        cache_hit,
        "stable Review Moment preparation cache lookup"
    );
}

fn emit_progress(
    emitter: &EventEmitter,
    game_import_id: &GameImportId,
    review_moment_id: &CriticalMomentId,
    stage: ReviewMomentPreparationProgressStage,
) {
    emitter.event(ReviewSessionEvent::Progress {
        stage: OperationProgress::ReviewMomentPreparation {
            game_import_id: game_import_id.clone(),
            review_moment_id: review_moment_id.clone(),
            stage,
        },
    });
}
