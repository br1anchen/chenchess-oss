use std::sync::Arc;

use crate::{
    lichess::LichessExportClient,
    player_plan_evaluation,
    review_session_contract::{
        CommandRejectionReason, CriticalMomentId, GameImportId, OperationCompletion, OperationKind,
        PlayerPlanEvaluationRequest, ProviderUnavailableReason, RejectionRecovery, RetryDirective,
    },
};

use super::{events::EventEmitter, ProcessorPrincipal, ReviewSessionProcessor};

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub(super) async fn evaluate_player_plan(
        &self,
        principal: &ProcessorPrincipal,
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        request: PlayerPlanEvaluationRequest,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(session) = self
            .session(
                &game_import_id,
                principal,
                &emitter,
                OperationKind::PlayerPlanEvaluation,
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
                OperationKind::PlayerPlanEvaluation,
            )
            .await
        else {
            return;
        };
        let Some(context) = review_moment.player_plan_evaluation_context().await else {
            emitter.unavailable(
                OperationKind::PlayerPlanEvaluation,
                ProviderUnavailableReason::StockfishProcess,
                RetryDirective::RetryAllowed,
            );
            return;
        };

        match request {
            PlayerPlanEvaluationRequest::Prepare => {
                emitter.completed(OperationCompletion::PlayerPlanEvaluationPrepared {
                    context: Box::new(context),
                });
            }
            PlayerPlanEvaluationRequest::Admit { draft } => {
                let Some(evaluation) = player_plan_evaluation::admit(&context, draft) else {
                    emitter.rejected(
                        OperationKind::PlayerPlanEvaluation,
                        CommandRejectionReason::InvalidCommand,
                        RejectionRecovery::CorrectInput,
                    );
                    return;
                };
                emitter.completed(OperationCompletion::PlayerPlanEvaluated { evaluation });
            }
        }
    }
}
