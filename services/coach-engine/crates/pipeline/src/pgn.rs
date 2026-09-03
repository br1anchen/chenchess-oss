use shakmaty::{
    fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, EnPassantMode, Move, Position, Role,
};

use crate::domain::{Game, ImportedMove, MoveSide};

mod syntax;

use syntax::parse_single_game;

#[derive(Debug, thiserror::Error)]
pub enum PgnImportError {
    #[error("PGN is empty")]
    Empty,
    #[error("PGN contains no legal main-line moves")]
    NoMoves,
    #[error("illegal move {notation} at ply {ply}")]
    IllegalMove { notation: String, ply: usize },
    #[error("invalid starting position: {0}")]
    InvalidStartingPosition(String),
    #[error("invalid PGN: {0}")]
    InvalidPgn(String),
}

#[derive(Debug, Clone, Default)]
pub struct PgnMetadata {
    pub game_id: Option<String>,
    pub opening_attribution_headers: Vec<String>,
    pub variant: Option<String>,
    pub white_elo: Option<String>,
    pub black_elo: Option<String>,
    pub eco: Option<String>,
    pub opening: Option<String>,
    pub termination: Option<String>,
    pub date: Option<String>,
    pub time: Option<String>,
    pub utc_date: Option<String>,
    pub utc_time: Option<String>,
    pub end_date: Option<String>,
    pub end_time: Option<String>,
    pub time_control: Option<String>,
}

#[derive(Default)]
struct GameHeaders {
    white: Option<String>,
    black: Option<String>,
    event: Option<String>,
    site: Option<String>,
    result: Option<String>,
    metadata: PgnMetadata,
}

#[derive(Debug, Clone)]
pub struct ParsedPgn {
    pub game: Game,
    pub metadata: PgnMetadata,
}

fn position_fen(position: &Chess) -> String {
    Fen::from_position(position.clone(), EnPassantMode::Legal).to_string()
}

pub fn parse_pgn(input: &str) -> Result<Game, PgnImportError> {
    Ok(parse_pgn_with_metadata(input)?.game)
}

pub fn parse_pgn_with_metadata(input: &str) -> Result<ParsedPgn, PgnImportError> {
    if input.trim().is_empty() {
        return Err(PgnImportError::Empty);
    }
    let syntax = parse_single_game(input).map_err(PgnImportError::InvalidPgn)?;
    let mut headers = GameHeaders::default();
    let mut position = Chess::default();

    for (key, value) in syntax.tags {
        match key.as_str() {
            "White" => headers.white = Some(value),
            "Black" => headers.black = Some(value),
            "Event" => headers.event = Some(value),
            "Site" => {
                headers.site = Some(value.clone());
                headers.metadata.opening_attribution_headers.push(value);
            }
            "Link" => headers.metadata.opening_attribution_headers.push(value),
            "Result" => headers.result = Some(value),
            "GameId" => headers.metadata.game_id = Some(value),
            "Variant" => headers.metadata.variant = Some(value),
            "WhiteElo" => headers.metadata.white_elo = Some(value),
            "BlackElo" => headers.metadata.black_elo = Some(value),
            "ECO" => headers.metadata.eco = Some(value),
            "Opening" => headers.metadata.opening = Some(value),
            "Termination" => headers.metadata.termination = Some(value),
            "Date" => headers.metadata.date = Some(value),
            "Time" | "StartTime" => headers.metadata.time = Some(value),
            "UTCDate" => headers.metadata.utc_date = Some(value),
            "UTCTime" => headers.metadata.utc_time = Some(value),
            "EndDate" => headers.metadata.end_date = Some(value),
            "EndTime" => headers.metadata.end_time = Some(value),
            "TimeControl" => headers.metadata.time_control = Some(value),
            "FEN" => {
                position = Fen::from_ascii(value.as_bytes())
                    .map_err(|error| PgnImportError::InvalidStartingPosition(error.to_string()))?
                    .into_position(CastlingMode::Standard)
                    .map_err(|error| PgnImportError::InvalidStartingPosition(error.to_string()))?;
            }
            _ => {}
        }
    }

    let mut moves = Vec::with_capacity(syntax.moves.len());
    for raw_notation in syntax.moves {
        let normalized_notation = normalize_castling(&raw_notation);
        let ply = moves.len() + 1;
        let side = if position.turn().is_white() {
            MoveSide::White
        } else {
            MoveSide::Black
        };
        let move_number = position.fullmoves().get();
        let move_position = position_fen(&position);
        let chess_move =
            resolve_move(&position, &normalized_notation).map_err(|error| match error {
                MoveResolutionError::Unsupported => {
                    PgnImportError::InvalidPgn(format!("unsupported move notation {raw_notation}"))
                }
                MoveResolutionError::Illegal => PgnImportError::IllegalMove {
                    notation: raw_notation.clone(),
                    ply,
                },
            })?;
        let san = SanPlus::from_move(position.clone(), &chess_move).to_string();
        let imported_move = ImportedMove {
            ply,
            move_number,
            side,
            san: san.clone(),
            uci: UciMove::from_standard(&chess_move).to_string(),
            position: move_position,
        };

        position.play_unchecked(&chess_move);
        moves.push(imported_move);
    }

    if moves.is_empty() {
        return Err(PgnImportError::NoMoves);
    }

    Ok(ParsedPgn {
        game: Game {
            white: headers.white,
            black: headers.black,
            event: headers.event,
            site: headers.site,
            result: headers.result,
            moves,
            final_position: position_fen(&position),
            is_terminal: position.is_game_over(),
        },
        metadata: headers.metadata,
    })
}

