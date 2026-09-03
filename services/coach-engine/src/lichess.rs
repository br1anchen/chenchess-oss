use std::{future::Future, pin::Pin, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::{
    header::{ACCEPT, CONTENT_TYPE, RETRY_AFTER},
    redirect::Policy,
};

use crate::provider_user_agent::{provider_user_agent, GAME_IMPORT_PATH};
use crate::retry_after::retry_after_seconds_u32;

pub const LICHESS_PGN_MEDIA_TYPE: &str = "application/x-chess-pgn";
pub const LICHESS_EXPORT_CONTRACT_VERSION: &str = "lichess-single-game-export/v1";
pub const LICHESS_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const LICHESS_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
pub const LICHESS_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

pub use coach_engine_contract::lichess::{LichessGameUrl, LichessSide, LichessUrlError};

pub fn export_request(source: &LichessGameUrl) -> LichessExportRequest {
    LichessExportRequest {
        url: format!(
            "https://lichess.org/game/export/{}?clocks=false&evals=false&accuracy=false&literate=false&opening=true",
            source.canonical_game_id()
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LichessExportRequest {
    url: String,
}

impl LichessExportRequest {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn accept(&self) -> &'static str {
        LICHESS_PGN_MEDIA_TYPE
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LichessExportResponse {
    pub body: Vec<u8>,
    pub content_type: String,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LichessExportError {
    #[error("could not construct the anonymous Lichess client: {0}")]
    Client(String),
    #[error("could not connect to Lichess")]
    Connection,
    #[error("Lichess export timed out")]
    Timeout,
    #[error("Lichess export request failed: {0}")]
    Transport(String),
    #[error("Lichess export returned HTTP {code}")]
    Status {
        code: u16,
        retry_after_seconds: Option<u32>,
    },
    #[error("Lichess export exceeded the {limit_bytes}-byte response limit")]
    ResponseTooLarge { limit_bytes: usize },
}

pub trait LichessExportClient: Send + Sync {
    fn export<'a>(
        &'a self,
        request: &'a LichessExportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LichessExportResponse, LichessExportError>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct ReqwestLichessExportClient {
    client: reqwest::Client,
}

impl ReqwestLichessExportClient {
    pub fn new() -> Result<Self, LichessExportError> {
        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .connect_timeout(LICHESS_CONNECT_TIMEOUT)
            .timeout(LICHESS_RESPONSE_TIMEOUT)
            .user_agent(provider_user_agent(GAME_IMPORT_PATH))
            .build()
            .map_err(|error| LichessExportError::Client(error.to_string()))?;
        Ok(Self { client })
    }
}

impl LichessExportClient for ReqwestLichessExportClient {
    fn export<'a>(
        &'a self,
        request: &'a LichessExportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LichessExportResponse, LichessExportError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut response = self
                .client
                .get(request.url())
                .header(ACCEPT, request.accept())
                .send()
                .await
                .map_err(classify_reqwest_error)?;
            if !response.status().is_success() {
                return Err(LichessExportError::Status {
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
                .is_some_and(|length| length > LICHESS_MAX_RESPONSE_BYTES as u64)
            {
                return Err(LichessExportError::ResponseTooLarge {
                    limit_bytes: LICHESS_MAX_RESPONSE_BYTES,
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
                    .min(LICHESS_MAX_RESPONSE_BYTES as u64) as usize,
            );
            while let Some(chunk) = response.chunk().await.map_err(classify_reqwest_error)? {
                if chunk.len() > LICHESS_MAX_RESPONSE_BYTES - body.len() {
                    return Err(LichessExportError::ResponseTooLarge {
                        limit_bytes: LICHESS_MAX_RESPONSE_BYTES,
                    });
                }
                body.extend_from_slice(&chunk);
            }
            let captured_at = Utc::now();
            Ok(LichessExportResponse {
                body,
                content_type,
                captured_at,
            })
        })
    }
}

fn classify_reqwest_error(error: reqwest::Error) -> LichessExportError {
    if error.is_timeout() {
        LichessExportError::Timeout
    } else if error.is_connect() {
        LichessExportError::Connection
    } else {
        LichessExportError::Transport(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::JoinHandle,
    };

    use super::*;

    #[tokio::test]
    async fn anonymous_client_refuses_redirects() {
        let (url, server) = serve_once(
            b"HTTP/1.1 302 Found\r\nLocation: https://example.com/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        )
        .await;
        let request = LichessExportRequest { url };

        let error = ReqwestLichessExportClient::new()
            .unwrap()
            .export(&request)
            .await
            .expect_err("redirects must remain visible as non-success statuses");

        assert!(matches!(
            error,
            LichessExportError::Status { code: 302, .. }
        ));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn content_length_over_the_response_cap_is_rejected_before_body_read() {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {LICHESS_PGN_MEDIA_TYPE}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            LICHESS_MAX_RESPONSE_BYTES + 1
        )
        .into_bytes();
        let (url, server) = serve_once(response).await;
        let request = LichessExportRequest { url };

        let error = ReqwestLichessExportClient::new()
            .unwrap()
            .export(&request)
            .await
            .expect_err("oversized Content-Length must be rejected");

        assert_eq!(
            error,
            LichessExportError::ResponseTooLarge {
                limit_bytes: LICHESS_MAX_RESPONSE_BYTES
            }
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn streamed_body_over_the_response_cap_is_rejected() {
        let payload = vec![b'x'; LICHESS_MAX_RESPONSE_BYTES + 1];
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {LICHESS_PGN_MEDIA_TYPE}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n",
            payload.len()
        )
        .into_bytes();
        response.extend_from_slice(&payload);
        response.extend_from_slice(b"\r\n0\r\n\r\n");
        let (url, server) = serve_once(response).await;
        let request = LichessExportRequest { url };

        let error = ReqwestLichessExportClient::new()
            .unwrap()
            .export(&request)
            .await
            .expect_err("streaming must not bypass the response cap");

        assert_eq!(
            error,
            LichessExportError::ResponseTooLarge {
                limit_bytes: LICHESS_MAX_RESPONSE_BYTES
            }
        );
        server.await.unwrap();
    }

    async fn serve_once(response: Vec<u8>) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).await.unwrap();
            stream.write_all(&response).await.unwrap();
            stream.shutdown().await.unwrap();
        });
        (format!("http://{address}/export"), server)
    }
}
