use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{
    engine_analysis::EngineAnalyzer,
    game_import_store::ReviewSessionGame,
    human_move_model::HumanMoveModel,
    language_layer_ledger::ReviewSessionSpend,
    quality_capture::HostedCaptureBuffer,
    review_analysis_cache::RestoredReviewSessionMoment,
    review_annotation_store::ReviewAnnotationLog,
    review_session_coaching::{
        AlternativeMoveAssessmentAuthor, AlternativeMoveCoaching, CoachTurnActivity,
    },
    review_session_contract::{EvidenceEntry, ReviewMomentCommentFacts},
    review_session_exploration::AlternativeMoveExploration,
};

use super::{
    intent_authoring::LazyIntentAuthoring, lifetime::ReviewSessionLifetime, AssembledSession,
    ProcessorReviewMoment, ProcessorReviewMomentEntry, ProcessorSession, SessionBuildError,
};

/// Ports and Language Layer bindings used to rebuild one Review Session.
pub(in crate::review_session_processor) struct SessionRestoreBindings {
    pub(in crate::review_session_processor) engine: Arc<dyn EngineAnalyzer>,
    pub(in crate::review_session_processor) human: Arc<dyn HumanMoveModel>,
    pub(in crate::review_session_processor) author: Arc<dyn AlternativeMoveAssessmentAuthor>,
    pub(in crate::review_session_processor) activity: Arc<CoachTurnActivity>,
    pub(in crate::review_session_processor) annotations: Arc<ReviewAnnotationLog>,
}

/// What one Review Session is built from: the Player, the frozen Game, and the
/// Review Moments the cache could answer for. Nothing here is a stored session.
pub(in crate::review_session_processor) struct RestoredReviewSession {
    pub(in crate::review_session_processor) owner:
        crate::review_session_processor::ProcessorPrincipal,
    pub(in crate::review_session_processor) game: ReviewSessionGame,
    pub(in crate::review_session_processor) moments: Vec<RestoredReviewSessionMoment>,
}

impl ProcessorSession {
    pub(in crate::review_session_processor) async fn restore(
        restored: RestoredReviewSession,
        bindings: SessionRestoreBindings,
    ) -> Result<Self, SessionBuildError> {
        // Coaching is ephemeral by design: an in-flight Coach Turn belongs to
        // the conversation that started it and never outlives this actor.
        let spend = Arc::new(ReviewSessionSpend::new());
        let captures = Arc::new(HostedCaptureBuffer::new());
        let coaching = Arc::new(AlternativeMoveCoaching::new(
            bindings.human.clone(),
            bindings.author,
            bindings.activity,
        ));
        let mut moments = Vec::with_capacity(restored.moments.len());
        for moment in restored.moments {
            moments.push(Arc::new(match moment {
                RestoredReviewSessionMoment::Pending { facts, core } => {
                    let decision_explanation = facts.decision_explanation.clone();
                    ProcessorReviewMomentEntry::pending(
                        *core,
                        Some(crate::game_import_store::ImportedCriticalMoment {
                            moment: facts,
                            engine_provenance: restored.game.engine_provenance.clone(),
                            decision_explanation,
                        }),
                    )
                }
                RestoredReviewSessionMoment::Prepared { facts, prepared } => {
                    let core = prepared.core.clone();
                    let runtime = Arc::new(
                        ProcessorReviewMoment::restore(
                            facts.clone(),
                            *prepared,
                            bindings.engine.clone(),
                            bindings.human.clone(),
                            bindings.annotations.clone(),
                        )
                        .await?,
                    );
                    ProcessorReviewMomentEntry::from_prepared(
                        core,
                        Some(crate::game_import_store::ImportedCriticalMoment {
                            decision_explanation: facts.decision_explanation.clone(),
                            moment: facts,
                            engine_provenance: restored.game.engine_provenance.clone(),
                        }),
                        runtime,
                    )
                }
            }));
        }
        Self::from_review_moments(AssembledSession {
            owner: restored.owner,
            game_import: Arc::new(restored.game),
            lifetime: ReviewSessionLifetime::new(chrono::Utc::now()),
            checkpoint_revision: 1,
            review_moments: moments,
            coaching,
            annotations: bindings.annotations,
            spend,
            captures,
        })
        .await
    }
}

impl ProcessorReviewMoment {
    async fn restore(
        facts: crate::review_session_contract::GameReviewCriticalMoment,
        restored: crate::review_analysis_cache::PreparedReviewSessionMoment,
        engine: Arc<dyn EngineAnalyzer>,
        human: Arc<dyn HumanMoveModel>,
        annotations: Arc<ReviewAnnotationLog>,
    ) -> Result<Self, SessionBuildError> {
        let root_engine = restored
            .core
            .evidence_packet
            .entries
            .iter()
            .find(|entry| {
                matches!(
                    entry,
                    EvidenceEntry::EngineAnalysis { position_ref, .. }
                        if position_ref == &restored.core.position_snapshot.position_ref
                )
            })
            .ok_or(SessionBuildError::MissingRootEngine)?;
        let exploration =
            AlternativeMoveExploration::new(restored.core.clone(), root_engine, engine.clone())?;
        exploration
            .restore_checkpoint(&restored.exploration)
            .await?;
        let automatic_decision_explanation = facts.decision_explanation.clone();
        let critical_moment = facts.clone();
        let comment_facts = ReviewMomentCommentFacts::try_from_presented_moment(facts)
            .map_err(|_| SessionBuildError::InvalidCommentFacts)?;
        let mut comment_publication = restored.comment_publication;
        super::comment_publication::seed_from_annotations(
            &mut comment_publication,
            &annotations,
            &restored.core.review_moment.moment_id,
        )
        .await;
        Ok(Self {
            core: restored.core,
            critical_moment: Some(critical_moment),
            automatic_decision_explanation,
            local_decision: restored.local_decision,
            exploration: Arc::new(exploration),
            intent_authoring: LazyIntentAuthoring::new(
                crate::projected_plan::ProjectedPlanBuilder::new(engine, human),
            ),
            idempotency_keys: Mutex::new(restored.idempotency_keys),
            comment_facts: Some(comment_facts),
            annotations,
            comment_publication: Mutex::new(comment_publication),
        })
    }
}
