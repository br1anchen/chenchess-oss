use shakmaty::{Position, Role};

use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, AtomicFactRef, CurriculumLearningConcept as Concept,
    PieceRole, SemanticOutcomeRef, Square,
};

use super::{fact_step_ref, proof, DetectedConcept};
use crate::decision_explanation::{
    candidate::{chess_square, ReplayedCandidate},
    DecisionExplanationContractError,
};

pub(super) fn detect(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Result<Vec<DetectedConcept>, DecisionExplanationContractError> {
    let mut detected = detect_check_relationship(candidate, facts);
    detected.extend(detect_fork(candidate, facts)?);
    detected.extend(detect_skewer(candidate, facts));
    detected.extend(detect_ray_relationship(candidate, facts));
    detected.extend(detect_discovered_attack(candidate, facts));
    detected.extend(detect_overloaded_piece(candidate, facts));
    detected.extend(detect_capturing_defender(candidate, facts));
    detected.extend(detect_hanging_piece(candidate, facts));
    detected.extend(detect_f2_f7(candidate, facts));
    detected.extend(detect_king_pressure(candidate, facts));
    detected.extend(detect_trapped_piece(candidate, facts));
    Ok(detected)
}

fn detect_fork(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Result<Option<DetectedConcept>, DecisionExplanationContractError> {
    let [setup, _reply, payoff, ..] = candidate.contract.line_steps.as_slice() else {
        return Ok(None);
    };
    if payoff.mover != setup.mover
        || payoff.from_square != setup.to_square
        || payoff.captured.is_none()
    {
        return Ok(None);
    }
    let after_setup = &candidate.positions[1];
    let setup_square = chess_square(&setup.to_square);
    let Some(attacker) = after_setup.board().piece_at(setup_square) else {
        return Ok(None);
    };
    if attacker.role == Role::King {
        return Ok(None);
    }
    let attacked = after_setup.board().attacks_from(setup_square)
        & after_setup.board().by_color(!attacker.color);
    let eligible = attacked
        .into_iter()
        .filter_map(|square| {
            after_setup
                .board()
                .piece_at(square)
                .map(|piece| (square, piece.role))
        })
        .filter(|(_, role)| *role == Role::King || piece_value(*role) > piece_value(attacker.role))
        .collect::<Vec<_>>();
    let payoff_square = chess_square(&payoff.to_square);
    if eligible.len() < 2
        || !eligible
            .iter()
            .any(|(square, role)| *square == payoff_square && *role != Role::King)
    {
        return Ok(None);
    }
    let attack =
        find_attack_set(facts, &setup.after_snapshot_ref, &setup.to_square).map(|(fact, _)| fact);
    let capture = find_step_fact(facts, &payoff.step_ref, |data| {
        matches!(data, AtomicChessFactData::PieceCaptured { .. })
    });
    let changed = find_step_fact(facts, &payoff.step_ref, |data| {
        matches!(data, AtomicChessFactData::MaterialChanged { .. })
    });
    let (Some(attack), Some(capture), Some(changed)) = (attack, capture, changed) else {
        return Err(DecisionExplanationContractError::InvalidProof(
            "fork proof facts were not retained",
        ));
    };
    let Some(outcome) = outcome_supported_by(candidate, &changed.fact_ref) else {
        return Err(DecisionExplanationContractError::InvalidProof(
            "fork payoff has no material outcome",
        ));
    };
    Ok(proof(
        candidate,
        Concept::Fork,
        0,
        2,
        vec![
            attack.fact_ref.clone(),
            capture.fact_ref.clone(),
            changed.fact_ref.clone(),
        ],
        vec![outcome],
    ))
}

fn detect_check_relationship(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = Vec::new();
    for (index, step) in candidate.contract.line_steps.iter().enumerate() {
        let Some((checkers, checking_pieces)) =
            facts.iter().copied().find_map(|fact| match &fact.data {
                AtomicChessFactData::Checkers {
                    snapshot_ref,
                    checking_pieces,
                    ..
                } if snapshot_ref == &step.after_snapshot_ref && !checking_pieces.is_empty() => {
                    Some((fact, checking_pieces))
                }
                _ => None,
            })
        else {
            continue;
        };
        let Some(changed) = find_step_fact(facts, &step.step_ref, |data| {
            matches!(data, AtomicChessFactData::CheckersChanged { .. })
        }) else {
            continue;
        };
        let Some(outcome) = outcome_supported_by(candidate, &changed.fact_ref) else {
            continue;
        };
        let concept = if checking_pieces.len() >= 2 {
            Concept::DoubleCheck
        } else if checking_pieces
            .iter()
            .all(|checker| checker.square != step.to_square)
        {
            Concept::DiscoveredCheck
        } else {
            continue;
        };
        let supporting_fact_refs = vec![checkers.fact_ref.clone(), changed.fact_ref.clone()];
        let outcome_refs = vec![outcome];
        if let Some(proof) = proof(
            candidate,
            concept,
            index,
            index,
            supporting_fact_refs.clone(),
            outcome_refs.clone(),
        ) {
            detected.push(proof);
        }
        if concept == Concept::DiscoveredCheck {
            // A discovered check is also a discovered attack on the king. The
            // broad realization intentionally cites the same check outcome so
            // graph specificity can recognize that these are the same event.
            if let Some(proof) = proof(
                candidate,
                Concept::DiscoveredAttack,
                index,
                index,
                supporting_fact_refs,
                outcome_refs,
            ) {
                detected.push(proof);
            }
        }
    }
    detected
}

fn detect_skewer(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let [setup, reply, payoff, ..] = candidate.contract.line_steps.as_slice() else {
        return None;
    };
    let captured = payoff.captured.as_ref()?;
    if payoff.mover != setup.mover
        || payoff.from_square != setup.to_square
        || captured.square != payoff.to_square
        || reply.mover == setup.mover
        || reply.role != PieceRole::King
    {
        return None;
    }
    let check_changed = find_step_fact(facts, &setup.step_ref, |data| {
        matches!(
            data,
            AtomicChessFactData::CheckersChanged {
                added_checkers,
                ..
            } if added_checkers.iter().any(|checker| checker.color == setup.mover
                && checker.role == payoff.role
                && checker.square == setup.to_square)
        )
    })?;
    let ray = facts.iter().find(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::SoleRayBlocker {
                snapshot_ref,
                attacker,
                blocker,
                target,
            } if snapshot_ref == &setup.after_snapshot_ref
                && attacker.color == setup.mover
                && attacker.role == payoff.role
                && attacker.square == setup.to_square
                && blocker.color == reply.mover
                && blocker.role == PieceRole::King
                && blocker.square == reply.from_square
                && target == captured
        )
    })?;
    let material_changed = find_step_fact(facts, &payoff.step_ref, |data| {
        matches!(data, AtomicChessFactData::MaterialChanged { .. })
    })?;
    proof(
        candidate,
        Concept::Skewer,
        0,
        2,
        vec![
            check_changed.fact_ref.clone(),
            ray.fact_ref.clone(),
            material_changed.fact_ref.clone(),
        ],
        vec![
            outcome_supported_by(candidate, &check_changed.fact_ref)?,
            outcome_supported_by(candidate, &material_changed.fact_ref)?,
        ],
    )
}

