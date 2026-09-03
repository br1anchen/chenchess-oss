use std::sync::Arc;

use chrono::Utc;

use crate::{
    lichess::LichessExportClient,
    quality_capture::QualityCaptureDraft,
    review_analysis_cache::{
        PreparedReviewSessionMoment, ReviewAnalysisCacheError, ReviewAnalysisMutation,
    },
    review_annotation_store::ReviewMomentAnnotation,
    review_session_contract::*,
};

use super::{
    events::EventEmitter,
    session::{
        ProcessorReviewMoment, ProcessorSession, ReviewMomentCommentPublication,
        StagedCommentPublication, StagedEvidenceMutation, StagedExplorationMutation,
    },
    ReviewSessionProcessor,
};

pub(super) struct SessionMutationPersistence<'a> {
    replacement: &'a PreparedReviewSessionMoment,
    quality_captures: Vec<QualityCaptureDraft>,
    operation: OperationKind,
    unavailable_review_moment: Option<&'a CriticalMomentId>,
    terminal_progress: Option<OperationProgress>,
}

impl<'a> SessionMutationPersistence<'a> {
    pub(super) fn business(
        replacement: &'a PreparedReviewSessionMoment,
        operation: OperationKind,
        unavailable_review_moment: Option<&'a CriticalMomentId>,
    ) -> Self {
        Self {
            replacement,
            quality_captures: Vec::new(),
            operation,
            unavailable_review_moment,
            terminal_progress: None,
        }
    }

    fn with_terminal_progress(mut self, progress: OperationProgress) -> Self {
        self.terminal_progress = Some(progress);
        self
    }

    fn with_quality_captures(
        replacement: &'a PreparedReviewSessionMoment,
        quality_captures: Vec<QualityCaptureDraft>,
        operation: OperationKind,
    ) -> Self {
        Self {
            replacement,
            quality_captures,
            operation,
            unavailable_review_moment: None,
            terminal_progress: None,
        }
    }

    fn with_unavailable_moment(mut self, moment_id: &'a CriticalMomentId) -> Self {
        self.unavailable_review_moment = Some(moment_id);
        self
    }

    fn with_appended_captures(mut self, captures: Vec<QualityCaptureDraft>) -> Self {
        self.quality_captures = captures;
        self
    }
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    /// Writes one Review Moment's newly prepared analysis to the shared cache.
    ///
    /// There is nothing to lose a race with. The entry is addressed by the
    /// review and the write is an unconditional upgrade, so the stale-writer
    /// path that optimistic revisions existed to detect cannot occur: two
    /// Players preparing the same Review Moment prepare the same analysis. The
    /// session still advances its own revision, which is the prefetch clock and
    /// nothing durable.
    pub(super) async fn persist_session_mutation(
        &self,
        session: &Arc<ProcessorSession>,
        persistence: SessionMutationPersistence<'_>,
        emitter: &EventEmitter,
    ) -> Option<super::session::SessionCheckpointSuccessor> {
        let SessionMutationPersistence {
            replacement,
            quality_captures,
            operation,
            unavailable_review_moment,
            terminal_progress,
        } = persistence;
        let Some(successor) = session.checkpoint_successor(Utc::now()).await else {
            self.evict_session(session).await;
            emit_terminal_progress(emitter, terminal_progress.as_ref());
            emitter.rejected(
                operation,
                CommandRejectionReason::UnknownSession,
                RejectionRecovery::StartNewReviewSession,
            );
            return None;
        };
        let mutation = match ReviewAnalysisMutation::try_new(
            session.address().clone(),
            session.owner.clone(),
            session.game_import().clone(),
            replacement.clone(),
            Utc::now(),
            quality_captures,
        ) {
            Ok(mutation) => mutation,
            Err(error) => {
                tracing::error!(
                    category = "cache-entry-build",
                    ?operation,
                    reason = %error,
                    "Review Moment analysis assembly failed"
                );
                emit_terminal_progress(emitter, terminal_progress.as_ref());
                emit_persistence_unavailable(
                    emitter,
                    operation,
                    session.address(),
                    unavailable_review_moment,
                );
                return None;
            }
        };
        let owner = mutation.owner.clone();
        let quality_captures = mutation.quality_captures.clone();
        match self.analysis_cache.replace_moment(mutation).await {
            Ok(()) => {
                self.quality_capture
                    .commit_in_process_after_persist(&owner, &quality_captures)
                    .await;
                Some(successor)
            }
            Err(error) => {
                log_persistence_failure(operation, &error);
                // The runtime holds state the durable cache refused, so it is
                // dropped: a retry rebuilds from what actually persisted rather
                // than colliding with an operation nobody can see.
                self.evict_session(session).await;
                emit_terminal_progress(emitter, terminal_progress.as_ref());
                emit_persistence_unavailable(
                    emitter,
                    operation,
                    session.address(),
                    unavailable_review_moment,
                );
                None
            }
        }
    }