fn normalize_castling(notation: &str) -> String {
    if notation.starts_with("0-0") {
        notation.replace('0', "O")
    } else {
        notation.to_string()
    }
}

enum MoveResolutionError {
    Unsupported,
    Illegal,
}

fn resolve_move(position: &Chess, notation: &str) -> Result<Move, MoveResolutionError> {
    if let Some((candidate, expected_role)) = parse_chessup_long_move(notation) {
        let chess_move = candidate
            .to_move(position)
            .ok()
            .filter(|chess_move| chess_move.role() == expected_role)
            .ok_or(MoveResolutionError::Illegal)?;
        return Ok(chess_move);
    }

    let san_plus =
        SanPlus::from_ascii(notation.as_bytes()).map_err(|_| MoveResolutionError::Unsupported)?;
    san_plus
        .san
        .to_move(position)
        .map_err(|_| MoveResolutionError::Illegal)
}

fn parse_chessup_long_move(notation: &str) -> Option<(UciMove, Role)> {
    let notation = notation
        .strip_suffix('+')
        .or_else(|| notation.strip_suffix('#'))
        .unwrap_or(notation);
    let bytes = notation.as_bytes();
    let (uci, expected_role) = match bytes {
        [piece @ (b'N' | b'B' | b'R' | b'Q' | b'K'), b'a'..=b'h', b'1'..=b'8', b'a'..=b'h', b'1'..=b'8'] => {
            (&bytes[1..], Role::from_char(char::from(*piece))?)
        }
        [b'a'..=b'h', b'1'..=b'8', b'a'..=b'h', b'1'..=b'8'] => (bytes, Role::Pawn),
        [b'a'..=b'h', b'1'..=b'8', b'a'..=b'h', b'1'..=b'8', b'N' | b'B' | b'R' | b'Q' | b'n' | b'b' | b'r' | b'q'] => {
            (bytes, Role::Pawn)
        }
        _ => return None,
    };
    Some((UciMove::from_ascii(uci).ok()?, expected_role))
}

#[cfg(test)]
mod tests {
    use super::{parse_pgn, parse_pgn_with_metadata};
    use crate::domain::MoveSide;

