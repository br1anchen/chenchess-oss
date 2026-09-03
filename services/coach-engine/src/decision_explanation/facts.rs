use std::{cmp::Ordering, collections::BTreeMap};

use shakmaty::{attacks, uci::UciMove, CastlingMode, Chess, Position, Role};

use crate::review_session_contract::{
    AtomicChessFact, AtomicChessFactData, AtomicFactRef, CastlingWing, DecisionCandidateRef,
    DecisionLineStep, ExplanationPathAttribution, GameReviewMomentClassification, KnowledgeNodeRef,
    KnowledgeRuleRef, LineStepRef, MaterialInventory, MaterialValuePolicyVersion, PieceRole,
    SemanticOutcome, SemanticOutcomeData, SemanticOutcomeRef,
};

use super::{
    candidate::{
        chess_square, contract_color, contract_role, contract_square, CandidateConstruction,
    },
    detectors,
    knowledge::{CompiledKnowledgeGraph, KnowledgeConcept},
    DecisionExplanationContractError,
};

pub(super) fn record_step_facts(
    before: &Chess,
    after: &Chess,
    step: &DecisionLineStep,
    promotion: Option<Role>,
    facts: &mut BTreeMap<AtomicFactRef, AtomicChessFact>,
    candidate_fact_refs: &mut Vec<AtomicFactRef>,
    outcomes: &mut Vec<SemanticOutcome>,
) -> Result<(), DecisionExplanationContractError> {
    record_position_facts(
        before,
        &step.before_snapshot_ref,
        facts,
        candidate_fact_refs,
    )?;
    record_position_facts(after, &step.after_snapshot_ref, facts, candidate_fact_refs)?;
    record_legal_recaptures(after, step, facts, candidate_fact_refs)?;
    super::outcomes::record_attack_transitions(
        before,
        after,
        step,
        facts,
        candidate_fact_refs,
        outcomes,
    )?;
    let to = chess_square(&step.to_square);
    let moved = crate::review_session_contract::PieceAtSquare {
        color: step.mover,
        role: step.role,
        square: step.to_square.clone(),
    };
    let resulting_piece = crate::review_session_contract::PieceAtSquare {
        color: step.mover,
        role: contract_role(promotion.unwrap_or_else(|| to_role(&step.role))),
        square: step.to_square.clone(),
    };
    let moved_ref = push_fact(
        facts,
        candidate_fact_refs,
        AtomicChessFactData::PieceMoved {
            step_ref: step.step_ref.clone(),
            piece: moved.clone(),
            from_square: step.from_square.clone(),
            to_square: step.to_square.clone(),
            uci: step.uci.clone(),
        },
    );
    let attacked_squares = after
        .board()
        .attacks_from(to)
        .into_iter()
        .map(contract_square)
        .collect::<Vec<_>>();
    push_fact(
        facts,
        candidate_fact_refs,
        AtomicChessFactData::AttackSet {
            snapshot_ref: step.after_snapshot_ref.clone(),
            attacker: resulting_piece.clone(),
            attacked_squares,
        },
    );

    if step.role == PieceRole::Pawn {
        let before_front_span_ref = find_pawn_front_span_ref(
            facts,
            &step.before_snapshot_ref,
            &step.from_square,
            step.mover,
        )
        .ok_or(DecisionExplanationContractError::InvalidProof(
            "a pawn transition must retain its before front span",
        ))?;
        let after_front_span_ref = if step.promotion.is_none() {
            Some(
                find_pawn_front_span_ref(
                    facts,
                    &step.after_snapshot_ref,
                    &step.to_square,
                    step.mover,
                )
                .ok_or(DecisionExplanationContractError::InvalidProof(
                    "an unpromoted pawn transition must retain its after front span",
                ))?,
            )
        } else {
            None
        };
        let data = SemanticOutcomeData::PawnProgressed {
            pawn: crate::review_session_contract::PieceAtSquare {
                color: step.mover,
                role: PieceRole::Pawn,
                square: step.from_square.clone(),
            },
            from_square: step.from_square.clone(),
            to_square: step.to_square.clone(),
            promotion_role: step.promotion,
            before_front_span_ref: before_front_span_ref.clone(),
            after_front_span_ref: after_front_span_ref.clone(),
        };
        let mut supporting_fact_refs = vec![moved_ref];
        supporting_fact_refs.push(before_front_span_ref);
        supporting_fact_refs.extend(after_front_span_ref);
        push_outcome(outcomes, data, supporting_fact_refs);
    }

    let promotion_ref = step.promotion.map(|promotion_role| {
        push_fact(
            facts,
            candidate_fact_refs,
            AtomicChessFactData::PiecePromoted {
                step_ref: step.step_ref.clone(),
                pawn_origin_square: step.from_square.clone(),
                promotion_square: step.to_square.clone(),
                promotion_role,
            },
        )
    });
    let capture_ref = step.captured.as_ref().map(|captured| {
        push_fact(
            facts,
            candidate_fact_refs,
            AtomicChessFactData::PieceCaptured {
                step_ref: step.step_ref.clone(),
                captured: captured.clone(),
            },
        )
    });
    record_special_move_facts(step, facts, candidate_fact_refs)?;
    let sides = [shakmaty::Color::White, shakmaty::Color::Black];
    let before_inventory_refs = sides
        .into_iter()
        .map(|side| {
            push_fact(
                facts,
                candidate_fact_refs,
                AtomicChessFactData::MaterialInventory {
                    snapshot_ref: step.before_snapshot_ref.clone(),
                    side: contract_color(side),
                    inventory: material_inventory_for(before, side),
                },
            )
        })
        .collect::<Vec<_>>();
    let after_inventory_refs = sides
        .into_iter()
        .map(|side| {
            push_fact(
                facts,
                candidate_fact_refs,
                AtomicChessFactData::MaterialInventory {
                    snapshot_ref: step.after_snapshot_ref.clone(),
                    side: contract_color(side),
                    inventory: material_inventory_for(after, side),
                },
            )
        })
        .collect::<Vec<_>>();
    if step.captured.is_some() || step.promotion.is_some() {
        let captured_value = step
            .captured
            .as_ref()
            .and_then(|captured| captured.role.conventional_material_value())
            .unwrap_or(0);
        let promotion_value = step
            .promotion
            .and_then(PieceRole::conventional_material_value)
            .map_or(0, |value| value.saturating_sub(1));
        let delta = i16::from(captured_value.saturating_add(promotion_value));
        let changed_ref = push_fact(
            facts,
            candidate_fact_refs,
            AtomicChessFactData::MaterialChanged {
                step_ref: step.step_ref.clone(),
                before_inventory_refs: before_inventory_refs.clone(),
                after_inventory_refs: after_inventory_refs.clone(),
                captured: step.captured.clone(),
                promoted: step.promotion,
                conventional_value_delta: delta,
                value_policy_version: MaterialValuePolicyVersion::V1,
            },
        );
        let mut gained = step.captured.clone().into_iter().collect::<Vec<_>>();
        let mut lost = Vec::new();
        if step.promotion.is_some() {
            gained.push(resulting_piece);
            lost.push(crate::review_session_contract::PieceAtSquare {
                color: step.mover,
                role: PieceRole::Pawn,
                square: step.from_square.clone(),
            });
        }
        let mut supporting_fact_refs = capture_ref.into_iter().collect::<Vec<_>>();
        supporting_fact_refs.extend(promotion_ref.clone());
        supporting_fact_refs.push(changed_ref);
        push_outcome(
            outcomes,
            SemanticOutcomeData::MaterialBalanceChanged {
                conventional_value_delta: delta,
                value_policy_version: MaterialValuePolicyVersion::V1,
                gained,
                lost,
            },
            supporting_fact_refs,
        );
        if step.promotion.is_some() {
            push_outcome(
                outcomes,
                SemanticOutcomeData::MaterialConfigurationChanged {
                    before_inventory_refs,
                    after_inventory_refs,
                },
                promotion_ref.into_iter().collect(),
            );
        }
    }

    let before_terminal = terminal_state(before);
    let after_terminal = terminal_state(after);
    let before_terminal_ref = push_fact(
        facts,
        candidate_fact_refs,
        AtomicChessFactData::TerminalPosition {
            snapshot_ref: step.before_snapshot_ref.clone(),
            state: before_terminal,
        },
    );
    let after_terminal_ref = push_fact(
        facts,
        candidate_fact_refs,
        AtomicChessFactData::TerminalPosition {
            snapshot_ref: step.after_snapshot_ref.clone(),
            state: after_terminal,
        },
    );
    if before_terminal == crate::review_session_contract::DecisionTerminalState::Ongoing
        && after_terminal != crate::review_session_contract::DecisionTerminalState::Ongoing
    {
        push_outcome(
            outcomes,
            SemanticOutcomeData::TerminalStateReached {
                before_state_ref: before_terminal_ref.clone(),
                after_state_ref: after_terminal_ref.clone(),
                result: after_terminal,
            },
            vec![before_terminal_ref, after_terminal_ref],
        );
    }
    Ok(())
}