fn detect_ray_relationship(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = Vec::new();
    for (index, step) in candidate.contract.line_steps.iter().enumerate() {
        for (ray, attacker, blocker, target) in facts.iter().copied().filter_map(|fact| match &fact
            .data
        {
            AtomicChessFactData::SoleRayBlocker {
                snapshot_ref,
                attacker,
                blocker,
                target,
            } if snapshot_ref == &step.after_snapshot_ref && attacker.square == step.to_square => {
                Some((fact, attacker, blocker, target))
            }
            _ => None,
        }) {
            let relationship_preexisted = facts.iter().any(|fact| {
                matches!(
                    &fact.data,
                    AtomicChessFactData::SoleRayBlocker {
                        snapshot_ref,
                        attacker: before_attacker,
                        blocker: before_blocker,
                        target: before_target,
                    } if snapshot_ref == &step.before_snapshot_ref
                        && before_attacker.color == attacker.color
                        && before_attacker.role == attacker.role
                        && before_attacker.square == step.from_square
                        && before_blocker == blocker
                        && before_target == target
                )
            });
            if relationship_preexisted {
                continue;
            }
            let concept = if target.role == PieceRole::King
                || piece_role_value(target.role) > piece_role_value(blocker.role)
            {
                Concept::Pin
            } else {
                Concept::XRayAttack
            };
            let Some(changed) = attack_change_ending_at(facts, &step.step_ref, &step.to_square)
            else {
                continue;
            };
            let Some(outcome) = outcome_supported_by(candidate, &changed.fact_ref) else {
                continue;
            };
            if let Some(proof) = proof(
                candidate,
                concept,
                index,
                index,
                vec![ray.fact_ref.clone(), changed.fact_ref.clone()],
                vec![outcome],
            ) {
                detected.push(proof);
            }
        }
    }
    detected
}

