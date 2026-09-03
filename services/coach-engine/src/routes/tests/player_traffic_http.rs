use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};
use tower::ServiceExt;

use crate::{
    auth::AuthConfig,
    engine_analysis::{
        EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalysisOutput,
        EngineAnalyzer, EngineProvenance, PositionEvaluation,
    },
    evaluation_recording::{
        PINNED_STOCKFISH_BINARY_DIGEST, PINNED_STOCKFISH_DEPTH, PINNED_STOCKFISH_HASH_MIB,
        PINNED_STOCKFISH_THREADS, PINNED_STOCKFISH_VERSION,
    },
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction},
    lichess::{
        LichessExportClient, LichessExportError, LichessExportRequest, LichessExportResponse,
    },
    quality_capture::NoQualityCaptureStore,
    review_session_contract::*,
    review_session_processor::{
        ControllableTrafficClock, PlayerTrafficPolicy, PLAYER_COMMAND_LIMIT,
        PLAYER_COMMAND_WINDOW_MS, PLAYER_IMPORT_LIMIT,
    },
    review_session_runtime::build_review_session_executor_with_traffic,
    review_session_transport::ReviewSessionWebBinding,
    types::{AppState, HumanMoveCandidate, SharedState},
};

use super::firebase_token_test_support::{
    firebase_token as valid_token, jwt_jwks, FIREBASE_PROJECT_ID,
};

const PLAYER_A: &str = "firebase-player-a";
const PLAYER_B: &str = "firebase-player-b";

#[tokio::test]
async fn command_window_rejects_the_121st_structurally_valid_attempt() {
    let clock = Arc::new(ControllableTrafficClock::new(1_000));
    let application = crate::app(state_with_clock(clock));
    let token = valid_token(PLAYER_A);
    for index in 0..PLAYER_COMMAND_LIMIT {
        let events = send_command(
            &application,
            &token,
            snapshot_command(DeliverySurface::Web, index),
        )
        .await;
        assert_ne!(terminal_kind(&events), "unavailable");
    }
    let limited = send_command(
        &application,
        &token,
        snapshot_command(DeliverySurface::CoachApp, PLAYER_COMMAND_LIMIT),
    )
    .await;
    assert_rate_limited(&limited, 60);
}

#[tokio::test]
async fn two_players_do_not_share_a_command_window() {
    let application = crate::app(state_with_clock(Arc::new(ControllableTrafficClock::new(
        1_000,
    ))));
    let token_a = valid_token(PLAYER_A);
    let token_b = valid_token(PLAYER_B);
    for index in 0..PLAYER_COMMAND_LIMIT {
        send_command(
            &application,
            &token_a,
            snapshot_command(DeliverySurface::Web, index),
        )
        .await;
    }
    assert_rate_limited(
        &send_command(
            &application,
            &token_a,
            snapshot_command(DeliverySurface::Web, PLAYER_COMMAND_LIMIT),
        )
        .await,
        60,
    );
    let other = send_command(
        &application,
        &token_b,
        snapshot_command(DeliverySurface::CoachApp, 0),
    )
    .await;
    assert_ne!(terminal_kind(&other), "unavailable");
}

#[tokio::test]
async fn command_window_recovers_when_the_rolling_interval_expires() {
    let clock = Arc::new(ControllableTrafficClock::new(1_000));
    let application = crate::app(state_with_clock(clock.clone()));
    let token = valid_token(PLAYER_A);
    for index in 0..PLAYER_COMMAND_LIMIT {
        send_command(
            &application,
            &token,
            snapshot_command(DeliverySurface::Web, index),
        )
        .await;
    }
    clock.advance_ms(PLAYER_COMMAND_WINDOW_MS);
    let recovered = send_command(
        &application,
        &token,
        snapshot_command(DeliverySurface::Web, PLAYER_COMMAND_LIMIT + 1),
    )
    .await;
    assert_ne!(terminal_kind(&recovered), "unavailable");
}

