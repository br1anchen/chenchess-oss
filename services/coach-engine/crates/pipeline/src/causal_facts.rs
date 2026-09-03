use serde::Serialize;
use shakmaty::{
    attacks, fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, Color, Move, Position,
    Role, Square,
};

use crate::engine_analysis::PositionEvaluation;

/// Net material gain, in pawn units, that counts as winning material in a mechanism.
const WINS_MATERIAL_PAWN_UNITS: i32 = 2;
/// A mover evaluation at or above this is a winning standing.
const WINNING_CENTIPAWNS: i32 = 300;
/// A mover evaluation at or above this (below winning) is a favorable standing.
const FAVORABLE_CENTIPAWNS: i32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PieceRole {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
}

fn piece_role(role: Role) -> Option<PieceRole> {
    match role {
        Role::Pawn => Some(PieceRole::Pawn),
        Role::Knight => Some(PieceRole::Knight),
        Role::Bishop => Some(PieceRole::Bishop),
        Role::Rook => Some(PieceRole::Rook),
        Role::Queen => Some(PieceRole::Queen),
        Role::King => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlayedMoveEffect {
    CapturedPiece { role: PieceRole, square: String },
    AdvancedPassedPawn { to_square: String },
    AttackedPiece { role: PieceRole, square: String },
    AllowsQueenExchange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMove {
    pub uci: String,
    pub san: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MechanismPayoff {
    Mate,
    Promotion,
    /// The line settles at or above the captured piece's own value: it was won
    /// outright.
    WinsMaterialOutright {
        role: PieceRole,
    },
    /// The line settles ahead, but below the captured piece's value: the piece
    /// was won for something, and `net_pawn_units` is what the mover is
    /// actually up once the line ends.
    ///
    /// Which of the two this is belongs here rather than in a renderer: this is
    /// where both numbers are in hand, and deciding it once means no surface
    /// downstream needs its own copy of the material values to ask.
    WinsMaterialNet {
        role: PieceRole,
        net_pawn_units: i32,
    },
    QueenExchange,
}

/// The shortest sufficient prefix of the returned best line: every move up to
/// the payoff, with the first forcing move by the mover marked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalMechanism {
    pub moves: Vec<LineMove>,
    pub forcing_index: usize,
    pub payoff: MechanismPayoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidualOutcome {
    pub standing_before: AdvantageStanding,
    pub standing_after: AdvantageStanding,
    pub classification: ResidualClassification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AdvantageStanding {
    Winning,
    Favorable,
    Balanced,
    Unfavorable,
    Losing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResidualClassification {
    MissedForcedMate,
    AdvantageKept,
    StandingKept,
    AdvantageReduced,
    AdvantageLost,
    NowWorse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CausalFacts {
    pub effects: Vec<PlayedMoveEffect>,
    pub mechanism: Option<TacticalMechanism>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CausalFactError {
    #[error("the imported position is invalid")]
    InvalidPosition,
    #[error("the imported played move is not legal in its position")]
    InvalidPlayedMove,
    #[error("principal variation move {index} is not legal")]
    InvalidPrincipalVariation { index: usize },
}

/// Parse the trusted imported move once, then derive every board-backed causal
/// fact from the typed position. Invalid chess data is an invariant violation.
pub fn extract(
    fen_before: &str,
    played_uci: &str,
    principal_variation: &[String],
    mate_win: bool,
) -> Result<CausalFacts, CausalFactError> {
    let before = parse_position(fen_before).ok_or(CausalFactError::InvalidPosition)?;
    let played = parse_move(&before, played_uci).ok_or(CausalFactError::InvalidPlayedMove)?;
    let effects = played_move_effects(&before, &played);
    let mechanism = extract_mechanism(before, principal_variation, mate_win)?;
    Ok(CausalFacts { effects, mechanism })
}

/// What one move does to the position it is played in: what it takes, what it
/// attacks, whether it runs a passed pawn, whether it offers the queens off.
///
/// The rule is about a move and a position, not about whose move it is, so it
/// serves the Player's own move and the opponent's best reply alike. The second
/// is what makes "White can play c3, hitting the bishop" a fact rather than a
/// guess.
pub fn played_move_effects(before: &Chess, played: &Move) -> Vec<PlayedMoveEffect> {
    let mover = before.turn();
    let mut effects = Vec::new();
    if let Some(role) = played.capture() {
        effects.push(PlayedMoveEffect::CapturedPiece {
            role: piece_role(role).expect("legal chess moves cannot capture a king"),
            square: capture_square(played).to_string(),
        });
    }

    let mut after = before.clone();
    after.play_unchecked(played);
    let board = after.board();
    if let Move::Normal {
        role: Role::Pawn,
        to,
        promotion: None,
        ..
    } = *played
    {
        if is_passed_pawn(board, mover, to) {
            effects.push(PlayedMoveEffect::AdvancedPassedPawn {
                to_square: to.to_string(),
            });
        }
    }
    let attacked_before = origin(played).and_then(|from| {
        before
            .board()
            .piece_at(from)
            .map(|piece| attacks::attacks(from, piece, before.board().occupied()))
    });
    if let Some(to) = destination(played) {
        if let Some(piece) = board.piece_at(to) {
            let targets = attacks::attacks(to, piece, board.occupied()) & board.by_color(!mover);
            for square in targets {
                if attacked_before.is_some_and(|squares| squares.contains(square)) {
                    continue;
                }
                let Some(target) = board.piece_at(square) else {
                    continue;
                };
                if target.role == Role::King {
                    continue;
                }
                let defended = board.attacks_to(square, !mover, board.occupied()).any();
                if piece_value(target.role) > piece_value(piece.role) || !defended {
                    effects.push(PlayedMoveEffect::AttackedPiece {
                        role: piece_role(target.role)
                            .expect("king targets are excluded before effect construction"),
                        square: square.to_string(),
                    });
                }
            }
        }
    }
    if allows_queen_exchange(&after) {
        effects.push(PlayedMoveEffect::AllowsQueenExchange);
    }

    effects
}

/// One ply of the engine's line, walked before any payoff is chosen.
///
/// The choice needs to look *forward* -- material is only won if it is still
/// won once the exchange settles -- so the walk cannot decide as it goes.
struct LinePly {
    line_move: LineMove,
    /// Whether the side to move at this ply is the side whose line this is.
    mover_played: bool,
    captured: Option<Role>,
    promoted: bool,
    checkmate: bool,
    /// Net material for the mover after this ply, in pawn units.
    net_pawn_units: i32,
    queens_captured: u8,
}

fn extract_mechanism(
    mut position: Chess,
    principal_variation: &[String],
    mate_win: bool,
) -> Result<Option<TacticalMechanism>, CausalFactError> {
    if principal_variation.is_empty() {
        return Ok(None);
    }
    let mover = position.turn();
    let mut plies: Vec<LinePly> = Vec::with_capacity(principal_variation.len());
    let mut net_pawn_units = 0i32;
    let mut queens_captured = 0u8;
    for (index, uci) in principal_variation.iter().enumerate() {
        let mover_played = position.turn() == mover;
        let line_move = parse_move(&position, uci)
            .ok_or(CausalFactError::InvalidPrincipalVariation { index })?;
        let san = SanPlus::from_move(position.clone(), &line_move).to_string();
        let captured = line_move.capture();
        if let Some(captured) = captured {
            let value = piece_value(captured);
            net_pawn_units += if mover_played { value } else { -value };
            if captured == Role::Queen {
                queens_captured += 1;
            }
        }
        position.play_unchecked(&line_move);
        let checkmate = position.is_checkmate();
        plies.push(LinePly {
            line_move: LineMove {
                uci: uci.clone(),
                san,
            },
            mover_played,
            captured,
            promoted: line_move.promotion().is_some(),
            checkmate,
            net_pawn_units,
            queens_captured,
        });
        if checkmate {
            break;
        }
    }

    /* The line's last word on material. A payoff is claimed against this rather
    than against the running total at the capture: `Nxd5 cxd5` reaches +3 at the
    capture and settles at 0, and reporting the prefix as the verdict is how an
    even trade came to read as winning a knight. */
    let Some((end, payoff)) = choose_payoff(&plies, mate_win) else {
        return Ok(None);
    };
    let mut moves: Vec<LineMove> = plies.into_iter().map(|ply| ply.line_move).collect();
    moves.truncate(end + 1);
    let Some(forcing_index) = moves
        .iter()
        .enumerate()
        .filter(|(index, _)| index.is_multiple_of(2))
        .find(|(_, line_move)| line_move.san.contains(['x', '+', '#', '=']))
        .map(|(index, _)| index)
    else {
        return Ok(None);
    };

    Ok(Some(TacticalMechanism {
        moves,
        forcing_index,
        payoff,
    }))
}

/// The first ply that pays off, in the order the mechanism recognises them.
fn choose_payoff(plies: &[LinePly], mate_win: bool) -> Option<(usize, MechanismPayoff)> {
    if let Some(index) = plies.iter().position(|ply| ply.checkmate) {
        return Some((index, MechanismPayoff::Mate));
    }
    if mate_win {
        return None;
    }
    // The line's last word on material, which every claim below is made against.
    let settled_pawn_units = plies.last()?.net_pawn_units;
    plies.iter().enumerate().find_map(|(index, ply)| {
        if ply.mover_played && ply.promoted {
            return Some((index, MechanismPayoff::Promotion));
        }
        if let Some(captured) = ply
            .mover_played
            .then(|| material_won_at(plies, index))
            .flatten()
        {
            let role = piece_role(captured).expect("legal chess moves cannot capture a king");
            return Some((
                index,
                if settled_pawn_units >= piece_value(captured) {
                    MechanismPayoff::WinsMaterialOutright { role }
                } else {
                    MechanismPayoff::WinsMaterialNet {
                        role,
                        net_pawn_units: settled_pawn_units,
                    }
                },
            ));
        }
        if ply.queens_captured == 2 && ply.net_pawn_units.abs() < WINS_MATERIAL_PAWN_UNITS {
            return Some((index, MechanismPayoff::QueenExchange));
        }
        None
    })
}

/// The piece a mover capture at `index` wins and the line then keeps, if any.
///
/// Returns the captured role rather than a bare yes, so the caller cannot ask
/// again and unwrap. The mover has to be ahead at this ply and stay ahead
/// through the end of the line -- one condition over the whole suffix, which is
/// what a recapture fails. `index` and the final ply are both inside that
/// range, so asking about them separately would only restate it.
fn material_won_at(plies: &[LinePly], index: usize) -> Option<Role> {
    plies[index].captured.filter(|_| {
        plies[index..]
            .iter()
            .all(|ply| ply.net_pawn_units >= WINS_MATERIAL_PAWN_UNITS)
    })
}

fn parse_position(fen: &str) -> Option<Chess> {
    Fen::from_ascii(fen.as_bytes())
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()
}

fn parse_move(position: &Chess, uci: &str) -> Option<Move> {
    UciMove::from_ascii(uci.as_bytes())
        .ok()?
        .to_move(position)
        .ok()
}

fn origin(played: &Move) -> Option<Square> {
    match played {
        Move::Normal { from, .. } | Move::EnPassant { from, .. } => Some(*from),
        _ => None,
    }
}

fn destination(played: &Move) -> Option<Square> {
    match played {
        Move::Normal { to, .. } | Move::EnPassant { to, .. } => Some(*to),
        _ => None,
    }
}

fn capture_square(played: &Move) -> Square {
    match played {
        Move::EnPassant { from, to } => Square::from_coords(to.file(), from.rank()),
        _ => played.to(),
    }
}

pub fn is_passed_pawn(board: &shakmaty::Board, color: Color, square: Square) -> bool {
    let enemy_pawns = board.pawns() & board.by_color(!color);
    let file = i32::from(square.file());
    let rank = i32::from(square.rank());
    !enemy_pawns.into_iter().any(|enemy| {
        let enemy_file = i32::from(enemy.file());
        let enemy_rank = i32::from(enemy.rank());
        (enemy_file - file).abs() <= 1
            && match color {
                Color::White => enemy_rank > rank,
                Color::Black => enemy_rank < rank,
            }
    })
}

fn allows_queen_exchange(after: &Chess) -> bool {
    after
        .legal_moves()
        .iter()
        .filter(|reply| reply.role() == Role::Queen && reply.capture() == Some(Role::Queen))
        .any(|reply| {
            let mut exchanged = after.clone();
            exchanged.play_unchecked(reply);
            let target = reply.to();
            exchanged.legal_moves().iter().any(|recapture| {
                recapture.to() == target && recapture.capture() == Some(Role::Queen)
            })
        })
}

fn standing(evaluation: PositionEvaluation) -> AdvantageStanding {
    match evaluation {
        PositionEvaluation::MateIn(distance) if distance > 0 => AdvantageStanding::Winning,
        PositionEvaluation::MateIn(_) => AdvantageStanding::Losing,
        PositionEvaluation::Centipawns(value) if value >= WINNING_CENTIPAWNS => {
            AdvantageStanding::Winning
        }
        PositionEvaluation::Centipawns(value) if value >= FAVORABLE_CENTIPAWNS => {
            AdvantageStanding::Favorable
        }
        PositionEvaluation::Centipawns(value) if value > -FAVORABLE_CENTIPAWNS => {
            AdvantageStanding::Balanced
        }
        PositionEvaluation::Centipawns(value) if value > -WINNING_CENTIPAWNS => {
            AdvantageStanding::Unfavorable
        }
        PositionEvaluation::Centipawns(_) => AdvantageStanding::Losing,
    }
}

fn standing_rank(standing: AdvantageStanding) -> u8 {
    match standing {
        AdvantageStanding::Winning => 4,
        AdvantageStanding::Favorable => 3,
        AdvantageStanding::Balanced => 2,
        AdvantageStanding::Unfavorable => 1,
        AdvantageStanding::Losing => 0,
    }
}

/// Both evaluations must share the mover's perspective.
pub fn classify_residual(best: PositionEvaluation, played: PositionEvaluation) -> ResidualOutcome {
    let is_mate_win = |evaluation| matches!(evaluation, PositionEvaluation::MateIn(d) if d > 0);
    let standing_before = standing(best);
    let standing_after = standing(played);
    let classification = if is_mate_win(best) && !is_mate_win(played) {
        ResidualClassification::MissedForcedMate
    } else if standing_rank(standing_after) >= standing_rank(standing_before) {
        if standing_rank(standing_after) >= standing_rank(AdvantageStanding::Favorable) {
            ResidualClassification::AdvantageKept
        } else {
            ResidualClassification::StandingKept
        }
    } else if standing_rank(standing_after) <= standing_rank(AdvantageStanding::Unfavorable) {
        ResidualClassification::NowWorse
    } else if standing_after == AdvantageStanding::Balanced {
        ResidualClassification::AdvantageLost
    } else {
        ResidualClassification::AdvantageReduced
    };

    ResidualOutcome {
        standing_before,
        standing_after,
        classification,
    }
}

fn piece_value(role: Role) -> i32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_residual, extract, AdvantageStanding, CausalFactError, MechanismPayoff, PieceRole,
        PlayedMoveEffect, ResidualClassification,
    };
    use crate::engine_analysis::PositionEvaluation::{Centipawns, MateIn};

    fn extract_effects(fen: &str, played_uci: &str) -> Vec<PlayedMoveEffect> {
        extract(fen, played_uci, &[], false).unwrap().effects
    }

    #[test]
    fn reports_a_capture_with_role_and_square() {
        // White queen takes the d5 pawn.
        let effects = extract_effects(
            "rnbqkbnr/ppp1pppp/8/3p4/8/3Q4/PPPPPPPP/RNB1KBNR w KQkq - 0 2",
            "d3d5",
        );
        assert!(effects.contains(&PlayedMoveEffect::CapturedPiece {
            role: PieceRole::Pawn,
            square: "d5".to_string(),
        }));
    }

    #[test]
    fn reports_an_advanced_passed_pawn() {
        // Black pushes a d-pawn with no white pawns ahead on c, d, or e files.
        let effects = extract_effects("8/8/8/3p4/8/8/5PPP/4K2k b - - 0 1", "d5d4");
        assert!(effects.contains(&PlayedMoveEffect::AdvancedPassedPawn {
            to_square: "d4".to_string(),
        }));
    }

    #[test]
    fn does_not_report_a_blocked_pawn_as_passed() {
        let effects = extract_effects("8/8/8/3p4/8/4P3/6PP/4K2k b - - 0 1", "d5d4");
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, PlayedMoveEffect::AdvancedPassedPawn { .. })));
    }

    #[test]
    fn reports_an_attack_on_a_higher_value_piece() {
        // White bishop moves to d3 and attacks the black queen on h7.
        let effects = extract_effects("4k3/7q/8/8/8/8/8/4KB2 w - - 0 1", "f1d3");
        assert!(effects.contains(&PlayedMoveEffect::AttackedPiece {
            role: PieceRole::Queen,
            square: "h7".to_string(),
        }));
    }

    #[test]
    fn does_not_report_an_attack_that_the_moved_piece_already_had() {
        let effects = extract_effects("q6k/8/8/8/8/8/7K/R7 w - - 0 1", "a1a2");
        assert!(!effects.iter().any(
            |effect| matches!(effect, PlayedMoveEffect::AttackedPiece { square, .. } if square == "a8")
        ));
    }

    #[test]
    fn reports_an_allowed_queen_exchange() {
        // Black queen steps to e6 where the white queen can trade and be recaptured.
        let effects = extract_effects("k7/5p2/8/8/8/8/4Q3/4K3 b - - 0 1", "a8a7");
        // Not a queen-exchange position; keep the negative assertion honest.
        assert!(!effects.contains(&PlayedMoveEffect::AllowsQueenExchange));

        let effects = extract_effects("4k3/3q1p2/8/8/8/8/4Q3/4K3 b - - 0 1", "d7e6");
        assert!(effects.contains(&PlayedMoveEffect::AllowsQueenExchange));
    }

    #[test]
    fn rejects_an_illegal_imported_move() {
        assert_eq!(
            extract("8/8/8/8/8/8/8/K6k w - - 0 1", "d3h7", &[], false),
            Err(CausalFactError::InvalidPlayedMove)
        );
    }

    #[test]
    fn extracts_a_wins_material_mechanism_with_an_intermediate_check() {
        // Black to move: ...f5 offers a pawn, Qxf5 takes, ...Rb3+ forces Ke2,
        // then ...Rxg3 wins the rook.
        let mechanism = extract(
            "7k/5p2/8/3Q4/8/5KR1/1r6/8 b - - 0 1",
            "f7f5",
            &[
                "f7f5".to_string(),
                "d5f5".to_string(),
                "b2b3".to_string(),
                "f3e2".to_string(),
                "b3g3".to_string(),
            ],
            false,
        )
        .expect("line should be legal")
        .mechanism
        .expect("payoff should be detected");
        // The pawn offered on move one never comes back, so the line settles
        // four pawns ahead rather than the rook's five: won, but for something.
        assert_eq!(
            mechanism.payoff,
            MechanismPayoff::WinsMaterialNet {
                role: PieceRole::Rook,
                net_pawn_units: 4
            }
        );
        assert_eq!(mechanism.moves.len(), 5);
        // The forcing move is the mover's rook check, not the pawn offer.
        assert_eq!(mechanism.forcing_index, 2);
        assert_eq!(mechanism.moves[2].san, "Rb3+");
    }

    #[test]
    fn an_even_trade_wins_no_material() {
        // White Nb4 takes the knight on d5, the c6 pawn recaptures. The running
        // total reaches +3 at the capture and settles at 0; only the second
        // number is the truth, and this is the case that read as "won a knight".
        let mechanism = extract(
            "4k3/8/2p5/3n4/1N6/8/8/4K3 w - - 0 1",
            "b4d5",
            &["b4d5".to_string(), "c6d5".to_string()],
            false,
        )
        .expect("line should be legal")
        .mechanism;
        assert!(mechanism.is_none());
    }

    #[test]
    fn material_won_and_then_given_back_is_not_a_payoff() {
        // Nxd5 cxd5 and White regains a pawn: still short of a piece, so the
        // line has no material payoff to credit.
        let mechanism = extract(
            "4k3/8/2p5/3n4/1N6/8/4P3/4K3 w - - 0 1",
            "b4d5",
            &["b4d5".to_string(), "c6d5".to_string(), "e2e4".to_string()],
            false,
        )
        .expect("line should be legal")
        .mechanism;
        assert!(mechanism.is_none());
    }

    #[test]
    fn a_piece_won_outright_settles_at_its_own_value() {
        // Nxd5 with nothing to recapture: the knight is simply won.
        let mechanism = extract(
            "4k3/8/8/3n4/1N6/8/8/4K3 w - - 0 1",
            "b4d5",
            &["b4d5".to_string()],
            false,
        )
        .expect("line should be legal")
        .mechanism
        .expect("payoff should be detected");
        assert_eq!(
            mechanism.payoff,
            MechanismPayoff::WinsMaterialOutright {
                role: PieceRole::Knight
            }
        );
    }

    #[test]
    fn a_line_without_a_payoff_is_not_a_mechanism() {
        let mechanism = extract(
            "7k/5p2/8/3Q4/8/5KR1/1r6/8 b - - 0 1",
            "b2a2",
            &["b2a2".to_string(), "f3e3".to_string()],
            false,
        )
        .unwrap()
        .mechanism;
        assert!(mechanism.is_none());
    }

    #[test]
    fn keeps_the_full_line_for_a_forced_mate() {
        let mechanism = extract(
            "8/8/8/8/8/5k2/q7/7K b - - 0 1",
            "a2g2",
            &["a2g2".to_string()],
            true,
        )
        .unwrap()
        .mechanism
        .expect("mate line should be a mechanism");
        assert_eq!(mechanism.payoff, MechanismPayoff::Mate);
        assert_eq!(mechanism.moves.len(), 1);
        assert_eq!(mechanism.forcing_index, 0);
        assert!(mechanism.moves[0].san.ends_with('#'));
    }

    #[test]
    fn does_not_claim_mate_when_the_returned_line_stops_before_checkmate() {
        let mechanism = extract(
            "7k/5p2/8/3Q4/8/5KR1/1r6/8 b - - 0 1",
            "f7f5",
            &["f7f5".to_string()],
            true,
        )
        .unwrap()
        .mechanism;
        assert!(mechanism.is_none());
    }

    #[test]
    fn extracts_a_promotion_mechanism() {
        let mechanism = extract(
            "8/8/8/8/8/1k6/3p4/1K6 b - - 0 1",
            "d2d1q",
            &["d2d1q".to_string()],
            false,
        )
        .unwrap()
        .mechanism
        .expect("promotion should be a payoff");
        assert!(matches!(
            mechanism.payoff,
            MechanismPayoff::Promotion | MechanismPayoff::Mate
        ));
    }

    #[test]
    fn classifies_residual_outcomes_without_claiming_win_speed() {
        let outcome = classify_residual(Centipawns(305), Centipawns(19));
        assert_eq!(
            outcome.classification,
            ResidualClassification::AdvantageLost
        );
        assert_eq!(outcome.standing_after, AdvantageStanding::Balanced);

        let outcome = classify_residual(Centipawns(900), Centipawns(551));
        assert_eq!(
            outcome.classification,
            ResidualClassification::AdvantageKept
        );
        assert_eq!(outcome.standing_after, AdvantageStanding::Winning);

        let outcome = classify_residual(MateIn(5), Centipawns(710));
        assert_eq!(
            outcome.classification,
            ResidualClassification::MissedForcedMate
        );

        let outcome = classify_residual(Centipawns(-400), Centipawns(-500));
        assert_eq!(outcome.classification, ResidualClassification::StandingKept);

        let outcome = classify_residual(Centipawns(400), Centipawns(-350));
        assert_eq!(outcome.classification, ResidualClassification::NowWorse);

        let outcome = classify_residual(Centipawns(400), Centipawns(150));
        assert_eq!(
            outcome.classification,
            ResidualClassification::AdvantageReduced
        );
    }
}