fn detect_discovered_attack(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = Vec::new();
    for (index, (step, after)) in candidate
        .contract
        .line_steps
        .iter()
        .zip(candidate.positions.iter().skip(1))
        .enumerate()
    {
        for changed in facts.iter().filter(|fact| {
            let AtomicChessFactData::AttackSetChanged {
                step_ref,
                after_attack_ref,
                added_squares,
                ..
            } = &fact.data
            else {
                return false;
            };
            if step_ref != &step.step_ref || added_squares.is_empty() {
                return false;
            }
            let Some(attacker) = attack_fact_by_ref(facts, after_attack_ref) else {
                return false;
            };
            attacker.0.square != step.to_square
                && added_squares.iter().any(|square| {
                    after
                        .board()
                        .piece_at(chess_square(square))
                        .is_some_and(|piece| {
                            piece.color != to_chess_color(step.mover) && piece.role != Role::King
                        })
                })
        }) {
            let Some(outcome) = outcome_supported_by(candidate, &changed.fact_ref) else {
                continue;
            };
            if let Some(proof) = proof(
                candidate,
                Concept::DiscoveredAttack,
                index,
                index,
                vec![changed.fact_ref.clone()],
                vec![outcome],
            ) {
                detected.push(proof);
            }
        }
    }
    detected
}

fn detect_overloaded_piece(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let [setup, reply, payoff, ..] = candidate.contract.line_steps.as_slice() else {
        return None;
    };
    if setup.captured.is_none()
        || reply.captured.is_none()
        || payoff.captured.is_none()
        || reply.mover == setup.mover
        || payoff.mover != setup.mover
    {
        return None;
    }
    let (defender_attacks, attacked_squares) =
        find_attack_set(facts, &setup.before_snapshot_ref, &reply.from_square)?;
    if !attacked_squares.contains(&setup.to_square) || !attacked_squares.contains(&payoff.to_square)
    {
        return None;
    }
    let setup_changed = find_step_fact(facts, &setup.step_ref, |data| {
        matches!(data, AtomicChessFactData::MaterialChanged { .. })
    })?;
    let payoff_changed = find_step_fact(facts, &payoff.step_ref, |data| {
        matches!(data, AtomicChessFactData::MaterialChanged { .. })
    })?;
    proof(
        candidate,
        Concept::OverloadedPiece,
        0,
        2,
        vec![
            defender_attacks.fact_ref.clone(),
            setup_changed.fact_ref.clone(),
            payoff_changed.fact_ref.clone(),
        ],
        vec![
            outcome_supported_by(candidate, &setup_changed.fact_ref)?,
            outcome_supported_by(candidate, &payoff_changed.fact_ref)?,
        ],
    )
}

fn detect_capturing_defender(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let setup = candidate.contract.line_steps.first()?;
    setup.captured.as_ref()?;
    let (payoff_index, payoff) = candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, step)| step.mover == setup.mover && step.captured.is_some())?;
    let (defender_attacks, attacked_squares) =
        find_attack_set(facts, &setup.before_snapshot_ref, &setup.to_square)?;
    if !attacked_squares.contains(&payoff.to_square) {
        return None;
    }
    let setup_changed = find_step_fact(facts, &setup.step_ref, |data| {
        matches!(data, AtomicChessFactData::MaterialChanged { .. })
    })?;
    let payoff_changed = find_step_fact(facts, &payoff.step_ref, |data| {
        matches!(data, AtomicChessFactData::MaterialChanged { .. })
    })?;
    proof(
        candidate,
        Concept::CapturingDefender,
        0,
        payoff_index,
        vec![
            defender_attacks.fact_ref.clone(),
            setup_changed.fact_ref.clone(),
            payoff_changed.fact_ref.clone(),
        ],
        vec![
            outcome_supported_by(candidate, &setup_changed.fact_ref)?,
            outcome_supported_by(candidate, &payoff_changed.fact_ref)?,
        ],
    )
}

