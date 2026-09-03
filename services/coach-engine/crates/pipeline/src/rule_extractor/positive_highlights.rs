use crate::{
    causal_facts::{MechanismPayoff, PlayedMoveEffect, TacticalMechanism},
    review_session_contract::{
        BoardTerminalOutcome, GameReviewMechanismPayoff, GameReviewPieceRole,
        PositiveHighlightAchievement,
    },
};

pub(super) fn positive_achievements(
    played_move_uci: &str,
    effects: &[PlayedMoveEffect],
    mechanism: Option<&TacticalMechanism>,
    terminal: Option<BoardTerminalOutcome>,
) -> Vec<PositiveHighlightAchievement> {
    let mut achievements = effects
        .iter()
        .filter_map(|effect| match effect {
            PlayedMoveEffect::CapturedPiece { role, square } => {
                Some(PositiveHighlightAchievement::CapturedPiece {
                    role: piece_role(*role),
                    square: square.clone(),
                })
            }
            PlayedMoveEffect::AdvancedPassedPawn { to_square } => {
                Some(PositiveHighlightAchievement::AdvancedPassedPawn {
                    to_square: to_square.clone(),
                })
            }
            PlayedMoveEffect::AttackedPiece { .. } | PlayedMoveEffect::AllowsQueenExchange => None,
        })
        .collect::<Vec<_>>();
    if let Some(payoff) = mechanism
        .filter(|mechanism| {
            mechanism
                .moves
                .first()
                .is_some_and(|line_move| line_move.uci == played_move_uci)
        })
        .and_then(credited_payoff)
    {
        achievements.push(PositiveHighlightAchievement::TacticalPayoff { payoff });
    }
    match terminal {
        Some(BoardTerminalOutcome::Checkmate { .. }) => {
            achievements.push(PositiveHighlightAchievement::CompletedCheckmate)
        }
        Some(BoardTerminalOutcome::Stalemate | BoardTerminalOutcome::InsufficientMaterial)
        | None => {}
    }
    achievements
}

/// The payoff the played move earned, as against one its line reaches later.
///
/// `moves` is truncated at the payoff ply, so a single move means the played
/// move is the payoff. A material payoff deeper than that belongs to the
/// continuation: the capture has not happened, the opponent has to walk into
/// it, and crediting it to the move is what put "You won a knight" on quiet
/// moves that captured nothing. Four such moments sit in the corpus, one of
/// them a queen move whose capture arrives two plies later.
///
/// Only material is gated. Mate, promotion and a settled queen exchange are not
/// claims about a piece the Player won, and a deep forced mate is a real
/// achievement rather than a misattributed one.
fn credited_payoff(mechanism: &TacticalMechanism) -> Option<GameReviewMechanismPayoff> {
    let payoff = mechanism_payoff(&mechanism.payoff);
    match payoff {
        GameReviewMechanismPayoff::WinsMaterialOutright { .. }
        | GameReviewMechanismPayoff::WinsMaterialNet { .. } => {
            (mechanism.moves.len() == 1).then_some(payoff)
        }
        GameReviewMechanismPayoff::Mate
        | GameReviewMechanismPayoff::Promotion
        | GameReviewMechanismPayoff::QueenExchange => Some(payoff),
    }
}

fn piece_role(role: crate::causal_facts::PieceRole) -> GameReviewPieceRole {
    match role {
        crate::causal_facts::PieceRole::Pawn => GameReviewPieceRole::Pawn,
        crate::causal_facts::PieceRole::Knight => GameReviewPieceRole::Knight,
        crate::causal_facts::PieceRole::Bishop => GameReviewPieceRole::Bishop,
        crate::causal_facts::PieceRole::Rook => GameReviewPieceRole::Rook,
        crate::causal_facts::PieceRole::Queen => GameReviewPieceRole::Queen,
    }
}

