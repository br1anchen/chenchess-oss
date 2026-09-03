use std::collections::{BTreeMap, BTreeSet};

use crate::review_session_contract::{
    AtomicFactRef, ConceptValidationProof, DecisionCandidate, DecisionCandidateOrigin,
    DecisionCandidateRef, DecisionExplanation, DecisionExplanationRef, EngineComparison,
    ExplanationPath, ExplanationPathRef, KnowledgeActivation, PreferenceProof, ProofCapability,
    SemanticComparisonRelation, SemanticOutcomeRef, CHESS_KNOWLEDGE_GRAPH_VERSION,
    DECISION_EXPLANATION_GENERATION,
};

use super::{
    candidate::{self, CandidateConstruction},
    facts::SelectedConceptProof,
    generation,
    knowledge::{self, CompiledKnowledgeGraph, KnowledgeConcept},
    minimization::{candidate_ref, minimal_fact_closure, minimize},
    DecisionExplanationContractError, DecisionExplanationInput,
};

pub(super) fn assemble_and_validate(
    input: DecisionExplanationInput,
    construction: CandidateConstruction,
    mut selected: SelectedConceptProof,
    graph: &CompiledKnowledgeGraph,
) -> Result<(DecisionExplanation, KnowledgeConcept), DecisionExplanationContractError> {
    let semantic_comparisons = selected.semantic_comparisons.clone();
    let selected_candidate = construction
        .candidates
        .iter()
        .find(|candidate| candidate.contract.candidate_ref == selected.candidate_ref)
        .ok_or(DecisionExplanationContractError::InvalidProof(
            "selected candidate is unavailable before proof minimization",
        ))?;
    let candidate_generation =
        generation::derive_candidate_generation(selected_candidate, &construction.facts, graph);
    let minimized = minimize(construction, &selected, candidate_generation.as_ref())?;
    selected.candidate_ref = minimized.selected_candidate_ref;
    let validation = ConceptValidationProof {
        candidate_ref: selected.candidate_ref.clone(),
        causal_step_ref: selected.causal_step_ref,
        payoff_step_ref: selected.payoff_step_ref,
        recognition_rule_ref: selected.recognition_rule_ref.clone(),
        supporting_fact_refs: selected.supporting_fact_refs.clone(),
        outcome_refs: selected.outcome_refs.clone(),
    };
    let activation = KnowledgeActivation {
        concept_node_ref: selected.concept_node_ref,
        recognition_rule_ref: selected.recognition_rule_ref,
        supporting_fact_refs: selected.supporting_fact_refs,
    };
    let candidate_generation_proof = candidate_generation
        .map(|generation| generation.into_proof(selected.candidate_ref.clone()));
    let path_content = (
        selected.attribution,
        &selected.candidate_ref,
        &activation,
        &candidate_generation_proof,
        &validation,
        &selected.outcome_refs,
    );
    let path = ExplanationPath {
        path_ref: ExplanationPathRef::from_content(&path_content),
        attribution: selected.attribution,
        candidate_ref: selected.candidate_ref.clone(),
        knowledge_activation: activation,
        candidate_generation_proof,
        concept_validation_proof: validation,
        outcome_refs: selected.outcome_refs,
    };

    // Player-Selected review reuses retained Single-PV evidence to validate
    // concepts, but that evidence is not a fresh comparative search. Keeping
    // the preference absent makes the derived capability ValidationOnly.
    let preference = if input.provenance
        == crate::review_session_contract::GameReviewMomentProvenance::PlayerSelected
    {
        None
    } else {
        preference_for(
            &selected.candidate_ref,
            &minimized.candidates,
            semantic_comparisons,
        )
    };
    let capability = derive_capability(&minimized.candidates, &preference);
    let mut explanation = DecisionExplanation {
        decision_explanation_ref: DecisionExplanationRef::from_content(&"pending"),
        generation: DECISION_EXPLANATION_GENERATION,
        knowledge_graph_version: CHESS_KNOWLEDGE_GRAPH_VERSION,
        game_ref: input.game_ref,
        critical_moment_id: input.critical_moment_id,
        position_snapshot: input.position_snapshot,
        candidate_evidence: input.candidate_evidence,
        snapshots: minimized.snapshots,
        facts: minimized.facts,
        candidates: minimized.candidates,
        selected_paths: vec![path],
        preference,
        capability,
    };
    let explanation_ref = {
        let content = explanation_identity_content(&explanation);
        DecisionExplanationRef::from_content(&content)
    };
    explanation.decision_explanation_ref = explanation_ref;
    validate_with_graph(&explanation, graph)?;
    Ok((explanation, selected.concept))
}

