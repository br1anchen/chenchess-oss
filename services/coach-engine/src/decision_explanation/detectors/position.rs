use shakmaty::{Color, Position};

use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, CurriculumLearningConcept as Concept,
    DecisionTerminalState, PieceRole, SemanticOutcomeData,
};

use super::{
    fact_step_ref, facts_for_step_and_snapshot, outcome_step_index, outcomes_for_step, proof,
    DetectedConcept,
};
use crate::decision_explanation::candidate::{chess_square, ReplayedCandidate};

pub(super) fn detect(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = detect_named_technique(candidate, facts);
    detected.extend(detect_zugzwang(candidate, facts));
    detected.extend(detect_material_endgame(candidate, facts));
    detected
}

fn detect_named_technique(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = Vec::new();
    for ((index, step), after) in candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .zip(candidate.positions.iter().skip(1))
    {
        let outcome_refs = candidate
            .contract
            .outcomes
            .iter()
            .filter(|outcome| {
                outcome.supporting_fact_refs.iter().any(|reference| {
                    facts.iter().any(|fact| {
                        &fact.fact_ref == reference
                            && fact_step_ref(&fact.data) == Some(&step.step_ref)
                    })
                })
            })
            .map(|outcome| outcome.outcome_ref.clone())
            .collect::<Vec<_>>();
        if outcome_refs.is_empty() {
            continue;
        }
        let concepts = recognize_position_techniques(after, step);
        if concepts.is_empty() {
            continue;
        }
        let supporting_fact_refs =
            facts_for_step_and_snapshot(facts, &step.step_ref, &step.after_snapshot_ref, |data| {
                matches!(
                    data,
                    AtomicChessFactData::PieceMoved { .. }
                        | AtomicChessFactData::PieceOccupancy { .. }
                        | AtomicChessFactData::PawnFrontSpanOccupancy { .. }
                        | AtomicChessFactData::LegalDestinations { .. }
                        | AtomicChessFactData::MaterialInventory { .. }
                )
            });
        for concept in concepts {
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
        }
    }
    detected
}

fn recognize_position_techniques(
    position: &shakmaty::Chess,
    step: &crate::review_session_contract::DecisionLineStep,
) -> Vec<Concept> {
    let board = position.board();
    let pawns = board.pawns();
    let rooks = board.rooks();
    let has_only_king_and_pawn_material = rooks.is_empty()
        && board.knights().is_empty()
        && board.bishops().is_empty()
        && board.queens().is_empty();
    let (Some(white_king), Some(black_king)) =
        (board.king_of(Color::White), board.king_of(Color::Black))
    else {
        return Vec::new();
    };
    let mut concepts = Vec::new();

    if step.role == PieceRole::Pawn {
        let destination = chess_square(&step.to_square);
        let is_rook_pawn = matches!(destination.file(), shakmaty::File::A | shakmaty::File::H);
        let mover = match step.mover {
            crate::review_session_contract::Color::White => Color::White,
            crate::review_session_contract::Color::Black => Color::Black,
        };
        let is_seventh = mover.fold_wb(
            destination.rank() == shakmaty::Rank::Seventh,
            destination.rank() == shakmaty::Rank::Second,
        );
        if has_only_king_and_pawn_material && is_rook_pawn && is_seventh {
            concepts.push(Concept::SeventhRankRookPawn);
        }
        if has_only_king_and_pawn_material && kings_in_opposition(white_king, black_king) {
            concepts.push(Concept::Opposition);
        }
        let own_king = mover.fold_wb(white_king, black_king);
        if has_only_king_and_pawn_material && king_supports_pawn(own_king, destination, mover) {
            concepts.push(Concept::KeySquares);
        }
    }

    let is_rook_and_pawn_ending = !pawns.is_empty()
        && board.knights().is_empty()
        && board.bishops().is_empty()
        && board.queens().is_empty()
        && (rooks & board.by_color(Color::White)).count() == 1
        && (rooks & board.by_color(Color::Black)).count() == 1;
    if is_rook_and_pawn_ending {
        if has_lucena_geometry(position) {
            concepts.push(Concept::Lucena);
        }
        if has_philidor_geometry(position) {
            concepts.push(Concept::Philidor);
        }
        if has_passive_rook_geometry(position) {
            concepts.push(Concept::PassiveRookDefense);
        }
        if step.role == PieceRole::Rook {
            concepts.push(Concept::IntermediateRookEndings);
        } else {
            concepts.push(Concept::PracticalRookEndings);
        }
    }
    concepts.sort();
    concepts.dedup();
    concepts
}

