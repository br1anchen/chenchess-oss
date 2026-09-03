use shakmaty::{fen::Fen, CastlingMode, Chess, EnPassantMode, Position, Role};

use super::{
    CastlingRights, Color, DrawClaimReason, DrawClaimState, DrawReason, EnPassantState,
    OccupiedSquare, Piece, PieceRole, PositionSnapshot, PositionStatus, RepetitionState, Square,
};

impl PositionSnapshot {
    pub(super) fn has_canonical_position_fields(&self) -> bool {
        let Ok(fen) = Fen::from_ascii(self.fen.as_bytes()) else {
            return false;
        };
        let Ok(position) = fen.into_position::<Chess>(CastlingMode::Standard) else {
            return false;
        };
        if Fen::from_position(position.clone(), EnPassantMode::Legal).to_string() != self.fen {
            return false;
        }

        let fields = self.fen.split_whitespace().collect::<Vec<_>>();
        let [_, side, castling, en_passant, halfmove, fullmove] = fields.as_slice() else {
            return false;
        };
        let expected_occupied = shakmaty::Square::ALL
            .into_iter()
            .filter_map(|square| {
                position
                    .board()
                    .piece_at(square)
                    .map(|piece| (square, piece))
            })
            .map(|(square, piece)| {
                Some(OccupiedSquare {
                    square: Square::try_from(square.to_string()).ok()?,
                    piece: Piece {
                        color: if piece.color.is_white() {
                            Color::White
                        } else {
                            Color::Black
                        },
                        role: match piece.role {
                            Role::Pawn => PieceRole::Pawn,
                            Role::Knight => PieceRole::Knight,
                            Role::Bishop => PieceRole::Bishop,
                            Role::Rook => PieceRole::Rook,
                            Role::Queen => PieceRole::Queen,
                            Role::King => PieceRole::King,
                        },
                    },
                })
            })
            .collect::<Option<Vec<_>>>();
        let expected_en_passant = match *en_passant {
            "-" => EnPassantState::None,
            square => {
                let Ok(square) = Square::try_from(square.to_string()) else {
                    return false;
                };
                EnPassantState::Available { square }
            }
        };
        let expected_side = match *side {
            "w" => Some(Color::White),
            "b" => Some(Color::Black),
            _ => None,
        };
        let expected_castling = CastlingRights {
            white_king_side: castling.contains('K'),
            white_queen_side: castling.contains('Q'),
            black_king_side: castling.contains('k'),
            black_queen_side: castling.contains('q'),
        };
        let Ok(expected_halfmove) = halfmove.parse::<u16>() else {
            return false;
        };
        let Ok(expected_fullmove) = fullmove.parse::<u32>() else {
            return false;
        };
        expected_occupied.as_ref() == Some(&self.occupied)
            && expected_side == Some(self.side_to_move)
            && expected_castling == self.castling_rights
            && expected_en_passant == self.en_passant
            && expected_halfmove == self.halfmove_clock
            && expected_fullmove == self.fullmove_number
            && self.has_canonical_rule_state(&position)
    }

    fn has_canonical_rule_state(&self, position: &Chess) -> bool {
        let occurrences = match self.repetition {
            RepetitionState::FirstOccurrence => 1,
            RepetitionState::Repeated { occurrences: 2 } => 2,
            RepetitionState::Repeated { .. } => return false,
            RepetitionState::DrawClaimAvailable { occurrences } if occurrences >= 3 => occurrences,
            RepetitionState::DrawClaimAvailable { .. } => return false,
        };
        let forced = position.is_checkmate()
            || position.is_stalemate()
            || position.is_insufficient_material()
            || occurrences >= 5
            || self.halfmove_clock >= 150;
        let expected_status = if position.is_checkmate() {
            PositionStatus::Checkmate {
                winner: match self.side_to_move {
                    Color::White => Color::Black,
                    Color::Black => Color::White,
                },
            }
        } else if position.is_stalemate() {
            PositionStatus::Draw {
                reason: DrawReason::Stalemate,
            }
        } else if position.is_insufficient_material() {
            PositionStatus::Draw {
                reason: DrawReason::InsufficientMaterial,
            }
        } else if occurrences >= 5 {
            PositionStatus::Draw {
                reason: DrawReason::FivefoldRepetition,
            }
        } else if self.halfmove_clock >= 150 {
            PositionStatus::Draw {
                reason: DrawReason::SeventyFiveMoveRule,
            }
        } else {
            let mut claims = Vec::new();
            if occurrences >= 3 {
                claims.push(DrawClaimReason::ThreefoldRepetition);
            }
            if self.halfmove_clock >= 100 {
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
        };
        self.status == expected_status
            || (!forced
                && (matches!(
                    self.status,
                    PositionStatus::Draw {
                        reason: DrawReason::Agreement
                    }
                ) || (occurrences >= 3
                    && matches!(
                        self.status,
                        PositionStatus::Draw {
                            reason: DrawReason::ThreefoldRepetition
                        }
                    ))
                    || (self.halfmove_clock >= 100
                        && matches!(
                            self.status,
                            PositionStatus::Draw {
                                reason: DrawReason::FiftyMoveRule
                            }
                        ))))
    }
}
