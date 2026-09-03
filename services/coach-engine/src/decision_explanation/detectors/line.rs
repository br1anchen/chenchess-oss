use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, AtomicFactRef, Color,
    CurriculumLearningConcept as Concept, DecisionCandidateOrigin, LineStepRef, PieceRole,
    SemanticComparison, SemanticComparisonRelation, SemanticOutcome, SemanticOutcomeData,
    SemanticOutcomeRef,
};

use super::{fact_step_ref, outcome_step_index, proof, DetectedConcept};
use crate::decision_explanation::candidate::{CandidateConstruction, ReplayedCandidate};

pub(super) fn detect(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    detect_quiet_move(candidate, facts).into_iter().collect()
}

pub(super) fn detect_specific(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
    construction: &CandidateConstruction,
) -> Vec<DetectedConcept> {
    [
        detect_special_move(candidate, facts),
        detect_counter_check(candidate, facts),
        detect_greek_gift(candidate, facts),
        detect_sacrifice(candidate, facts),
        detect_intermezzo(candidate, facts),
        detect_desperado(candidate, facts),
        detect_clearance(candidate, facts),
        detect_interference(candidate, facts),
        detect_attraction(candidate, facts),
        detect_deflection(candidate, facts),
        detect_collinear(candidate, facts),
        detect_defensive_move(candidate, facts, construction),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn detect_special_move(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    let (concept, special_ref) = facts.iter().find_map(|fact| match &fact.data {
        AtomicChessFactData::Castled { step_ref, .. } if step_ref == &first.step_ref => {
            Some((Concept::Castling, fact.fact_ref.clone()))
        }
        AtomicChessFactData::EnPassantCaptured { step_ref, .. } if step_ref == &first.step_ref => {
            Some((Concept::EnPassant, fact.fact_ref.clone()))
        }
        _ => None,
    })?;
    let (payoff, outcome_refs) = first_outcome(candidate, facts, 0, |_| true)?;
    let mut supporting = facts_for_range(candidate, facts, 0, payoff);
    supporting.push(special_ref);
    proof(candidate, concept, 0, payoff, supporting, outcome_refs)
}

fn detect_counter_check(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    let changed = facts.iter().find_map(|fact| match &fact.data {
        AtomicChessFactData::CheckersChanged {
            step_ref,
            after_checkers_ref,
            added_checkers,
            ..
        } if step_ref == &first.step_ref && !added_checkers.is_empty() => {
            Some((fact.fact_ref.clone(), after_checkers_ref))
        }
        _ => None,
    })?;
    let before_checker = facts.iter().find(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::Checkers {
                snapshot_ref,
                king,
                checking_pieces,
            } if snapshot_ref == &first.before_snapshot_ref
                && king.color == first.mover
                && !checking_pieces.is_empty()
        )
    })?;
    let outcomes = outcomes_for_step(candidate, facts, 0, |outcome| {
        matches!(
            outcome,
            SemanticOutcomeData::CheckStateChanged { added_checkers, .. }
                if !added_checkers.is_empty()
        )
    });
    proof(
        candidate,
        Concept::CounterCheck,
        0,
        0,
        vec![
            changed.0,
            changed.1.clone(),
            before_checker.fact_ref.clone(),
        ],
        outcomes,
    )
}

fn detect_greek_gift(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    if first.role != PieceRole::Bishop
        || !matches!(first.to_square.as_str(), "h7" | "h2")
        || first
            .captured
            .as_ref()
            .is_none_or(|piece| piece.role != PieceRole::Pawn)
    {
        return None;
    }
    let bishop_is_taken = candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, step)| {
            step.captured
                .as_ref()
                .is_some_and(|captured| {
                    captured.color == first.mover
                        && captured.role == PieceRole::Bishop
                        && captured.square == first.to_square
                })
                .then_some(index)
        })?;
    let (payoff, outcome_refs) =
        first_outcome_by_mover(candidate, facts, bishop_is_taken, first.mover, |outcome| {
            matches!(
                outcome,
                SemanticOutcomeData::CheckStateChanged { added_checkers, .. }
                    if !added_checkers.is_empty()
            ) || matches!(
                outcome,
                SemanticOutcomeData::KingZonePressureChanged { added_attackers, .. }
                    if !added_attackers.is_empty()
            ) || matches!(outcome, SemanticOutcomeData::TerminalStateReached { .. })
        })?;
    proof(
        candidate,
        Concept::GreekGift,
        0,
        payoff,
        facts_for_range(candidate, facts, 0, payoff),
        outcome_refs,
    )
}