#[tokio::test]
async fn invalid_envelopes_do_not_consume_the_command_window() {
    let application = crate::app(state_with_clock(Arc::new(ControllableTrafficClock::new(
        1_000,
    ))));
    let token = valid_token(PLAYER_A);
    for _ in 0..8 {
        let response = application
            .clone()
            .oneshot(authorized_request(&token, b"not-json"))
            .await
            .expect("request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let events = read_events(response).await;
        assert_eq!(events[0]["event"]["kind"], "rejected");
        assert_eq!(events[0]["event"]["reason"], "malformedInput");
    }
    for index in 0..PLAYER_COMMAND_LIMIT {
        let events = send_command(
            &application,
            &token,
            snapshot_command(DeliverySurface::Web, index),
        )
        .await;
        assert_ne!(terminal_kind(&events), "unavailable");
    }
}

#[tokio::test]
async fn concurrent_commands_cannot_exceed_the_player_window() {
    let application = crate::app(state_with_clock(Arc::new(ControllableTrafficClock::new(
        1_000,
    ))));
    let token = valid_token(PLAYER_A);
    let mut tasks = Vec::new();
    for index in 0..PLAYER_COMMAND_LIMIT + 4 {
        let application = application.clone();
        let token = token.clone();
        tasks.push(tokio::spawn(async move {
            send_command(
                &application,
                &token,
                snapshot_command(DeliverySurface::Web, index),
            )
            .await
        }));
    }
    let mut limited = 0;
    for task in tasks {
        if terminal_kind(&task.await.unwrap()) == "unavailable" {
            limited += 1;
        }
    }
    assert_eq!(limited, 4);
}

#[tokio::test]
async fn import_window_rejects_the_11th_accepted_import() {
    let application = crate::app(state_with_clock(Arc::new(ControllableTrafficClock::new(
        1_000,
    ))));
    let token = valid_token(PLAYER_A);
    for index in 0..PLAYER_IMPORT_LIMIT {
        let events = send_command(&application, &token, import_command(index, index)).await;
        assert!(
            events
                .iter()
                .any(|event| event["event"]["kind"] == "completed"),
            "import {index} should complete: {events:?}"
        );
    }
    assert_rate_limited(
        &send_command(
            &application,
            &token,
            import_command(PLAYER_IMPORT_LIMIT, PLAYER_IMPORT_LIMIT),
        )
        .await,
        600,
    );
}

#[tokio::test]
async fn invalid_pgn_and_already_limited_imports_do_not_consume_import_allowance() {
    let application = crate::app(state_with_clock(Arc::new(ControllableTrafficClock::new(
        1_000,
    ))));
    let token = valid_token(PLAYER_A);
    let invalid = send_command(&application, &token, invalid_pgn_command(0)).await;
    assert_eq!(invalid[0]["event"]["kind"], "rejected");
    assert_eq!(invalid[0]["event"]["reason"], "invalidPgn");
    for index in 0..PLAYER_IMPORT_LIMIT {
        send_command(&application, &token, import_command(index, index + 1)).await;
    }
    let limited = send_command(
        &application,
        &token,
        import_command(PLAYER_IMPORT_LIMIT, PLAYER_IMPORT_LIMIT + 1),
    )
    .await;
    assert_rate_limited(&limited, 600);
    let still_limited = send_command(
        &application,
        &token,
        import_command(PLAYER_IMPORT_LIMIT + 1, PLAYER_IMPORT_LIMIT + 2),
    )
    .await;
    assert_rate_limited(&still_limited, 600);
}

#[tokio::test]
async fn idempotent_import_redelivery_does_not_consume_another_import() {
    let application = crate::app(state_with_clock(Arc::new(ControllableTrafficClock::new(
        1_000,
    ))));
    let token = valid_token(PLAYER_A);
    send_command(&application, &token, import_command(0, 0)).await;
    send_command(&application, &token, import_command(0, 0)).await;
    for index in 1..PLAYER_IMPORT_LIMIT {
        send_command(&application, &token, import_command(index, index)).await;
    }
    let overflow = send_command(
        &application,
        &token,
        import_command(PLAYER_IMPORT_LIMIT, PLAYER_IMPORT_LIMIT),
    )
    .await;
    assert_rate_limited(&overflow, 600);
}

#[tokio::test]
async fn post_admission_provider_failure_still_consumes_import_allowance() {
    let clock = Arc::new(ControllableTrafficClock::new(1_000));
    let application = crate::app(state_with_failing_lichess(clock));
    let token = valid_token(PLAYER_A);
    for index in 0..PLAYER_IMPORT_LIMIT {
        let events = send_command(&application, &token, lichess_import_command(index)).await;
        assert_eq!(
            events
                .last()
                .and_then(|event| event["event"]["kind"].as_str()),
            Some("unavailable")
        );
        assert_eq!(
            events
                .last()
                .and_then(|event| event["event"]["reason"]["kind"].as_str()),
            Some("lichessTransport")
        );
    }
    assert_rate_limited(
        &send_command(
            &application,
            &token,
            lichess_import_command(PLAYER_IMPORT_LIMIT),
        )
        .await,
        600,
    );
}

#[tokio::test]
async fn player_traffic_composes_with_engine_admission() {
    let clock = Arc::new(ControllableTrafficClock::new(1_000));
    let engine = Arc::new(HoldEngine::default());
    let application = crate::app(state_with_engine(clock, engine.clone()));
    let token = valid_token(PLAYER_A);
    let first = tokio::spawn({
        let application = application.clone();
        let token = token.clone();
        async move { send_command(&application, &token, import_command(0, 0)).await }
    });
    engine.wait_until_entered().await;
    let busy = send_command(&application, &token, import_command(1, 1)).await;
    assert_eq!(
        busy.iter()
            .find_map(|event| event["event"]["reason"]["kind"].as_str()),
        Some("admissionLimit")
    );
    engine.release();
    let completed = first.await.unwrap();
    assert!(completed
        .iter()
        .any(|event| event["event"]["kind"] == "completed"));
}

#[tokio::test]
async fn rate_limited_command_does_not_take_the_engine_lease() {
    let clock = Arc::new(ControllableTrafficClock::new(1_000));
    let application = crate::app(state_with_clock(clock));
    let token_a = valid_token(PLAYER_A);
    let token_b = valid_token(PLAYER_B);
    for index in 0..PLAYER_COMMAND_LIMIT {
        send_command(
            &application,
            &token_a,
            snapshot_command(DeliverySurface::Web, index),
        )
        .await;
    }
    assert_rate_limited(
        &send_command(
            &application,
            &token_a,
            import_command(0, PLAYER_COMMAND_LIMIT),
        )
        .await,
        60,
    );
    let other = send_command(&application, &token_b, import_command(0, 0)).await;
    assert!(other
        .iter()
        .any(|event| event["event"]["kind"] == "completed"));
}

fn snapshot_command(surface: DeliverySurface, index: usize) -> ReviewSessionCommandEnvelope {
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from(format!("request:traffic:snapshot:{index}")).unwrap(),
        operation_id: OperationId::try_from(format!("operation:traffic:snapshot:{index}")).unwrap(),
        surface,
        command: ReviewSessionCommand::ReadGameReviewSnapshot {
            game_import_id: GameImportId::try_from("game-import:traffic:unknown".to_string())
                .unwrap(),
            known_content_digest: None,
        },
    }
}

