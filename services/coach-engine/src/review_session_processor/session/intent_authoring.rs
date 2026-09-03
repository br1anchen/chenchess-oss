use tokio::sync::Mutex;

use crate::{
    critical_moment_comment::intent_authoring_context_for,
    projected_plan::{ProjectedPlanBuilder, ProjectedPlanProvenance},
    review_session_contract::{
        Color, CriticalMomentIntentAuthoringContext, IntentEnrichment, ReviewMomentCommentFacts,
        ReviewSessionCoreContract, ReviewSide,
    },
};

pub(super) struct LazyIntentAuthoring {
    projected_plan: ProjectedPlanBuilder,
    /// `None` until the first `prepare`; afterwards the one context every later
    /// caller reuses, publication included.
    ///
    /// Publication does not consume it. The Review Moment's facts are immutable,
    /// so a second logical write grounds against exactly the enrichment the
    /// first one did instead of falling back to facts alone.
    state: Mutex<Option<PreparedIntentAuthoring>>,
}

#[derive(Clone)]
struct PreparedIntentAuthoring {
    context: Option<CriticalMomentIntentAuthoringContext>,
    provenance: Option<ProjectedPlanProvenance>,
}

impl LazyIntentAuthoring {
    pub(super) fn new(projected_plan: ProjectedPlanBuilder) -> Self {
        Self {
            projected_plan,
            state: Mutex::new(None),
        }
    }

    /// Prepares one reusable authoring context. The lock intentionally makes
    /// concurrent opens share one provider request. Provider failure returns a
    /// facts-only fallback without caching it, so a later open may retry.
    pub(super) async fn prepare(
        &self,
        facts: &ReviewMomentCommentFacts,
        core: &ReviewSessionCoreContract,
    ) -> Option<CriticalMomentIntentAuthoringContext> {
        let mut state = self.state.lock().await;
        if let Some(prepared) = &*state {
            return prepared.context.clone();
        }
        if !intent_applies(facts, core) {
            *state = Some(PreparedIntentAuthoring {
                context: None,
                provenance: None,
            });
            return None;
        }

        match self.projected_plan.build(core).await {
            Ok(projected_plan) => {
                let context = intent_authoring_context_for(
                    facts,
                    Some(IntentEnrichment {
                        projected_plan_san: projected_plan.projected_plan_san().to_vec(),
                        objective_counterplay_san: projected_plan
                            .objective_counterplay_san()
                            .to_vec(),
                    }),
                );
                *state = Some(PreparedIntentAuthoring {
                    context: context.clone(),
                    provenance: projected_plan.provenance().cloned(),
                });
                context
            }
            Err(_) => intent_authoring_context_for(facts, None),
        }
    }

    pub(super) async fn provenance(&self) -> Option<ProjectedPlanProvenance> {
        self.state
            .lock()
            .await
            .as_ref()
            .and_then(|prepared| prepared.provenance.clone())
    }
}

fn intent_applies(facts: &ReviewMomentCommentFacts, core: &ReviewSessionCoreContract) -> bool {
    !matches!(facts, ReviewMomentCommentFacts::Neutral { .. })
        && matches!(
            (
                core.imported_game.review_side,
                core.coach_turn_context.reviewed_move.side,
            ),
            (ReviewSide::White, Color::White)
                | (ReviewSide::Black, Color::Black)
                | (ReviewSide::Both, _)
        )
}