fn find_pawn_front_span_ref(
    facts: &BTreeMap<AtomicFactRef, AtomicChessFact>,
    snapshot_ref: &crate::review_session_contract::DecisionPositionSnapshotRef,
    square: &crate::review_session_contract::Square,
    color: crate::review_session_contract::Color,
) -> Option<AtomicFactRef> {
    facts.values().find_map(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::PawnFrontSpanOccupancy {
                snapshot_ref: fact_snapshot_ref,
                pawn,
                ..
            } if fact_snapshot_ref == snapshot_ref
                && &pawn.square == square
                && pawn.color == color
        )
        .then(|| fact.fact_ref.clone())
    })
}

fn record_position_facts(
    position: &Chess,
    snapshot_ref: &crate::review_session_contract::DecisionPositionSnapshotRef,
    facts: &mut BTreeMap<AtomicFactRef, AtomicChessFact>,
    candidate_fact_refs: &mut Vec<AtomicFactRef>,
) -> Result<(), DecisionExplanationContractError> {
    let board = position.board();
    let legal_moves = position.legal_moves();
    for square in board.occupied() {
        let Some(piece) = board.piece_at(square) else {
            continue;
        };
        let contract_piece = crate::review_session_contract::PieceAtSquare {
            color: contract_color(piece.color),
            role: contract_role(piece.role),
            square: contract_square(square),
        };
        push_fact(
            facts,
            candidate_fact_refs,
            AtomicChessFactData::PieceOccupancy {
                snapshot_ref: snapshot_ref.clone(),
                piece: contract_piece.clone(),
            },
        );
        let attacked_squares = board
            .attacks_from(square)
            .into_iter()
            .map(contract_square)
            .collect::<Vec<_>>();
        push_fact(
            facts,
            candidate_fact_refs,
            AtomicChessFactData::AttackSet {
                snapshot_ref: snapshot_ref.clone(),
                attacker: contract_piece.clone(),
                attacked_squares,
            },
        );
        if piece.color == position.turn() {
            let destinations = legal_moves
                .iter()
                .filter(|chess_move| chess_move.from() == Some(square))
                .map(|chess_move| contract_square(chess_move.to()))
                .collect::<Vec<_>>();
            push_fact(
                facts,
                candidate_fact_refs,
                AtomicChessFactData::LegalDestinations {
                    snapshot_ref: snapshot_ref.clone(),
                    piece: contract_piece.clone(),
                    destinations,
                },
            );
        }
        if piece.role == Role::Pawn {
            let front_span = shakmaty::Square::ALL
                .into_iter()
                .filter(|candidate| {
                    let file_distance =
                        (candidate.file().char() as i16 - square.file().char() as i16).abs();
                    let ahead = piece.color.fold_wb(
                        candidate.rank() > square.rank(),
                        candidate.rank() < square.rank(),
                    );
                    file_distance <= 1 && ahead
                })
                .collect::<Vec<_>>();
            let opposing_pawns = front_span
                .iter()
                .copied()
                .filter(|candidate| {
                    board.piece_at(*candidate).is_some_and(|occupant| {
                        occupant.color != piece.color && occupant.role == Role::Pawn
                    })
                })
                .map(contract_square)
                .collect::<Vec<_>>();
            push_fact(
                facts,
                candidate_fact_refs,
                AtomicChessFactData::PawnFrontSpanOccupancy {
                    snapshot_ref: snapshot_ref.clone(),
                    pawn: contract_piece,
                    front_span: front_span
                        .into_iter()
                        .map(contract_square)
                        .collect::<Vec<_>>(),
                    opposing_pawns,
                },
            );
        }
    }
    for slider in board.sliders() {
        let Some(attacker) = board.piece_at(slider) else {
            continue;
        };
        for target in board.by_color(!attacker.color) {
            let blockers = attacks::between(slider, target) & board.occupied();
            let Some(blocker) = blockers.single_square() else {
                continue;
            };
            let Some(blocker_piece) = board.piece_at(blocker) else {
                continue;
            };
            if blocker_piece.color != board.color_at(target).unwrap_or(attacker.color)
                || !attacks::attacks(slider, attacker, board.occupied().without(blocker))
                    .contains(target)
            {
                continue;
            }
            push_fact(
                facts,
                candidate_fact_refs,
                AtomicChessFactData::SoleRayBlocker {
                    snapshot_ref: snapshot_ref.clone(),
                    attacker: contract_piece_at(board, slider)?,
                    blocker: contract_piece_at(board, blocker)?,
                    target: contract_piece_at(board, target)?,
                },
            );
        }
    }
    for color in [shakmaty::Color::White, shakmaty::Color::Black] {
        let Some(king_square) = board.king_of(color) else {
            continue;
        };
        let king = contract_piece_at(board, king_square)?;
        let checking_pieces = board
            .attacks_to(king_square, !color, board.occupied())
            .into_iter()
            .map(|square| contract_piece_at(board, square))
            .collect::<Result<Vec<_>, _>>()?;
        push_fact(
            facts,
            candidate_fact_refs,
            AtomicChessFactData::Checkers {
                snapshot_ref: snapshot_ref.clone(),
                king: king.clone(),
                checking_pieces,
            },
        );
        let zone = attacks::king_attacks(king_square).with(king_square);
        let attacking_pieces = board
            .by_color(!color)
            .into_iter()
            .filter(|square| !(board.attacks_from(*square) & zone).is_empty())
            .map(|square| contract_piece_at(board, square))
            .collect::<Result<Vec<_>, _>>()?;
        push_fact(
            facts,
            candidate_fact_refs,
            AtomicChessFactData::KingZonePressure {
                snapshot_ref: snapshot_ref.clone(),
                king,
                zone_squares: zone.into_iter().map(contract_square).collect::<Vec<_>>(),
                attacking_pieces,
            },
        );
    }
    Ok(())
}