fn detect_sacrifice(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    let received = first.captured.as_ref().map_or(0, |piece| value(piece.role));
    if value(first.role) <= received {
        return None;
    }
    let loss_index = candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, step)| {
            step.captured
                .as_ref()
                .is_some_and(|captured| {
                    captured.color == first.mover
                        && captured.role == first.role
                        && captured.square == first.to_square
                })
                .then_some(index)
        })?;
    let (payoff, outcome_refs) = first_outcome_by_mover(
        candidate,
        facts,
        loss_index + 1,
        first.mover,
        is_forcing_payoff,
    )?;
    proof(
        candidate,
        Concept::Sacrifice,
        0,
        payoff,
        facts_for_range(candidate, facts, 0, payoff),
        outcome_refs,
    )
}

fn detect_intermezzo(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    let gives_check = step_gives_check(facts, &first.step_ref);
    if first.captured.is_none() && !gives_check {
        return None;
    }
    let recapture = legal_recaptures_at(facts, &first.after_snapshot_ref, &first.to_square)?;
    let reply = candidate.contract.line_steps.get(1)?;
    if reply.to_square != first.to_square || reply.captured.is_none() {
        return None;
    }
    let (payoff, outcome_refs) =
        first_outcome_by_mover(candidate, facts, 2, first.mover, is_forcing_payoff)?;
    let mut supporting = facts_for_range(candidate, facts, 0, payoff);
    supporting.push(recapture);
    proof(
        candidate,
        Concept::Intermezzo,
        0,
        payoff,
        supporting,
        outcome_refs,
    )
}

fn detect_desperado(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    first.captured.as_ref()?;
    if !facts.iter().any(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::AttackSet {
                snapshot_ref,
                attacker,
                attacked_squares,
            } if snapshot_ref == &first.before_snapshot_ref
                && attacker.color != first.mover
                && attacked_squares.contains(&first.from_square)
        )
    }) {
        return None;
    }
    let recapture = legal_recaptures_at(facts, &first.after_snapshot_ref, &first.to_square)?;
    let outcomes = outcomes_for_step(candidate, facts, 0, |outcome| {
        matches!(outcome, SemanticOutcomeData::MaterialBalanceChanged { .. })
    });
    let mut supporting = facts_for_range(candidate, facts, 0, 0);
    supporting.push(recapture);
    proof(candidate, Concept::Desperado, 0, 0, supporting, outcomes)
}

fn detect_clearance(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    if first.captured.is_some() || step_gives_check(facts, &first.step_ref) {
        return None;
    }
    let opened_access = facts.iter().find(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::AttackSetChanged {
                step_ref,
                before_attack_ref,
                added_squares,
                ..
            } if step_ref == &first.step_ref
                && !added_squares.is_empty()
                && facts.iter().any(|attack| {
                    &attack.fact_ref == before_attack_ref
                        && matches!(
                            &attack.data,
                            AtomicChessFactData::AttackSet { attacker, .. }
                                if attacker.color == first.mover
                                    && attacker.square != first.from_square
                        )
                })
        )
    })?;
    let (payoff, outcome_refs) =
        first_outcome_by_mover(candidate, facts, 1, first.mover, is_forcing_payoff)?;
    let mut supporting = facts_for_range(candidate, facts, 0, payoff);
    supporting.push(opened_access.fact_ref.clone());
    proof(
        candidate,
        Concept::Clearance,
        0,
        payoff,
        supporting,
        outcome_refs,
    )
}