    pub(super) async fn commit_staged_evidence_with_capture(
        &self,
        session: &Arc<ProcessorSession>,
        review_moment: &ProcessorReviewMoment,
        staged: StagedEvidenceMutation,
        operation: OperationKind,
        quality_captures: Vec<QualityCaptureDraft>,
        emitter: &EventEmitter,
    ) -> bool {
        let Some(successor) = self
            .persist_session_mutation(
                session,
                SessionMutationPersistence::business(staged.checkpoint(), operation, None)
                    .with_appended_captures(quality_captures),
                emitter,
            )
            .await
        else {
            return false;
        };
        review_moment.apply_staged_evidence(staged).await;
        session.commit_checkpoint_successor(&successor).await;
        true
    }

    pub(super) async fn commit_staged_exploration<T>(
        &self,
        session: &Arc<ProcessorSession>,
        review_moment: &ProcessorReviewMoment,
        staged: StagedExplorationMutation<T>,
        operation: OperationKind,
        emitter: &EventEmitter,
    ) -> Option<T> {
        let persistence =
            SessionMutationPersistence::business(staged.checkpoint(), operation, None);
        let persistence = if operation == OperationKind::AlternativeMoveEvaluation {
            persistence.with_terminal_progress(OperationProgress::AlternativeMoveAllowance {
                remaining: review_moment.exploration.remaining_allowance().await,
            })
        } else {
            persistence
        };
        let successor = self
            .persist_session_mutation(session, persistence, emitter)
            .await?;
        let result = review_moment.apply_staged_exploration(staged).await;
        session.commit_checkpoint_successor(&successor).await;
        Some(result)
    }

    pub(super) async fn commit_staged_comment_publication(
        &self,
        session: &Arc<ProcessorSession>,
        review_moment: &ProcessorReviewMoment,
        staged: StagedCommentPublication,
        emitter: &EventEmitter,
    ) -> Option<super::session::ReviewMomentCommentPublication> {
        // The annotation store is the comment's durable home, so it is written
        // before the Review Session records the write. A checkpoint failure
        // afterwards costs this conversation its in-memory record; the Player
        // keeps the comment, and replaying the key returns it.
        let staged = match review_moment.publish_staged_annotation(staged).await {
            Ok(staged) => staged,
            Err(error) => {
                tracing::error!(
                    category = error.diagnostic_category(),
                    "Review Moment Comment annotation persistence failed"
                );
                emitter.unavailable(
                    OperationKind::ReviewMomentCommentPublication,
                    ProviderUnavailableReason::Persistence,
                    RetryDirective::RetryAllowed,
                );
                return None;
            }
        };
        let successor = self
            .persist_session_mutation(
                session,
                SessionMutationPersistence::with_quality_captures(
                    staged.checkpoint(),
                    staged.quality_captures(),
                    OperationKind::ReviewMomentCommentPublication,
                ),
                emitter,
            )
            .await?;
        let result = review_moment.apply_staged_comment_publication(staged).await;
        session.commit_checkpoint_successor(&successor).await;
        Some(result)
    }

    /// Persists a first-open hosted comment.
    ///
    /// Firestore uses one product-DB commit of annotation + cache + outbox.
    /// In-memory stores have no shared commit, so they append then replace.
    pub(super) async fn persist_first_open_comment(
        &self,
        session: &Arc<ProcessorSession>,
        review_moment: &ProcessorReviewMoment,
        staged: StagedCommentPublication,
        emitter: &EventEmitter,
    ) -> Option<super::session::ReviewMomentCommentPublication> {
        if self.first_open_persist.is_some() {
            self.persist_first_open_bundled(session, review_moment, staged, emitter)
                .await
        } else {
            self.persist_first_open_sequential(session, review_moment, staged, emitter)
                .await
        }
    }