fn record_legal_recaptures(
    position: &Chess,
    preceding_step: &DecisionLineStep,
    facts: &mut BTreeMap<AtomicFactRef, AtomicChessFact>,
    candidate_fact_refs: &mut Vec<AtomicFactRef>,
) -> Result<(), DecisionExplanationContractError> {
    let target = chess_square(&preceding_step.to_square);
    let mut moves = position
        .legal_moves()
        .iter()
        .filter(|chess_move| chess_move.is_capture() && chess_move.to() == target)
        .map(|chess_move| UciMove::from_move(chess_move, CastlingMode::Standard).to_string())
        .collect::<Vec<_>>();
    moves.sort();
    push_fact(
        facts,
        candidate_fact_refs,
        AtomicChessFactData::LegalRecaptures {
            snapshot_ref: preceding_step.after_snapshot_ref.clone(),
            side: contract_color(position.turn()),
            target_square: preceding_step.to_square.clone(),
            moves,
        },
    );
    Ok(())
}

fn record_special_move_facts(
    step: &DecisionLineStep,
    facts: &mut BTreeMap<AtomicFactRef, AtomicChessFact>,
    candidate_fact_refs: &mut Vec<AtomicFactRef>,
) -> Result<(), DecisionExplanationContractError> {
    let from = chess_square(&step.from_square);
    let to = chess_square(&step.to_square);
    if step.role == PieceRole::King
        && from.rank() == to.rank()
        && u32::from(from.file().char()).abs_diff(u32::from(to.file().char())) == 2
    {
        let king_side = to.file() > from.file();
        let (rook_from, rook_to) = if king_side {
            (
                shakmaty::Square::from_coords(shakmaty::File::H, from.rank()),
                shakmaty::Square::from_coords(shakmaty::File::F, from.rank()),
            )
        } else {
            (
                shakmaty::Square::from_coords(shakmaty::File::A, from.rank()),
                shakmaty::Square::from_coords(shakmaty::File::D, from.rank()),
            )
        };
        push_fact(
            facts,
            candidate_fact_refs,
            AtomicChessFactData::Castled {
                step_ref: step.step_ref.clone(),
                side: step.mover,
                wing: if king_side {
                    CastlingWing::KingSide
                } else {
                    CastlingWing::QueenSide
                },
                king_from_square: step.from_square.clone(),
                king_to_square: step.to_square.clone(),
                rook_from_square: contract_square(rook_from),
                rook_to_square: contract_square(rook_to),
            },
        );
    }
    if step.role == PieceRole::Pawn {
        if let Some(captured) = &step.captured {
            if captured.role == PieceRole::Pawn && captured.square != step.to_square {
                push_fact(
                    facts,
                    candidate_fact_refs,
                    AtomicChessFactData::EnPassantCaptured {
                        step_ref: step.step_ref.clone(),
                        capturing_pawn: crate::review_session_contract::PieceAtSquare {
                            color: step.mover,
                            role: PieceRole::Pawn,
                            square: step.to_square.clone(),
                        },
                        from_square: step.from_square.clone(),
                        to_square: step.to_square.clone(),
                        captured_pawn_square: captured.square.clone(),
                    },
                );
            }
        }
    }
    Ok(())
}

