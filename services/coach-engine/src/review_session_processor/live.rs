use std::{collections::btree_map::Entry, sync::Arc};

use crate::{lichess::LichessExportClient, review_session_contract::*};

use super::{
    coaching::CoachingTransition, events::EventEmitter, terminal::emit_coach_error, LiveOperation,
    ProcessorPrincipal, ReviewSessionProcessor,
};

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub(super) async fn cancel_operation(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: &GameImportId,
        operation_id: &OperationId,
        idempotency_key: &IdempotencyKey,
        emitter: Arc<EventEmitter>,
    ) {
        let handle = self.live.lock().await.get(operation_id).cloned();
        let Some(handle) = handle else {
            emitter.conflict(
                OperationKind::Cancellation,
                OperationConflictReason::IdempotencyKeyMismatch,
            );
            return;
        };
        if handle.owner() != principal {
            emitter.rejected(
                OperationKind::Cancellation,
                CommandRejectionReason::UnknownGameImport,
                RejectionRecovery::CorrectInput,
            );
            return;
        }
        if handle.game_import_id() != game_import_id || handle.idempotency_key() != idempotency_key
        {
            emitter.conflict(
                OperationKind::Cancellation,
                OperationConflictReason::IdempotencyKeyMismatch,
            );
            return;
        }
        match handle {
            LiveOperation::ReviewMomentPreparation { cancellation, .. } => {
                cancellation.cancel();
            }
            LiveOperation::AlternativeMove {
                review_moment_id,
                cancellation,
                ..
            } => {
                let Some(session) = self
                    .session(
                        game_import_id,
                        principal,
                        &emitter,
                        OperationKind::Cancellation,
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
                        OperationKind::Cancellation,
                    )
                    .await
                else {
                    return;
                };
                let _mutation = session.begin_mutation().await;
                let Some(staged) = review_moment
                    .stage_exploration_terminal(
                        operation_id,
                        idempotency_key,
                        crate::review_session_exploration::AlternativeMoveOperationOutcome::Cancelled,
                    )
                    .await
                else {
                    emitter.conflict(
                        OperationKind::Cancellation,
                        OperationConflictReason::IdempotencyKeyMismatch,
                    );
                    return;
                };
                if self
                    .commit_staged_exploration(
                        &session,
                        &review_moment,
                        staged,
                        OperationKind::Cancellation,
                        &emitter,
                    )
                    .await
                    .is_none()
                {
                    return;
                }
                cancellation.cancel();
            }
            LiveOperation::CoachTurn {
                coach_turn_id,
                coaching,
                idempotency_key,
                review_moment_id,
                ..
            } => {
                let Some(session) = self
                    .session(
                        game_import_id,
                        principal,
                        &emitter,
                        OperationKind::Cancellation,
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
                        OperationKind::Cancellation,
                    )
                    .await
                else {
                    return;
                };
                if let Err(error) = coaching.cancel(&coach_turn_id, &idempotency_key).await {
                    emit_coach_error(&emitter, error);
                    return;
                }
                let _mutation = session.begin_mutation().await;
                if !self
                    .persist_coaching_transition(
                        &session,
                        &review_moment,
                        CoachingTransition {
                            idempotency_key: None,
                            evidence_entries: Vec::new(),
                            operation: OperationKind::Cancellation,
                            quality_captures: Vec::new(),
                        },
                        &emitter,
                    )
                    .await
                {
                    self.evict_session(&session).await;
                    return;
                }
            }
            LiveOperation::HostTurn { cancellation, .. } => {
                cancellation.cancel();
            }
        }
        emitter.cancelled(OperationKind::Cancellation);
    }

    pub(super) async fn register_live(
        &self,
        operation_id: OperationId,
        handle: LiveOperation,
    ) -> bool {
        match self.live.lock().await.entry(operation_id) {
            Entry::Vacant(entry) => {
                entry.insert(handle);
                true
            }
            Entry::Occupied(_) => false,
        }
    }
}
