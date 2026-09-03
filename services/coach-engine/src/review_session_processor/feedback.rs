use std::sync::Arc;

use crate::{
    learning_path_feedback::{resolve_sample, LearningPathFeedbackError, LearningPathSample},
    lichess::LichessExportClient,
    review_session_contract::*,
};

use super::{events::EventEmitter, ProcessorPrincipal, ReviewSessionProcessor};

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub(super) async fn record_learning_path_exposure(
        &self,
        principal: ProcessorPrincipal,
        surface: DeliverySurface,
        game_import_id: GameImportId,
        learning_path_ref: LearningPathRef,
        emitter: Arc<EventEmitter>,
    ) {
        let Some((player_id, sample)) = self
            .feedback_sample(&principal, &game_import_id, &learning_path_ref, &emitter)
            .await
        else {
            return;
        };
        match self
            .learning_path_feedback
            .record_exposure(&player_id, sample, surface)
            .await
        {
            Ok(feedback) => {
                emitter.completed(OperationCompletion::LearningPathFeedbackRecorded { feedback })
            }
            Err(error) => emit_feedback_error(&emitter, error),
        }
    }

    pub(super) async fn update_learning_path_vote(
        &self,
        principal: ProcessorPrincipal,
        surface: DeliverySurface,
        game_import_id: GameImportId,
        learning_path_ref: LearningPathRef,
        vote: Option<LearningPathVote>,
        emitter: Arc<EventEmitter>,
    ) {
        let Some((player_id, sample)) = self
            .feedback_sample(&principal, &game_import_id, &learning_path_ref, &emitter)
            .await
        else {
            return;
        };
        match self
            .learning_path_feedback
            .update_vote(&player_id, sample, surface, vote)
            .await
        {
            Ok(feedback) => {
                emitter.completed(OperationCompletion::LearningPathFeedbackRecorded { feedback })
            }
            Err(error) => emit_feedback_error(&emitter, error),
        }
    }

    async fn feedback_sample(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: &GameImportId,
        learning_path_ref: &LearningPathRef,
        emitter: &EventEmitter,
    ) -> Option<(PlayerId, LearningPathSample)> {
        let ProcessorPrincipal::Player(player_id) = principal else {
            emitter.rejected(
                OperationKind::LearningPathFeedback,
                CommandRejectionReason::AuthenticationRequired,
                RejectionRecovery::None,
            );
            return None;
        };
        let session = self
            .session(
                game_import_id,
                principal,
                emitter,
                OperationKind::LearningPathFeedback,
            )
            .await?;
        let Some(sample) = resolve_sample(session.game_import(), learning_path_ref) else {
            emitter.rejected(
                OperationKind::LearningPathFeedback,
                CommandRejectionReason::UnknownTarget,
                RejectionRecovery::CorrectInput,
            );
            return None;
        };
        Some((player_id.clone(), sample))
    }
}

fn emit_feedback_error(emitter: &EventEmitter, error: LearningPathFeedbackError) {
    match error {
        LearningPathFeedbackError::InvalidSample | LearningPathFeedbackError::ExposureRequired => {
            emitter.rejected(
                OperationKind::LearningPathFeedback,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            )
        }
        LearningPathFeedbackError::Unavailable => emitter.unavailable(
            OperationKind::LearningPathFeedback,
            ProviderUnavailableReason::Persistence,
            RetryDirective::RetryAllowed,
        ),
    }
}
