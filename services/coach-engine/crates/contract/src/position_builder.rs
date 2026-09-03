use anyhow::{bail, Result};
use serde::Serialize;
use shakmaty::{fen::Fen, CastlingMode, Chess, EnPassantMode, Position, Role};

use super::*;

pub const STANDARD_STARTING_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

pub fn build_position_snapshot(
    fen: &str,
    preceding_position_fens: &[&str],
) -> Result<PositionSnapshot> {
    let position: Chess = Fen::from_ascii(fen.as_bytes())?.into_position(CastlingMode::Standard)?;
    let canonical_fen = Fen::from_position(position.clone(), EnPassantMode::Legal).to_string();
    let occupied = shakmaty::Square::ALL
        .into_iter()
        .filter_map(|square| {
            position
                .board()
                .piece_at(square)
                .map(|piece| (square, piece))
        })
        .map(|(square, piece)| {
            Ok(OccupiedSquare {
                square: Square::try_from(square.to_string())?,
                piece: Piece {
                    color: color(piece.color),
                    role: role(piece.role),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let fields = fen_fields(&canonical_fen)?;
    let castling = fields[2];
    let castling_rights = CastlingRights {
        white_king_side: castling.contains('K'),
        white_queen_side: castling.contains('Q'),
        black_king_side: castling.contains('k'),
        black_queen_side: castling.contains('q'),
    };
    let en_passant = match fields[3] {
        "-" => EnPassantState::None,
        square => EnPassantState::Available {
            square: Square::try_from(square.to_string())?,
        },
    };

    let current_repetition_key = repetition_key(fen)?;
    let mut repetition_keys = preceding_position_fens
        .iter()
        .map(|preceding| repetition_key(preceding))
        .collect::<Result<Vec<_>>>()?;
    let occurrence_count = repetition_keys
        .iter()
        .filter(|key| *key == &current_repetition_key)
        .count()
        + 1;
    let repetition = repetition_state(occurrence_count);
    repetition_keys.push(current_repetition_key);
    let history_digest =
        ArtifactDigest::try_from(canonical_sha256(&PositionHistory { repetition_keys }))?;
    let halfmove_clock = fields[4].parse::<u16>()?;
    let fullmove_number = fields[5].parse()?;
    let side_to_move = if fields[1] == "w" {
        Color::White
    } else {
        Color::Black
    };
    let status = position_status(&position, side_to_move, occurrence_count, halfmove_clock);
    let mut snapshot = PositionSnapshot {
        position_ref: PositionRef::try_from(zero_digest())?,
        variant: PositionVariant::Standard,
        fen: canonical_fen,
        occupied,
        side_to_move,
        castling_rights,
        en_passant,
        halfmove_clock,
        fullmove_number,
        repetition,
        status,
        history_digest,
    };
    snapshot.position_ref = snapshot.computed_position_ref();
    Ok(snapshot)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionHistory {
    repetition_keys: Vec<String>,
}

fn position_status(
    position: &Chess,
    side_to_move: Color,
    occurrence_count: usize,
    halfmove_clock: u16,
) -> PositionStatus {
    if position.is_checkmate() {
        PositionStatus::Checkmate {
            winner: opposite(side_to_move),
        }
    } else if position.is_stalemate() {
        PositionStatus::Draw {
            reason: DrawReason::Stalemate,
        }
    } else if position.is_insufficient_material() {
        PositionStatus::Draw {
            reason: DrawReason::InsufficientMaterial,
        }
    } else if occurrence_count >= 5 {
        PositionStatus::Draw {
            reason: DrawReason::FivefoldRepetition,
        }
    } else if halfmove_clock >= 150 {
        PositionStatus::Draw {
            reason: DrawReason::SeventyFiveMoveRule,
        }
    } else {
        ongoing_status(occurrence_count, halfmove_clock)
    }
}

fn ongoing_status(occurrence_count: usize, halfmove_clock: u16) -> PositionStatus {
    let mut claims = Vec::new();
    if occurrence_count >= 3 {
        claims.push(DrawClaimReason::ThreefoldRepetition);
    }
    if halfmove_clock >= 100 {
        claims.push(DrawClaimReason::FiftyMoveRule);
    }
    PositionStatus::Ongoing {
        draw_claims: claims
            .first()
            .copied()
            .map_or(DrawClaimState::None, |first| DrawClaimState::Available {
                first,
                rest: claims[1..].to_vec(),
            }),
    }
}

fn repetition_state(occurrence_count: usize) -> RepetitionState {
    match occurrence_count {
        1 => RepetitionState::FirstOccurrence,
        2 => RepetitionState::Repeated { occurrences: 2 },
        count => RepetitionState::DrawClaimAvailable {
            occurrences: u8::try_from(count).unwrap_or(u8::MAX),
        },
    }
}

fn repetition_key(fen: &str) -> Result<String> {
    let position: Chess = Fen::from_ascii(fen.as_bytes())?.into_position(CastlingMode::Standard)?;
    let canonical = Fen::from_position(position, EnPassantMode::Legal).to_string();
    Ok(fen_fields(&canonical)?[..4].join(" "))
}

fn fen_fields(fen: &str) -> Result<Vec<&str>> {
    let fields = fen.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 6 || !matches!(fields[1], "w" | "b") {
        bail!("canonical FEN must have six fields and a valid side to move")
    }
    Ok(fields)
}

fn color(color: shakmaty::Color) -> Color {
    if color.is_white() {
        Color::White
    } else {
        Color::Black
    }
}

fn opposite(color: Color) -> Color {
    match color {
        Color::White => Color::Black,
        Color::Black => Color::White,
    }
}

fn role(role: Role) -> PieceRole {
    match role {
        Role::Pawn => PieceRole::Pawn,
        Role::Knight => PieceRole::Knight,
        Role::Bishop => PieceRole::Bishop,
        Role::Rook => PieceRole::Rook,
        Role::Queen => PieceRole::Queen,
        Role::King => PieceRole::King,
    }
}

fn zero_digest() -> String {
    format!("sha256:{}", "0".repeat(64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_uses_the_canonical_legal_en_passant_fen() {
        let snapshot = build_position_snapshot(
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
            &[],
        )
        .unwrap();

        assert_eq!(
            snapshot.fen,
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
        );
        assert_eq!(snapshot.en_passant, EnPassantState::None);
    }
}