fn preference_for(
    preferred_ref: &crate::review_session_contract::DecisionCandidateRef,
    candidates: &[DecisionCandidate],
    semantic_comparisons: Vec<crate::review_session_contract::SemanticComparison>,
) -> Option<PreferenceProof> {
    let preferred = candidates
        .iter()
        .find(|candidate| &candidate.candidate_ref == preferred_ref)?;
    if candidates.len() < 2 || preferred.assessment.rank != Some(1) {
        return None;
    }
    Some(PreferenceProof {
        preferred_candidate_ref: preferred_ref.clone(),
        engine_comparisons: candidates
            .iter()
            .filter(|candidate| &candidate.candidate_ref != preferred_ref)
            .map(|alternative| EngineComparison {
                preferred_candidate_ref: preferred_ref.clone(),
                alternative_candidate_ref: alternative.candidate_ref.clone(),
                preferred_assessment_ref: preferred.assessment.assessment_ref.clone(),
                alternative_assessment_ref: alternative.assessment.assessment_ref.clone(),
            })
            .collect(),
        semantic_comparisons,
    })
}

fn derive_capability(
    candidates: &[DecisionCandidate],
    preference: &Option<PreferenceProof>,
) -> ProofCapability {
    let Some(preference) = preference else {
        return ProofCapability::ValidationOnly;
    };
    let alternatives = candidates.len().saturating_sub(1);
    let engine_complete = preference.engine_comparisons.len() == alternatives;
    if !engine_complete {
        return ProofCapability::ValidationOnly;
    }
    let semantic_complete = alternatives > 0
        && preference.semantic_comparisons.len() == alternatives
        && preference.semantic_comparisons.iter().all(|comparison| {
            matches!(
                comparison.relation,
                SemanticComparisonRelation::Dominates | SemanticComparisonRelation::Refutes
            )
        });
    if semantic_complete {
        ProofCapability::SemanticPreference
    } else {
        ProofCapability::EnginePreference
    }
}

pub fn validate_decision_explanation(
    explanation: &DecisionExplanation,
) -> Result<(), DecisionExplanationContractError> {
    let graph = knowledge::compiled_graph()?;
    validate_with_graph(explanation, &graph)
}