fn detect_material_endgame(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = Vec::new();
    for ((index, step), position) in candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .zip(candidate.positions.iter().skip(1))
    {
        let Some(concept) = material_endgame_class(position) else {
            continue;
        };
        let outcome_refs = outcomes_for_step(
            &candidate.contract.outcomes,
            facts,
            &step.step_ref,
            |data| {
                matches!(
                    data,
                    SemanticOutcomeData::MaterialBalanceChanged { .. }
                        | SemanticOutcomeData::MaterialConfigurationChanged { .. }
                        | SemanticOutcomeData::PawnProgressed { .. }
                        | SemanticOutcomeData::TerminalStateReached { .. }
                )
            },
        );
        let supporting_fact_refs =
            facts_for_step_and_snapshot(facts, &step.step_ref, &step.after_snapshot_ref, |data| {
                matches!(
                    data,
                    AtomicChessFactData::PieceMoved { .. }
                        | AtomicChessFactData::PieceOccupancy { .. }
                        | AtomicChessFactData::PawnFrontSpanOccupancy { .. }
                        | AtomicChessFactData::LegalDestinations { .. }
                        | AtomicChessFactData::MaterialInventory { .. }
                        | AtomicChessFactData::TerminalPosition { .. }
                        | AtomicChessFactData::MaterialChanged { .. }
                )
            });
        if let Some(proof) = proof(
            candidate,
            concept,
            index,
            index,
            supporting_fact_refs,
            outcome_refs,
        ) {
            detected.push(proof);
        }
    }
    detected
}

fn detect_zugzwang(
    candidate: &ReplayedCandidate,
    facts: &[&AtomicChessFact],
) -> Vec<DetectedConcept> {
    let mut detected = Vec::new();
    for ((step_index, step), position) in candidate
        .contract
        .line_steps
        .iter()
        .enumerate()
        .zip(candidate.positions.iter().skip(1))
    {
        let legal_moves = position.legal_moves();
        if legal_moves.is_empty() {
            continue;
        }
        let side = position.turn();
        let occupied = position.board().occupied();
        let every_destination_is_attacked_and_undefended = legal_moves.iter().all(|chess_move| {
            let destination = chess_move.to();
            position
                .board()
                .attacks_to(destination, !side, occupied)
                .any()
                && position
                    .board()
                    .attacks_to(destination, side, occupied)
                    .is_empty()
        });
        if !every_destination_is_attacked_and_undefended {
            continue;
        }
        let adverse = candidate.contract.outcomes.iter().find_map(|outcome| {
            if !matches!(
                outcome.data,
                SemanticOutcomeData::MaterialBalanceChanged { .. }
                    | SemanticOutcomeData::TerminalStateReached {
                        result: DecisionTerminalState::Checkmate,
                        ..
                    }
            ) {
                return None;
            }
            let payoff_index = outcome_step_index(candidate, facts, outcome)?;
            (payoff_index >= step_index).then_some((outcome, payoff_index))
        });
        let Some((adverse, payoff_index)) = adverse else {
            continue;
        };
        let mut supporting_fact_refs = facts
            .iter()
            .filter(|fact| {
                matches!(
                    &fact.data,
                    AtomicChessFactData::LegalDestinations { snapshot_ref, .. }
                        | AtomicChessFactData::AttackSet { snapshot_ref, .. }
                        if snapshot_ref == &step.after_snapshot_ref
                )
            })
            .map(|fact| fact.fact_ref.clone())
            .collect::<Vec<_>>();
        supporting_fact_refs.extend(adverse.supporting_fact_refs.clone());
        supporting_fact_refs.sort();
        supporting_fact_refs.dedup();
        if let Some(proof) = proof(
            candidate,
            Concept::Zugzwang,
            step_index,
            payoff_index,
            supporting_fact_refs,
            vec![adverse.outcome_ref.clone()],
        ) {
            detected.push(proof);
        }
    }
    detected
}

fn material_endgame_class(position: &shakmaty::Chess) -> Option<Concept> {
    let board = position.board();
    let has_pawns = !board.pawns().is_empty();
    let has_knights = !board.knights().is_empty();
    let has_bishops = !board.bishops().is_empty();
    let has_rooks = !board.rooks().is_empty();
    let has_queens = !board.queens().is_empty();
    let non_king_class_count = usize::from(has_knights)
        + usize::from(has_bishops)
        + usize::from(has_rooks)
        + usize::from(has_queens);
    if non_king_class_count == 0 && has_pawns {
        Some(Concept::PawnEndgame)
    } else if has_rooks && has_queens && !has_knights && !has_bishops {
        Some(Concept::QueenAndRookEndgame)
    } else if has_queens && non_king_class_count == 1 {
        Some(Concept::QueenEndgame)
    } else if has_rooks && non_king_class_count == 1 {
        Some(Concept::RookEndgame)
    } else if has_bishops && non_king_class_count == 1 {
        Some(Concept::BishopEndgame)
    } else if has_knights && non_king_class_count == 1 {
        Some(Concept::KnightEndgame)
    } else {
        None
    }
}

