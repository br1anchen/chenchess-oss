use crate::review_session_contract::{
    AtomicChessFact, CurriculumLearningConcept as Concept, DecisionTerminalState,
    SemanticOutcomeData,
};

use super::{outcome_step_index, proof, DetectedConcept};
use crate::decision_explanation::candidate::ReplayedCandidate;

pub(super) fn detect(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    candidate
        .contract
        .outcomes
        .iter()
        .filter_map(|outcome| {
            let concept = match &outcome.data {
                SemanticOutcomeData::TerminalStateReached {
                    result: DecisionTerminalState::Stalemate | DecisionTerminalState::Draw,
                    ..
                } => Concept::Equality,
                SemanticOutcomeData::MaterialBalanceChanged {
                    conventional_value_delta: 1..=2,
                    ..
                } => Concept::Advantage,
                SemanticOutcomeData::MaterialBalanceChanged {
                    conventional_value_delta: 3..,
                    ..
                } => Concept::CrushingAdvantage,
                _ => return None,
            };
            let payoff_index = outcome_step_index(candidate, facts, outcome)
                .unwrap_or_else(|| candidate.contract.line_steps.len().saturating_sub(1));
            proof(
                candidate,
                concept,
                payoff_index,
                payoff_index,
                outcome.supporting_fact_refs.clone(),
                vec![outcome.outcome_ref.clone()],
            )
        })
        .collect()
}
