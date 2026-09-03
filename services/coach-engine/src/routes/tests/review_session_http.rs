use std::{future::Future, pin::Pin, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    extract::Path,
    http::{header, Method, Request, StatusCode},
    routing::get,
    Json, Router,
};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};
use tower::ServiceExt;

use crate::{
    auth::AuthConfig,
    critical_moment_comment::grounding_ledger_for,
    engine_analysis::{
        EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalysisOutput,
        EngineAnalyzer, EngineProvenance, PositionEvaluation,
    },
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction},
    quality_capture::{
        InMemoryQualityCaptureStore, NoQualityCaptureStore, QualityCapturePreferenceStore,
        RetentionPreference,
    },
    review_session_runtime::build_review_session_executor_with_providers,
    review_session_transport::ReviewSessionWebBinding,
    types::{AppState, HumanMoveCandidate, SharedState},
};

use crate::evaluation_recording::{
    PINNED_STOCKFISH_BINARY_DIGEST, PINNED_STOCKFISH_DEPTH, PINNED_STOCKFISH_HASH_MIB,
    PINNED_STOCKFISH_THREADS, PINNED_STOCKFISH_VERSION,
};
use crate::review_session_contract::*;

const PLAYER_ID: &str = "firebase-player-a";

use super::firebase_token_test_support::{
    firebase_token as valid_token, jwt_jwks, verified_firebase_token as firebase_token,
    FIREBASE_PROJECT_ID,
};

