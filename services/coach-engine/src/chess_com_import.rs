use std::{collections::BTreeMap, sync::Arc};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use shakmaty::{fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, Position};

use crate::{
    chess_com::{
        game_request, ChessComGameClient, ChessComGameFetchError, ChessComGameKind,
        ChessComGameResponse, ChessComGameUrl, CHESS_COM_JSON_MEDIA_TYPE,
        CHESS_COM_MAX_RESPONSE_BYTES,
    },
    game_eligibility::{completed_standard_outcome, GameEligibilityError},
    pgn::{parse_pgn_with_metadata, ParsedPgn},
    review_session_contract::{
        ArtifactDigest, CompletedGameOutcome, ImportProgressStage, ReviewSessionLimits,
    },
};

const MOVE_ALPHABET: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!?{~}(^)[_]@#$,./&-*++=";
const PROMOTION_ROLES: &str = "qnrbkp";
const DEFAULT_RETRY_AFTER_SECONDS: u32 = 60;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChessComImportError {
    #[error("Chess.com Game was not found")]
    GameNotFound,
    #[error("Chess.com Game is private")]
    PrivateGame,
    #[error("Chess.com Game is ongoing")]
    OngoingGame,
    #[error("Chess.com Game was aborted")]
    AbortedGame,
    #[error("Chess.com Game uses an unsupported variant")]
    UnsupportedVariant,
    #[error("Chess.com Game contains invalid PGN")]
    InvalidPgn,
    #[error("Chess.com returned a malformed Game response")]
    MalformedResponse,
    #[error("Chess.com Game response exceeded an import size limit")]
    ResponseTooLarge,
    #[error("Chess.com transport is unavailable")]
    Transport,
    #[error("Chess.com Game request timed out")]
    Timeout,
    #[error("Chess.com is rate limited until {retry_at}")]
    RateLimited {
        retry_after_seconds: u32,
        retry_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedChessComGame {
    pub(crate) parsed: ParsedPgn,
    pub(crate) pgn: Vec<u8>,
    pub(crate) outcome: CompletedGameOutcome,
    pub(crate) captured_at: DateTime<Utc>,
    pub(crate) response_digest: ArtifactDigest,
    pub(crate) pgn_digest: ArtifactDigest,
}

pub(crate) struct ChessComImportGateway<C> {
    client: C,
}

impl<C> ChessComImportGateway<C> {
    pub(crate) fn new(client: C) -> Self {
        Self { client }
    }
}

impl<C: ChessComGameClient> ChessComImportGateway<C> {
    pub(crate) async fn import<F>(
        &self,
        source: &ChessComGameUrl,
        progress: &F,
    ) -> Result<Arc<PreparedChessComGame>, ChessComImportError>
    where
        F: Fn(ImportProgressStage),
    {
        progress(ImportProgressStage::WaitingForChessCom);
        progress(ImportProgressStage::FetchingGame);
        let response = self
            .client
            .fetch(&game_request(source))
            .await
            .map_err(map_fetch_error)?;
        progress(ImportProgressStage::ValidatingGame);
        prepare_chess_com_game(source, response).map(Arc::new)
    }
}

pub(crate) fn prepare_chess_com_game(
    source: &ChessComGameUrl,
    response: ChessComGameResponse,
) -> Result<PreparedChessComGame, ChessComImportError> {
    if response.body.len() > CHESS_COM_MAX_RESPONSE_BYTES {
        return Err(ChessComImportError::ResponseTooLarge);
    }
    if response
        .content_type
        .split(';')
        .next()
        .is_none_or(|value| value.trim() != CHESS_COM_JSON_MEDIA_TYPE)
    {
        return Err(ChessComImportError::MalformedResponse);
    }
    let response_digest = artifact_digest(&response.body)?;
    let wire: ChessComGameEnvelope = serde_json::from_slice(&response.body)
        .map_err(|_| ChessComImportError::MalformedResponse)?;
    if wire.game.id.to_string() != source.canonical_game_id()
        || !source_kind_matches(source.kind(), &wire)
    {
        return Err(ChessComImportError::MalformedResponse);
    }
    if wire.game.game_type != "chess" {
        return Err(ChessComImportError::UnsupportedVariant);
    }
    if wire
        .game
        .game_end_reason
        .as_deref()
        .is_some_and(|reason| reason.eq_ignore_ascii_case("aborted"))
    {
        return Err(ChessComImportError::AbortedGame);
    }
    if !wire.game.is_finished {
        return Err(ChessComImportError::OngoingGame);
    }

    prepare_chess_com_pgn(
        build_pgn(&wire.game)?,
        response.captured_at,
        response_digest,
    )
}

fn prepare_chess_com_pgn(
    pgn: String,
    captured_at: DateTime<Utc>,
    response_digest: ArtifactDigest,
) -> Result<PreparedChessComGame, ChessComImportError> {
    if pgn.len() > usize::try_from(ReviewSessionLimits::V1.max_pgn_bytes).unwrap() {
        return Err(ChessComImportError::ResponseTooLarge);
    }
    let parsed = parse_pgn_with_metadata(&pgn).map_err(|_| ChessComImportError::InvalidPgn)?;
    let outcome = completed_standard_outcome(&parsed).map_err(map_eligibility_error)?;
    let pgn = pgn.into_bytes();
    let pgn_digest = artifact_digest(&pgn)?;
    Ok(PreparedChessComGame {
        parsed,
        pgn,
        outcome,
        captured_at,
        response_digest,
        pgn_digest,
    })
}

pub(crate) fn prepare_chess_com_archive_game(
    pgn: String,
    captured_at: DateTime<Utc>,
    response_digest: ArtifactDigest,
) -> Result<PreparedChessComGame, ChessComImportError> {
    prepare_chess_com_pgn(pgn, captured_at, response_digest)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChessComGameEnvelope {
    game: ChessComGame,
    players: ChessComPlayers,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChessComGame {
    id: u64,
    #[serde(default)]
    initial_setup: String,
    is_finished: bool,
    #[serde(default)]
    is_vs_computer: Option<bool>,
    #[serde(default)]
    is_live_game: Option<bool>,
    #[serde(default)]
    days_per_turn: Option<u32>,
    #[serde(default)]
    game_end_reason: Option<String>,
    move_list: String,
    pgn_headers: BTreeMap<String, Value>,
    ply_count: usize,
    #[serde(rename = "type")]
    game_type: String,
}

#[derive(Debug, Deserialize)]
struct ChessComPlayers {
    top: ChessComPlayer,
    bottom: ChessComPlayer,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChessComPlayer {
    is_computer: bool,
}

fn source_kind_matches(kind: ChessComGameKind, wire: &ChessComGameEnvelope) -> bool {
    let computer_players =
        usize::from(wire.players.top.is_computer) + usize::from(wire.players.bottom.is_computer);
    match kind {
        ChessComGameKind::Computer => {
            wire.game.is_vs_computer == Some(true) && computer_players == 1
        }
        ChessComGameKind::Daily => {
            wire.game.days_per_turn.is_some()
                && wire.game.is_vs_computer != Some(true)
                && computer_players == 0
        }
        ChessComGameKind::Live => {
            wire.game.is_live_game == Some(true)
                && wire.game.is_vs_computer != Some(true)
                && computer_players == 0
        }
    }
}

fn build_pgn(game: &ChessComGame) -> Result<String, ChessComImportError> {
    let result = required_header(&game.pgn_headers, "Result")?;
    if !matches!(result.as_str(), "1-0" | "0-1" | "1/2-1/2") {
        return if result == "*" {
            Err(ChessComImportError::OngoingGame)
        } else {
            Err(ChessComImportError::MalformedResponse)
        };
    }
    required_header(&game.pgn_headers, "White")?;
    required_header(&game.pgn_headers, "Black")?;

    let starting_fen = optional_header(&game.pgn_headers, "FEN")
        .filter(|value| !value.is_empty())
        .or_else(|| (!game.initial_setup.is_empty()).then(|| game.initial_setup.clone()));
    let mut position = match &starting_fen {
        Some(fen) => Fen::from_ascii(fen.as_bytes())
            .map_err(|_| ChessComImportError::InvalidPgn)?
            .into_position(CastlingMode::Standard)
            .map_err(|_| ChessComImportError::InvalidPgn)?,
        None => Chess::default(),
    };
    let decoded = decode_move_list(&game.move_list)?;
    if decoded.len() != game.ply_count || decoded.is_empty() {
        return Err(ChessComImportError::MalformedResponse);
    }

    let mut movetext = Vec::with_capacity(decoded.len() * 2 + 1);
    for (index, decoded_move) in decoded.into_iter().enumerate() {
        if position.turn().is_white() {
            movetext.push(format!("{}.", position.fullmoves()));
        } else if index == 0 {
            movetext.push(format!("{}...", position.fullmoves()));
        }
        let uci = decoded_move.uci();
        let chess_move = UciMove::from_ascii(uci.as_bytes())
            .map_err(|_| ChessComImportError::InvalidPgn)?
            .to_move(&position)
            .map_err(|_| ChessComImportError::InvalidPgn)?;
        movetext.push(SanPlus::from_move(position.clone(), &chess_move).to_string());
        position.play_unchecked(&chess_move);
    }
    movetext.push(result);

    let mut headers = Vec::new();
    for key in [
        "Event",
        "Site",
        "Date",
        "Round",
        "White",
        "Black",
        "Result",
        "ECO",
        "Opening",
        "Link",
        "WhiteElo",
        "BlackElo",
        "TimeControl",
        "EndDate",
        "EndTime",
        "Termination",
        "SetUp",
        "FEN",
    ] {
        let value = match key {
            "Round" => optional_header(&game.pgn_headers, key).or_else(|| Some("-".to_string())),
            "FEN" => optional_header(&game.pgn_headers, key).or_else(|| starting_fen.clone()),
            _ => optional_header(&game.pgn_headers, key),
        };
        if let Some(value) = value {
            headers.push(format!("[{key} \"{}\"]", escape_header(&value)));
        }
    }
    Ok(format!("{}\n\n{}", headers.join("\n"), movetext.join(" ")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedMove {
    from: String,
    to: String,
    promotion: Option<char>,
}

impl DecodedMove {
    fn uci(&self) -> String {
        let mut uci = format!("{}{}", self.from, self.to);
        if let Some(promotion) = self.promotion {
            uci.push(promotion);
        }
        uci
    }
}

fn decode_move_list(encoded: &str) -> Result<Vec<DecodedMove>, ChessComImportError> {
    if !encoded.len().is_multiple_of(2) || !encoded.is_ascii() {
        return Err(ChessComImportError::MalformedResponse);
    }
    let alphabet = MOVE_ALPHABET.as_bytes();
    let promotions = PROMOTION_ROLES.as_bytes();
    let mut moves = Vec::with_capacity(encoded.len() / 2);
    for pair in encoded.as_bytes().chunks_exact(2) {
        let from_index = alphabet
            .iter()
            .position(|candidate| candidate == &pair[0])
            .ok_or(ChessComImportError::MalformedResponse)?;
        let encoded_to_index = alphabet
            .iter()
            .position(|candidate| candidate == &pair[1])
            .ok_or(ChessComImportError::MalformedResponse)?;
        if from_index > 63 {
            return Err(ChessComImportError::UnsupportedVariant);
        }
        let (to_index, promotion) = if encoded_to_index > 63 {
            let promotion_index = (encoded_to_index - 64) / 3;
            let promotion = promotions
                .get(promotion_index)
                .copied()
                .map(char::from)
                .ok_or(ChessComImportError::MalformedResponse)?;
            let rank_step = if from_index < 16 { -8 } else { 8 };
            let file_step = i32::try_from((encoded_to_index - 1) % 3)
                .map_err(|_| ChessComImportError::MalformedResponse)?
                - 1;
            let destination = i32::try_from(from_index)
                .map_err(|_| ChessComImportError::MalformedResponse)?
                + rank_step
                + file_step;
            (
                usize::try_from(destination)
                    .ok()
                    .filter(|index| *index < 64)
                    .ok_or(ChessComImportError::MalformedResponse)?,
                Some(promotion),
            )
        } else {
            (encoded_to_index, None)
        };
        moves.push(DecodedMove {
            from: square(from_index),
            to: square(to_index),
            promotion,
        });
    }
    Ok(moves)
}

fn square(index: usize) -> String {
    let file = char::from(b'a' + u8::try_from(index % 8).unwrap());
    let rank = char::from(b'1' + u8::try_from(index / 8).unwrap());
    format!("{file}{rank}")
}

fn required_header(
    headers: &BTreeMap<String, Value>,
    key: &str,
) -> Result<String, ChessComImportError> {
    optional_header(headers, key)
        .filter(|value| !value.is_empty())
        .ok_or(ChessComImportError::MalformedResponse)
}

fn optional_header(headers: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match headers.get(key)? {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn escape_header(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn artifact_digest(bytes: &[u8]) -> Result<ArtifactDigest, ChessComImportError> {
    ArtifactDigest::try_from(format!("sha256:{:x}", Sha256::digest(bytes)))
        .map_err(|_| ChessComImportError::MalformedResponse)
}

fn map_fetch_error(error: ChessComGameFetchError) -> ChessComImportError {
    match error {
        ChessComGameFetchError::Status { code: 404, .. } => ChessComImportError::GameNotFound,
        ChessComGameFetchError::Status {
            code: 401 | 403, ..
        } => ChessComImportError::PrivateGame,
        ChessComGameFetchError::Status {
            code: 408 | 504, ..
        }
        | ChessComGameFetchError::Timeout => ChessComImportError::Timeout,
        ChessComGameFetchError::Status {
            code: 429,
            retry_after_seconds,
        } => {
            let retry_after_seconds = retry_after_seconds.unwrap_or(DEFAULT_RETRY_AFTER_SECONDS);
            ChessComImportError::RateLimited {
                retry_after_seconds,
                retry_at: Utc::now() + chrono::Duration::seconds(i64::from(retry_after_seconds)),
            }
        }
        ChessComGameFetchError::ResponseTooLarge { .. } => ChessComImportError::ResponseTooLarge,
        ChessComGameFetchError::Client(_)
        | ChessComGameFetchError::Connection
        | ChessComGameFetchError::Transport(_)
        | ChessComGameFetchError::Status { .. } => ChessComImportError::Transport,
    }
}

fn map_eligibility_error(error: GameEligibilityError) -> ChessComImportError {
    match error {
        GameEligibilityError::UnsupportedVariant => ChessComImportError::UnsupportedVariant,
        GameEligibilityError::Ongoing => ChessComImportError::OngoingGame,
        GameEligibilityError::Aborted => ChessComImportError::AbortedGame,
        GameEligibilityError::Invalid(_) => ChessComImportError::InvalidPgn,
    }
}

#[cfg(test)]
mod tests {
    use super::decode_move_list;

    #[test]
    fn decodes_chess_com_square_and_promotion_encoding() {
        let decoded = decode_move_list("gvZJk~").unwrap();
        assert_eq!(decoded[0].uci(), "g1f3");
        assert_eq!(decoded[1].uci(), "d7d5");
        assert_eq!(decoded[2].uci(), "c2c1q");
    }
}