fn mechanism_payoff(payoff: &MechanismPayoff) -> GameReviewMechanismPayoff {
    match payoff {
        MechanismPayoff::Mate => GameReviewMechanismPayoff::Mate,
        MechanismPayoff::Promotion => GameReviewMechanismPayoff::Promotion,
        MechanismPayoff::WinsMaterialOutright { role } => {
            GameReviewMechanismPayoff::WinsMaterialOutright {
                role: piece_role(*role),
            }
        }
        MechanismPayoff::WinsMaterialNet {
            role,
            net_pawn_units,
        } => GameReviewMechanismPayoff::WinsMaterialNet {
            role: piece_role(*role),
            net_pawn_units: *net_pawn_units,
        },
        MechanismPayoff::QueenExchange => GameReviewMechanismPayoff::QueenExchange,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::causal_facts::{LineMove, PieceRole};

    fn line(sans: &[(&str, &str)]) -> Vec<LineMove> {
        sans.iter()
            .map(|(uci, san)| LineMove {
                uci: (*uci).to_string(),
                san: (*san).to_string(),
            })
            .collect()
    }

    fn mechanism(moves: Vec<LineMove>, payoff: MechanismPayoff) -> TacticalMechanism {
        TacticalMechanism {
            moves,
            forcing_index: 0,
            payoff,
        }
    }

    #[test]
    fn a_capture_the_played_move_makes_is_credited() {
        let mechanism = mechanism(
            line(&[("e5d4", "exd4")]),
            MechanismPayoff::WinsMaterialOutright {
                role: PieceRole::Knight,
            },
        );
        assert_eq!(
            positive_achievements("e5d4", &[], Some(&mechanism), None),
            vec![PositiveHighlightAchievement::TacticalPayoff {
                payoff: GameReviewMechanismPayoff::WinsMaterialOutright {
                    role: GameReviewPieceRole::Knight,
                },
            }]
        );
    }

    /// The corpus shape of the defect: a quiet queen move whose capture arrives
    /// two plies later, stored as having won a knight.
    #[test]
    fn a_capture_the_continuation_makes_is_not_credited() {
        let mechanism = mechanism(
            line(&[("d8c7", "Qc7"), ("c2c4", "c4"), ("c7e5", "Qxe5")]),
            MechanismPayoff::WinsMaterialOutright {
                role: PieceRole::Knight,
            },
        );
        assert!(positive_achievements("d8c7", &[], Some(&mechanism), None).is_empty());
    }

    #[test]
    fn a_settled_net_the_continuation_makes_is_not_credited_either() {
        let mechanism = mechanism(
            line(&[("c8a6", "Ba6"), ("c2c3", "c3"), ("a6f1", "Bxf1")]),
            MechanismPayoff::WinsMaterialNet {
                role: PieceRole::Rook,
                net_pawn_units: 3,
            },
        );
        assert!(positive_achievements("c8a6", &[], Some(&mechanism), None).is_empty());
    }

    /// Depth is gated for material only. A forced mate is not a claim about a
    /// piece the Player won, and its whole point is that it is still coming.
    #[test]
    fn a_forced_mate_deeper_in_the_line_is_still_credited() {
        let mechanism = mechanism(
            line(&[("d1d4", "Qd4"), ("g1f3", "Kf3"), ("d4f4", "Qf4#")]),
            MechanismPayoff::Mate,
        );
        assert_eq!(
            positive_achievements("d1d4", &[], Some(&mechanism), None),
            vec![PositiveHighlightAchievement::TacticalPayoff {
                payoff: GameReviewMechanismPayoff::Mate,
            }]
        );
    }

    /// The gate takes the payoff away, not the moment. A move that captured
    /// something still says so.
    #[test]
    fn an_ungated_effect_survives_the_gate() {
        let mechanism = mechanism(
            line(&[("d8c7", "Qc7"), ("c2c4", "c4"), ("c7e5", "Qxe5")]),
            MechanismPayoff::WinsMaterialOutright {
                role: PieceRole::Knight,
            },
        );
        let effects = [PlayedMoveEffect::CapturedPiece {
            role: PieceRole::Pawn,
            square: "c7".to_string(),
        }];
        assert_eq!(
            positive_achievements("d8c7", &effects, Some(&mechanism), None),
            vec![PositiveHighlightAchievement::CapturedPiece {
                role: GameReviewPieceRole::Pawn,
                square: "c7".to_string(),
            }]
        );
    }
}