fn contract_piece_at(
    board: &shakmaty::Board,
    square: shakmaty::Square,
) -> Result<crate::review_session_contract::PieceAtSquare, DecisionExplanationContractError> {
    let piece = board
        .piece_at(square)
        .ok_or(DecisionExplanationContractError::InvalidProof(
            "a retained fact references an empty square",
        ))?;
    Ok(crate::review_session_contract::PieceAtSquare {
        color: contract_color(piece.color),
        role: contract_role(piece.role),
        square: contract_square(square),
    })
}

pub(super) fn push_outcome(
    outcomes: &mut Vec<SemanticOutcome>,
    data: SemanticOutcomeData,
    supporting_fact_refs: Vec<AtomicFactRef>,
) {
    let outcome_ref = SemanticOutcomeRef::from_content(&(&data, &supporting_fact_refs));
    outcomes.push(SemanticOutcome {
        outcome_ref,
        data,
        supporting_fact_refs,
    });
}

pub(super) fn push_fact(
    facts: &mut BTreeMap<AtomicFactRef, AtomicChessFact>,
    candidate_fact_refs: &mut Vec<AtomicFactRef>,
    data: AtomicChessFactData,
) -> AtomicFactRef {
    let fact_ref = AtomicFactRef::from_content(&data);
    facts
        .entry(fact_ref.clone())
        .or_insert_with(|| AtomicChessFact {
            fact_ref: fact_ref.clone(),
            data,
        });
    candidate_fact_refs.push(fact_ref.clone());
    fact_ref
}

