use shakmaty::{Position, Role};

use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, AtomicFactRef, CandidateGenerationProof,
    DecisionCandidate, DecisionCandidateRef, PieceAtSquare, PositionGoal, SemanticOutcomeRef,
};

use super::{
    candidate::{chess_square, contract_color, contract_role, contract_square, ReplayedCandidate},
    facts::piece_value,
    knowledge::{CompiledKnowledgeGraph, GoalTemplate, PreMoveRule},
    DecisionExplanationContractError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CandidateGenerationMatch {
    pub(super) supporting_fact_refs: Vec<AtomicFactRef>,
    concept_node_ref: crate::review_session_contract::KnowledgeNodeRef,
    position_goal: PositionGoal,
    pub(super) satisfying_outcome_ref: SemanticOutcomeRef,
}

pub(super) struct ValidatedCandidateGeneration {
    pub(super) satisfying_outcome_ref: SemanticOutcomeRef,
}

impl CandidateGenerationMatch {
    pub(super) fn into_proof(
        self,
        suggested_candidate_ref: DecisionCandidateRef,
    ) -> CandidateGenerationProof {
        CandidateGenerationProof {
            supporting_fact_refs: self.supporting_fact_refs,
            concept_node_ref: self.concept_node_ref,
            position_goal: self.position_goal,
            suggested_candidate_ref,
        }
    }
}

pub(super) fn derive_candidate_generation(
    candidate: &ReplayedCandidate,
    facts: &[AtomicChessFact],
    graph: &CompiledKnowledgeGraph,
) -> Option<CandidateGenerationMatch> {
    graph
        .generation_knowledge()
        .find_map(|(concept_node_ref, knowledge)| {
            let (targets, supporting_fact_refs) =
                match (knowledge.pre_move_rule, knowledge.goal_template) {
                    (PreMoveRule::ReachableForkV1, GoalTemplate::GainMaterial) => {
                        reachable_fork(candidate, facts)?
                    }
                };
            let position_goal = match knowledge.goal_template {
                GoalTemplate::GainMaterial => PositionGoal::GainMaterial { targets },
            };
            let satisfying_outcome_ref = candidate
                .contract
                .outcomes
                .iter()
                .find(|outcome| position_goal.is_satisfied_by(outcome))?
                .outcome_ref
                .clone();
            Some(CandidateGenerationMatch {
                supporting_fact_refs,
                concept_node_ref: concept_node_ref.clone(),
                position_goal,
                satisfying_outcome_ref,
            })
        })
}

pub(super) fn validate_candidate_generation(
    proof: &Option<CandidateGenerationProof>,
    candidate: &DecisionCandidate,
    recomputed_candidate: &ReplayedCandidate,
    persisted_facts: &[AtomicChessFact],
    recomputed_facts: &[AtomicChessFact],
    graph: &CompiledKnowledgeGraph,
) -> Result<Option<ValidatedCandidateGeneration>, DecisionExplanationContractError> {
    if let Some(proof) = proof {
        if proof.suggested_candidate_ref != candidate.candidate_ref
            || proof
                .supporting_fact_refs
                .iter()
                .any(|fact_ref| !candidate.fact_refs.contains(fact_ref))
        {
            return Err(DecisionExplanationContractError::InvalidProof(
                "Candidate Generation Proof is cross-candidate",
            ));
        }
        let root_snapshot_ref = &candidate
            .line_steps
            .first()
            .ok_or(DecisionExplanationContractError::InvalidProof(
                "Candidate Generation Proof requires a root move",
            ))?
            .before_snapshot_ref;
        if proof.supporting_fact_refs.iter().any(|fact_ref| {
            !persisted_facts.iter().any(|fact| {
                &fact.fact_ref == fact_ref
                    && matches!(
                        &fact.data,
                        AtomicChessFactData::PieceOccupancy { snapshot_ref, .. }
                            | AtomicChessFactData::LegalDestinations { snapshot_ref, .. }
                            if snapshot_ref == root_snapshot_ref
                    )
            })
        }) {
            return Err(DecisionExplanationContractError::InvalidProof(
                "Candidate Generation Proof may cite only pre-move position facts",
            ));
        }
    }

    let expected = derive_candidate_generation(recomputed_candidate, recomputed_facts, graph);
    match (proof, expected) {
        (None, None) => Ok(None),
        (Some(actual), Some(expected)) => {
            candidate
                .outcomes
                .iter()
                .find(|outcome| outcome.outcome_ref == expected.satisfying_outcome_ref)
                .ok_or(DecisionExplanationContractError::InvalidProof(
                    "Candidate Generation Proof has no retained satisfying outcome",
                ))?;
            if actual != &expected.clone().into_proof(candidate.candidate_ref.clone()) {
                return Err(DecisionExplanationContractError::InvalidProof(
                    "Candidate Generation Proof does not reproduce from pre-move knowledge",
                ));
            }
            Ok(Some(ValidatedCandidateGeneration {
                satisfying_outcome_ref: expected.satisfying_outcome_ref,
            }))
        }
        _ => Err(DecisionExplanationContractError::InvalidProof(
            "Candidate Generation Proof presence does not reproduce from pre-move knowledge",
        )),
    }
}

fn reachable_fork(
    candidate: &ReplayedCandidate,
    facts: &[AtomicChessFact],
) -> Option<(Vec<PieceAtSquare>, Vec<AtomicFactRef>)> {
    let root_step = candidate
        .contract
        .line_steps
        .first()
        .expect("a replayed candidate has a root line step");
    let before = candidate
        .positions
        .first()
        .expect("a replayed candidate has a root position");
    let after = candidate
        .positions
        .get(1)
        .expect("a replayed candidate has a position after its root move");
    let to = chess_square(&root_step.to_square);
    if root_step.role == crate::review_session_contract::PieceRole::King {
        return None;
    }
    let contract_root_piece = PieceAtSquare {
        color: root_step.mover,
        role: root_step.role,
        square: root_step.from_square.clone(),
    };
    let legal_destination_ref = facts
        .iter()
        .find_map(|fact| match &fact.data {
            AtomicChessFactData::LegalDestinations {
                snapshot_ref,
                piece,
                destinations,
            } if snapshot_ref == &root_step.before_snapshot_ref
                && piece == &contract_root_piece
                && destinations.contains(&root_step.to_square)
                && candidate.contract.fact_refs.contains(&fact.fact_ref) =>
            {
                Some(fact.fact_ref.clone())
            }
            _ => None,
        })
        .expect("a replayed root move owns its pre-move Legal Destinations fact");

    let attacker = after
        .board()
        .piece_at(to)
        .expect("a replayed root move ends on its recorded destination");
    let attacked = after.board().attacks_from(to) & after.board().by_color(!attacker.color);
    let qualified_targets = attacked
        .into_iter()
        .filter_map(|square| {
            let target = after
                .board()
                .piece_at(square)
                .expect("an attacked enemy-occupancy square contains a piece");
            let defended = !after
                .board()
                .attacks_to(square, target.color, after.board().occupied())
                .is_empty();
            let qualifies_without_absence =
                target.role == Role::King || piece_value(target.role) > piece_value(attacker.role);
            (qualifies_without_absence || !defended).then(|| {
                (
                    PieceAtSquare {
                        color: contract_color(target.color),
                        role: contract_role(target.role),
                        square: contract_square(square),
                    },
                    !qualifies_without_absence,
                )
            })
        })
        .collect::<Vec<_>>();
    if qualified_targets.len() < 2 {
        return None;
    }
    let needs_complete_occupancy = matches!(attacker.role, Role::Bishop | Role::Rook | Role::Queen)
        || qualified_targets
            .iter()
            .any(|(_, qualifies_by_absence)| *qualifies_by_absence);
    let mut targets = qualified_targets
        .into_iter()
        .map(|(target, _)| target)
        .collect::<Vec<_>>();
    targets.sort_by(|left, right| left.square.as_str().cmp(right.square.as_str()));

    let occupancy_dependencies = if needs_complete_occupancy {
        before
            .board()
            .occupied()
            .into_iter()
            .map(|square| {
                let piece = before
                    .board()
                    .piece_at(square)
                    .expect("an occupied square contains a piece");
                PieceAtSquare {
                    color: contract_color(piece.color),
                    role: contract_role(piece.role),
                    square: contract_square(square),
                }
            })
            .collect::<Vec<_>>()
    } else {
        targets.clone()
    };
    let mut supporting_fact_refs = occupancy_dependencies
        .iter()
        .map(|piece| root_occupancy_ref(candidate, facts, root_step, piece))
        .collect::<Vec<_>>();
    supporting_fact_refs.push(legal_destination_ref);
    supporting_fact_refs.sort();
    supporting_fact_refs.dedup();
    Some((targets, supporting_fact_refs))
}

fn root_occupancy_ref(
    candidate: &ReplayedCandidate,
    facts: &[AtomicChessFact],
    root_step: &crate::review_session_contract::DecisionLineStep,
    expected: &PieceAtSquare,
) -> AtomicFactRef {
    facts
        .iter()
        .find_map(|fact| match &fact.data {
            AtomicChessFactData::PieceOccupancy {
                snapshot_ref,
                piece,
            } if snapshot_ref == &root_step.before_snapshot_ref
                && piece == expected
                && candidate.contract.fact_refs.contains(&fact.fact_ref) =>
            {
                Some(fact.fact_ref.clone())
            }
            _ => None,
        })
        .expect("a replayed candidate owns every required root Piece Occupancy fact")
}
