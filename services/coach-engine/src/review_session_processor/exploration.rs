use std::sync::Arc;

use crate::{
    lichess::LichessExportClient,
    review_session_contract::*,
    review_session_exploration::{
        AlternativeMoveAdmission, AlternativeMoveCancellation, AlternativeMoveCommit,
        AlternativeMoveExploration, AlternativeMoveOperationOutcome, ExploreAlternativeMoveError,
        ExploreAlternativeMoveRequest,
    },
};

use super::{
    admission::EngineWorkload,
    events::{EventEmitter, OperationProgressEmitter},
    session::ExplorationAdmissionStage,
    terminal::emit_exploration_error,
    LiveOperation, ReviewSessionProcessor,
};

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub(super) async fn explore_move(
        &self,
        principal: super::ProcessorPrincipal,
        operation_id: OperationId,
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        request: ExploreAlternativeMoveRequest,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(session) = self
            .session(
                &game_import_id,
                &principal,
                &emitter,
                OperationKind::AlternativeMoveEvaluation,
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
                OperationKind::AlternativeMoveEvaluation,
            )
            .await
        else {
            return;
        };
        let cancellation = AlternativeMoveCancellation::default();
        if !self
            .register_live(
                operation_id.clone(),
                LiveOperation::AlternativeMove {
                    owner: principal.clone(),
                    game_import_id: game_import_id.clone(),
                    review_moment_id,
                    idempotency_key: request.idempotency_key.clone(),
                    cancellation: cancellation.clone(),
                },
            )
            .await
        {
            emit_alternative_move_allowance(&review_moment.exploration, &emitter).await;
            emitter.rejected(
                OperationKind::AlternativeMoveEvaluation,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            );
            return;
        }

        let admitted = {
            let _mutation = session.begin_mutation().await;
            match review_moment
                .stage_exploration_admission(operation_id.clone(), request)
                .await
            {
                Ok(ExplorationAdmissionStage::Existing(commit)) => {
                    Some(AlternativeMoveAdmission::Existing(*commit))
                }
                Ok(ExplorationAdmissionStage::Mutation(staged)) => {
                    if staged.starts_evaluation()
                        && session.has_active_alternative_move_evaluation().await
                    {
                        emit_alternative_move_allowance(&review_moment.exploration, &emitter).await;
                        emit_exploration_error(
                            &emitter,
                            ExploreAlternativeMoveError::Conflict(
                                OperationConflictReason::AlternativeMoveEvaluationAlreadyActive,
                            ),
                        );
                        None
                    } else {
                        self.commit_staged_exploration(
                            &session,
                            &review_moment,
                            *staged,
                            OperationKind::AlternativeMoveEvaluation,
                            &emitter,
                        )
                        .await
                    }
                }
                Err(error) => {
                    emit_alternative_move_allowance(&review_moment.exploration, &emitter).await;
                    emit_exploration_error(&emitter, error);
                    None
                }
            }
        };
        let Some(admitted) = admitted else {
            self.live.lock().await.remove(&operation_id);
            return;
        };
        emitter.accepted(OperationKind::AlternativeMoveEvaluation);
        let prepared = match admitted {
            AlternativeMoveAdmission::Existing(commit)
            | AlternativeMoveAdmission::Completed(commit) => {
                self.live.lock().await.remove(&operation_id);
                emit_alternative_move_completion(&review_moment.exploration, &emitter, commit)
                    .await;
                return;
            }
            AlternativeMoveAdmission::Started(prepared) => prepared,
        };

        let progress = OperationProgressEmitter::new(
            emitter.clone(),
            OperationProgress::AlternativeMove {
                stage: AlternativeMoveProgressStage::WaitingForStockfish,
            },
        );
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(ExploreAlternativeMoveError::Cancelled),
            admission = progress.run(
                self.engine_admission
                    .acquire(EngineWorkload::Interactive, &principal)
            ) => {
                match admission {
                    Ok(_engine_lease) => {
                        progress.set(OperationProgress::AlternativeMove {
                            stage: AlternativeMoveProgressStage::EvaluatingMove,
                        });
                        progress
                            .run(review_moment.exploration.evaluate(&prepared, cancellation))
                            .await
                    }
                    Err(reason) => Err(ExploreAlternativeMoveError::Unavailable(reason)),
                }
            }
        };
        match result {
            Ok(draft) => {
                progress.set(OperationProgress::AlternativeMove {
                    stage: AlternativeMoveProgressStage::CommittingMove,
                });
                let committed = {
                    let _mutation = session.begin_mutation().await;
                    match review_moment
                        .stage_exploration_completion(&prepared, draft)
                        .await
                    {
                        Ok(staged) => {
                            self.commit_staged_exploration(
                                &session,
                                &review_moment,
                                staged,
                                OperationKind::AlternativeMoveEvaluation,
                                &emitter,
                            )
                            .await
                        }
                        Err(error) => {
                            let persisted = self
                                .commit_exploration_terminal(
                                    &session,
                                    &review_moment,
                                    &prepared,
                                    AlternativeMoveOperationOutcome::Interrupted,
                                    &emitter,
                                )
                                .await;
                            if persisted {
                                emit_alternative_move_allowance(
                                    &review_moment.exploration,
                                    &emitter,
                                )
                                .await;
                                emit_exploration_error(&emitter, error);
                            }
                            None
                        }
                    }
                };
                self.live.lock().await.remove(&operation_id);
                if let Some(commit) = committed {
                    emit_alternative_move_completion(&review_moment.exploration, &emitter, commit)
                        .await;
                } else {
                    self.evict_session(&session).await;
                }
            }
            Err(error) => {
                let terminal = match error {
                    ExploreAlternativeMoveError::Cancelled => {
                        AlternativeMoveOperationOutcome::Cancelled
                    }
                    _ => AlternativeMoveOperationOutcome::Interrupted,
                };
                let persisted = {
                    let _mutation = session.begin_mutation().await;
                    self.commit_exploration_terminal(
                        &session,
                        &review_moment,
                        &prepared,
                        terminal,
                        &emitter,
                    )
                    .await
                };
                self.live.lock().await.remove(&operation_id);
                if persisted {
                    emit_alternative_move_allowance(&review_moment.exploration, &emitter).await;
                    emit_exploration_error(&emitter, error);
                } else {
                    self.evict_session(&session).await;
                }
            }
        }
    }

    async fn commit_exploration_terminal(
        &self,
        session: &Arc<super::session::ProcessorSession>,
        review_moment: &super::session::ProcessorReviewMoment,
        prepared: &crate::review_session_exploration::PreparedMove,
        outcome: AlternativeMoveOperationOutcome,
        emitter: &EventEmitter,
    ) -> bool {
        match review_moment
            .stage_exploration_terminal(
                prepared.operation_id(),
                prepared.idempotency_key(),
                outcome,
            )
            .await
        {
            Some(staged) => self
                .commit_staged_exploration(
                    session,
                    review_moment,
                    staged,
                    OperationKind::AlternativeMoveEvaluation,
                    emitter,
                )
                .await
                .is_some(),
            None => true,
        }
    }
}

async fn emit_alternative_move_completion(
    exploration: &AlternativeMoveExploration,
    emitter: &EventEmitter,
    commit: AlternativeMoveCommit,
) {
    emit_alternative_move_allowance(exploration, emitter).await;
    emitter.completed(OperationCompletion::AlternativeMoveEvaluated {
        alternative_move: Box::new(commit.alternative_move),
    });
}

async fn emit_alternative_move_allowance(
    exploration: &AlternativeMoveExploration,
    emitter: &EventEmitter,
) {
    emitter.event(ReviewSessionEvent::Progress {
        stage: OperationProgress::AlternativeMoveAllowance {
            remaining: exploration.remaining_allowance().await,
        },
    });
}
