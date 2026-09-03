use crate::{
    domain::ImportedMove,
    review_session_contract::{PositionPhase, PositionPhaseKind, POSITION_PHASE_POLICY_VERSION},
};

/// Classifies the pre-move Position with the shared deterministic V1 policy.
pub fn classify_position_phase(game_move: &ImportedMove) -> PositionPhase {
    let material_units = game_move
        .position
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .bytes()
        .map(|piece| match piece.to_ascii_lowercase() {
            b'n' | b'b' => 1,
            b'r' => 2,
            b'q' => 4,
            _ => 0,
        })
        .sum::<u32>();
    let phase = if game_move.move_number <= 12 && material_units >= 18 {
        PositionPhaseKind::Opening
    } else if material_units <= 8 {
        PositionPhaseKind::Endgame
    } else {
        PositionPhaseKind::Middlegame
    };
    PositionPhase {
        policy_version: POSITION_PHASE_POLICY_VERSION,
        phase,
    }
}

#[cfg(test)]
mod tests {
    use crate::{pgn::parse_pgn, review_session_contract::PositionPhasePolicyVersion};

    use super::*;

    #[test]
    fn v1_classifies_the_pre_move_position_without_opening_metadata() {
        let game = parse_pgn(
            "[Result \"*\"]\n\n1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Bxc6 dxc6 5. d3 Be6 6. Be3 Qd7 7. Nbd2 O-O-O 8. O-O f6 9. a3 g5 10. b4 h5 11. c4 h4 12. d4 g4 13. d5 g3 14. dxe6",
        )
        .unwrap();

        assert_eq!(
            classify_position_phase(&game.moves[0]).phase,
            PositionPhaseKind::Opening
        );
        assert_eq!(
            classify_position_phase(&game.moves[24]).phase,
            PositionPhaseKind::Middlegame
        );
        assert_eq!(
            classify_position_phase(game.moves.last().unwrap()).policy_version,
            PositionPhasePolicyVersion::V1
        );

        let endgame =
            parse_pgn("[FEN \"7k/8/8/8/8/8/P7/K7 w - - 0 1\"]\n[Result \"*\"]\n\n1. a4 *").unwrap();
        assert_eq!(
            classify_position_phase(&endgame.moves[0]).phase,
            PositionPhaseKind::Endgame
        );
    }
}