fn detect_hanging_piece(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = Vec::new();
    for (index, (capture, (before, after))) in candidate
        .contract
        .line_steps
        .iter()
        .zip(
            candidate
                .positions
                .iter()
                .zip(candidate.positions.iter().skip(1)),
        )
        .enumerate()
    {
        let Some(captured) = capture.captured.as_ref() else {
            continue;
        };
        if captured.role == PieceRole::Pawn {
            continue;
        }
        let capture_square = chess_square(&capture.to_square);
        let Some(mover_king) = before.board().king_of(to_chess_color(capture.mover)) else {
            continue;
        };
        let captured_was_checking = before
            .board()
            .piece_at(capture_square)
            .is_some_and(|piece| {
                shakmaty::attacks::attacks(capture_square, piece, before.board().occupied())
                    .contains(mover_king)
            });
        if captured_was_checking
            || after
                .legal_moves()
                .iter()
                .any(|reply| reply.to() == capture_square && reply.capture().is_some())
        {
            continue;
        }
        let Some(changed) = find_step_fact(facts, &capture.step_ref, |data| {
            matches!(data, AtomicChessFactData::MaterialChanged { .. })
        }) else {
            continue;
        };
        let Some(outcome) = outcome_supported_by(candidate, &changed.fact_ref) else {
            continue;
        };
        if let Some(proof) = proof(
            candidate,
            Concept::HangingPiece,
            index,
            index,
            vec![changed.fact_ref.clone()],
            vec![outcome],
        ) {
            detected.push(proof);
        }
    }
    detected
}

fn detect_f2_f7(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let step = candidate.contract.line_steps.first()?;
    let (target, target_square) = match step.mover {
        crate::review_session_contract::Color::White => ("f7", shakmaty::Square::F7),
        crate::review_session_contract::Color::Black => ("f2", shakmaty::Square::F2),
    };
    let (attack, attacked_squares) =
        find_attack_set(facts, &step.after_snapshot_ref, &step.to_square)?;
    if !attacked_squares
        .iter()
        .any(|square| square.as_str() == target)
    {
        return None;
    }
    let after = &candidate.positions[1];
    if after
        .board()
        .piece_at(target_square)
        .is_none_or(|piece| piece.color == to_chess_color(step.mover))
    {
        return None;
    }
    let changed = attack_change_ending_at(facts, &step.step_ref, &step.to_square)?;
    proof(
        candidate,
        Concept::AttackingF2F7,
        0,
        0,
        vec![attack.fact_ref.clone(), changed.fact_ref.clone()],
        vec![outcome_supported_by(candidate, &changed.fact_ref)?],
    )
}

fn detect_king_pressure(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let step = candidate.contract.line_steps.first()?;
    let (pressure, king) = facts.iter().copied().find_map(|fact| match &fact.data {
        AtomicChessFactData::KingZonePressure {
            snapshot_ref,
            king,
            attacking_pieces,
            ..
        } if snapshot_ref == &step.after_snapshot_ref
            && king.color != step.mover
            && attacking_pieces.len() >= 2
            && attacking_pieces
                .iter()
                .any(|piece| piece.square == step.to_square) =>
        {
            Some((fact, king))
        }
        _ => None,
    })?;
    let changed = find_step_fact(facts, &step.step_ref, |data| {
        matches!(
            data,
            AtomicChessFactData::KingZonePressureChanged {
                added_attackers,
                ..
            } if !added_attackers.is_empty()
        )
    })?;
    let after = &candidate.positions[1];
    let king_square = chess_square(&king.square);
    let king_color = to_chess_color(king.color);
    let pawn_shield = shakmaty::attacks::king_attacks(king_square)
        .into_iter()
        .filter(|square| {
            after
                .board()
                .piece_at(*square)
                .is_some_and(|piece| piece.color == king_color && piece.role == Role::Pawn)
        })
        .count();
    let concept = if pawn_shield == 0 {
        Concept::ExposedKing
    } else {
        match king_square.file() {
            shakmaty::File::E | shakmaty::File::F | shakmaty::File::G | shakmaty::File::H => {
                Concept::KingsideAttack
            }
            shakmaty::File::A | shakmaty::File::B | shakmaty::File::C | shakmaty::File::D => {
                Concept::QueensideAttack
            }
        }
    };
    proof(
        candidate,
        concept,
        0,
        0,
        vec![pressure.fact_ref.clone(), changed.fact_ref.clone()],
        vec![outcome_supported_by(candidate, &changed.fact_ref)?],
    )
}

