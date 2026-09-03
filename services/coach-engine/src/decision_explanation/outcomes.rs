use std::collections::BTreeMap;

use shakmaty::{Chess, Position};

use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, AtomicFactRef, DecisionLineStep, SemanticOutcome,
    SemanticOutcomeData,
};

use super::{
    candidate::{chess_square, contract_square},
    facts::{push_fact, push_outcome},
    DecisionExplanationContractError,
};

pub(super) fn record_attack_transitions(
    before: &Chess,
    after: &Chess,
    step: &DecisionLineStep,
    facts: &mut BTreeMap<AtomicFactRef, AtomicChessFact>,
    candidate_fact_refs: &mut Vec<AtomicFactRef>,
    outcomes: &mut Vec<SemanticOutcome>,
) -> Result<(), DecisionExplanationContractError> {
    let mut transitions = before
        .board()
        .occupied()
        .into_iter()
        .filter_map(|square| {
            let piece = before.board().piece_at(square)?;
            (after.board().piece_at(square) == Some(piece)).then_some((square, square))
        })
        .collect::<Vec<_>>();
    let from = chess_square(&step.from_square);
    let to = chess_square(&step.to_square);
    transitions.push((from, to));
    transitions.sort();
    transitions.dedup();

    for (before_square, after_square) in transitions {
        let Some((before_ref, before_squares)) = find_attack_set(
            facts,
            &step.before_snapshot_ref,
            &contract_square(before_square),
        ) else {
            continue;
        };
        let Some((after_ref, after_squares)) = find_attack_set(
            facts,
            &step.after_snapshot_ref,
            &contract_square(after_square),
        ) else {
            continue;
        };
        let added_squares = after_squares
            .iter()
            .filter(|square| !before_squares.contains(square))
            .cloned()
            .collect::<Vec<_>>();
        let removed_squares = before_squares
            .iter()
            .filter(|square| !after_squares.contains(square))
            .cloned()
            .collect::<Vec<_>>();
        if added_squares.is_empty() && removed_squares.is_empty() {
            continue;
        }
        let changed_ref = push_fact(
            facts,
            candidate_fact_refs,
            AtomicChessFactData::AttackSetChanged {
                step_ref: step.step_ref.clone(),
                before_attack_ref: before_ref.clone(),
                after_attack_ref: after_ref.clone(),
                added_squares: added_squares.clone(),
                removed_squares: removed_squares.clone(),
            },
        );
        push_outcome(
            outcomes,
            SemanticOutcomeData::AttackAccessChanged {
                before_attack_ref: before_ref,
                after_attack_ref: after_ref,
                added_squares,
                removed_squares,
            },
            vec![changed_ref],
        );
    }
    for color in [
        crate::review_session_contract::Color::White,
        crate::review_session_contract::Color::Black,
    ] {
        record_check_transition(step, color, facts, candidate_fact_refs, outcomes);
        record_pressure_transition(step, color, facts, candidate_fact_refs, outcomes);
    }
    Ok(())
}

fn record_check_transition(
    step: &DecisionLineStep,
    color: crate::review_session_contract::Color,
    facts: &mut BTreeMap<AtomicFactRef, AtomicChessFact>,
    candidate_fact_refs: &mut Vec<AtomicFactRef>,
    outcomes: &mut Vec<SemanticOutcome>,
) {
    let Some((before_ref, before_checkers)) =
        find_checkers(facts, &step.before_snapshot_ref, color)
    else {
        return;
    };
    let Some((after_ref, after_checkers)) = find_checkers(facts, &step.after_snapshot_ref, color)
    else {
        return;
    };
    let added_checkers = after_checkers
        .iter()
        .filter(|piece| !before_checkers.contains(piece))
        .cloned()
        .collect::<Vec<_>>();
    let removed_checkers = before_checkers
        .iter()
        .filter(|piece| !after_checkers.contains(piece))
        .cloned()
        .collect::<Vec<_>>();
    if added_checkers.is_empty() && removed_checkers.is_empty() {
        return;
    }
    let changed_ref = push_fact(
        facts,
        candidate_fact_refs,
        AtomicChessFactData::CheckersChanged {
            step_ref: step.step_ref.clone(),
            before_checkers_ref: before_ref.clone(),
            after_checkers_ref: after_ref.clone(),
            added_checkers: added_checkers.clone(),
            removed_checkers: removed_checkers.clone(),
        },
    );
    push_outcome(
        outcomes,
        SemanticOutcomeData::CheckStateChanged {
            before_checkers_ref: before_ref,
            after_checkers_ref: after_ref,
            added_checkers,
            removed_checkers,
        },
        vec![changed_ref],
    );
}