fn validate_with_graph(
    explanation: &DecisionExplanation,
    graph: &CompiledKnowledgeGraph,
) -> Result<(), DecisionExplanationContractError> {
    if explanation.generation != DECISION_EXPLANATION_GENERATION
        || explanation.knowledge_graph_version != CHESS_KNOWLEDGE_GRAPH_VERSION
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "generation and Knowledge Graph versions must be pinned",
        ));
    }
    if explanation.selected_paths.is_empty() || explanation.selected_paths.len() > 2 {
        return Err(DecisionExplanationContractError::InvalidProof(
            "an explanation must select one or two paths",
        ));
    }
    let recomputed = validate_candidate_evidence(explanation)?;
    let snapshot_refs = explanation
        .snapshots
        .iter()
        .map(|snapshot| &snapshot.snapshot_ref)
        .collect::<BTreeSet<_>>();
    if snapshot_refs.len() != explanation.snapshots.len()
        || explanation.snapshots.iter().any(|snapshot| {
            snapshot.snapshot_ref
                != crate::review_session_contract::DecisionPositionSnapshotRef::from_content(&(
                    &snapshot.canonical_position_ref,
                    &snapshot.fen,
                ))
        })
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "Position Snapshot references must be unique and content-derived",
        ));
    }
    let fact_refs = explanation
        .facts
        .iter()
        .map(|fact| &fact.fact_ref)
        .collect::<BTreeSet<_>>();
    if fact_refs.len() != explanation.facts.len()
        || explanation
            .facts
            .iter()
            .any(|fact| fact.fact_ref != AtomicFactRef::from_content(&fact.data))
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "Atomic Fact references must be unique and content-derived",
        ));
    }
    let candidate_refs = explanation
        .candidates
        .iter()
        .map(|candidate| &candidate.candidate_ref)
        .collect::<BTreeSet<_>>();
    if candidate_refs.len() != explanation.candidates.len() {
        return Err(DecisionExplanationContractError::InvalidProof(
            "Decision Candidate references must be unique",
        ));
    }
    for candidate in &explanation.candidates {
        validate_candidate(candidate, &snapshot_refs, &fact_refs)?;
    }
    let generation_outcome_refs = explanation
        .selected_paths
        .iter()
        .map(|path| validate_path(path, explanation, &recomputed, graph))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .map(|validated| validated.satisfying_outcome_ref)
        .collect::<Vec<_>>();
    validate_preference(explanation)?;
    validate_minimal_proof_sets(explanation, &generation_outcome_refs)?;
    if explanation.capability != derive_capability(&explanation.candidates, &explanation.preference)
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "Proof Capability must be derived from complete comparison coverage",
        ));
    }
    if explanation.decision_explanation_ref
        != DecisionExplanationRef::from_content(&explanation_identity_content(explanation))
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "Decision Explanation reference is not content-derived",
        ));
    }
    Ok(())
}

fn validate_candidate_evidence(
    explanation: &DecisionExplanation,
) -> Result<ValidatedCandidateEvidence, DecisionExplanationContractError> {
    let player = explanation
        .candidates
        .iter()
        .find(|candidate| {
            candidate
                .origins
                .contains(&DecisionCandidateOrigin::PlayerPlayed)
        })
        .ok_or(DecisionExplanationContractError::InvalidProof(
            "candidate evidence must retain the Player move",
        ))?;
    let normalized = candidate::normalize_evidence(
        &explanation.candidate_evidence,
        &player.root_move_uci,
        &explanation.position_snapshot.fen,
    )?;
    let recomputed = candidate::replay_candidates(&explanation.position_snapshot, normalized)?;
    if recomputed.snapshots != explanation.snapshots {
        return Err(DecisionExplanationContractError::InvalidProof(
            "persisted Position Snapshots do not reproduce from Candidate Evidence",
        ));
    }
    let recomputed_facts = recomputed
        .facts
        .iter()
        .map(|fact| (fact.fact_ref.clone(), fact))
        .collect::<std::collections::BTreeMap<_, _>>();
    if explanation
        .facts
        .iter()
        .any(|fact| recomputed_facts.get(&fact.fact_ref).copied() != Some(fact))
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "persisted Atomic Facts do not reproduce from Candidate Evidence",
        ));
    }
    if explanation.candidates.len() != recomputed.candidates.len() {
        return Err(DecisionExplanationContractError::InvalidProof(
            "persisted Decision Candidates do not reproduce from Candidate Evidence",
        ));
    }
    let mut candidate_indices = BTreeMap::new();
    let mut matched_indices = BTreeSet::new();
    for candidate in &explanation.candidates {
        let Some((index, replayed)) =
            recomputed
                .candidates
                .iter()
                .enumerate()
                .find(|(_, replayed)| {
                    replayed.contract.root_move_uci == candidate.root_move_uci
                        && replayed.contract.origins == candidate.origins
                        && replayed.contract.retained_variation == candidate.retained_variation
                        && replayed.contract.line_steps == candidate.line_steps
                        && replayed.contract.assessment == candidate.assessment
                })
        else {
            return Err(DecisionExplanationContractError::InvalidProof(
                "persisted Decision Candidates do not reproduce from Candidate Evidence",
            ));
        };
        if !matched_indices.insert(index) {
            return Err(DecisionExplanationContractError::InvalidProof(
                "persisted Decision Candidates do not reproduce from Candidate Evidence",
            ));
        }
        candidate_indices.insert(candidate.candidate_ref.clone(), index);
        if candidate
            .fact_refs
            .iter()
            .any(|reference| !replayed.contract.fact_refs.contains(reference))
            || candidate
                .outcomes
                .iter()
                .any(|outcome| !replayed.contract.outcomes.contains(outcome))
        {
            return Err(DecisionExplanationContractError::InvalidProof(
                "persisted candidate proof data does not reproduce from Candidate Evidence",
            ));
        }
    }
    Ok(ValidatedCandidateEvidence {
        construction: recomputed,
        candidate_indices,
    })
}