    async fn persist_first_open_sequential(
        &self,
        session: &Arc<ProcessorSession>,
        review_moment: &ProcessorReviewMoment,
        staged: StagedCommentPublication,
        emitter: &EventEmitter,
    ) -> Option<super::session::ReviewMomentCommentPublication> {
        let staged = match review_moment.publish_staged_annotation(staged).await {
            Ok(staged) => staged,
            Err(error) => {
                tracing::error!(
                    category = error.diagnostic_category(),
                    "first-open Review Moment Comment annotation persistence failed"
                );
                emitter.review_moment_unavailable(
                    session.address(),
                    review_moment.moment_id(),
                    ProviderUnavailableReason::Persistence,
                    RetryDirective::RetryAllowed,
                );
                return None;
            }
        };
        let successor = self
            .persist_session_mutation(
                session,
                SessionMutationPersistence::with_quality_captures(
                    staged.checkpoint(),
                    staged.quality_captures(),
                    OperationKind::ReviewMomentOpen,
                )
                .with_unavailable_moment(review_moment.moment_id()),
                emitter,
            )
            .await?;
        let result = review_moment.apply_staged_comment_publication(staged).await;
        session.commit_checkpoint_successor(&successor).await;
        Some(result)
    }

    async fn persist_first_open_bundled(
        &self,
        session: &Arc<ProcessorSession>,
        review_moment: &ProcessorReviewMoment,
        mut staged: StagedCommentPublication,
        emitter: &EventEmitter,
    ) -> Option<super::session::ReviewMomentCommentPublication> {
        let persist = self.first_open_persist.as_ref()?;
        let ReviewMomentCommentPublication::Published {
            comment,
            authoring_provenance,
        } = &staged.result
        else {
            return self
                .persist_first_open_sequential(session, review_moment, staged, emitter)
                .await;
        };
        let annotation = ReviewMomentAnnotation {
            moment_id: review_moment.moment_id().clone(),
            idempotency_key: staged.idempotency_key().clone(),
            comment: comment.clone(),
            authoring_provenance: authoring_provenance.as_ref().clone(),
            published_at: Utc::now(),
        };
        let Some(successor) = session.checkpoint_successor(Utc::now()).await else {
            self.evict_session(session).await;
            emitter.rejected(
                OperationKind::ReviewMomentOpen,
                CommandRejectionReason::UnknownSession,
                RejectionRecovery::StartNewReviewSession,
            );
            return None;
        };
        let mutation = match ReviewAnalysisMutation::try_new(
            session.address().clone(),
            session.owner.clone(),
            session.game_import().clone(),
            staged.checkpoint().clone(),
            Utc::now(),
            staged.quality_captures(),
        ) {
            Ok(mutation) => mutation,
            Err(error) => {
                tracing::error!(
                    category = "cache-entry-build",
                    reason = %error,
                    "first-open Review Moment analysis assembly failed"
                );
                emit_persistence_unavailable(
                    emitter,
                    OperationKind::ReviewMomentOpen,
                    session.address(),
                    Some(review_moment.moment_id()),
                );
                return None;
            }
        };
        let stored = match persist
            .persist(session.annotations().address(), annotation, mutation)
            .await
        {
            Ok(stored) => stored,
            Err(error) => {
                log_persistence_failure(OperationKind::ReviewMomentOpen, &error);
                self.evict_session(session).await;
                emit_persistence_unavailable(
                    emitter,
                    OperationKind::ReviewMomentOpen,
                    session.address(),
                    Some(review_moment.moment_id()),
                );
                return None;
            }
        };
        session.annotations().adopt(stored.clone()).await;
        staged.adopt_published_annotation(stored);
        let result = review_moment.apply_staged_comment_publication(staged).await;
        session.commit_checkpoint_successor(&successor).await;
        Some(result)
    }

    pub(super) async fn evict_session(&self, session: &Arc<ProcessorSession>) {
        let mut sessions = self.sessions.lock().await;
        if sessions
            .get(session.address())
            .is_some_and(|current| Arc::ptr_eq(current, session))
        {
            sessions.remove(session.address());
        }
    }
}

fn emit_terminal_progress(emitter: &EventEmitter, progress: Option<&OperationProgress>) {
    if let Some(stage) = progress {
        emitter.event(ReviewSessionEvent::Progress {
            stage: stage.clone(),
        });
    }
}

fn emit_persistence_unavailable(
    emitter: &EventEmitter,
    operation: OperationKind,
    game_import_id: &GameImportId,
    review_moment_id: Option<&CriticalMomentId>,
) {
    match review_moment_id {
        Some(review_moment_id) => emitter.review_moment_unavailable(
            game_import_id,
            review_moment_id,
            ProviderUnavailableReason::Persistence,
            RetryDirective::RetryAllowed,
        ),
        None => emitter.unavailable(
            operation,
            ProviderUnavailableReason::Persistence,
            RetryDirective::RetryAllowed,
        ),
    }
}

fn log_persistence_failure(operation: OperationKind, error: &ReviewAnalysisCacheError) {
    tracing::error!(
        category = error.diagnostic_category(),
        ?operation,
        "Review Session mutation persistence failed"
    );
}
