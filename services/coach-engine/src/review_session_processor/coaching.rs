use std::sync::Arc;

use crate::{
    lichess::LichessExportClient,
    review_session_coaching::{
        ground_coach_turn_publication, AdmittedTurn, CoachTurnExecution, CoachTurnReplay,
        CoachTurnResult, CoachTurnTargetSelection, PreparedAssessmentPublication,
        PreparedCoachTurnTarget, StartAlternativeMoveCoachTurn,
    },
    review_session_contract::*,
};

use super::{
    events::{EventEmitter, OperationProgressEmitter},
    session::EvidenceMutationStageError,
    terminal::emit_coach_error,
    LiveOperation, ProcessorPrincipal, ReviewSessionProcessor,
};

pub(super) struct CoachingTransition {
    pub(super) idempotency_key: Option<IdempotencyKey>,
    pub(super) evidence_entries: Vec<EvidenceEntry>,
    pub(super) operation: OperationKind,
    pub(super) quality_captures: Vec<crate::quality_capture::QualityCaptureDraft>,
}

struct CoachTurnAdmission {
    operation_id: OperationId,
    coach_turn_id: CoachTurnId,
    message: String,
    idempotency_key: IdempotencyKey,
    prior_turn: PriorCoachTurn,
    target: PreparedCoachTurnTarget,
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn start_coach_turn(
        &self,
        principal: ProcessorPrincipal,
        surface: DeliverySurface,
        operation_id: OperationId,
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        coach_turn_id: CoachTurnId,
        context: CoachTurnContext,
        message: String,
        idempotency_key: IdempotencyKey,
        prior_turn: PriorCoachTurn,
        emitter: Arc<EventEmitter>,
    ) {
        if matches!(surface, DeliverySurface::Web) {
            emitter.rejected(
                OperationKind::CoachTurn,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            );
            return;
        }
        let Some(session) = self
            .session(
                &game_import_id,
                &principal,
                &emitter,
                OperationKind::CoachTurn,
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
                OperationKind::CoachTurn,
            )
            .await
        else {
            return;
        };
        let target = match review_moment.target_for_context(&context).await {
            Ok(target) => target,
            Err(_) => {
                emitter.rejected(
                    OperationKind::CoachTurn,
                    CommandRejectionReason::UnknownTarget,
                    RejectionRecovery::CorrectInput,
                );
                return;
            }
        };
        let terminal_authority = (coach_turn_id.clone(), idempotency_key.clone());
        let start = StartAlternativeMoveCoachTurn {
            coach_turn_id: coach_turn_id.clone(),
            message: message.clone(),
            idempotency_key: idempotency_key.clone(),
            prior_turn: prior_turn.clone(),
            target: CoachTurnTargetSelection::Explicit(Box::new(target.clone())),
        };
        match session
            .coaching
            .replay_for_operation(&operation_id, &start)
            .await
        {
            Ok(Some(replay)) => {
                emitter.accepted(OperationKind::CoachTurn);
                match replay {
                    CoachTurnReplay::Completed(commit) => {
                        emitter.completed(OperationCompletion::CoachTurnCompleted {
                            assessment: Box::new(commit.assessment),
                        });
                    }
                    CoachTurnReplay::Prepared(preparation) => {
                        emitter.completed(OperationCompletion::CoachTurnPrepared {
                            facts: Box::new(preparation.facts),
                        });
                    }
                }
                return;
            }
            Ok(None) => {}
            Err(error) => {
                emit_coach_error(&emitter, error);
                return;
            }
        }
        if !self
            .register_live(
                operation_id.clone(),
                LiveOperation::CoachTurn {
                    owner: principal.clone(),
                    game_import_id: game_import_id.clone(),
                    review_moment_id: review_moment_id.clone(),
                    idempotency_key: idempotency_key.clone(),
                    coach_turn_id: coach_turn_id.clone(),
                    coaching: session.coaching.clone(),
                },
            )
            .await
        {
            emitter.rejected(
                OperationKind::CoachTurn,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            );
            return;
        }
        let Some(admitted) = self
            .admit_durable_coach_turn(
                &session,
                &review_moment,
                CoachTurnAdmission {
                    operation_id: operation_id.clone(),
                    coach_turn_id,
                    message,
                    idempotency_key,
                    prior_turn,
                    target: match start.target {
                        CoachTurnTargetSelection::Explicit(target) => *target,
                        CoachTurnTargetSelection::Preserve => {
                            unreachable!("new Coach Turn replay target is explicit")
                        }
                    },
                },
                &emitter,
            )
            .await
        else {
            return;
        };
        emitter.accepted(OperationKind::CoachTurn);
        let progress = OperationProgressEmitter::new(
            emitter.clone(),
            OperationProgress::CoachTurn {
                stage: CoachTurnProgressStage::Queued,
            },
        );
        let execution = CoachTurnExecution::PrepareForAuthor;
        let admitted_progress = progress.clone();
        let admission = async {
            let permit = self.coach_admission.acquire(&principal).await?;
            admitted_progress.set(OperationProgress::CoachTurn {
                stage: CoachTurnProgressStage::GeneratingResponse,
            });
            Ok(permit)
        };
        let result = progress
            .run(
                session
                    .coaching
                    .execute_admitted(admitted, execution, admission),
            )
            .await;
        let evidence_entries = match &result {
            Ok(CoachTurnResult::Completed(commit)) => commit.evidence_entries.clone(),
            Ok(CoachTurnResult::Prepared(_)) | Err(_) => Vec::new(),
        };
        let terminal_persisted = {
            let _mutation = session.begin_mutation().await;
            let active = session.coaching.current_state().await.active_turn;
            if active.as_ref().is_some_and(|active| {
                active.coach_turn_id == terminal_authority.0
                    && active.idempotency_key == terminal_authority.1
            }) {
                self.live.lock().await.remove(&operation_id);
                emitter.unavailable(
                    OperationKind::CoachTurn,
                    ProviderUnavailableReason::Persistence,
                    RetryDirective::RetryAllowed,
                );
                return;
            }
            if active.is_some()
                && matches!(
                    &result,
                    Err(crate::review_session_coaching::AlternativeMoveCoachTurnError::Cancelled)
                )
            {
                true
            } else {
                self.persist_coaching_transition(
                    &session,
                    &review_moment,
                    CoachingTransition {
                        idempotency_key: None,
                        evidence_entries,
                        operation: OperationKind::CoachTurn,
                        quality_captures: session.captures.take(),
                    },
                    &emitter,
                )
                .await
            }
        };
        self.live.lock().await.remove(&operation_id);
        if !terminal_persisted {
            self.evict_session(&session).await;
            return;
        }
        match result {
            Ok(CoachTurnResult::Completed(commit)) => {
                emitter.completed(OperationCompletion::CoachTurnCompleted {
                    assessment: Box::new(commit.assessment),
                });
            }
            Ok(CoachTurnResult::Prepared(preparation)) => {
                emitter.completed(OperationCompletion::CoachTurnPrepared {
                    facts: Box::new(preparation.facts),
                });
            }
            Err(error) => emit_coach_error(&emitter, error),
        }
    }