struct ValidatedCandidateEvidence {
    construction: CandidateConstruction,
    candidate_indices: BTreeMap<DecisionCandidateRef, usize>,
}

impl ValidatedCandidateEvidence {
    fn candidate(&self, reference: &DecisionCandidateRef) -> &candidate::ReplayedCandidate {
        &self.construction.candidates[self.candidate_indices[reference]]
    }
}

fn validate_minimal_proof_sets(
    explanation: &DecisionExplanation,
    generation_outcome_refs: &[SemanticOutcomeRef],
) -> Result<(), DecisionExplanationContractError> {
    let mut required_outcomes = explanation
        .selected_paths
        .iter()
        .flat_map(|path| path.outcome_refs.iter().cloned())
        .collect::<BTreeSet<_>>();
    required_outcomes.extend(generation_outcome_refs.iter().cloned());
    if let Some(preference) = &explanation.preference {
        for comparison in &preference.semantic_comparisons {
            required_outcomes.insert(comparison.preferred_outcome_ref.clone());
            required_outcomes.insert(comparison.alternative_outcome_ref.clone());
        }
    }
    let persisted_outcomes = explanation
        .candidates
        .iter()
        .flat_map(|candidate| candidate.outcomes.iter())
        .map(|outcome| outcome.outcome_ref.clone())
        .collect::<BTreeSet<_>>();
    if persisted_outcomes != required_outcomes {
        return Err(DecisionExplanationContractError::InvalidProof(
            "persisted Semantic Outcomes are not the minimal selected proof set",
        ));
    }

    let facts = explanation
        .facts
        .iter()
        .cloned()
        .map(|fact| (fact.fact_ref.clone(), fact))
        .collect::<std::collections::BTreeMap<_, _>>();
    let required_facts = minimal_fact_closure(
        explanation.selected_paths.iter().flat_map(|path| {
            path.knowledge_activation
                .supporting_fact_refs
                .iter()
                .chain(&path.concept_validation_proof.supporting_fact_refs)
                .chain(
                    path.candidate_generation_proof
                        .iter()
                        .flat_map(|proof| proof.supporting_fact_refs.iter()),
                )
        }),
        explanation
            .candidates
            .iter()
            .flat_map(|candidate| candidate.outcomes.iter()),
        &facts,
    )?;
    if required_facts != facts.keys().cloned().collect() {
        return Err(DecisionExplanationContractError::InvalidProof(
            "persisted Atomic Facts are not the minimal selected proof closure",
        ));
    }
    Ok(())
}

