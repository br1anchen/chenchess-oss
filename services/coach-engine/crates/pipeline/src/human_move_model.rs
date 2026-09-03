use std::{
    future::Future,
    pin::Pin,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use shakmaty::uci::UciMove;

use crate::domain::{EloProfile, HumanMoveCandidate};
use crate::operating_limits::PROVIDER_POSITION_TIMEOUT_SECONDS;

mod cache;

pub use cache::ExactHumanMoveCache;

#[derive(Debug, Clone, Copy)]
pub struct HumanMoveInput<'a> {
    pub position: &'a str,
    pub elo: EloProfile,
    pub limit: u8,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanMovePrediction {
    pub candidates: Vec<HumanMoveCandidate>,
    pub win_probability: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum HumanMoveModelError {
    #[error("Human Move Model input is invalid: {0}")]
    InvalidInput(String),
    #[error("Human Move Model request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Human Move Model service returned HTTP {status}")]
    Service { status: u16 },
    #[error("Human Move Model returned an invalid response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanMoveCacheIdentity {
    provider: String,
    package: String,
    model: String,
    image: String,
    model_digest: String,
    config_digest: String,
}

impl HumanMoveCacheIdentity {
    pub fn is_pinned_maia(&self) -> bool {
        use crate::evaluation_recording::{
            PINNED_MAIA_CONFIG_DIGEST, PINNED_MAIA_IMAGE, PINNED_MAIA_MODEL,
            PINNED_MAIA_MODEL_DIGEST, PINNED_MAIA_PACKAGE,
        };

        self.package == PINNED_MAIA_PACKAGE
            && self.model == PINNED_MAIA_MODEL
            && self.image == PINNED_MAIA_IMAGE
            && self.model_digest == PINNED_MAIA_MODEL_DIGEST
            && self.config_digest == PINNED_MAIA_CONFIG_DIGEST
    }
}

pub trait HumanMoveModel: Send + Sync + 'static {
    fn provider_name(&self) -> &'static str {
        "Human Move Model adapter"
    }

    fn predict<'a>(
        &'a self,
        input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>;

    fn cache_identity(&self) -> Option<HumanMoveCacheIdentity> {
        None
    }
}

#[derive(Clone)]
pub struct MaiaHttpAdapter {
    client: reqwest::Client,
    base_url: String,
}

impl MaiaHttpAdapter {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn from_env() -> Option<Self> {
        std::env::var("MAIA_BASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(Self::new)
    }
}

impl HumanMoveModel for MaiaHttpAdapter {
    fn provider_name(&self) -> &'static str {
        "Maia"
    }