fn record_pressure_transition(
    step: &DecisionLineStep,
    color: crate::review_session_contract::Color,
    facts: &mut BTreeMap<AtomicFactRef, AtomicChessFact>,
    candidate_fact_refs: &mut Vec<AtomicFactRef>,
    outcomes: &mut Vec<SemanticOutcome>,
) {
    let Some((before_ref, before_attackers)) =
        find_pressure(facts, &step.before_snapshot_ref, color)
    else {
        return;
    };
    let Some((after_ref, after_attackers)) = find_pressure(facts, &step.after_snapshot_ref, color)
    else {
        return;
    };
    let added_attackers = after_attackers
        .iter()
        .filter(|piece| !before_attackers.contains(piece))
        .cloned()
        .collect::<Vec<_>>();
    let removed_attackers = before_attackers
        .iter()
        .filter(|piece| !after_attackers.contains(piece))
        .cloned()
        .collect::<Vec<_>>();
    if added_attackers.is_empty() && removed_attackers.is_empty() {
        return;
    }
    let changed_ref = push_fact(
        facts,
        candidate_fact_refs,
        AtomicChessFactData::KingZonePressureChanged {
            step_ref: step.step_ref.clone(),
            before_pressure_ref: before_ref.clone(),
            after_pressure_ref: after_ref.clone(),
            added_attackers: added_attackers.clone(),
            removed_attackers: removed_attackers.clone(),
        },
    );
    push_outcome(
        outcomes,
        SemanticOutcomeData::KingZonePressureChanged {
            before_pressure_ref: before_ref,
            after_pressure_ref: after_ref,
            added_attackers,
            removed_attackers,
        },
        vec![changed_ref],
    );
}

fn find_attack_set(
    facts: &BTreeMap<AtomicFactRef, AtomicChessFact>,
    snapshot_ref: &crate::review_session_contract::DecisionPositionSnapshotRef,
    square: &crate::review_session_contract::Square,
) -> Option<(AtomicFactRef, Vec<crate::review_session_contract::Square>)> {
    facts.values().find_map(|fact| match &fact.data {
        AtomicChessFactData::AttackSet {
            snapshot_ref: fact_snapshot,
            attacker,
            attacked_squares,
        } if fact_snapshot == snapshot_ref && &attacker.square == square => {
            Some((fact.fact_ref.clone(), attacked_squares.clone()))
        }
        _ => None,
    })
}

fn find_checkers(
    facts: &BTreeMap<AtomicFactRef, AtomicChessFact>,
    snapshot_ref: &crate::review_session_contract::DecisionPositionSnapshotRef,
    color: crate::review_session_contract::Color,
) -> Option<(
    AtomicFactRef,
    Vec<crate::review_session_contract::PieceAtSquare>,
)> {
    facts.values().find_map(|fact| match &fact.data {
        AtomicChessFactData::Checkers {
            snapshot_ref: fact_snapshot,
            king,
            checking_pieces,
        } if fact_snapshot == snapshot_ref && king.color == color => {
            Some((fact.fact_ref.clone(), checking_pieces.clone()))
        }
        _ => None,
    })
}

fn find_pressure(
    facts: &BTreeMap<AtomicFactRef, AtomicChessFact>,
    snapshot_ref: &crate::review_session_contract::DecisionPositionSnapshotRef,
    color: crate::review_session_contract::Color,
) -> Option<(
    AtomicFactRef,
    Vec<crate::review_session_contract::PieceAtSquare>,
)> {
    facts.values().find_map(|fact| match &fact.data {
        AtomicChessFactData::KingZonePressure {
            snapshot_ref: fact_snapshot,
            king,
            attacking_pieces,
            ..
        } if fact_snapshot == snapshot_ref && king.color == color => {
            Some((fact.fact_ref.clone(), attacking_pieces.clone()))
        }
        _ => None,
    })
}