fn material_inventory_for(position: &Chess, side: shakmaty::Color) -> MaterialInventory {
    let material = position.board().material_side(side);
    MaterialInventory {
        pawns: material.pawn,
        knights: material.knight,
        bishops: material.bishop,
        rooks: material.rook,
        queens: material.queen,
    }
}

fn terminal_state(position: &Chess) -> crate::review_session_contract::DecisionTerminalState {
    if position.is_checkmate() {
        crate::review_session_contract::DecisionTerminalState::Checkmate
    } else if position.is_stalemate() {
        crate::review_session_contract::DecisionTerminalState::Stalemate
    } else if position.outcome().is_some() {
        crate::review_session_contract::DecisionTerminalState::Draw
    } else {
        crate::review_session_contract::DecisionTerminalState::Ongoing
    }
}

fn to_role(role: &PieceRole) -> Role {
    match role {
        PieceRole::Pawn => Role::Pawn,
        PieceRole::Knight => Role::Knight,
        PieceRole::Bishop => Role::Bishop,
        PieceRole::Rook => Role::Rook,
        PieceRole::Queen => Role::Queen,
        PieceRole::King => Role::King,
    }
}

pub(super) fn piece_value(role: Role) -> u8 {
    contract_role(role)
        .conventional_material_value()
        .unwrap_or(u8::MAX)
}