fn import_command(game: usize, operation: usize) -> ReviewSessionCommandEnvelope {
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from(format!("request:traffic:import:{operation}")).unwrap(),
        operation_id: OperationId::try_from(format!("operation:traffic:import:{operation}"))
            .unwrap(),
        surface: DeliverySurface::Web,
        command: ReviewSessionCommand::ImportGame {
            source: GameInputSource::PastedPgn {
                pgn: unique_pgn(game),
            },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
            elo_profile: RequestedEloProfile::PlayerProvided {
                rating: EloRating::try_from(1200).unwrap(),
            },
        },
    }
}

fn invalid_pgn_command(operation: usize) -> ReviewSessionCommandEnvelope {
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from(format!("request:traffic:invalid:{operation}")).unwrap(),
        operation_id: OperationId::try_from(format!("operation:traffic:invalid:{operation}"))
            .unwrap(),
        surface: DeliverySurface::Web,
        command: ReviewSessionCommand::ImportGame {
            source: GameInputSource::PastedPgn {
                pgn: "not a game".to_string(),
            },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
            elo_profile: RequestedEloProfile::PlayerProvided {
                rating: EloRating::try_from(1200).unwrap(),
            },
        },
    }
}

fn lichess_import_command(operation: usize) -> ReviewSessionCommandEnvelope {
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from(format!("request:traffic:lichess:{operation}")).unwrap(),
        operation_id: OperationId::try_from(format!("operation:traffic:lichess:{operation}"))
            .unwrap(),
        surface: DeliverySurface::Web,
        command: ReviewSessionCommand::ImportGame {
            source: GameInputSource::LichessUrl {
                url: format!("https://lichess.org/abcd{operation:04}"),
            },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
            elo_profile: RequestedEloProfile::PlayerProvided {
                rating: EloRating::try_from(1200).unwrap(),
            },
        },
    }
}