fn detect_interference(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    let reply = candidate.contract.line_steps.get(1)?;
    if reply.mover == first.mover || reply.role == PieceRole::King {
        return None;
    }
    let (blocker, blocker_square, target_square) =
        facts.iter().find_map(|fact| match &fact.data {
            AtomicChessFactData::SoleRayBlocker {
                snapshot_ref,
                blocker,
                attacker,
                target,
            } if snapshot_ref == &reply.after_snapshot_ref
                && blocker.square == reply.to_square
                && blocker.color == reply.mover
                && attacker.color == first.mover =>
            {
                Some((*fact, &blocker.square, &target.square))
            }
            _ => None,
        })?;
    let payoff = candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .skip(2)
        .find_map(|(index, step)| {
            (step.mover == first.mover
                && step.captured.as_ref().is_some_and(|captured| {
                    &captured.square == blocker_square || &captured.square == target_square
                }))
            .then_some(index)
        })?;
    let outcome_refs = outcomes_for_step(candidate, facts, payoff, |outcome| {
        matches!(outcome, SemanticOutcomeData::MaterialBalanceChanged { .. })
    });
    let mut supporting = facts_for_range(candidate, facts, 0, payoff);
    supporting.push(blocker.fact_ref.clone());
    proof(
        candidate,
        Concept::Interference,
        0,
        payoff,
        supporting,
        outcome_refs,
    )
}

fn detect_attraction(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    let reply = candidate.contract.line_steps.get(1)?;
    if reply.mover == first.mover || reply.role == PieceRole::King {
        return None;
    }
    let payoff = candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .skip(2)
        .find_map(|(index, step)| {
            step.captured
                .as_ref()
                .is_some_and(|captured| {
                    captured.color == reply.mover
                        && captured.role == reply.role
                        && captured.square == reply.to_square
                })
                .then_some(index)
        })?;
    let outcomes = outcomes_for_step(candidate, facts, payoff, is_forcing_payoff);
    proof(
        candidate,
        Concept::Attraction,
        0,
        payoff,
        facts_for_range(candidate, facts, 0, payoff),
        outcomes,
    )
}

fn detect_deflection(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    if first.captured.is_none() && !step_gives_check(facts, &first.step_ref) {
        return None;
    }
    let reply = candidate.contract.line_steps.get(1)?;
    if reply.mover == first.mover || reply.role == PieceRole::King {
        return None;
    }
    let reply_changed_access = facts.iter().any(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::AttackSetChanged { step_ref, .. }
                if step_ref == &reply.step_ref
        )
    });
    if !reply_changed_access {
        return None;
    }
    let (payoff, outcomes) =
        first_outcome_by_mover(candidate, facts, 2, first.mover, is_forcing_payoff)?;
    proof(
        candidate,
        Concept::Deflection,
        0,
        payoff,
        facts_for_range(candidate, facts, 0, payoff),
        outcomes,
    )
}

fn detect_collinear(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    if !matches!(
        first.role,
        PieceRole::Bishop | PieceRole::Rook | PieceRole::Queen
    ) || !aligned(&first.from_square, &first.to_square)
    {
        return None;
    }
    let (payoff, outcomes) =
        first_outcome_by_mover(candidate, facts, 1, first.mover, is_forcing_payoff)?;
    proof(
        candidate,
        Concept::CollinearMove,
        0,
        payoff,
        facts_for_range(candidate, facts, 0, payoff),
        outcomes,
    )
}