    async fn admit_durable_coach_turn(
        &self,
        session: &Arc<super::session::ProcessorSession>,
        review_moment: &super::session::ProcessorReviewMoment,
        admission: CoachTurnAdmission,
        emitter: &EventEmitter,
    ) -> Option<AdmittedTurn> {
        let CoachTurnAdmission {
            operation_id,
            coach_turn_id,
            message,
            idempotency_key,
            prior_turn,
            target,
        } = admission;
        let _admission = session.coaching.lock_admission().await;
        let active = session.coaching.current_state().await.active_turn;
        let selection = match &prior_turn {
            PriorCoachTurn::Steers { .. }
                if active.as_ref().is_some_and(|active| {
                    active.alternative_move_id == target.target().alternative_move_id
                }) =>
            {
                CoachTurnTargetSelection::Preserve
            }
            PriorCoachTurn::RetriesUnavailable { .. } => CoachTurnTargetSelection::Preserve,
            PriorCoachTurn::None | PriorCoachTurn::Steers { .. } => {
                CoachTurnTargetSelection::Explicit(Box::new(target))
            }
        };
        let _mutation = session.begin_mutation().await;
        let admitted = match session
            .coaching
            .admit_for_operation(
                StartAlternativeMoveCoachTurn {
                    coach_turn_id,
                    message,
                    idempotency_key: idempotency_key.clone(),
                    prior_turn,
                    target: selection,
                },
                operation_id.clone(),
            )
            .await
        {
            Ok(admitted) => admitted,
            Err(error) => {
                self.live.lock().await.remove(&operation_id);
                emit_coach_error(emitter, error);
                return None;
            }
        };
        if !self
            .persist_coaching_transition(
                session,
                review_moment,
                CoachingTransition {
                    idempotency_key: Some(idempotency_key),
                    evidence_entries: Vec::new(),
                    operation: OperationKind::CoachTurn,
                    quality_captures: Vec::new(),
                },
                emitter,
            )
            .await
        {
            session
                .coaching
                .discard_unpersisted_admission(&admitted)
                .await;
            self.evict_session(session).await;
            self.live.lock().await.remove(&operation_id);
            return None;
        }
        Some(admitted)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn publish_coach_turn(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        coach_turn_id: CoachTurnId,
        assessment: AlternativeMoveAssessment,
        idempotency_key: IdempotencyKey,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(session) = self
            .session(
                &game_import_id,
                principal,
                &emitter,
                OperationKind::CoachTurn,
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
                OperationKind::CoachTurn,
            )
            .await
        else {
            return;
        };
        emitter.event(ReviewSessionEvent::Progress {
            stage: OperationProgress::CoachTurn {
                stage: CoachTurnProgressStage::ValidatingResponse,
            },
        });
        let _mutation = session.begin_mutation().await;
        if let Some(existing) = session
            .coaching
            .published_turn(&coach_turn_id, &idempotency_key)
            .await
        {
            // A replay is answered with what was published, not with what was
            // resubmitted. Prose equality stopped being the test when the gate
            // began substituting markers: the host holds the marker form it
            // wrote, the record holds the substituted form the Player reads,
            // and those never compare equal. The target still has to agree —
            // one key naming a different Alternative Move is a real conflict,
            // not a retry.
            if existing.assessment.alternative_move_id == assessment.alternative_move_id {
                emitter.completed(OperationCompletion::CoachTurnCompleted {
                    assessment: Box::new(existing.assessment),
                });
            } else {
                log_invalid_coach_turn_publication("published-assessment-mismatch");
                emitter.rejected(
                    OperationKind::CoachTurn,
                    CommandRejectionReason::InvalidCommand,
                    RejectionRecovery::CorrectInput,
                );
            }
            return;
        }
        let target = match review_moment.target_for_assessment(&assessment).await {
            Ok(value) => value,
            Err(_) => {
                emitter.rejected(
                    OperationKind::CoachTurn,
                    CommandRejectionReason::UnknownTarget,
                    RejectionRecovery::CorrectInput,
                );
                return;
            }
        };
        let Some(preparation) = session.coaching.prepared_turn(&coach_turn_id).await else {
            log_invalid_coach_turn_publication("missing-preparation");
            emitter.rejected(
                OperationKind::CoachTurn,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            );
            return;
        };
        let Some(operation_key) = session
            .coaching
            .prepared_operation_key(&coach_turn_id)
            .await
        else {
            log_invalid_coach_turn_publication("missing-operation-key");
            emitter.rejected(
                OperationKind::CoachTurn,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            );
            return;
        };
        // Grounding is also substitution, so the published assessment is the
        // one it returns — never the marker form the host submitted.
        let Ok(assessment) = ground_coach_turn_publication(
            &coach_turn_id,
            &target,
            &assessment,
            &preparation.facts.evidence_packet,
        ) else {
            log_invalid_coach_turn_publication("invalid-assessment-grounding");
            emitter.rejected(
                OperationKind::CoachTurn,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            );
            return;
        };
        let evidence_entries = preparation.evidence_entries;
        let commit = crate::review_session_coaching::CoachTurnCommit {
            assessment: assessment.clone(),
            evidence_entries: evidence_entries.clone(),
        };
        let publication = match session
            .coaching
            .record_prepared_assessment(&idempotency_key, commit)
            .await
        {
            Ok(publication) => publication,
            Err(error) => {
                log_invalid_coach_turn_publication("record-prepared-assessment");
                emit_coach_error(&emitter, error);
                return;
            }
        };
        if matches!(publication, PreparedAssessmentPublication::Published)
            && !self
                .persist_coaching_transition(
                    &session,
                    &review_moment,
                    CoachingTransition {
                        idempotency_key: (idempotency_key != operation_key)
                            .then_some(idempotency_key),
                        evidence_entries,
                        operation: OperationKind::CoachTurn,
                        quality_captures: session.captures.take(),
                    },
                    &emitter,
                )
                .await
        {
            self.evict_session(&session).await;
            return;
        }
        emitter.completed(OperationCompletion::CoachTurnCompleted {
            assessment: Box::new(assessment),
        });
    }

    pub(super) async fn persist_coaching_transition(
        &self,
        session: &Arc<super::session::ProcessorSession>,
        review_moment: &super::session::ProcessorReviewMoment,
        transition: CoachingTransition,
        emitter: &EventEmitter,
    ) -> bool {
        let CoachingTransition {
            idempotency_key,
            evidence_entries,
            operation,
            quality_captures,
        } = transition;
        let staged = match review_moment
            .stage_evidence(idempotency_key, evidence_entries)
            .await
        {
            Ok(staged) => staged,
            Err(EvidenceMutationStageError::IdempotencyKeyMismatch) => {
                emitter.conflict(operation, OperationConflictReason::IdempotencyKeyMismatch);
                return false;
            }
        };
        self.commit_staged_evidence_with_capture(
            session,
            review_moment,
            staged,
            operation,
            quality_captures,
            emitter,
        )
        .await
    }
}

fn log_invalid_coach_turn_publication(category: &'static str) {
    tracing::error!(category, "Coach Turn publication was rejected as invalid");
}