#[derive(Debug, Clone)]
pub(super) struct SelectedConceptProof {
    pub(super) concept: KnowledgeConcept,
    pub(super) candidate_ref: DecisionCandidateRef,
    pub(super) attribution: ExplanationPathAttribution,
    pub(super) concept_node_ref: KnowledgeNodeRef,
    pub(super) recognition_rule_ref: KnowledgeRuleRef,
    pub(super) causal_step_ref: LineStepRef,
    pub(super) payoff_step_ref: LineStepRef,
    pub(super) supporting_fact_refs: Vec<AtomicFactRef>,
    pub(super) outcome_refs: Vec<SemanticOutcomeRef>,
    pub(super) semantic_comparisons: Vec<crate::review_session_contract::SemanticComparison>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ConceptSelectionKey {
    payoff_index: usize,
    cited_prefix_len: usize,
    supporting_fact_count: usize,
    curriculum_order: crate::review_session_contract::CurriculumLearningConcept,
}

fn selection_key(
    detected: &detectors::DetectedConcept,
    step_index: &mut impl FnMut(&LineStepRef) -> Option<usize>,
) -> Result<ConceptSelectionKey, DecisionExplanationContractError> {
    let causal_index = step_index(&detected.causal_step_ref).ok_or(
        DecisionExplanationContractError::InvalidProof(
            "selected causal step does not belong to its candidate",
        ),
    )?;
    let payoff_index = step_index(&detected.payoff_step_ref).ok_or(
        DecisionExplanationContractError::InvalidProof(
            "selected payoff step does not belong to its candidate",
        ),
    )?;
    Ok(ConceptSelectionKey {
        payoff_index,
        cited_prefix_len: causal_index.max(payoff_index) + 1,
        supporting_fact_count: detected.supporting_fact_refs.len(),
        curriculum_order: detected.concept,
    })
}

pub(super) fn suppress_broader_realizations(
    detected: Vec<detectors::DetectedConcept>,
    graph: &CompiledKnowledgeGraph,
) -> Vec<detectors::DetectedConcept> {
    detected
        .iter()
        .enumerate()
        .filter(|(broader_index, broader)| {
            let (broader_ref, _) = graph.references(KnowledgeConcept::Curriculum(broader.concept));
            !detected
                .iter()
                .enumerate()
                .any(|(specific_index, specific)| {
                    if specific_index == *broader_index
                        || specific.causal_step_ref != broader.causal_step_ref
                        || specific.payoff_step_ref != broader.payoff_step_ref
                        || !specific
                            .outcome_refs
                            .iter()
                            .any(|outcome| broader.outcome_refs.contains(outcome))
                    {
                        return false;
                    }
                    let (specific_ref, _) =
                        graph.references(KnowledgeConcept::Curriculum(specific.concept));
                    graph.refines(&specific_ref, &broader_ref)
                })
        })
        .map(|(_, detected)| detected.clone())
        .collect()
}

fn canonical_realization_order(
    left: &detectors::DetectedConcept,
    right: &detectors::DetectedConcept,
) -> Ordering {
    left.concept
        .cmp(&right.concept)
        .then_with(|| left.causal_step_ref.cmp(&right.causal_step_ref))
        .then_with(|| left.payoff_step_ref.cmp(&right.payoff_step_ref))
        .then_with(|| left.supporting_fact_refs.cmp(&right.supporting_fact_refs))
        .then_with(|| left.outcome_refs.cmp(&right.outcome_refs))
        .then_with(|| {
            left.semantic_comparisons
                .iter()
                .map(|comparison| {
                    (
                        &comparison.preferred_outcome_ref,
                        &comparison.alternative_outcome_ref,
                        comparison.relation,
                    )
                })
                .cmp(right.semantic_comparisons.iter().map(|comparison| {
                    (
                        &comparison.preferred_outcome_ref,
                        &comparison.alternative_outcome_ref,
                        comparison.relation,
                    )
                }))
        })
}

pub(super) fn select_detected_realization(
    detected: Vec<detectors::DetectedConcept>,
    graph: &CompiledKnowledgeGraph,
    mut step_index: impl FnMut(&LineStepRef) -> Option<usize>,
) -> Result<
    Option<(ConceptSelectionKey, detectors::DetectedConcept)>,
    DecisionExplanationContractError,
> {
    let mut ranked = suppress_broader_realizations(detected, graph)
        .into_iter()
        .map(|detected| {
            let key = selection_key(&detected, &mut step_index)?;
            Ok((key, detected))
        })
        .collect::<Result<Vec<_>, DecisionExplanationContractError>>()?;
    ranked.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            // The four fields above are the complete pedagogical policy. This
            // only canonicalizes otherwise identical policy results so
            // detector enumeration cannot alter persisted bytes.
            .then_with(|| canonical_realization_order(&left.1, &right.1))
    });
    Ok(ranked.into_iter().next())
}