fn detect_defensive_move(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
    construction: &CandidateConstruction,
) -> Option<DetectedConcept> {
    if candidate.contract.assessment.rank != Some(1)
        || construction
            .candidates
            .iter()
            .filter(|candidate| {
                candidate
                    .contract
                    .origins
                    .contains(&DecisionCandidateOrigin::EngineRanked)
            })
            .count()
            < 2
    {
        return None;
    }
    let focal_outcome = candidate.contract.outcomes.iter().find(|outcome| {
        matches!(
            outcome.data,
            SemanticOutcomeData::CheckStateChanged {
                ref removed_checkers,
                ..
            } if !removed_checkers.is_empty()
        ) || matches!(
            outcome.data,
            SemanticOutcomeData::AttackAccessChanged {
                ref removed_squares,
                ..
            } if !removed_squares.is_empty()
        ) || matches!(
            outcome.data,
            SemanticOutcomeData::KingZonePressureChanged {
                ref removed_attackers,
                ..
            } if !removed_attackers.is_empty()
        )
    })?;
    let mut comparisons = Vec::new();
    for alternative in construction
        .candidates
        .iter()
        .filter(|other| other.contract.candidate_ref != candidate.contract.candidate_ref)
    {
        let alternative_facts = construction
            .facts
            .iter()
            .filter(|fact| alternative.contract.fact_refs.contains(&fact.fact_ref))
            .collect::<Vec<_>>();
        let adverse = adverse_outcome(
            alternative,
            &alternative_facts,
            candidate.contract.line_steps[0].mover,
        )?;
        comparisons.push(SemanticComparison {
            preferred_outcome_ref: focal_outcome.outcome_ref.clone(),
            alternative_outcome_ref: adverse.outcome_ref.clone(),
            relation: SemanticComparisonRelation::Refutes,
        });
    }
    if comparisons.is_empty() {
        return None;
    }
    let payoff = outcome_step_index(candidate, facts, focal_outcome)?;
    let mut detected = proof(
        candidate,
        Concept::DefensiveMove,
        0,
        payoff,
        facts_for_range(candidate, facts, 0, payoff),
        vec![focal_outcome.outcome_ref.clone()],
    )?;
    detected.semantic_comparisons = comparisons;
    Some(detected)
}

fn detect_quiet_move(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let first = candidate.contract.line_steps.first()?;
    if first.captured.is_some() || step_gives_check(facts, &first.step_ref) {
        return None;
    }
    let reply = candidate.contract.line_steps.get(1)?;
    if step_is_adverse(facts, reply) {
        return None;
    }
    let (payoff, outcomes) =
        first_outcome_by_mover(candidate, facts, 2, first.mover, is_forcing_payoff)?;
    let mut supporting = facts_for_range(candidate, facts, 0, payoff);
    supporting.extend(complete_absence_facts(facts, first));
    supporting.extend(complete_absence_facts(facts, reply));
    proof(
        candidate,
        Concept::QuietMove,
        0,
        payoff,
        supporting,
        outcomes,
    )
}

fn adverse_outcome<'a>(
    candidate: &'a ReplayedCandidate,
    facts: &[&AtomicChessFact],
    defended_side: Color,
) -> Option<&'a SemanticOutcome> {
    candidate.contract.outcomes.iter().find(|outcome| {
        let Some(index) = outcome_step_index(candidate, facts, outcome) else {
            return false;
        };
        candidate.contract.line_steps[index].mover != defended_side
            && is_forcing_payoff(&outcome.data)
    })
}

fn first_outcome(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
    start: usize,
    predicate: impl Fn(&SemanticOutcomeData) -> bool,
) -> Option<(usize, Vec<SemanticOutcomeRef>)> {
    candidate
        .contract
        .outcomes
        .iter()
        .filter(|outcome| predicate(&outcome.data))
        .filter_map(|outcome| {
            let index = outcome_step_index(candidate, facts, outcome)?;
            (index >= start).then_some((index, outcome.outcome_ref.clone()))
        })
        .min_by_key(|(index, reference)| (*index, reference.clone()))
        .map(|(index, reference)| (index, vec![reference]))
}

fn first_outcome_by_mover(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
    start: usize,
    mover: Color,
    predicate: impl Fn(&SemanticOutcomeData) -> bool,
) -> Option<(usize, Vec<SemanticOutcomeRef>)> {
    candidate
        .contract
        .outcomes
        .iter()
        .filter(|outcome| predicate(&outcome.data))
        .filter_map(|outcome| {
            let index = outcome_step_index(candidate, facts, outcome)?;
            (index >= start && candidate.contract.line_steps[index].mover == mover)
                .then_some((index, outcome.outcome_ref.clone()))
        })
        .min_by_key(|(index, reference)| (*index, reference.clone()))
        .map(|(index, reference)| (index, vec![reference]))
}

fn outcomes_for_step(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
    index: usize,
    predicate: impl Fn(&SemanticOutcomeData) -> bool,
) -> Vec<SemanticOutcomeRef> {
    candidate
        .contract
        .outcomes
        .iter()
        .filter(|outcome| predicate(&outcome.data))
        .filter(|outcome| outcome_step_index(candidate, facts, outcome) == Some(index))
        .map(|outcome| outcome.outcome_ref.clone())
        .collect()
}