fn validate_candidate(
    candidate: &DecisionCandidate,
    snapshot_refs: &BTreeSet<&crate::review_session_contract::DecisionPositionSnapshotRef>,
    fact_refs: &BTreeSet<&AtomicFactRef>,
) -> Result<(), DecisionExplanationContractError> {
    if candidate.retained_variation.is_empty()
        || candidate.retained_variation.first() != Some(&candidate.root_move_uci)
        || candidate.line_steps.len() != candidate.retained_variation.len()
        || candidate
            .line_steps
            .iter()
            .zip(&candidate.retained_variation)
            .any(|(step, uci)| {
                &step.uci != uci
                    || !snapshot_refs.contains(&step.before_snapshot_ref)
                    || !snapshot_refs.contains(&step.after_snapshot_ref)
                    || step.step_ref
                        != crate::review_session_contract::LineStepRef::from_content(&(
                            &step.before_snapshot_ref,
                            &step.after_snapshot_ref,
                            &step.uci,
                            step.mover,
                            step.role,
                            &step.from_square,
                            &step.to_square,
                            &step.captured,
                            &step.promotion,
                        ))
            })
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "candidate line steps are malformed",
        ));
    }
    if candidate
        .fact_refs
        .iter()
        .any(|fact_ref| !fact_refs.contains(fact_ref))
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "candidate references an unavailable Atomic Fact",
        ));
    }
    for outcome in &candidate.outcomes {
        if outcome.outcome_ref
            != crate::review_session_contract::SemanticOutcomeRef::from_content(&(
                &outcome.data,
                &outcome.supporting_fact_refs,
            ))
            || outcome.supporting_fact_refs.is_empty()
            || outcome
                .supporting_fact_refs
                .iter()
                .any(|fact_ref| !candidate.fact_refs.contains(fact_ref))
        {
            return Err(DecisionExplanationContractError::InvalidProof(
                "candidate Semantic Outcome is malformed or cross-candidate",
            ));
        }
    }
    if candidate.assessment.assessment_ref
        != crate::review_session_contract::EngineAssessmentRef::from_content(&(
            candidate.assessment.rank,
            &candidate.assessment.score,
            &candidate.assessment.provenance,
        ))
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "Engine Assessment reference is not content-derived",
        ));
    }
    if candidate.candidate_ref != candidate_ref(candidate) {
        return Err(DecisionExplanationContractError::InvalidProof(
            "Decision Candidate reference is not content-derived",
        ));
    }
    Ok(())
}

fn validate_path(
    path: &ExplanationPath,
    explanation: &DecisionExplanation,
    recomputed: &ValidatedCandidateEvidence,
    graph: &CompiledKnowledgeGraph,
) -> Result<Option<generation::ValidatedCandidateGeneration>, DecisionExplanationContractError> {
    let candidate = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == path.candidate_ref)
        .ok_or(DecisionExplanationContractError::InvalidProof(
            "selected path references an unavailable candidate",
        ))?;
    let validation = &path.concept_validation_proof;
    if validation.candidate_ref != candidate.candidate_ref
        || path.outcome_refs.is_empty()
        || validation.outcome_refs != path.outcome_refs
        || validation
            .supporting_fact_refs
            .iter()
            .any(|fact_ref| !candidate.fact_refs.contains(fact_ref))
        || path
            .knowledge_activation
            .supporting_fact_refs
            .iter()
            .any(|fact_ref| !candidate.fact_refs.contains(fact_ref))
        || !candidate
            .line_steps
            .iter()
            .any(|step| step.step_ref == validation.causal_step_ref)
        || !candidate
            .line_steps
            .iter()
            .any(|step| step.step_ref == validation.payoff_step_ref)
        || path.outcome_refs.iter().any(|outcome_ref| {
            !candidate
                .outcomes
                .iter()
                .any(|outcome| &outcome.outcome_ref == outcome_ref)
        })
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "selected path contains a cross-candidate or malformed proof reference",
        ));
    }
    if validation.recognition_rule_ref != path.knowledge_activation.recognition_rule_ref
        || !graph.resolves(
            &path.knowledge_activation.concept_node_ref,
            &path.knowledge_activation.recognition_rule_ref,
        )
    {
        return Err(DecisionExplanationContractError::InvalidProof(
            "selected path has unresolved Chess Knowledge",
        ));
    }
    let recomputed_candidate = recomputed.candidate(&candidate.candidate_ref);
    let validated_generation = generation::validate_candidate_generation(
        &path.candidate_generation_proof,
        candidate,
        recomputed_candidate,
        &explanation.facts,
        &recomputed.construction.facts,
        graph,
    )?;
    let expected_ref = ExplanationPathRef::from_content(&(
        path.attribution,
        &path.candidate_ref,
        &path.knowledge_activation,
        &path.candidate_generation_proof,
        &path.concept_validation_proof,
        &path.outcome_refs,
    ));
    if path.path_ref != expected_ref {
        return Err(DecisionExplanationContractError::InvalidProof(
            "Explanation Path reference is not content-derived",
        ));
    }
    Ok(validated_generation)
}