fn unique_pgn(index: usize) -> String {
    let files = [
        "e4", "d4", "c4", "Nf3", "g3", "b3", "f4", "Nc3", "e3", "d3", "c3",
    ];
    let reply = [
        "e5", "d5", "c5", "Nc6", "g6", "b6", "f5", "Nc6", "e6", "d6", "c6",
    ];
    format!(
        "[Event \"Traffic {index}\"]\n[Site \"https://lichess.org/traffic{index}\"]\n[White \"White Player\"]\n[Black \"Black Player\"]\n[Result \"1-0\"]\n\n1. {} {} 1-0\n",
        files[index % files.len()],
        reply[index % reply.len()]
    )
}

async fn send_command(
    application: &axum::Router,
    token: &str,
    command: ReviewSessionCommandEnvelope,
) -> Vec<serde_json::Value> {
    let response = application
        .clone()
        .oneshot(authorized_request(
            token,
            &serde_json::to_vec(&command).unwrap(),
        ))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    read_events(response).await
}

async fn read_events(response: axum::response::Response) -> Vec<serde_json::Value> {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("event stream should complete");
    body.split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

fn authorized_request(token: &str, body: &[u8]) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/api/v1/review-session/commands")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_vec()))
        .expect("valid request")
}

fn terminal_kind(events: &[serde_json::Value]) -> &str {
    events
        .iter()
        .rev()
        .find_map(|event| event["event"]["kind"].as_str())
        .unwrap_or("missing")
}

fn assert_rate_limited(events: &[serde_json::Value], retry_after_seconds: u32) {
    let terminal = events
        .iter()
        .find(|event| event["event"]["kind"] == "unavailable")
        .expect("rate limited commands return unavailable");
    assert_eq!(terminal["event"]["reason"]["kind"], "rateLimited");
    assert_eq!(
        terminal["event"]["reason"]["retryAfterSeconds"],
        retry_after_seconds
    );
    assert_eq!(terminal["event"]["retry"]["kind"], "retryAfter");
    assert_eq!(terminal["event"]["retry"]["seconds"], retry_after_seconds);
}

fn state_with_clock(clock: Arc<ControllableTrafficClock>) -> SharedState {
    state_with_engine(clock, Arc::new(FakeEngine))
}

fn state_with_engine(
    clock: Arc<ControllableTrafficClock>,
    engine: Arc<dyn EngineAnalyzer>,
) -> SharedState {
    let traffic = Arc::new(PlayerTrafficPolicy::v1_with_clock(clock));
    Arc::new(AppState {
        account_deletion: crate::account_deletion::AccountDeletionRuntime::disabled(),
        auth: AuthConfig::new_firebase(FIREBASE_PROJECT_ID, jwt_jwks())
            .expect("test key should be valid"),
        beta_access: crate::beta_access::BetaAccessRuntime::disabled(),
        daily_coaching: crate::daily_coaching::DailyCoachingRuntime::disabled(),
        imported_games: crate::imported_games::ImportedGamesRuntime::in_memory(),
        opening_analysis: crate::opening_analysis::OpeningAnalysisRuntime::disabled(),
        review_session: ReviewSessionWebBinding::new(build_review_session_executor_with_traffic(
            crate::lichess::ReqwestLichessExportClient::new().expect("client"),
            engine,
            Arc::new(FakeHumanMoveModel),
            traffic,
        ))
        .with_quality_capture_store(Arc::new(NoQualityCaptureStore)),
    })
}

fn state_with_failing_lichess(clock: Arc<ControllableTrafficClock>) -> SharedState {
    let traffic = Arc::new(PlayerTrafficPolicy::v1_with_clock(clock));
    Arc::new(AppState {
        account_deletion: crate::account_deletion::AccountDeletionRuntime::disabled(),
        auth: AuthConfig::new_firebase(FIREBASE_PROJECT_ID, jwt_jwks())
            .expect("test key should be valid"),
        beta_access: crate::beta_access::BetaAccessRuntime::disabled(),
        daily_coaching: crate::daily_coaching::DailyCoachingRuntime::disabled(),
        imported_games: crate::imported_games::ImportedGamesRuntime::in_memory(),
        opening_analysis: crate::opening_analysis::OpeningAnalysisRuntime::disabled(),
        review_session: ReviewSessionWebBinding::new(build_review_session_executor_with_traffic(
            FailingLichess,
            Arc::new(FakeEngine),
            Arc::new(FakeHumanMoveModel),
            traffic,
        ))
        .with_quality_capture_store(Arc::new(NoQualityCaptureStore)),
    })
}