fn facts_for_range(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
    start: usize,
    end: usize,
) -> Vec<AtomicFactRef> {
    let step_refs = candidate.contract.line_steps[start..=end]
        .iter()
        .map(|step| &step.step_ref)
        .collect::<Vec<_>>();
    facts
        .iter()
        .filter(|fact| {
            fact_step_ref(&fact.data).is_some_and(|step_ref| step_refs.contains(&step_ref))
        })
        .map(|fact| fact.fact_ref.clone())
        .collect()
}

fn complete_absence_facts(
    facts: &[&AtomicChessFact],
    step: &crate::review_session_contract::DecisionLineStep,
) -> Vec<AtomicFactRef> {
    facts
        .iter()
        .filter(|fact| {
            matches!(
                &fact.data,
                AtomicChessFactData::PieceMoved { step_ref, .. }
                    if step_ref == &step.step_ref
            ) || matches!(
                &fact.data,
                AtomicChessFactData::Checkers {
                    snapshot_ref,
                    checking_pieces,
                    ..
                } if snapshot_ref == &step.after_snapshot_ref && checking_pieces.is_empty()
            ) || matches!(
                &fact.data,
                AtomicChessFactData::MaterialInventory { snapshot_ref, .. }
                    if snapshot_ref == &step.before_snapshot_ref
                        || snapshot_ref == &step.after_snapshot_ref
            )
        })
        .map(|fact| fact.fact_ref.clone())
        .collect()
}

fn legal_recaptures_at(
    facts: &[&AtomicChessFact],
    snapshot_ref: &crate::review_session_contract::DecisionPositionSnapshotRef,
    target_square: &crate::review_session_contract::Square,
) -> Option<AtomicFactRef> {
    facts.iter().find_map(|fact| match &fact.data {
        AtomicChessFactData::LegalRecaptures {
            snapshot_ref: fact_snapshot,
            target_square: fact_target,
            moves,
            ..
        } if fact_snapshot == snapshot_ref && fact_target == target_square && !moves.is_empty() => {
            Some(fact.fact_ref.clone())
        }
        _ => None,
    })
}

fn step_gives_check(facts: &[&AtomicChessFact], step_ref: &LineStepRef) -> bool {
    facts.iter().any(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::CheckersChanged {
                step_ref: fact_step,
                added_checkers,
                ..
            } if fact_step == step_ref && !added_checkers.is_empty()
        )
    })
}

fn step_is_adverse(
    facts: &[&AtomicChessFact],
    step: &crate::review_session_contract::DecisionLineStep,
) -> bool {
    facts.iter().any(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::MaterialChanged { step_ref, .. }
                if step_ref == &step.step_ref
        ) || matches!(
            &fact.data,
            AtomicChessFactData::CheckersChanged {
                step_ref,
                added_checkers,
                ..
            } if step_ref == &step.step_ref && !added_checkers.is_empty()
        )
    })
}

fn is_forcing_payoff(outcome: &SemanticOutcomeData) -> bool {
    matches!(
        outcome,
        SemanticOutcomeData::MaterialBalanceChanged { .. }
            | SemanticOutcomeData::TerminalStateReached { .. }
    ) || matches!(
        outcome,
        SemanticOutcomeData::CheckStateChanged { added_checkers, .. }
            if !added_checkers.is_empty()
    ) || matches!(
        outcome,
        SemanticOutcomeData::KingZonePressureChanged { added_attackers, .. }
            if !added_attackers.is_empty()
    )
}

fn aligned(
    from: &crate::review_session_contract::Square,
    to: &crate::review_session_contract::Square,
) -> bool {
    let bytes_from = from.as_str().as_bytes();
    let bytes_to = to.as_str().as_bytes();
    bytes_from[0] == bytes_to[0]
        || bytes_from[1] == bytes_to[1]
        || bytes_from[0].abs_diff(bytes_to[0]) == bytes_from[1].abs_diff(bytes_to[1])
}

fn value(role: PieceRole) -> u8 {
    role.conventional_material_value().unwrap_or(u8::MAX)
}
