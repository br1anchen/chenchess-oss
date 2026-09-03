use shakmaty::{fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, Position};

use crate::{
    domain::{Game, MoveSide},
    human_move_model::HumanMovePrediction,
    review_session_contract::BoardTerminalOutcome,
};

use super::RuleExtractorError;

pub(super) fn legal_move_count(position: &str) -> Option<usize> {
    Fen::from_ascii(position.as_bytes())
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()
        .map(|position: Chess| position.legal_moves().len())
}

pub(super) fn validate_legal_move(
    position: &str,
    uci: &str,
    ply: usize,
) -> Result<(), RuleExtractorError> {
    let chess: Chess = Fen::from_ascii(position.as_bytes())
        .ok()
        .and_then(|fen| fen.into_position(CastlingMode::Standard).ok())
        .ok_or(RuleExtractorError::InvalidClassificationEvidence {
            ply,
            reason: "the recorded Position cannot be reconstructed",
        })?;
    UciMove::from_ascii(uci.as_bytes())
        .ok()
        .and_then(|move_| move_.to_move(&chess).ok())
        .ok_or(RuleExtractorError::InvalidClassificationEvidence {
            ply,
            reason: "a recorded move is not legal in the recorded Position",
        })?;
    Ok(())
}

pub(super) fn san_for_uci(
    position: &str,
    uci: &str,
    ply: usize,
) -> Result<String, RuleExtractorError> {
    let chess: Chess = Fen::from_ascii(position.as_bytes())
        .ok()
        .and_then(|fen| fen.into_position(CastlingMode::Standard).ok())
        .ok_or(RuleExtractorError::InvalidClassificationEvidence {
            ply,
            reason: "the recorded Position cannot be reconstructed",
        })?;
    let chess_move = UciMove::from_ascii(uci.as_bytes())
        .ok()
        .and_then(|move_| move_.to_move(&chess).ok())
        .ok_or(RuleExtractorError::InvalidClassificationEvidence {
            ply,
            reason: "a recorded move is not legal in the recorded Position",
        })?;
    Ok(SanPlus::from_move(chess, &chess_move).to_string())
}

pub(super) fn human_evidence_is_legal(position: &str, prediction: &HumanMovePrediction) -> bool {
    !prediction.candidates.is_empty()
        && prediction.candidates.iter().all(|candidate| {
            candidate.probability.is_finite()
                && (0.0..=1.0).contains(&candidate.probability)
                && candidate.rank > 0
                && validate_legal_move(position, &candidate.uci, 0).is_ok()
        })
}

pub fn board_terminal_outcome(
    game: &Game,
    game_move: &crate::domain::ImportedMove,
) -> Result<BoardTerminalOutcome, RuleExtractorError> {
    if game
        .moves
        .last()
        .is_none_or(|last| last.ply != game_move.ply)
    {
        return Err(RuleExtractorError::InvalidTerminalOutcome { ply: game_move.ply });
    }
    let position: Chess = Fen::from_ascii(game.final_position.as_bytes())
        .ok()
        .and_then(|fen| fen.into_position(CastlingMode::Standard).ok())
        .ok_or(RuleExtractorError::InvalidTerminalOutcome { ply: game_move.ply })?;
    if position.is_checkmate() {
        Ok(BoardTerminalOutcome::Checkmate {
            winner: color_from_side(game_move.side),
        })
    } else if position.is_stalemate() {
        Ok(BoardTerminalOutcome::Stalemate)
    } else if position.is_insufficient_material() {
        Ok(BoardTerminalOutcome::InsufficientMaterial)
    } else {
        Err(RuleExtractorError::InvalidTerminalOutcome { ply: game_move.ply })
    }
}

pub(super) fn color_from_side(side: MoveSide) -> crate::review_session_contract::Color {
    match side {
        MoveSide::White => crate::review_session_contract::Color::White,
        MoveSide::Black => crate::review_session_contract::Color::Black,
    }
}