#[tokio::test]
async fn authentication_and_routing_stay_at_the_http_boundary() {
    let missing_auth = crate::app(state())
        .oneshot(review_session_request(Method::POST, None, b"{}"))
        .await
        .expect("request should complete");
    assert_eq!(missing_auth.status(), StatusCode::UNAUTHORIZED);

    let token = valid_token(PLAYER_ID);
    let wrong_method = crate::app(state())
        .oneshot(review_session_request(
            Method::GET,
            Some(&format!("Bearer {token}")),
            b"",
        ))
        .await
        .expect("request should complete");
    assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);

    let unknown = Request::builder()
        .uri("/api/v1/review-session/unknown")
        .body(Body::empty())
        .expect("valid request");
    let unknown = crate::app(state())
        .oneshot(unknown)
        .await
        .expect("request should complete");
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

    for legacy in ["/api/analyze", "/api/review-moment"] {
        let response = crate::app(state())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(legacy)
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("request should complete");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn firebase_identity_bridge_returns_only_the_verified_player_id() {
    for sign_in_provider in ["password", "google.com"] {
        let response = crate::app(state())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/internal/v1/oauth/firebase-identity")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({
                            "firebaseIdToken": firebase_token(
                                PLAYER_ID,
                                true,
                                sign_in_provider,
                            ),
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .expect("identity bridge request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            )
            .unwrap(),
            serde_json::json!({
                "authorizationKind": "player",
                "playerId": PLAYER_ID,
            })
        );
    }
}

#[tokio::test]
async fn firebase_identity_bridge_rejects_unverified_and_unsupported_sign_in() {
    for (email_verified, sign_in_provider) in [
        (false, "password"),
        (true, "github.com"),
        (true, "anonymous"),
    ] {
        let response = crate::app(state())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/internal/v1/oauth/firebase-identity")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({
                            "firebaseIdToken": firebase_token(
                                PLAYER_ID,
                                email_verified,
                                sign_in_provider,
                            ),
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .expect("identity bridge request should complete");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn account_deletion_is_not_exposed_by_a_nonproduction_runtime() {
    let token = valid_token(PLAYER_ID);
    let response = crate::app(state())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/account/deletion")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "confirmation":
                            crate::account_deletion::ACCOUNT_DELETION_CONFIRMATION,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("account deletion request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deletion_marker_blocks_regular_player_requests_immediately() {
    let marker_service = Router::new().route(
        "/v1/projects/chenchess-test/databases/coach-app-production/documents/deletedUsers/:document_id",
        get(|Path(document_id): Path<String>| async move {
            Json(serde_json::json!({
                "name": format!(
                    "projects/chenchess-test/databases/coach-app-production/documents/deletedUsers/{document_id}"
                ),
                "fields": {
                    "schemaVersion": { "integerValue": "1" },
                    "playerId": { "stringValue": PLAYER_ID },
                    "startedAt": { "timestampValue": "2026-08-01T10:00:00Z" },
                    "phase": { "stringValue": "markersWritten" },
                },
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, marker_service).await });
    let account_deletion = crate::account_deletion::AccountDeletionRuntime::marker_only(
        crate::firestore::FirestoreDatabase::production_emulator(
            FIREBASE_PROJECT_ID,
            address.to_string(),
        )
        .unwrap(),
    );
    let application = crate::app(state_with_account_deletion(account_deletion));
    let token = valid_token(PLAYER_ID);

    let response = application
        .oneshot(retention_request(Method::GET, Some(&token), None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    server.abort();
}

#[tokio::test]
async fn retention_preference_is_authenticated_default_enabled_and_player_scoped() {
    let store = Arc::new(InMemoryQualityCaptureStore::default());
    let application = crate::app(state_with_quality_capture_store(
        Arc::new(FakeEngine),
        Arc::new(FakeHumanMoveModel),
        store,
    ));
    let player_a = valid_token(PLAYER_ID);
    let player_b = valid_token("firebase-player-b");

    let missing = application
        .clone()
        .oneshot(retention_request(Method::GET, None, None))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    assert_eq!(
        retention_response(&application, Method::GET, &player_a, None).await,
        RetentionPreference {
            available: true,
            enabled: true,
            disclosure_required: true,
            deleted_review_snapshots: 0,
        }
    );
    assert_eq!(
        retention_response(
            &application,
            Method::PUT,
            &player_a,
            Some(serde_json::json!({ "enabled": false })),
        )
        .await,
        RetentionPreference {
            available: true,
            enabled: false,
            disclosure_required: false,
            deleted_review_snapshots: 0,
        }
    );
    assert!(
        retention_response(&application, Method::GET, &player_b, None)
            .await
            .enabled,
        "one authenticated Player cannot change another Player's preference"
    );
}

async fn retention_response(
    application: &axum::Router,
    method: Method,
    token: &str,
    body: Option<serde_json::Value>,
) -> RetentionPreference {
    let response = application
        .clone()
        .oneshot(retention_request(method, Some(token), body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

fn retention_request(
    method: Method,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri("/api/v1/review-artifacts/preference")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    request
        .body(Body::from(
            body.map(|value| serde_json::to_vec(&value).unwrap())
                .unwrap_or_default(),
        ))
        .unwrap()
}

#[tokio::test]
async fn malformed_input_streams_one_stable_rejection() {
    let token = valid_token(PLAYER_ID);
    let response = crate::app(state())
        .oneshot(review_session_request(
            Method::POST,
            Some(&format!("Bearer {token}")),
            b"not-json",
        ))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/x-ndjson"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("event stream should complete");
    let lines = body.split(|byte| *byte == b'\n').collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    let event: serde_json::Value =
        serde_json::from_slice(lines[0]).expect("rejection should be JSON");
    assert_eq!(event["requestId"], "request:command-admission");
    assert_eq!(event["operationId"], "operation:command-admission");
    assert_eq!(event["event"]["kind"], "rejected");
    assert_eq!(event["event"]["operation"], "commandAdmission");
    assert_eq!(event["event"]["reason"], "malformedInput");
}

#[tokio::test]
async fn authenticated_command_streams_server_owned_ndjson_events() {
    let token = valid_token(PLAYER_ID);
    let command = ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from("request:http:import".to_string()).unwrap(),
        operation_id: OperationId::try_from("operation:http:import".to_string()).unwrap(),
        surface: DeliverySurface::Web,
        command: ReviewSessionCommand::ImportGame {
            source: GameInputSource::PastedPgn {
                pgn: sample_pgn().to_string(),
            },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::Both,
            },
            elo_profile: RequestedEloProfile::PlayerProvided {
                rating: EloRating::try_from(1200).unwrap(),
            },
        },
    };
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/review-session/commands")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&command).unwrap()))
        .expect("valid request");
    let application = crate::app(state());
    let response = application
        .clone()
        .oneshot(request)
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("event stream should complete");
    let events = body
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<ReviewSessionEventEnvelope>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        events.first().map(|event| &event.event),
        Some(ReviewSessionEvent::Accepted {
            operation: OperationKind::GameImport,
            ..
        })
    ));
    let imported = events.iter().find_map(|event| match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::GameImported {
                game_import_id,
                review,
                ..
            } => Some((
                game_import_id.clone(),
                review
                    .critical_moments
                    .first()
                    .expect("test review should contain a Critical Moment")
                    .critical_moment_id
                    .clone(),
            )),
            _ => None,
        },
        _ => None,
    });
    let (game_import_id, _) = imported.expect("import should return review facts");

    let start = ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from("request:http:start".to_string()).unwrap(),
        operation_id: OperationId::try_from("operation:http:start".to_string()).unwrap(),
        surface: DeliverySurface::Web,
        command: ReviewSessionCommand::StartReviewSession { game_import_id },
    };
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/review-session/commands")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&start).unwrap()))
        .expect("valid request");
    let response = application
        .clone()
        .oneshot(request)
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("session event stream should complete");
    let (game_import_id, started) = body
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<ReviewSessionEventEnvelope>(line).unwrap())
        .find_map(|event| match event.event {
            ReviewSessionEvent::Completed { result } => match *result {
                OperationCompletion::ReviewSessionStarted {
                    game_import_id,
                    review_moments,
                    ..
                } => Some((game_import_id, review_moments)),
                _ => None,
            },
            _ => None,
        })
        .expect("Review Session should start");
    assert!(!started.is_empty());
    let admitted = &started[0];
    let core = admitted
        .prepared_core()
        .expect("Web starts return the complete prepared set");
    let open = ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from("request:http:open".to_string()).unwrap(),
        operation_id: OperationId::try_from("operation:http:open".to_string()).unwrap(),
        surface: DeliverySurface::CoachApp,
        command: ReviewSessionCommand::OpenReviewMoment {
            game_import_id,
            selection: admitted.review_moment.selection.clone(),
            idempotency_key: IdempotencyKey::try_from("idempotency-key:http:open".to_string())
                .unwrap(),
        },
    };
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/review-session/commands")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&open).unwrap()))
        .expect("valid request");
    let response = application
        .oneshot(request)
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("open-moment event stream should complete");
    let authoring_context = body
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<ReviewSessionEventEnvelope>(line).unwrap())
        .find_map(|event| match event.event {
            ReviewSessionEvent::Completed { result } => match *result {
                OperationCompletion::ReviewMomentOpened {
                    authoring_context, ..
                } => authoring_context.map(|context| *context),
                _ => None,
            },
            _ => None,
        })
        .expect("Coach App open should return immutable comment authoring authority");
    assert_eq!(
        authoring_context.facts.moment().critical_moment_id,
        core.review_moment.moment_id
    );
    assert_eq!(
        authoring_context.required_grounding_ledger,
        grounding_ledger_for(&authoring_context.facts)
    );
}

fn review_session_request(
    method: Method,
    authorization: Option<&str>,
    body: &'static [u8],
) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri("/api/v1/review-session/commands")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(authorization) = authorization {
        request = request.header(header::AUTHORIZATION, authorization);
    }
    request.body(Body::from(body)).expect("valid request")
}

fn state() -> SharedState {
    let engine = Arc::new(FakeEngine) as Arc<dyn EngineAnalyzer>;
    let human = Arc::new(FakeHumanMoveModel) as Arc<dyn HumanMoveModel>;
    state_with_human(engine, human)
}

fn state_with_account_deletion(
    account_deletion: crate::account_deletion::AccountDeletionRuntime,
) -> SharedState {
    Arc::new(AppState {
        account_deletion,
        auth: AuthConfig::new_firebase(FIREBASE_PROJECT_ID, jwt_jwks())
            .expect("test key should be valid"),
        beta_access: crate::beta_access::BetaAccessRuntime::disabled(),
        daily_coaching: crate::daily_coaching::DailyCoachingRuntime::disabled(),
        imported_games: crate::imported_games::ImportedGamesRuntime::in_memory(),
        opening_analysis: crate::opening_analysis::OpeningAnalysisRuntime::disabled(),
        review_session: ReviewSessionWebBinding::new(
            build_review_session_executor_with_providers(
                Arc::new(FakeEngine),
                Arc::new(FakeHumanMoveModel),
            )
            .expect("the checked-in Review Session recording should be valid"),
        )
        .with_quality_capture_store(Arc::new(NoQualityCaptureStore)),
    })
}

fn state_with_human(
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
) -> SharedState {
    state_with_quality_capture_store(engine, human, Arc::new(NoQualityCaptureStore))
}

fn state_with_quality_capture_store(
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
    quality_capture: Arc<dyn QualityCapturePreferenceStore>,
) -> SharedState {
    Arc::new(AppState {
        account_deletion: crate::account_deletion::AccountDeletionRuntime::disabled(),
        auth: AuthConfig::new_firebase(FIREBASE_PROJECT_ID, jwt_jwks())
            .expect("test key should be valid"),
        beta_access: crate::beta_access::BetaAccessRuntime::disabled(),
        daily_coaching: crate::daily_coaching::DailyCoachingRuntime::disabled(),
        imported_games: crate::imported_games::ImportedGamesRuntime::in_memory(),
        opening_analysis: crate::opening_analysis::OpeningAnalysisRuntime::disabled(),
        review_session: ReviewSessionWebBinding::new(
            build_review_session_executor_with_providers(engine, human)
                .expect("the checked-in Review Session recording should be valid"),
        )
        .with_quality_capture_store(quality_capture),
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

fn sample_pgn() -> &'static str {
    r#"[Event "Casual Game"]
[Site "https://lichess.org/example"]
[White "White Player"]
[Black "Black Player"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0"#
}
