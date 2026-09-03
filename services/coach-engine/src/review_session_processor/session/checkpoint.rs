use std::collections::BTreeSet;

use crate::{
    review_analysis_cache::{CheckpointReviewSessionMoment, PreparedReviewSessionMoment},
    review_session_contract::{EvidenceEntry, IdempotencyKey, OperationId},
    review_session_exploration::{
        AlternativeMoveAdmission, AlternativeMoveCommit, AlternativeMoveOperationOutcome,
        CommitDraft, ExploreAlternativeMoveError, ExploreAlternativeMoveRequest, PreparedMove,
        StagedAlternativeMoveExploration,
    },
};

use super::{ProcessorReviewMoment, ProcessorReviewMomentEntry, ProcessorSession};

pub(in crate::review_session_processor) struct StagedEvidenceMutation {
    evidence_entries: Vec<EvidenceEntry>,
    idempotency_keys: BTreeSet<IdempotencyKey>,
    checkpoint: PreparedReviewSessionMoment,
}

pub(in crate::review_session_processor) enum EvidenceMutationStageError {
    IdempotencyKeyMismatch,
}

pub(in crate::review_session_processor) struct StagedExplorationMutation<T> {
    staged: StagedAlternativeMoveExploration<T>,
    idempotency_keys: BTreeSet<IdempotencyKey>,
    checkpoint: PreparedReviewSessionMoment,
}

pub(in crate::review_session_processor) enum ExplorationAdmissionStage {
    Existing(Box<AlternativeMoveCommit>),
    Mutation(Box<StagedExplorationMutation<AlternativeMoveAdmission>>),
}

impl StagedEvidenceMutation {
    pub(in crate::review_session_processor) fn checkpoint(&self) -> &PreparedReviewSessionMoment {
        &self.checkpoint
    }
}

impl<T> StagedExplorationMutation<T> {
    pub(in crate::review_session_processor) fn checkpoint(&self) -> &PreparedReviewSessionMoment {
        &self.checkpoint
    }
}

impl StagedExplorationMutation<AlternativeMoveAdmission> {
    pub(in crate::review_session_processor) fn starts_evaluation(&self) -> bool {
        matches!(self.staged.result, AlternativeMoveAdmission::Started(_))
    }
}

impl ProcessorSession {
    pub(in crate::review_session_processor) async fn prepared_checkpoint_moments(
        &self,
    ) -> Option<Vec<CheckpointReviewSessionMoment>> {
        self.prepared_checkpoint_moments_with(None).await
    }

    pub(in crate::review_session_processor) async fn prepared_checkpoint_moments_with(
        &self,
        replacement: Option<&PreparedReviewSessionMoment>,
    ) -> Option<Vec<CheckpointReviewSessionMoment>> {
        let moments = self.review_moments.lock().await;
        let mut prepared = Vec::with_capacity(moments.len() + usize::from(replacement.is_some()));
        let mut replaced = false;
        for moment in moments.values() {
            if let Some(replacement) = replacement {
                if replacement.core.review_moment.moment_id == *moment.moment_id() {
                    prepared.push(CheckpointReviewSessionMoment::Prepared(Box::new(
                        replacement.clone(),
                    )));
                    replaced = true;
                    continue;
                }
            }
            prepared.push(moment.checkpoint().await?);
        }
        if let Some(replacement) = replacement {
            if !replaced {
                prepared.push(CheckpointReviewSessionMoment::Prepared(Box::new(
                    replacement.clone(),
                )));
            }
        }
        prepared.sort_by_key(|moment| match moment {
            CheckpointReviewSessionMoment::Pending { core } => core.review_moment.ply,
            CheckpointReviewSessionMoment::Prepared(prepared) => prepared.core.review_moment.ply,
        });
        Some(prepared)
    }
}

impl ProcessorReviewMomentEntry {
    async fn checkpoint(&self) -> Option<CheckpointReviewSessionMoment> {
        match self.prepared_moment().await {
            Some(prepared) => Some(CheckpointReviewSessionMoment::Prepared(Box::new(
                prepared.prepared_checkpoint().await?,
            ))),
            None => Some(CheckpointReviewSessionMoment::Pending {
                core: Box::new(self.core.clone()),
            }),
        }
    }
}

impl ProcessorReviewMoment {
    pub(in crate::review_session_processor) async fn prepared_checkpoint(
        &self,
    ) -> Option<PreparedReviewSessionMoment> {
        Some(PreparedReviewSessionMoment {
            core: self.core_snapshot().await,
            local_decision: self.local_decision.clone(),
            idempotency_keys: self.idempotency_keys.lock().await.clone(),
            exploration: self.exploration.checkpoint().await,
            comment_publication: self.comment_publication.lock().await.clone(),
        })
    }

