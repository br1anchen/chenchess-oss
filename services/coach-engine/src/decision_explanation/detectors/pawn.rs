use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, CurriculumLearningConcept as Concept, PieceRole,
    SemanticOutcomeData,
};

use super::{facts_for_step_and_snapshot, outcomes_for_step, proof, DetectedConcept};
use crate::decision_explanation::candidate::ReplayedCandidate;

pub(super) fn detect(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = detect_promotion(candidate, facts);
    detected.extend(detect_advanced_pawn(candidate, facts));
    detected
}

fn detect_promotion(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = Vec::new();
    for (index, step) in candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .filter(|(_, step)| step.promotion.is_some())
    {
        let supporting_fact_refs = facts
            .iter()
            .filter(|fact| {
                matches!(
                    &fact.data,
                    AtomicChessFactData::PiecePromoted { step_ref, .. }
                        | AtomicChessFactData::PieceMoved { step_ref, .. }
                        | AtomicChessFactData::MaterialChanged { step_ref, .. }
                        if step_ref == &step.step_ref
                )
            })
            .map(|fact| fact.fact_ref.clone())
            .collect::<Vec<_>>();
        let outcome_refs = outcomes_for_step(
            &candidate.contract.outcomes,
            facts,
            &step.step_ref,
            |data| {
                matches!(
                    data,
                    SemanticOutcomeData::PawnProgressed {
                        promotion_role: Some(_),
                        ..
                    } | SemanticOutcomeData::MaterialConfigurationChanged { .. }
                )
            },
        );
        let concepts: &[Concept] = if step.promotion == Some(PieceRole::Queen) {
            &[Concept::Promotion]
        } else {
            &[Concept::Underpromotion, Concept::Promotion]
        };
        for concept in concepts {
            if let Some(proof) = proof(
                candidate,
                *concept,
                index,
                index,
                supporting_fact_refs.clone(),
                outcome_refs.clone(),
            ) {
                detected.push(proof);
            }
        }
    }
    detected
}

fn detect_advanced_pawn(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            step.role == PieceRole::Pawn
                && step.promotion.is_none()
                && step
                    .to_square
                    .as_str()
                    .as_bytes()
                    .get(1)
                    .is_some_and(|rank| {
                        (step.mover == crate::review_session_contract::Color::White
                            && *rank == b'7')
                            || (step.mover == crate::review_session_contract::Color::Black
                                && *rank == b'2')
                    })
        })
        .filter_map(|(index, step)| {
            let supporting_fact_refs = facts_for_step_and_snapshot(
                facts,
                &step.step_ref,
                &step.after_snapshot_ref,
                |data| {
                    matches!(
                        data,
                        AtomicChessFactData::PieceMoved { .. }
                            | AtomicChessFactData::PawnFrontSpanOccupancy { .. }
                    )
                },
            );
            let outcome_refs = outcomes_for_step(
                &candidate.contract.outcomes,
                facts,
                &step.step_ref,
                |data| matches!(data, SemanticOutcomeData::PawnProgressed { .. }),
            );
            proof(
                candidate,
                Concept::AdvancedPawn,
                index,
                index,
                supporting_fact_refs,
                outcome_refs,
            )
        })
        .collect()
}