struct FakeEngine;

impl EngineAnalyzer for FakeEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        let best_move = first_legal_uci(input.position);
        Box::pin(async move {
            Ok(EngineAnalysis {
                best_move: best_move.clone(),
                evaluation: PositionEvaluation::Centipawns(500),
                principal_variation: vec![best_move],
                depth: 16,
            })
        })
    }

    fn analyze_with_provenance<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysisOutput, EngineAnalysisError>> + Send + 'a>>
    {
        let analysis = self.analyze(input);
        Box::pin(async move {
            Ok(EngineAnalysisOutput {
                analysis: analysis.await?,
                provenance: Some(fake_engine_provenance()),
            })
        })
    }
}

#[derive(Default)]
struct HoldEngine {
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    entered_count: AtomicUsize,
}

impl HoldEngine {
    async fn wait_until_entered(&self) {
        let started = tokio::time::Instant::now();
        while self.entered_count.load(Ordering::Acquire) == 0 {
            tokio::select! {
                _ = self.entered.notified() => {}
                _ = tokio::time::sleep(std::time::Duration::from_millis(5)) => {}
            }
            if started.elapsed() > std::time::Duration::from_secs(5) {
                panic!("the first import should enter engine admission");
            }
        }
    }

    fn release(&self) {
        self.release.notify_waiters();
    }
}

impl EngineAnalyzer for HoldEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        let best_move = first_legal_uci(input.position);
        Box::pin(async move {
            self.entered_count.fetch_add(1, Ordering::AcqRel);
            self.entered.notify_waiters();
            self.release.notified().await;
            Ok(EngineAnalysis {
                best_move: best_move.clone(),
                evaluation: PositionEvaluation::Centipawns(500),
                principal_variation: vec![best_move],
                depth: 16,
            })
        })
    }

    fn analyze_with_provenance<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysisOutput, EngineAnalysisError>> + Send + 'a>>
    {
        let analysis = self.analyze(input);
        Box::pin(async move {
            Ok(EngineAnalysisOutput {
                analysis: analysis.await?,
                provenance: Some(fake_engine_provenance()),
            })
        })
    }
}

fn fake_engine_provenance() -> EngineProvenance {
    EngineProvenance {
        version: PINNED_STOCKFISH_VERSION.to_string(),
        binary_sha256: PINNED_STOCKFISH_BINARY_DIGEST
            .strip_prefix("sha256:")
            .expect("pinned digest has a prefix")
            .to_string(),
        depth: PINNED_STOCKFISH_DEPTH,
        threads: PINNED_STOCKFISH_THREADS,
        hash_mib: PINNED_STOCKFISH_HASH_MIB,
    }
}

struct FakeHumanMoveModel;

impl HumanMoveModel for FakeHumanMoveModel {
    fn predict<'a>(
        &'a self,
        input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        let candidate = first_legal_uci(input.position);
        Box::pin(async move {
            Ok(HumanMovePrediction {
                candidates: vec![HumanMoveCandidate {
                    uci: candidate,
                    probability: 0.8,
                    rank: 1,
                }],
                win_probability: Some(0.5),
            })
        })
    }
}

struct FailingLichess;

impl LichessExportClient for FailingLichess {
    fn export<'a>(
        &'a self,
        _request: &'a LichessExportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LichessExportResponse, LichessExportError>> + Send + 'a>>
    {
        Box::pin(async { Err(LichessExportError::Transport("fixture".to_string())) })
    }
}

fn first_legal_uci(fen: &str) -> String {
    let position: Chess = Fen::from_ascii(fen.as_bytes())
        .expect("test position should be valid FEN")
        .into_position(CastlingMode::Standard)
        .expect("test position should be legal");
    let chess_move = position
        .legal_moves()
        .into_iter()
        .next()
        .expect("test position should have a legal move");
    UciMove::from_standard(&chess_move).to_string()
}
