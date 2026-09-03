use std::{future::Future, pin::Pin, sync::OnceLock, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::{
    header::{ACCEPT, CONTENT_TYPE, RETRY_AFTER},
    redirect::Policy,
};

use crate::provider_user_agent::{provider_user_agent, GAME_IMPORT_PATH};
use crate::retry_after::retry_after_seconds_u32;

pub use coach_engine_contract::chess_com::{
    chess_com_game_id_is_canonical, chess_com_game_url_pattern, parse_chess_com_game_identity,
    ChessComGameKind, ChessComGameUrl, ChessComUrlError,
};

pub const CHESS_COM_JSON_MEDIA_TYPE: &str = "application/json";
pub const CHESS_COM_COMPUTER_GAME_CONTRACT_VERSION: &str = "chess-com-computer-game-callback/v1";
pub const CHESS_COM_DAILY_GAME_CONTRACT_VERSION: &str = "chess-com-daily-game-callback/v1";
pub const CHESS_COM_LIVE_GAME_CONTRACT_VERSION: &str = "chess-com-live-game-callback/v1";
pub const CHESS_COM_PUBAPI_ARCHIVE_CONTRACT_VERSION: &str = "chess-com-pubapi-archive/v1";
pub const CHESS_COM_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const CHESS_COM_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
pub const CHESS_COM_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

static HTTP_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

/// Which versioned Chess.com callback contract a Game of this kind is read
/// under. Transport, so it stays here rather than with the URL grammar.
pub fn fetch_contract_version(source: &ChessComGameUrl) -> &'static str {
    match source.kind() {
        ChessComGameKind::Computer => CHESS_COM_COMPUTER_GAME_CONTRACT_VERSION,
        ChessComGameKind::Daily => CHESS_COM_DAILY_GAME_CONTRACT_VERSION,
        ChessComGameKind::Live => CHESS_COM_LIVE_GAME_CONTRACT_VERSION,
    }
}

pub fn game_request(source: &ChessComGameUrl) -> ChessComGameRequest {
    let path = match source.kind() {
        ChessComGameKind::Computer => "computer/callback/game",
        ChessComGameKind::Daily => "callback/daily/game",
        ChessComGameKind::Live => "callback/live/game",
    };
    ChessComGameRequest {
        url: format!(
            "https://www.chess.com/{path}/{}",
            source.canonical_game_id()
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChessComGameRequest {
    url: String,
}

impl ChessComGameRequest {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn accept(&self) -> &'static str {
        CHESS_COM_JSON_MEDIA_TYPE
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChessComGameResponse {
    pub body: Vec<u8>,
    pub content_type: String,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChessComGameFetchError {
    #[error("could not construct the anonymous Chess.com client: {0}")]
    Client(String),
    #[error("could not connect to Chess.com")]
    Connection,
    #[error("Chess.com Game request timed out")]
    Timeout,
    #[error("Chess.com Game request failed: {0}")]
    Transport(String),
    #[error("Chess.com Game request returned HTTP {code}")]
    Status {
        code: u16,
        retry_after_seconds: Option<u32>,
    },
    #[error("Chess.com Game response exceeded the {limit_bytes}-byte response limit")]
    ResponseTooLarge { limit_bytes: usize },
}

pub trait ChessComGameClient: Send + Sync {
    fn fetch<'a>(
        &'a self,
        request: &'a ChessComGameRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<ChessComGameResponse, ChessComGameFetchError>> + Send + 'a>,
    >;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReqwestChessComGameClient;

impl ChessComGameClient for ReqwestChessComGameClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ChessComGameRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<ChessComGameResponse, ChessComGameFetchError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let client = HTTP_CLIENT
                .get_or_init(|| {
                    reqwest::Client::builder()
                        .redirect(Policy::none())
                        .connect_timeout(CHESS_COM_CONNECT_TIMEOUT)
                        .timeout(CHESS_COM_RESPONSE_TIMEOUT)
                        .user_agent(provider_user_agent(GAME_IMPORT_PATH))
                        .build()
                        .map_err(|error| error.to_string())
                })
                .as_ref()
                .map_err(|error| ChessComGameFetchError::Client(error.clone()))?;
            let mut response = client
                .get(request.url())
                .header(ACCEPT, request.accept())
                .send()
                .await
                .map_err(classify_reqwest_error)?;
            if !response.status().is_success() {
                return Err(ChessComGameFetchError::Status {
                    code: response.status().as_u16(),
                    retry_after_seconds: response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| retry_after_seconds_u32(value, Utc::now())),
                });
            }
            if response
                .content_length()
                .is_some_and(|length| length > CHESS_COM_MAX_RESPONSE_BYTES as u64)
            {
                return Err(ChessComGameFetchError::ResponseTooLarge {
                    limit_bytes: CHESS_COM_MAX_RESPONSE_BYTES,
                });
            }
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string();
            let mut body = Vec::with_capacity(
                response
                    .content_length()
                    .unwrap_or_default()
                    .min(CHESS_COM_MAX_RESPONSE_BYTES as u64) as usize,
            );
            while let Some(chunk) = response.chunk().await.map_err(classify_reqwest_error)? {
                if chunk.len() > CHESS_COM_MAX_RESPONSE_BYTES - body.len() {
                    return Err(ChessComGameFetchError::ResponseTooLarge {
                        limit_bytes: CHESS_COM_MAX_RESPONSE_BYTES,
                    });
                }
                body.extend_from_slice(&chunk);
            }
            Ok(ChessComGameResponse {
                body,
                content_type,
                captured_at: Utc::now(),
            })
        })
    }
}

fn classify_reqwest_error(error: reqwest::Error) -> ChessComGameFetchError {
    if error.is_timeout() {
        ChessComGameFetchError::Timeout
    } else if error.is_connect() {
        ChessComGameFetchError::Connection
    } else {
        ChessComGameFetchError::Transport(error.to_string())
    }
}
