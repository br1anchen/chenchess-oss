use std::collections::{BTreeMap, BTreeSet};

use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, AtomicFactRef, DecisionCandidate, DecisionCandidateRef,
    DecisionPositionSnapshot, SemanticOutcome, SemanticOutcomeData,
};

use super::{
    candidate::CandidateConstruction, facts::SelectedConceptProof,
    generation::CandidateGenerationMatch, DecisionExplanationContractError,
};

pub(super) struct MinimalConstruction {
    pub snapshots: Vec<DecisionPositionSnapshot>,
    pub facts: Vec<AtomicChessFact>,
    pub candidates: Vec<DecisionCandidate>,
    pub selected_candidate_ref: DecisionCandidateRef,
}

pub(super) fn minimize(
    construction: CandidateConstruction,
    selected: &SelectedConceptProof,
    candidate_generation: Option<&CandidateGenerationMatch>,
) -> Result<MinimalConstruction, DecisionExplanationContractError> {
    let mut required_outcomes = selected
        .outcome_refs
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if let Some(candidate_generation) = candidate_generation {
        required_outcomes.insert(candidate_generation.satisfying_outcome_ref.clone());
    }
    for comparison in &selected.semantic_comparisons {
        required_outcomes.insert(comparison.preferred_outcome_ref.clone());
        required_outcomes.insert(comparison.alternative_outcome_ref.clone());
    }
    let all_facts = construction
        .facts
        .into_iter()
        .map(|fact| (fact.fact_ref.clone(), fact))
        .collect::<BTreeMap<_, _>>();
    let required_facts = minimal_fact_closure(
        selected.supporting_fact_refs.iter().chain(
            candidate_generation
                .into_iter()
                .flat_map(|proof| proof.supporting_fact_refs.iter()),
        ),
        construction.candidates.iter().flat_map(|candidate| {
            candidate
                .contract
                .outcomes
                .iter()
                .filter(|outcome| required_outcomes.contains(&outcome.outcome_ref))
        }),
        &all_facts,
    )?;

    let mut selected_candidate_ref = None;
    let mut candidates = Vec::with_capacity(construction.candidates.len());
    for replayed in construction.candidates {
        let old_ref = replayed.contract.candidate_ref.clone();
        let mut candidate = replayed.contract;
        candidate
            .fact_refs
            .retain(|reference| required_facts.contains(reference));
        candidate
            .outcomes
            .retain(|outcome| required_outcomes.contains(&outcome.outcome_ref));
        candidate.candidate_ref = candidate_ref(&candidate);
        if old_ref == selected.candidate_ref {
            selected_candidate_ref = Some(candidate.candidate_ref.clone());
        }
        candidates.push(candidate);
    }
    let selected_candidate_ref =
        selected_candidate_ref.ok_or(DecisionExplanationContractError::InvalidProof(
            "selected candidate is unavailable during proof minimization",
        ))?;
    let facts = required_facts
        .into_iter()
        .map(|reference| {
            all_facts.get(&reference).cloned().ok_or(
                DecisionExplanationContractError::InvalidProof(
                    "minimal proof references an unavailable Atomic Fact",
                ),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MinimalConstruction {
        snapshots: construction.snapshots,
        facts,
        candidates,
        selected_candidate_ref,
    })
}

pub(super) fn minimal_fact_closure<'a>(
    supporting_fact_refs: impl IntoIterator<Item = &'a AtomicFactRef>,
    outcomes: impl IntoIterator<Item = &'a SemanticOutcome>,
    all_facts: &BTreeMap<AtomicFactRef, AtomicChessFact>,
) -> Result<BTreeSet<AtomicFactRef>, DecisionExplanationContractError> {
    let mut required = supporting_fact_refs
        .into_iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for outcome in outcomes {
        required.extend(outcome.supporting_fact_refs.iter().cloned());
        required.extend(outcome_data_fact_refs(&outcome.data));
    }
    close_fact_dependencies(&mut required, all_facts)?;
    Ok(required)
}

fn close_fact_dependencies(
    required: &mut BTreeSet<AtomicFactRef>,
    all_facts: &BTreeMap<AtomicFactRef, AtomicChessFact>,
) -> Result<(), DecisionExplanationContractError> {
    let mut pending = required.iter().cloned().collect::<Vec<_>>();
    while let Some(reference) = pending.pop() {
        let fact =
            all_facts
                .get(&reference)
                .ok_or(DecisionExplanationContractError::InvalidProof(
                    "minimal proof references an unavailable Atomic Fact",
                ))?;
        for dependency in fact_data_fact_refs(&fact.data) {
            if required.insert(dependency.clone()) {
                pending.push(dependency);
            }
        }
    }
    Ok(())
}

fn fact_data_fact_refs(data: &AtomicChessFactData) -> Vec<AtomicFactRef> {
    match data {
        AtomicChessFactData::MaterialChanged {
            before_inventory_refs,
            after_inventory_refs,
            ..
        } => before_inventory_refs
            .iter()
            .chain(after_inventory_refs)
            .cloned()
            .collect(),
        AtomicChessFactData::AttackSetChanged {
            before_attack_ref,
            after_attack_ref,
            ..
        } => vec![before_attack_ref.clone(), after_attack_ref.clone()],
        AtomicChessFactData::CheckersChanged {
            before_checkers_ref,
            after_checkers_ref,
            ..
        } => vec![before_checkers_ref.clone(), after_checkers_ref.clone()],
        AtomicChessFactData::KingZonePressureChanged {
            before_pressure_ref,
            after_pressure_ref,
            ..
        } => vec![before_pressure_ref.clone(), after_pressure_ref.clone()],
        _ => Vec::new(),
    }
}

fn outcome_data_fact_refs(data: &SemanticOutcomeData) -> Vec<AtomicFactRef> {
    match data {
        SemanticOutcomeData::PawnProgressed {
            before_front_span_ref,
            after_front_span_ref,
            ..
        } => std::iter::once(before_front_span_ref.clone())
            .chain(after_front_span_ref.iter().cloned())
            .collect(),
        SemanticOutcomeData::MaterialConfigurationChanged {
            before_inventory_refs,
            after_inventory_refs,
        } => before_inventory_refs
            .iter()
            .chain(after_inventory_refs)
            .cloned()
            .collect(),
        SemanticOutcomeData::TerminalStateReached {
            before_state_ref,
            after_state_ref,
            ..
        } => vec![before_state_ref.clone(), after_state_ref.clone()],
        SemanticOutcomeData::AttackAccessChanged {
            before_attack_ref,
            after_attack_ref,
            ..
        } => vec![before_attack_ref.clone(), after_attack_ref.clone()],
        SemanticOutcomeData::CheckStateChanged {
            before_checkers_ref,
            after_checkers_ref,
            ..
        } => vec![before_checkers_ref.clone(), after_checkers_ref.clone()],
        SemanticOutcomeData::KingZonePressureChanged {
            before_pressure_ref,
            after_pressure_ref,
            ..
        } => vec![before_pressure_ref.clone(), after_pressure_ref.clone()],
        SemanticOutcomeData::MaterialBalanceChanged { .. } => Vec::new(),
    }
}

pub(super) fn candidate_ref(candidate: &DecisionCandidate) -> DecisionCandidateRef {
    DecisionCandidateRef::from_content(&(
        &candidate.root_move_uci,
        &candidate.origins,
        &candidate.retained_variation,
        candidate
            .line_steps
            .iter()
            .map(|step| &step.step_ref)
            .collect::<Vec<_>>(),
        &candidate.fact_refs,
        candidate
            .outcomes
            .iter()
            .map(|outcome| &outcome.outcome_ref)
            .collect::<Vec<_>>(),
        &candidate.assessment.assessment_ref,
    ))
}