fn detect_trapped_piece(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Option<DetectedConcept> {
    let step = candidate.contract.line_steps.first()?;
    let after = &candidate.positions[1];
    let (moved_attack, attacked_squares) =
        find_attack_set(facts, &step.after_snapshot_ref, &step.to_square)?;
    let trapped = facts.iter().find(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::LegalDestinations {
                snapshot_ref,
                piece,
                destinations,
            } if snapshot_ref == &step.after_snapshot_ref
                && piece.color != step.mover
                && !matches!(piece.role, PieceRole::Pawn | PieceRole::King)
                && destinations.is_empty()
                && attacked_squares.contains(&piece.square)
                && after.board().piece_at(chess_square(&piece.square)).is_some()
        )
    })?;
    let changed = attack_change_ending_at(facts, &step.step_ref, &step.to_square)?;
    proof(
        candidate,
        Concept::TrappedPiece,
        0,
        0,
        vec![
            moved_attack.fact_ref.clone(),
            trapped.fact_ref.clone(),
            changed.fact_ref.clone(),
        ],
        vec![outcome_supported_by(candidate, &changed.fact_ref)?],
    )
}

fn find_attack_set<'a>(
    facts: &'a [&AtomicChessFact],
    snapshot_ref: &crate::review_session_contract::DecisionPositionSnapshotRef,
    square: &crate::review_session_contract::Square,
) -> Option<(&'a AtomicChessFact, &'a Vec<Square>)> {
    facts.iter().copied().find_map(|fact| match &fact.data {
        AtomicChessFactData::AttackSet {
            snapshot_ref: fact_snapshot,
            attacker,
            attacked_squares,
        } if fact_snapshot == snapshot_ref && &attacker.square == square => {
            Some((fact, attacked_squares))
        }
        _ => None,
    })
}

fn find_step_fact<'a>(
    facts: &'a [&AtomicChessFact],
    step_ref: &crate::review_session_contract::LineStepRef,
    predicate: impl Fn(&AtomicChessFactData) -> bool,
) -> Option<&'a AtomicChessFact> {
    facts
        .iter()
        .copied()
        .find(|fact| fact_step_ref(&fact.data) == Some(step_ref) && predicate(&fact.data))
}

fn attack_change_ending_at<'a>(
    facts: &'a [&AtomicChessFact],
    step_ref: &crate::review_session_contract::LineStepRef,
    square: &crate::review_session_contract::Square,
) -> Option<&'a AtomicChessFact> {
    facts.iter().copied().find(|fact| {
        let AtomicChessFactData::AttackSetChanged {
            step_ref: fact_step,
            after_attack_ref,
            ..
        } = &fact.data
        else {
            return false;
        };
        fact_step == step_ref
            && attack_fact_by_ref(facts, after_attack_ref)
                .is_some_and(|(attacker, _)| &attacker.square == square)
    })
}

fn attack_fact_by_ref<'a>(
    facts: &'a [&AtomicChessFact],
    reference: &AtomicFactRef,
) -> Option<(
    &'a crate::review_session_contract::PieceAtSquare,
    &'a Vec<crate::review_session_contract::Square>,
)> {
    facts.iter().find_map(|fact| match &fact.data {
        AtomicChessFactData::AttackSet {
            attacker,
            attacked_squares,
            ..
        } if &fact.fact_ref == reference => Some((attacker, attacked_squares)),
        _ => None,
    })
}

fn outcome_supported_by(
    candidate: &ReplayedCandidate,
    fact_ref: &AtomicFactRef,
) -> Option<SemanticOutcomeRef> {
    candidate
        .contract
        .outcomes
        .iter()
        .find(|outcome| outcome.supporting_fact_refs.contains(fact_ref))
        .map(|outcome| outcome.outcome_ref.clone())
}

fn to_chess_color(color: crate::review_session_contract::Color) -> shakmaty::Color {
    match color {
        crate::review_session_contract::Color::White => shakmaty::Color::White,
        crate::review_session_contract::Color::Black => shakmaty::Color::Black,
    }
}

fn piece_value(role: Role) -> u8 {
    piece_role_value(match role {
        Role::Pawn => PieceRole::Pawn,
        Role::Knight => PieceRole::Knight,
        Role::Bishop => PieceRole::Bishop,
        Role::Rook => PieceRole::Rook,
        Role::Queen => PieceRole::Queen,
        Role::King => PieceRole::King,
    })
}

fn piece_role_value(role: PieceRole) -> u8 {
    role.conventional_material_value().unwrap_or(u8::MAX)
}
