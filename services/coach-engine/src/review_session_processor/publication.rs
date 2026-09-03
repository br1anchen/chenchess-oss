use std::sync::Arc;

use crate::{lichess::LichessExportClient, review_session_contract::*};

use super::{
    events::EventEmitter,
    session::{
        CommentPublicationStage, CommentPublicationStageError, ReviewMomentCommentPublication,
    },
    ProcessorPrincipal, ReviewSessionProcessor,
};

pub(super) struct ReviewMomentCommentPublicationInput {
    pub(super) game_import_id: GameImportId,
    pub(super) review_moment_id: CriticalMomentId,
    pub(super) text: String,
    pub(super) grounding_ledger: CriticalMomentGroundingLedger,
    pub(super) idempotency_key: IdempotencyKey,
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub(super) async fn publish_review_moment_comment(
        &self,
        principal: &ProcessorPrincipal,
        input: ReviewMomentCommentPublicationInput,
        emitter: Arc<EventEmitter>,
    ) {
        let ReviewMomentCommentPublicationInput {
            game_import_id,
            review_moment_id,
            text,
            grounding_ledger,
            idempotency_key,
        } = input;
        let operation = OperationKind::ReviewMomentCommentPublication;
        let Some(session) = self
            .session(&game_import_id, principal, &emitter, operation)
            .await
        else {
            return;
        };
        let Some(review_moment) = self
            .review_moment(&session, &review_moment_id, &emitter, operation)
            .await
        else {
            return;
        };
        let publication = {
            let _mutation = session.begin_mutation().await;
            match review_moment
                .stage_comment_publication(idempotency_key, text, grounding_ledger)
                .await
            {
                Ok(CommentPublicationStage::Existing(publication)) => Some(publication),
                Ok(CommentPublicationStage::Mutation(staged)) => {
                    self.commit_staged_comment_publication(
                        &session,
                        &review_moment,
                        *staged,
                        &emitter,
                    )
                    .await
                }
                Err(CommentPublicationStageError::MissingAuthority) => {
                    emitter.rejected(
                        operation,
                        CommandRejectionReason::MissingEvidence,
                        RejectionRecovery::StartNewReviewSession,
                    );
                    None
                }
                Err(CommentPublicationStageError::InvalidCommand) => {
                    emitter.rejected(
                        operation,
                        CommandRejectionReason::InvalidCommand,
                        RejectionRecovery::CorrectInput,
                    );
                    None
                }
            }
        };
        match publication {
            Some(ReviewMomentCommentPublication::Published {
                comment,
                authoring_provenance: _,
            }) => {
                emitter.completed(OperationCompletion::ReviewMomentCommentPublished {
                    comment: Box::new(comment),
                });
            }
            Some(ReviewMomentCommentPublication::RetryRejected) => {
                emitter.rejected(
                    operation,
                    CommandRejectionReason::InvalidCommand,
                    RejectionRecovery::CorrectInput,
                );
            }
            None => {}
        }
    }
}