    fn predict<'a>(
        &'a self,
        input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        Box::pin(async move {
            if input.position.trim().is_empty() {
                return Err(HumanMoveModelError::InvalidInput(
                    "position must be a non-empty FEN string".to_string(),
                ));
            }
            if !(1..=20).contains(&input.limit) {
                return Err(HumanMoveModelError::InvalidInput(
                    "limit must be between 1 and 20".to_string(),
                ));
            }

            let request_payload = MaiaRequest {
                position: input.position,
                player_elo: input.elo.rating(),
                opponent_elo: input.elo.rating(),
                limit: input.limit,
            };
            let request_bytes = serde_json::to_vec(&request_payload)
                .expect("the Maia request is serializable")
                .len();
            let mut telemetry = HumanMoveTelemetry::new(self.provider_name(), request_bytes);
            let response = self
                .client
                .post(format!("{}/v1/predict", self.base_url))
                .json(&request_payload)
                .timeout(Duration::from_secs(PROVIDER_POSITION_TIMEOUT_SECONDS))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                return Err(HumanMoveModelError::Service {
                    status: status.as_u16(),
                });
            }

            let response_body = response.bytes().await?;
            telemetry.observe_response(response_body.len());
            let payload = serde_json::from_slice::<MaiaResponse>(&response_body).map_err(|_| {
                HumanMoveModelError::InvalidResponse(
                    "response body was not valid Maia JSON".to_string(),
                )
            })?;
            let mut moves = payload.moves;
            if moves.is_empty() {
                return Err(HumanMoveModelError::InvalidResponse(
                    "response has no move candidates".to_string(),
                ));
            }
            for candidate in &moves {
                if !matches!(candidate.uci.parse(), Ok(UciMove::Normal { .. }))
                    || !candidate.probability.is_finite()
                    || !(0.0..=1.0).contains(&candidate.probability)
                {
                    return Err(HumanMoveModelError::InvalidResponse(format!(
                        "invalid probability or UCI move for {}",
                        candidate.uci
                    )));
                }
            }
            moves.sort_by(|left, right| right.probability.total_cmp(&left.probability));
            let candidates = moves
                .into_iter()
                .take(usize::from(input.limit))
                .enumerate()
                .map(|(index, candidate)| HumanMoveCandidate {
                    uci: candidate.uci,
                    probability: candidate.probability,
                    rank: index + 1,
                })
                .collect();

            if payload.win_probability.is_some_and(|probability| {
                !probability.is_finite() || !(0.0..=1.0).contains(&probability)
            }) {
                return Err(HumanMoveModelError::InvalidResponse(
                    "win probability must be between zero and one".to_string(),
                ));
            }

            let prediction = HumanMovePrediction {
                candidates,
                win_probability: payload.win_probability,
            };
            telemetry.succeed(prediction.candidates.len());
            Ok(prediction)
        })
    }

    fn cache_identity(&self) -> Option<HumanMoveCacheIdentity> {
        use crate::evaluation_recording::{
            PINNED_MAIA_CONFIG_DIGEST, PINNED_MAIA_IMAGE, PINNED_MAIA_MODEL,
            PINNED_MAIA_MODEL_DIGEST, PINNED_MAIA_PACKAGE,
        };

        Some(HumanMoveCacheIdentity {
            provider: self.provider_name().to_owned(),
            package: PINNED_MAIA_PACKAGE.to_owned(),
            model: PINNED_MAIA_MODEL.to_owned(),
            image: PINNED_MAIA_IMAGE.to_owned(),
            model_digest: PINNED_MAIA_MODEL_DIGEST.to_owned(),
            config_digest: PINNED_MAIA_CONFIG_DIGEST.to_owned(),
        })
    }
}

struct HumanMoveTelemetry {
    candidate_count: usize,
    provider: &'static str,
    request_bytes: usize,
    response_bytes: usize,
    response_bytes_known: bool,
    started_at: Instant,
    status: &'static str,
}

impl HumanMoveTelemetry {
    fn new(provider: &'static str, request_bytes: usize) -> Self {
        Self {
            candidate_count: 0,
            provider,
            request_bytes,
            response_bytes: 0,
            response_bytes_known: false,
            started_at: Instant::now(),
            status: "failed",
        }
    }

    fn observe_response(&mut self, response_bytes: usize) {
        self.response_bytes = response_bytes;
        self.response_bytes_known = true;
    }

    fn succeed(&mut self, candidate_count: usize) {
        self.candidate_count = candidate_count;
        self.status = "succeeded";
    }
}