    #[test]
    fn imports_chess_com_game_with_ordered_moves_and_positions() {
        let game = parse_pgn(
            r#"[Event "Live Chess"]
[Site "Chess.com"]
[Date "2026.07.11"]
[Round "-"]
[White "Ada"]
[Black "Grace"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0"#,
        )
        .expect("Chess.com PGN should import");

        assert_eq!(game.white.as_deref(), Some("Ada"));
        assert_eq!(game.black.as_deref(), Some("Grace"));
        assert_eq!(game.site.as_deref(), Some("Chess.com"));
        assert_eq!(game.result.as_deref(), Some("1-0"));
        assert_eq!(game.summary().move_count, 3);
        assert_eq!(game.moves.len(), 6);
        assert_eq!(
            game.moves[0].position,
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
        assert_eq!(game.moves[0].ply, 1);
        assert_eq!(game.moves[0].move_number, 1);
        assert_eq!(game.moves[0].side, MoveSide::White);
        assert_eq!(game.moves[0].san, "e4");
        assert_eq!(game.moves[0].uci, "e2e4");
        assert_ne!(game.moves[0].position, game.moves[1].position);
        assert_eq!(game.moves[5].ply, 6);
        assert_eq!(game.moves[5].move_number, 3);
        assert_eq!(game.moves[5].side, MoveSide::Black);
    }

    #[test]
    fn imports_lichess_game_and_keeps_stable_ply_references() {
        let pgn = r#"[Event "rated rapid game"]
[Site "https://lichess.org/abcdefgh"]
[Date "2026.07.11"]
[Round "-"]
[White "Ada"]
[Black "Grace"]
[Result "1/2-1/2"]
[UTCDate "2026.07.11"]
[UTCTime "08:00:00"]

1. d4 Nf6 2. c4 e6 3. Nc3 Bb4 1/2-1/2"#;

        let first = parse_pgn(pgn).expect("Lichess PGN should import");
        let second = parse_pgn(pgn).expect("same Lichess PGN should import again");

        assert_eq!(first.site.as_deref(), Some("https://lichess.org/abcdefgh"));
        assert_eq!(
            first
                .moves
                .iter()
                .map(|game_move| game_move.ply)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6]
        );
        assert_eq!(first.moves, second.moves);
    }

    #[test]
    fn imports_game_without_optional_metadata() {
        let game = parse_pgn("1. d4 d5 2. c4 *").expect("headerless PGN should import");

        assert_eq!(game.white, None);
        assert_eq!(game.black, None);
        assert_eq!(game.event, None);
        assert_eq!(game.site, None);
        assert_eq!(game.result, None);
        assert_eq!(game.moves.len(), 3);
    }

    #[test]
    fn preserves_played_time_and_time_control_metadata_for_imported_game_cards() {
        let parsed = parse_pgn_with_metadata(
            r#"[UTCDate "2026.08.12"]
[UTCTime "10:00:00"]
[EndDate "2026.08.12"]
[EndTime "10:42:01"]
[TimeControl "600+5"]

1. e4 e5 *"#,
        )
        .unwrap();

        assert_eq!(parsed.metadata.utc_date.as_deref(), Some("2026.08.12"));
        assert_eq!(parsed.metadata.utc_time.as_deref(), Some("10:00:00"));
        assert_eq!(parsed.metadata.end_date.as_deref(), Some("2026.08.12"));
        assert_eq!(parsed.metadata.end_time.as_deref(), Some("10:42:01"));
        assert_eq!(parsed.metadata.time_control.as_deref(), Some("600+5"));
    }

    #[test]
    fn imports_annotated_mainline_and_skips_variations() {
        let game = parse_pgn("1. e4! e5 $1 2. Nf3 (2. Bc4 Nf6) Nc6 {main line} *")
            .expect("annotations and variations should not alter the main line");

        assert_eq!(
            game.moves
                .iter()
                .map(|game_move| game_move.san.as_str())
                .collect::<Vec<_>>(),
            vec!["e4", "e5", "Nf3", "Nc6"]
        );
    }

    #[test]
    fn imports_utf8_bom_prefixed_pgn() {
        let game = parse_pgn("\u{feff}1. e4 e5 *").expect("UTF-8 BOM should not invalidate PGN");

        assert_eq!(game.moves.len(), 2);
    }

    #[test]
    fn imports_zero_based_castling_notation_supported_by_pgn_grammar() {
        let game = parse_pgn("1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6 4. 0-0 *")
            .expect("zero-based castling notation should import");

        assert_eq!(
            game.moves.last().map(|game_move| game_move.san.as_str()),
            Some("O-O")
        );
    }

    #[test]
    fn imports_chessup_long_notation_as_canonical_san_and_uci() {
        let game = parse_pgn(
            r#"[Event "ChessUp OTB"]
[Site "ChessUp Mobile App"]
[Date "2026.07.5"]
[Time "23:16:43"]
[White "oppo-white"]
[Black "oppo-black"]
[Result "1-0"]
[Variant "standard"]
[TimeControl "0+0"]
[Annotator "ChessUp"]

1. Ng1f3 g7g6 2. g2g3 Bf8g7 3. Bf1g2 Ng8f6 4. Nb1c3 Ke8g8
5. Ke1g1 Nb8c6 6. d2d4 d7d6 7. a2a3 Bc8g4 8. h2h3 Bg4f3
9. Bg2f3 Nf6d7 10. Bf3c6 b7c6 11. e2e4 e7e5 12. Nc3e2 Nd7b6
13. b2b3 Qd8d7 14. g3g4 d6d5 15. f2f3 Rf8e8 16. Bc1b2 Qd7d8
17. Ne2g3 Bg7f8 18. Qd1d3 c6c5 19. d4e5 Nb6d7 20. f3f4 c7c6
21. e4d5 Qd8h4 22. Kg1g2 Ra8c8 23. Ra1d1 Nd7b6 24. c2c4 Bf8h6
25. g4g5 Bh6g5 26. f4g5 Qh4g5 27. Rd1e1 h7h5 28. Kg2h2 h5h4
29. Qd3f3 Rc8c7 30. Ng3e4 Re8e5 31. Ne4g5 Re5e1 32. Rf1e1 f7f5
33. Re1e8 1-0"#,
        )
        .expect("ChessUp long notation should import without preprocessing");

        assert_eq!(game.white.as_deref(), Some("oppo-white"));
        assert_eq!(game.black.as_deref(), Some("oppo-black"));
        assert_eq!(game.result.as_deref(), Some("1-0"));
        assert_eq!(game.moves.len(), 65);
        assert_eq!(game.moves[0].san, "Nf3");
        assert_eq!(game.moves[0].uci, "g1f3");
        assert_eq!(game.moves[1].san, "g6");
        assert_eq!(game.moves[1].uci, "g7g6");
        assert_eq!(game.moves[7].san, "O-O");
        assert_eq!(game.moves[8].san, "O-O");
        assert_eq!(game.moves[18].san, "Bxc6");
        assert_eq!(game.moves[19].san, "bxc6");
        assert_eq!(game.moves[64].san, "Re8#");
        assert_eq!(game.moves[64].uci, "e1e8");
        assert!(game.is_terminal);
    }

    #[test]
    fn imports_chessup_long_notation_promotion() {
        let game = parse_pgn(
            r#"[FEN "k7/4P3/8/8/8/8/8/7K w - - 0 1"]

1. e7e8Q *"#,
        )
        .expect("ChessUp promotion notation should import");

        assert_eq!(game.moves[0].san, "e8=Q+");
        assert_eq!(game.moves[0].uci, "e7e8q");
    }

    #[test]
    fn rejects_chessup_long_notation_with_the_wrong_piece_prefix() {
        let error = parse_pgn("1. Bg1f3 *")
            .expect_err("the prefix must identify the piece on the source square")
            .to_string();

        assert!(error.contains("illegal move Bg1f3 at ply 1"));
    }

    #[test]
    fn rejects_multiple_games_in_one_import() {
        let result = parse_pgn("1. e4 *\n\n1. d4 *");

        let error = result
            .expect_err("one import should contain exactly one game")
            .to_string();
        assert!(
            error.contains("multiple games"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_non_pgn_text_with_a_clear_error() {
        let result = parse_pgn("this is not PGN");

        let error = result.expect_err("non-PGN text should fail").to_string();
        assert!(error.contains("invalid"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_malformed_movetext_after_legal_moves() {
        let result = parse_pgn("1. e4 e5 2. definitely-not-a-move *");

        let error = result
            .expect_err("malformed movetext should fail")
            .to_string();
        assert!(error.contains("invalid"), "unexpected error: {error}");
    }
}
