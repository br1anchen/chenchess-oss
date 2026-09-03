use shakmaty::{
    fen::Fen,
    san::{San, SanError, SanPlus},
    uci::UciMove,
    CastlingMode, Chess, EnPassantMode, Move, Position,
};

use crate::{
    engine_analysis::{EngineAnalysis, PositionEvaluation},
    evaluation_recording::PINNED_STOCKFISH_DEPTH,
    review_session_contract::*,
};

use super::{evidence::validate_engine_evidence, rejected, ExploreAlternativeMoveError};

pub(crate) struct AppliedMove {
    pub(crate) uci: String,
    pub(crate) resulting_position: PositionSnapshot,
    pub(crate) resulting_history: Vec<String>,
}

pub(crate) fn apply_move(
    source: &PositionSnapshot,
    source_history: &[String],
    input: &MoveInput,
) -> Result<AppliedMove, ExploreAlternativeMoveError> {
    let mut position =
        parse_position(source).expect("exploration state contains only canonical Positions");
    let (chess_move, uci) = parse_move(&position, input)?;
    position.play_unchecked(&chess_move);
    let fen = Fen::from_position(position, EnPassantMode::Legal).to_string();
    let preceding = source_history
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let resulting_position = build_position_snapshot(&fen, &preceding).map_err(|_| {
        rejected(
            CommandRejectionReason::IllegalMove,
            RejectionRecovery::CorrectInput,
        )
    })?;
    let mut resulting_history = source_history.to_vec();
    resulting_history.push(fen);
    Ok(AppliedMove {
        uci,
        resulting_position,
        resulting_history,
    })
}

fn parse_move(
    position: &Chess,
    input: &MoveInput,
) -> Result<(Move, String), ExploreAlternativeMoveError> {
    let chess_move = match input {
        MoveInput::Uci { uci } => UciMove::from_ascii(uci.as_bytes())
            .ok()
            .and_then(|candidate| candidate.to_move(position).ok())
            .ok_or_else(|| {
                rejected(
                    CommandRejectionReason::IllegalMove,
                    RejectionRecovery::ChooseLegalMove {
                        matching_moves: Vec::new(),
                    },
                )
            })?,
        MoveInput::San { san } => {
            let parsed = SanPlus::from_ascii(san.as_bytes()).map_err(|_| {
                rejected(
                    CommandRejectionReason::IllegalMove,
                    RejectionRecovery::ChooseLegalMove {
                        matching_moves: Vec::new(),
                    },
                )
            })?;
            let chess_move = match parsed.san.to_move(position) {
                Ok(chess_move) => chess_move,
                Err(SanError::AmbiguousSan) => {
                    return Err(rejected(
                        CommandRejectionReason::AmbiguousMove,
                        RejectionRecovery::ChooseLegalMove {
                            matching_moves: matching_san_moves(position, &parsed.san),
                        },
                    ));
                }
                Err(SanError::IllegalSan) => {
                    return Err(rejected(
                        CommandRejectionReason::IllegalMove,
                        RejectionRecovery::ChooseLegalMove {
                            matching_moves: Vec::new(),
                        },
                    ));
                }
            };
            if SanPlus::from_move(position.clone(), &chess_move) != parsed {
                return Err(rejected(
                    CommandRejectionReason::IllegalMove,
                    RejectionRecovery::ChooseLegalMove {
                        matching_moves: Vec::new(),
                    },
                ));
            }
            chess_move
        }
    };
    let uci = UciMove::from_move(&chess_move, CastlingMode::Standard).to_string();
    Ok((chess_move, uci))
}

fn matching_san_moves(position: &Chess, san: &San) -> Vec<String> {
    let mut matching = position
        .legal_moves()
        .into_iter()
        .filter(|candidate| san.matches(candidate))
        .map(|candidate| UciMove::from_move(&candidate, CastlingMode::Standard).to_string())
        .collect::<Vec<_>>();
    matching.sort();
    matching
}

pub(crate) fn normalize_engine_analysis(
    position: &PositionSnapshot,
    analysis: EngineAnalysis,
) -> Result<EngineAnalysisEvidence, ()> {
    if analysis.depth != PINNED_STOCKFISH_DEPTH {
        return Err(());
    }
    let evaluation = match analysis.evaluation {
        PositionEvaluation::Centipawns(value) => EngineEvaluation::Centipawns {
            value,
            perspective: position.side_to_move,
        },
        PositionEvaluation::MateIn(value) => EngineEvaluation::Mate {
            outcome: if value > 0 {
                MateOutcome::Win
            } else {
                MateOutcome::Loss
            },
            distance_plies: u16::try_from(value.unsigned_abs()).map_err(|_| ())?,
            perspective: position.side_to_move,
        },
    };
    let evidence = EngineAnalysisEvidence {
        evaluation,
        best_move_uci: analysis.best_move,
        principal_variation: analysis.principal_variation,
    };
    validate_engine_evidence(position, &evidence)?;
    Ok(evidence)
}

pub(crate) fn normalize_child_evaluation(
    evaluation: &EngineEvaluation,
    mover: Color,
) -> Option<EngineEvaluation> {
    match evaluation {
        EngineEvaluation::Centipawns { value, perspective } if *perspective == opposite(mover) => {
            Some(EngineEvaluation::Centipawns {
                value: value.checked_neg()?,
                perspective: mover,
            })
        }
        EngineEvaluation::Mate {
            outcome,
            distance_plies,
            perspective,
        } if *perspective == opposite(mover) => Some(EngineEvaluation::Mate {
            outcome: opposite_outcome(*outcome),
            distance_plies: distance_plies.checked_add(1)?,
            perspective: mover,
        }),
        _ => None,
    }
}

pub(crate) fn compare_evaluations(
    best: &EngineEvaluation,
    selected: &EngineEvaluation,
) -> Option<EvaluationLoss> {
    if evaluation_perspective(best) != evaluation_perspective(selected) {
        return None;
    }
    match (best, selected) {
        (
            EngineEvaluation::Centipawns {
                value: best_value, ..
            },
            EngineEvaluation::Centipawns {
                value: selected_value,
                ..
            },
        ) => Some(EvaluationLoss::Centipawns {
            value: u32::try_from((i64::from(*best_value) - i64::from(*selected_value)).max(0))
                .ok()?,
        }),
        _ => Some(EvaluationLoss::Mate {
            best: mate_comparison(best),
            selected: mate_comparison(selected),
        }),
    }
}

fn mate_comparison(evaluation: &EngineEvaluation) -> MateComparison {
    match evaluation {
        EngineEvaluation::Centipawns { .. } => MateComparison::NotForced,
        EngineEvaluation::Mate {
            outcome,
            distance_plies,
            ..
        } => MateComparison::Forced {
            outcome: *outcome,
            distance_plies: *distance_plies,
        },
    }
}

fn parse_position(position: &PositionSnapshot) -> anyhow::Result<Chess> {
    Ok(Fen::from_ascii(position.fen.as_bytes())?.into_position(CastlingMode::Standard)?)
}

fn evaluation_perspective(evaluation: &EngineEvaluation) -> Color {
    match evaluation {
        EngineEvaluation::Centipawns { perspective, .. }
        | EngineEvaluation::Mate { perspective, .. } => *perspective,
    }
}

fn opposite(color: Color) -> Color {
    match color {
        Color::White => Color::Black,
        Color::Black => Color::White,
    }
}

fn opposite_outcome(outcome: MateOutcome) -> MateOutcome {
    match outcome {
        MateOutcome::Win => MateOutcome::Loss,
        MateOutcome::Loss => MateOutcome::Win,
    }
}