fn validate_preference(
    explanation: &DecisionExplanation,
) -> Result<(), DecisionExplanationContractError> {
    let Some(preference) = &explanation.preference else {
        return Ok(());
    };
    let preferred = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == preference.preferred_candidate_ref)
        .ok_or(DecisionExplanationContractError::InvalidProof(
            "preference references an unavailable preferred candidate",
        ))?;
    let alternatives = explanation
        .candidates
        .iter()
        .filter(|candidate| candidate.candidate_ref != preferred.candidate_ref)
        .collect::<Vec<_>>();
    if preference.engine_comparisons.len() != alternatives.len() {
        return Err(DecisionExplanationContractError::InvalidProof(
            "engine preference must cover every retained alternative",
        ));
    }
    let mut compared = BTreeSet::new();
    for comparison in &preference.engine_comparisons {
        let Some(alternative) = alternatives
            .iter()
            .find(|candidate| candidate.candidate_ref == comparison.alternative_candidate_ref)
        else {
            return Err(DecisionExplanationContractError::InvalidProof(
                "engine comparison references an unavailable alternative",
            ));
        };
        if comparison.preferred_candidate_ref != preferred.candidate_ref
            || comparison.preferred_assessment_ref != preferred.assessment.assessment_ref
            || comparison.alternative_assessment_ref != alternative.assessment.assessment_ref
            || !compared.insert(&comparison.alternative_candidate_ref)
        {
            return Err(DecisionExplanationContractError::InvalidProof(
                "engine comparison is inconsistent or duplicated",
            ));
        }
    }
    let mut semantically_compared = BTreeSet::new();
    for comparison in &preference.semantic_comparisons {
        if !preferred
            .outcomes
            .iter()
            .any(|outcome| outcome.outcome_ref == comparison.preferred_outcome_ref)
        {
            return Err(DecisionExplanationContractError::InvalidProof(
                "semantic comparison references an unavailable preferred outcome",
            ));
        }
        let matching_alternatives = alternatives
            .iter()
            .filter(|candidate| {
                candidate
                    .outcomes
                    .iter()
                    .any(|outcome| outcome.outcome_ref == comparison.alternative_outcome_ref)
            })
            .collect::<Vec<_>>();
        let [alternative] = matching_alternatives.as_slice() else {
            return Err(DecisionExplanationContractError::InvalidProof(
                "semantic comparison must reference one retained alternative outcome",
            ));
        };
        if !semantically_compared.insert(&alternative.candidate_ref) {
            return Err(DecisionExplanationContractError::InvalidProof(
                "semantic comparison duplicates a retained alternative",
            ));
        }
    }
    Ok(())
}

fn explanation_identity_content(explanation: &DecisionExplanation) -> impl serde::Serialize + '_ {
    (
        explanation.generation,
        explanation.knowledge_graph_version,
        &explanation.game_ref,
        &explanation.critical_moment_id,
        &explanation.position_snapshot,
        &explanation.candidate_evidence,
        &explanation.snapshots,
        &explanation.facts,
        &explanation.candidates,
        &explanation.selected_paths,
        &explanation.preference,
        explanation.capability,
    )
}