    pub(in crate::review_session_processor) async fn stage_evidence(
        &self,
        idempotency_key: Option<IdempotencyKey>,
        evidence_entries: Vec<EvidenceEntry>,
    ) -> Result<StagedEvidenceMutation, EvidenceMutationStageError> {
        let packet = self.exploration.current_state().await.evidence_packet;
        let mut idempotency_keys = self.idempotency_keys.lock().await.clone();
        if idempotency_key
            .as_ref()
            .is_some_and(|key| idempotency_keys.contains(key))
        {
            return Err(EvidenceMutationStageError::IdempotencyKeyMismatch);
        }

        if let Some(idempotency_key) = idempotency_key {
            idempotency_keys.insert(idempotency_key);
        }
        let checkpoint = PreparedReviewSessionMoment {
            core: self
                .core_with_packet(packet.appended(evidence_entries.clone()))
                .await,
            local_decision: self.local_decision.clone(),
            idempotency_keys: idempotency_keys.clone(),
            exploration: self.exploration.checkpoint().await,
            comment_publication: self.comment_publication.lock().await.clone(),
        };
        Ok(StagedEvidenceMutation {
            evidence_entries,
            idempotency_keys,
            checkpoint,
        })
    }

    pub(in crate::review_session_processor) async fn apply_staged_evidence(
        &self,
        staged: StagedEvidenceMutation,
    ) {
        self.exploration
            .append_evidence_entries(staged.evidence_entries)
            .await;
        *self.idempotency_keys.lock().await = staged.idempotency_keys;
    }

    pub(in crate::review_session_processor) async fn stage_exploration_admission(
        &self,
        operation_id: OperationId,
        request: ExploreAlternativeMoveRequest,
    ) -> Result<ExplorationAdmissionStage, ExploreAlternativeMoveError> {
        let key = request.idempotency_key.clone();
        let staged = self
            .exploration
            .stage_admission(operation_id, request)
            .await?;
        if let AlternativeMoveAdmission::Existing(commit) = &staged.result {
            return Ok(ExplorationAdmissionStage::Existing(Box::new(
                commit.clone(),
            )));
        }
        let mut idempotency_keys = self.idempotency_keys.lock().await.clone();
        if !idempotency_keys.insert(key) {
            return Err(ExploreAlternativeMoveError::Conflict(
                crate::review_session_contract::OperationConflictReason::IdempotencyKeyMismatch,
            ));
        }
        Ok(ExplorationAdmissionStage::Mutation(Box::new(
            self.wrap_staged_exploration(staged, idempotency_keys)
                .await
                .ok_or_else(persistence_exploration_error)?,
        )))
    }

    pub(in crate::review_session_processor) async fn stage_exploration_completion(
        &self,
        prepared: &PreparedMove,
        draft: CommitDraft,
    ) -> Result<StagedExplorationMutation<AlternativeMoveCommit>, ExploreAlternativeMoveError> {
        let staged = self.exploration.stage_completion(prepared, draft).await?;
        self.wrap_staged_exploration(staged, self.idempotency_keys.lock().await.clone())
            .await
            .ok_or_else(persistence_exploration_error)
    }

    pub(in crate::review_session_processor) async fn stage_exploration_terminal(
        &self,
        operation_id: &OperationId,
        key: &IdempotencyKey,
        outcome: AlternativeMoveOperationOutcome,
    ) -> Option<StagedExplorationMutation<()>> {
        let staged = self
            .exploration
            .stage_terminal(operation_id, key, outcome)
            .await?;
        self.wrap_staged_exploration(staged, self.idempotency_keys.lock().await.clone())
            .await
    }

    pub(in crate::review_session_processor) async fn apply_staged_exploration<T>(
        &self,
        staged: StagedExplorationMutation<T>,
    ) -> T {
        *self.idempotency_keys.lock().await = staged.idempotency_keys;
        self.exploration.apply_staged(staged.staged).await
    }

    async fn wrap_staged_exploration<T>(
        &self,
        staged: StagedAlternativeMoveExploration<T>,
        idempotency_keys: BTreeSet<IdempotencyKey>,
    ) -> Option<StagedExplorationMutation<T>> {
        let checkpoint = PreparedReviewSessionMoment {
            core: self
                .core_with_packet(staged.evidence_packet().clone())
                .await,
            local_decision: self.local_decision.clone(),
            idempotency_keys: idempotency_keys.clone(),
            exploration: staged.checkpoint().clone(),
            comment_publication: self.comment_publication.lock().await.clone(),
        };
        Some(StagedExplorationMutation {
            staged,
            idempotency_keys,
            checkpoint,
        })
    }
}

fn persistence_exploration_error() -> ExploreAlternativeMoveError {
    ExploreAlternativeMoveError::Unavailable(
        crate::review_session_contract::ProviderUnavailableReason::Persistence,
    )
}