pub(super) fn select_concept_proof(
    construction: &CandidateConstruction,
    graph: &CompiledKnowledgeGraph,
    classification: &GameReviewMomentClassification,
) -> Result<Option<SelectedConceptProof>, DecisionExplanationContractError> {
    select_concept_proof_with_detector(construction, graph, classification, |candidate| {
        detectors::detect_all(candidate, construction)
    })
}

#[cfg(test)]
pub(super) fn select_concept_proof_with_family_order(
    construction: &CandidateConstruction,
    graph: &CompiledKnowledgeGraph,
    classification: &GameReviewMomentClassification,
    families: &[detectors::DetectorFamily],
) -> Result<Option<SelectedConceptProof>, DecisionExplanationContractError> {
    select_concept_proof_with_detector(construction, graph, classification, |candidate| {
        detectors::detect_with_family_order(candidate, construction, families)
    })
}

fn select_concept_proof_with_detector(
    construction: &CandidateConstruction,
    graph: &CompiledKnowledgeGraph,
    classification: &GameReviewMomentClassification,
    mut detect: impl FnMut(
        &super::candidate::ReplayedCandidate,
    )
        -> Result<Vec<detectors::DetectedConcept>, DecisionExplanationContractError>,
) -> Result<Option<SelectedConceptProof>, DecisionExplanationContractError> {
    let candidates = match classification {
        GameReviewMomentClassification::ImprovementOpportunity { .. } => {
            let best = construction
                .candidates
                .iter()
                .find(|candidate| candidate.contract.assessment.rank == Some(1))
                .map(|candidate| (candidate, ExplanationPathAttribution::MissedBest));
            let player = construction
                .candidates
                .iter()
                .find(|candidate| {
                    candidate.contract.origins.contains(
                        &crate::review_session_contract::DecisionCandidateOrigin::PlayerPlayed,
                    ) && candidate.contract.assessment.rank != Some(1)
                })
                .map(|candidate| (candidate, ExplanationPathAttribution::ConcededRefutation));
            best.into_iter().chain(player).collect::<Vec<_>>()
        }
        GameReviewMomentClassification::PositiveHighlight { .. } => construction
            .candidates
            .iter()
            .find(|candidate| {
                candidate.contract.origins.contains(
                    &crate::review_session_contract::DecisionCandidateOrigin::PlayerPlayed,
                )
            })
            .map(|candidate| vec![(candidate, ExplanationPathAttribution::Reinforcement)])
            .unwrap_or_default(),
        GameReviewMomentClassification::Neutral { .. } => return Ok(None),
    };
    let mut valid = Vec::new();
    for (candidate, attribution) in candidates {
        let selected = select_detected_realization(detect(candidate)?, graph, |reference| {
            candidate
                .contract
                .line_steps
                .iter()
                .position(|step| &step.step_ref == reference)
        })?;
        if let Some((key, detected)) = selected {
            let concept = KnowledgeConcept::Curriculum(detected.concept);
            let (concept_node_ref, recognition_rule_ref) = graph.references(concept);
            valid.push((
                key,
                SelectedConceptProof {
                    concept,
                    candidate_ref: candidate.contract.candidate_ref.clone(),
                    attribution,
                    concept_node_ref,
                    recognition_rule_ref,
                    causal_step_ref: detected.causal_step_ref,
                    payoff_step_ref: detected.payoff_step_ref,
                    supporting_fact_refs: detected.supporting_fact_refs,
                    outcome_refs: detected.outcome_refs,
                    semantic_comparisons: detected.semantic_comparisons,
                },
            ));
        }
    }
    valid.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            // Candidate identity is persistence canonicalization after an
            // exact four-field policy tie, not a fifth pedagogical criterion.
            .then_with(|| left.1.candidate_ref.cmp(&right.1.candidate_ref))
    });
    Ok(valid.into_iter().next().map(|(_, proof)| proof))
}