impl Drop for HumanMoveTelemetry {
    fn drop(&mut self) {
        tracing::info!(
            event = "coach_human_move_model_completion",
            provider = self.provider,
            candidate_count = self.candidate_count,
            request_bytes = self.request_bytes,
            response_bytes = self.response_bytes,
            response_bytes_known = self.response_bytes_known,
            status = self.status,
            wall_milliseconds = self.started_at.elapsed().as_millis(),
            "human move model request metrics"
        );
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MaiaRequest<'a> {
    position: &'a str,
    player_elo: u16,
    opponent_elo: u16,
    limit: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaiaResponse {
    moves: Vec<MaiaMove>,
    win_probability: Option<f64>,
}

#[derive(Deserialize)]
struct MaiaMove {
    uci: String,
    probability: f64,
}

#[cfg(test)]
mod tests {
    use axum::{http::StatusCode, routing::post, Json, Router};
    use serde_json::{json, Value};
    use tokio::{net::TcpListener, sync::mpsc};

    use super::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, MaiaHttpAdapter};
    use crate::domain::EloProfile;

    #[tokio::test]
    async fn maia_adapter_maps_provider_neutral_input_and_ranked_candidates() {
        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        let app = Router::new().route(
            "/v1/predict",
            post(move |Json(body): Json<Value>| {
                let request_tx = request_tx.clone();
                async move {
                    request_tx
                        .send(body)
                        .expect("test receiver should remain open");
                    Json(json!({
                        "moves": [
                            { "uci": "e2e4", "probability": 0.46 },
                            { "uci": "d2d4", "probability": 0.31 }
                        ],
                        "winProbability": 0.53
                    }))
                }
            }),
        );
        let (base_url, server) = spawn_service(app).await;
        let adapter = MaiaHttpAdapter::new(base_url);

        let prediction = adapter
            .predict(HumanMoveInput {
                position: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                elo: EloProfile::try_from(1450).expect("test Elo should be valid"),
                limit: 5,
            })
            .await
            .expect("fake Maia service should return a prediction");

        let request = request_rx.recv().await.expect("request should be captured");
        assert_eq!(
            request["position"],
            json!("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
        );
        assert_eq!(request["playerElo"], 1450);
        assert_eq!(request["opponentElo"], 1450);
        assert_eq!(request["limit"], 5);
        assert_eq!(prediction.candidates[0].uci, "e2e4");
        assert_eq!(prediction.candidates[0].rank, 1);
        assert_eq!(prediction.candidates[1].probability, 0.31);
        assert_eq!(prediction.win_probability, Some(0.53));

        server.abort();
    }

    #[tokio::test]
    async fn maia_adapter_returns_recoverable_error_when_service_is_unavailable() {
        let app = Router::new().route(
            "/v1/predict",
            post(|| async { (StatusCode::SERVICE_UNAVAILABLE, "model loading") }),
        );
        let (base_url, server) = spawn_service(app).await;
        let adapter = MaiaHttpAdapter::new(base_url);

        let error = adapter
            .predict(HumanMoveInput {
                position: "8/8/8/8/8/8/8/K6k w - - 0 1",
                elo: EloProfile::try_from(1200).expect("test Elo should be valid"),
                limit: 3,
            })
            .await
            .expect_err("503 should be recoverable");

        assert!(matches!(
            error,
            HumanMoveModelError::Service { status: 503 }
        ));
        server.abort();
    }

    #[tokio::test]
    async fn maia_adapter_rejects_an_invalid_prediction_limit_before_transport() {
        let adapter = MaiaHttpAdapter::new("http://127.0.0.1:1");

        let error = adapter
            .predict(HumanMoveInput {
                position: "8/8/8/8/8/8/8/K6k w - - 0 1",
                elo: EloProfile::try_from(1200).expect("test Elo should be valid"),
                limit: 0,
            })
            .await
            .expect_err("zero candidates is outside the service contract");

        assert!(matches!(error, HumanMoveModelError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn maia_adapter_rejects_an_invalid_win_probability() {
        let app = Router::new().route(
            "/v1/predict",
            post(|| async {
                Json(json!({
                    "moves": [{ "uci": "e2e4", "probability": 0.46 }],
                    "winProbability": 1.5
                }))
            }),
        );
        let (base_url, server) = spawn_service(app).await;
        let adapter = MaiaHttpAdapter::new(base_url);

        let error = adapter
            .predict(HumanMoveInput {
                position: "8/8/8/8/8/8/8/K6k w - - 0 1",
                elo: EloProfile::try_from(1200).expect("test Elo should be valid"),
                limit: 3,
            })
            .await
            .expect_err("probabilities outside zero to one are invalid");

        assert!(matches!(error, HumanMoveModelError::InvalidResponse(_)));
        server.abort();
    }

    #[tokio::test]
    async fn maia_adapter_validates_every_candidate_before_truncating() {
        let app = Router::new().route(
            "/v1/predict",
            post(|| async {
                Json(json!({
                    "moves": [
                        { "uci": "e2e4", "probability": 0.46 },
                        { "uci": "not-a-move", "probability": 0.01 }
                    ],
                    "winProbability": 0.5
                }))
            }),
        );
        let (base_url, server) = spawn_service(app).await;
        let adapter = MaiaHttpAdapter::new(base_url);

        let error = adapter
            .predict(HumanMoveInput {
                position: "8/8/8/8/8/8/8/K6k w - - 0 1",
                elo: EloProfile::try_from(1200).expect("test Elo should be valid"),
                limit: 1,
            })
            .await
            .expect_err("invalid model candidates must not be hidden by truncation");

        assert!(matches!(error, HumanMoveModelError::InvalidResponse(_)));
        server.abort();
    }

    async fn spawn_service(app: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should have an address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test service should run");
        });
        (format!("http://{address}"), server)
    }
}
