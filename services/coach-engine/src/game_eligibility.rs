use shakmaty::{fen::Fen, CastlingMode, Chess, Position};

use crate::{
    pgn::ParsedPgn,
    review_session_contract::{
        Color, CompletedGameOutcome, DecisiveGameTermination, DrawGameTermination,
        STANDARD_STARTING_FEN,
    },
    types::Game,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum GameEligibilityError {
    #[error("only complete standard-chess Games are supported")]
    UnsupportedVariant,
    #[error("Game is ongoing")]
    Ongoing,
    #[error("Game was aborted")]
    Aborted,
    #[error("invalid Game import: {0}")]
    Invalid(&'static str),
}

pub(crate) fn completed_standard_outcome(
    parsed: &ParsedPgn,
) -> Result<CompletedGameOutcome, GameEligibilityError> {
    if parsed
        .metadata
        .variant
        .as_deref()
        .is_some_and(|variant| !variant.eq_ignore_ascii_case("standard"))
        || parsed
            .game
            .moves
            .first()
            .map(|game_move| game_move.position.as_str())
            != Some(STANDARD_STARTING_FEN)
    {
        return Err(GameEligibilityError::UnsupportedVariant);
    }

    if parsed
        .metadata
        .termination
        .as_deref()
        .is_some_and(is_aborted_termination)
    {
        return Err(GameEligibilityError::Aborted);
    }

    if !matches!(
        parsed.game.result.as_deref(),
        Some("1-0" | "0-1" | "1/2-1/2")
    ) {
        return Err(GameEligibilityError::Ongoing);
    }

    completed_outcome(&parsed.game)
}

/// The Games Daily Coaching may select from: completed standard Games that both
/// sides played to a finish.
///
/// A Game decided by a player walking away is a Game the Player played, so
/// [`completed_standard_outcome`] admits it and they may import and review it.
/// It is still not material for a Digest they did not ask for — nobody chose
/// that ending, and there is nothing to coach about it — so the automatic feed
/// asks this narrower question instead.
///
/// `Option` rather than `Result` because an ineligible archive record is one the
/// feed skips, not an error it reports. No caller reads a reason.
pub(crate) fn daily_coaching_outcome(parsed: &ParsedPgn) -> Option<CompletedGameOutcome> {
    let outcome = completed_standard_outcome(parsed).ok()?;
    if parsed
        .metadata
        .termination
        .as_deref()
        .is_some_and(is_decisive_abandonment)
    {
        return None;
    }
    Some(outcome)
}

/// Whether the Game was voided rather than concluded.
///
/// A voided Game names no winner and carries no result, so this stays narrow and
/// the result check in [`completed_standard_outcome`] catches the rest.
fn is_aborted_termination(value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("aborted") || value.eq_ignore_ascii_case("abandoned")
}

/// Whether the Game was decided by a player walking away from a Game that had
/// been played.
///
/// Chess.com states it as a sentence naming the winner — "Hikaru won - game
/// abandoned" — on a full Game score with a decisive result.
fn is_decisive_abandonment(value: &str) -> bool {
    value
        .trim()
        .to_ascii_lowercase()
        .ends_with(" won - game abandoned")
}

fn completed_outcome(game: &Game) -> Result<CompletedGameOutcome, GameEligibilityError> {
    let position: Chess = Fen::from_ascii(game.final_position.as_bytes())
        .map_err(|_| GameEligibilityError::Invalid("invalid final Position"))?
        .into_position(CastlingMode::Standard)
        .map_err(|_| GameEligibilityError::Invalid("invalid final Position"))?;

    match game.result.as_deref() {
        Some("1-0") | Some("0-1") => {
            let winner = if game.result.as_deref() == Some("1-0") {
                Color::White
            } else {
                Color::Black
            };
            if position.is_stalemate() || position.is_insufficient_material() {
                return Err(GameEligibilityError::Invalid(
                    "decisive result contradicts the final Position",
                ));
            }
            if position.is_checkmate() && position.turn().is_white() == (winner == Color::White) {
                return Err(GameEligibilityError::Invalid(
                    "checkmate winner contradicts the final Position",
                ));
            }
            Ok(CompletedGameOutcome::Decisive {
                winner,
                termination: if position.is_checkmate() {
                    DecisiveGameTermination::Checkmate
                } else {
                    DecisiveGameTermination::Other
                },
            })
        }
        Some("1/2-1/2") => {
            if position.is_checkmate() {
                return Err(GameEligibilityError::Invalid(
                    "draw result contradicts the final Position",
                ));
            }
            let termination = if position.is_stalemate() {
                DrawGameTermination::Stalemate
            } else if position.is_insufficient_material() {
                DrawGameTermination::InsufficientMaterial
            } else {
                DrawGameTermination::Other
            };
            Ok(CompletedGameOutcome::Draw { termination })
        }
        _ => Err(GameEligibilityError::Ongoing),
    }
}

#[cfg(test)]
mod tests {
    use crate::pgn::parse_pgn_with_metadata;

    use super::*;

    fn pgn(termination: &str) -> ParsedPgn {
        parse_pgn_with_metadata(&format!(
            "[Result \"1-0\"]\n[Termination \"{termination}\"]\n\n1. e4 e5 1-0"
        ))
        .unwrap()
    }

    /// A Game won because the opponent walked away is a Game the Player played,
    /// and they may import and review it. Ten Games in the commentary ladder
    /// carry this exact form on complete scores of 39 to 143 moves.
    #[test]
    fn an_import_admits_a_win_by_opponent_abandonment() {
        assert_eq!(
            completed_standard_outcome(&pgn("Player won - game abandoned")),
            Ok(CompletedGameOutcome::Decisive {
                winner: Color::White,
                termination: DecisiveGameTermination::Other,
            })
        );
    }

    /// The same Game is not Digest material: nobody chose that ending.
    #[test]
    fn daily_coaching_skips_a_win_by_opponent_abandonment() {
        assert_eq!(
            daily_coaching_outcome(&pgn("Player won - game abandoned")),
            None
        );
    }

    #[test]
    fn daily_coaching_keeps_a_game_that_was_played_to_a_finish() {
        assert!(daily_coaching_outcome(&pgn("Player won by resignation")).is_some());
    }

    /// A Game that really was voided names no winner, and neither path takes it.
    #[test]
    fn a_bare_abort_or_abandonment_is_not_a_completed_game() {
        for termination in ["Aborted", "abandoned"] {
            assert_eq!(
                completed_standard_outcome(&pgn(termination)),
                Err(GameEligibilityError::Aborted),
                "{termination} names no winner"
            );
            assert_eq!(daily_coaching_outcome(&pgn(termination)), None);
        }
    }

    /// The result check, not the termination, is what rejects a voided Game.
    #[test]
    fn a_game_with_no_result_is_rejected_whatever_its_termination_says() {
        let parsed =
            parse_pgn_with_metadata("[Result \"*\"]\n[Termination \"Game aborted\"]\n\n1. e4 e5 *")
                .unwrap();

        assert_eq!(
            completed_standard_outcome(&parsed),
            Err(GameEligibilityError::Ongoing)
        );
    }
}