fn kings_in_opposition(white: shakmaty::Square, black: shakmaty::Square) -> bool {
    let file_distance = (white.file().char() as i16 - black.file().char() as i16).abs();
    let rank_distance = (white.rank().char() as i16 - black.rank().char() as i16).abs();
    (file_distance == 2 && rank_distance == 0) || (file_distance == 0 && rank_distance == 2)
}

fn king_supports_pawn(king: shakmaty::Square, pawn: shakmaty::Square, color: Color) -> bool {
    let file_distance = (king.file().char() as i16 - pawn.file().char() as i16).abs();
    let rank_delta = king.rank().char() as i16 - pawn.rank().char() as i16;
    file_distance <= 1 && color.fold_wb(rank_delta >= 0, rank_delta <= 0)
}

fn has_lucena_geometry(position: &shakmaty::Chess) -> bool {
    if !canonical_rook_and_pawn_material(position) {
        return false;
    }
    position.board().pawns().into_iter().any(|pawn| {
        let Some(piece) = position.board().piece_at(pawn) else {
            return false;
        };
        let on_seventh = piece.color.fold_wb(
            pawn.rank() == shakmaty::Rank::Seventh,
            pawn.rank() == shakmaty::Rank::Second,
        );
        let promotion_rank = piece
            .color
            .fold_wb(shakmaty::Rank::Eighth, shakmaty::Rank::First);
        let promotion_square = shakmaty::Square::from_coords(pawn.file(), promotion_rank);
        let own_king = position.board().king_of(piece.color);
        let defending_king = position.board().king_of(!piece.color);
        on_seventh
            && !matches!(pawn.file(), shakmaty::File::A | shakmaty::File::H)
            && own_king == Some(promotion_square)
            && defending_king.is_some_and(|king| {
                (king.file().char() as i16 - pawn.file().char() as i16).abs() >= 2
            })
    })
}

fn has_philidor_geometry(position: &shakmaty::Chess) -> bool {
    if !canonical_rook_and_pawn_material(position) {
        return false;
    }
    position.board().pawns().into_iter().any(|pawn| {
        let Some(piece) = position.board().piece_at(pawn) else {
            return false;
        };
        let defending_rook_rank = piece
            .color
            .fold_wb(shakmaty::Rank::Sixth, shakmaty::Rank::Third);
        let pawn_not_beyond_fifth = piece.color.fold_wb(
            pawn.rank() <= shakmaty::Rank::Fifth,
            pawn.rank() >= shakmaty::Rank::Fourth,
        );
        let defending_king = position.board().king_of(!piece.color);
        pawn_not_beyond_fifth
            && defending_king.is_some_and(|king| {
                king.file() == pawn.file()
                    && piece
                        .color
                        .fold_wb(king.rank() > pawn.rank(), king.rank() < pawn.rank())
            })
            && position.board().rooks().into_iter().any(|rook| {
                position.board().piece_at(rook).is_some_and(|rook_piece| {
                    rook_piece.color != piece.color && rook.rank() == defending_rook_rank
                })
            })
    })
}

fn has_passive_rook_geometry(position: &shakmaty::Chess) -> bool {
    if !canonical_rook_and_pawn_material(position) {
        return false;
    }
    position.board().pawns().into_iter().any(|pawn| {
        let Some(pawn_piece) = position.board().piece_at(pawn) else {
            return false;
        };
        let advanced = pawn_piece.color.fold_wb(
            pawn.rank() >= shakmaty::Rank::Sixth,
            pawn.rank() <= shakmaty::Rank::Third,
        );
        let defending_color = !pawn_piece.color;
        let back_rank = defending_color.fold_wb(shakmaty::Rank::First, shakmaty::Rank::Eighth);
        advanced
            && position
                .board()
                .king_of(defending_color)
                .is_some_and(|king| king.rank() == back_rank)
            && position.board().rooks().into_iter().any(|rook| {
                position
                    .board()
                    .piece_at(rook)
                    .is_some_and(|piece| piece.color == defending_color && rook.rank() == back_rank)
            })
    })
}

fn canonical_rook_and_pawn_material(position: &shakmaty::Chess) -> bool {
    let board = position.board();
    board.occupied().count() == 5
        && board.pawns().count() == 1
        && (board.rooks() & board.by_color(Color::White)).count() == 1
        && (board.rooks() & board.by_color(Color::Black)).count() == 1
}
