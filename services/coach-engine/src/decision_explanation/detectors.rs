mod attack;
mod line;
mod mate;
mod outcome;
mod pawn;
mod position;

use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, AtomicFactRef, CurriculumLearningConcept, LineStepRef,
    SemanticComparison, SemanticOutcome, SemanticOutcomeData, SemanticOutcomeRef,
};

use super::{
    candidate::{CandidateConstruction, ReplayedCandidate},
    DecisionExplanationContractError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DetectedConcept {
    pub(super) concept: CurriculumLearningConcept,
    pub(super) causal_step_ref: LineStepRef,
    pub(super) payoff_step_ref: LineStepRef,
    pub(super) supporting_fact_refs: Vec<AtomicFactRef>,
    pub(super) outcome_refs: Vec<SemanticOutcomeRef>,
    pub(super) semantic_comparisons: Vec<SemanticComparison>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum DetectorFamily {
    Mate,
    SpecificLine,
    Attack,
    GeneralLine,
    Pawn,
    Position,
    Outcome,
}

pub(super) const DETECTOR_FAMILIES: [DetectorFamily; 7] = [
    DetectorFamily::Mate,
    DetectorFamily::SpecificLine,
    DetectorFamily::Attack,
    DetectorFamily::GeneralLine,
    DetectorFamily::Pawn,
    DetectorFamily::Position,
    DetectorFamily::Outcome,
];

pub(super) fn detect_all(
    candidate: &ReplayedCandidate,
    construction: &CandidateConstruction,
) -> Result<Vec<DetectedConcept>, DecisionExplanationContractError> {
    detect_with_family_order(candidate, construction, &DETECTOR_FAMILIES)
}

pub(super) fn detect_with_family_order(
    candidate: &ReplayedCandidate,
    construction: &CandidateConstruction,
    families: &[DetectorFamily],
) -> Result<Vec<DetectedConcept>, DecisionExplanationContractError> {
    let facts = construction
        .facts
        .iter()
        .filter(|fact| candidate.contract.fact_refs.contains(&fact.fact_ref))
        .collect::<Vec<_>>();
    let mut detected = Vec::new();
    for family in families {
        match family {
            DetectorFamily::Mate => detected.extend(mate::detect(candidate, &facts)),
            DetectorFamily::SpecificLine => {
                detected.extend(line::detect_specific(candidate, &facts, construction));
            }
            DetectorFamily::Attack => detected.extend(attack::detect(candidate, &facts)?),
            DetectorFamily::GeneralLine => detected.extend(line::detect(candidate, &facts)),
            DetectorFamily::Pawn => detected.extend(pawn::detect(candidate, &facts)),
            DetectorFamily::Position => detected.extend(position::detect(candidate, &facts)),
            DetectorFamily::Outcome => detected.extend(outcome::detect(candidate, &facts)),
        }
    }
    Ok(detected)
}

fn proof(
    candidate: &ReplayedCandidate,
    concept: CurriculumLearningConcept,
    causal_index: usize,
    payoff_index: usize,
    mut supporting_fact_refs: Vec<AtomicFactRef>,
    mut outcome_refs: Vec<SemanticOutcomeRef>,
) -> Option<DetectedConcept> {
    if supporting_fact_refs.is_empty() || outcome_refs.is_empty() {
        return None;
    }
    supporting_fact_refs.sort();
    supporting_fact_refs.dedup();
    outcome_refs.sort();
    outcome_refs.dedup();
    Some(DetectedConcept {
        concept,
        causal_step_ref: candidate
            .contract
            .line_steps
            .get(causal_index)?
            .step_ref
            .clone(),
        payoff_step_ref: candidate
            .contract
            .line_steps
            .get(payoff_index)?
            .step_ref
            .clone(),
        supporting_fact_refs,
        outcome_refs,
        semantic_comparisons: Vec::new(),
    })
}

fn facts_for_step_and_snapshot(
    facts: &[&AtomicChessFact],
    step_ref: &LineStepRef,
    snapshot_ref: &crate::review_session_contract::DecisionPositionSnapshotRef,
    predicate: impl Fn(&AtomicChessFactData) -> bool,
) -> Vec<AtomicFactRef> {
    facts
        .iter()
        .filter(|fact| {
            predicate(&fact.data)
                && (fact_step_ref(&fact.data) == Some(step_ref)
                    || fact_snapshot_ref(&fact.data) == Some(snapshot_ref))
        })
        .map(|fact| fact.fact_ref.clone())
        .collect()
}

fn outcomes_for_step(
    outcomes: &[SemanticOutcome],
    facts: &[&AtomicChessFact],
    step_ref: &LineStepRef,
    predicate: impl Fn(&SemanticOutcomeData) -> bool,
) -> Vec<SemanticOutcomeRef> {
    outcomes
        .iter()
        .filter(|outcome| {
            predicate(&outcome.data)
                && outcome.supporting_fact_refs.iter().any(|reference| {
                    facts.iter().any(|fact| {
                        &fact.fact_ref == reference && fact_step_ref(&fact.data) == Some(step_ref)
                    })
                })
        })
        .map(|outcome| outcome.outcome_ref.clone())
        .collect()
}

fn outcome_step_index(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
    outcome: &SemanticOutcome,
) -> Option<usize> {
    candidate.contract.line_steps.iter().position(|step| {
        outcome.supporting_fact_refs.iter().any(|reference| {
            facts.iter().any(|fact| {
                &fact.fact_ref == reference && fact_step_ref(&fact.data) == Some(&step.step_ref)
            })
        })
    })
}

fn fact_step_ref(data: &AtomicChessFactData) -> Option<&LineStepRef> {
    match data {
        AtomicChessFactData::PieceMoved { step_ref, .. }
        | AtomicChessFactData::PieceCaptured { step_ref, .. }
        | AtomicChessFactData::PiecePromoted { step_ref, .. }
        | AtomicChessFactData::MaterialChanged { step_ref, .. }
        | AtomicChessFactData::AttackSetChanged { step_ref, .. }
        | AtomicChessFactData::CheckersChanged { step_ref, .. }
        | AtomicChessFactData::KingZonePressureChanged { step_ref, .. }
        | AtomicChessFactData::Castled { step_ref, .. }
        | AtomicChessFactData::EnPassantCaptured { step_ref, .. } => Some(step_ref),
        _ => None,
    }
}

fn fact_snapshot_ref(
    data: &AtomicChessFactData,
) -> Option<&crate::review_session_contract::DecisionPositionSnapshotRef> {
    match data {
        AtomicChessFactData::PieceOccupancy { snapshot_ref, .. }
        | AtomicChessFactData::AttackSet { snapshot_ref, .. }
        | AtomicChessFactData::SoleRayBlocker { snapshot_ref, .. }
        | AtomicChessFactData::Checkers { snapshot_ref, .. }
        | AtomicChessFactData::KingZonePressure { snapshot_ref, .. }
        | AtomicChessFactData::LegalRecaptures { snapshot_ref, .. }
        | AtomicChessFactData::PawnFrontSpanOccupancy { snapshot_ref, .. }
        | AtomicChessFactData::LegalDestinations { snapshot_ref, .. }
        | AtomicChessFactData::TerminalPosition { snapshot_ref, .. }
        | AtomicChessFactData::MaterialInventory { snapshot_ref, .. } => Some(snapshot_ref),
        _ => None,
    }
}
