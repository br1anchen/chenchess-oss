use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    extract::Path,
    http::{header, Method, Request, StatusCode},
    routing::get,
    Json, Router,
};
use tokio::sync::mpsc;
use tower::ServiceExt;

use crate::{
    account_deletion::AccountDeletionRuntime,
    auth::AuthConfig,
    beta_access::{BetaAccessRuntime, InMemoryBetaAccessStore},
    deployment::DeploymentEnvironment,
    review_session_contract::ReviewSessionEventEnvelope,
    review_session_processor::{ProcessorCommandAdmission, ProcessorPrincipal},
    review_session_transport::{ReviewSessionCommandExecutor, ReviewSessionWebBinding},
    types::AppState,
};

use super::firebase_token_test_support::{
    coach_token, firebase_token_with_conformance_claim, jwt_jwks, mcp_conformance_coach_token,
    mcp_conformance_firebase_token, COACH_ISSUER, COACH_RESOURCE, COACH_SCOPE, FIREBASE_PROJECT_ID,
    MCP_CONFORMANCE_PLAYER_ID,
};

const TEST_KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn staging_bridge_authorizes_exact_conformance_identity_without_a_beta_grant() {
    let store = Arc::new(InMemoryBetaAccessStore::default());
    let application = application(
        staging_auth(),
        BetaAccessRuntime::in_memory(store.clone(), TEST_KEY).unwrap(),
        AccountDeletionRuntime::disabled(),
    );

    let response = application
        .oneshot(identity_bridge_request(&mcp_conformance_firebase_token(
            MCP_CONFORMANCE_PLAYER_ID,
        )))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &to_bytes(response.into_body(), usize::MAX).await.unwrap()
        )
        .unwrap(),
        serde_json::json!({
            "authorizationKind": "mcpConformance",
            "playerId": MCP_CONFORMANCE_PLAYER_ID,
        })
    );
    assert_eq!(store.access_grant_count(), 0);
}

#[tokio::test]
async fn staging_coach_token_keeps_conformance_authorization_without_a_beta_grant() {
    let store = Arc::new(InMemoryBetaAccessStore::default());
    let application = application(
        staging_auth(),
        BetaAccessRuntime::in_memory(store.clone(), TEST_KEY).unwrap(),
        AccountDeletionRuntime::disabled(),
    );

    let response = application
        .oneshot(authorization_request(&mcp_conformance_coach_token(
            MCP_CONFORMANCE_PLAYER_ID,
        )))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(store.access_grant_count(), 0);
}

#[tokio::test]
async fn conformance_identity_is_rejected_by_default_and_on_public_firebase_routes() {
    let token = mcp_conformance_firebase_token(MCP_CONFORMANCE_PLAYER_ID);
    let production = application(
        default_auth(),
        BetaAccessRuntime::disabled(),
        AccountDeletionRuntime::disabled(),
    );
    assert_eq!(
        production
            .clone()
            .oneshot(identity_bridge_request(&token))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        production
            .oneshot(authorization_request(&mcp_conformance_coach_token(
                MCP_CONFORMANCE_PLAYER_ID,
            )))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let public_route = application(
        staging_auth(),
        BetaAccessRuntime::in_memory(Arc::new(InMemoryBetaAccessStore::default()), TEST_KEY)
            .unwrap(),
        AccountDeletionRuntime::disabled(),
    );
    let response = public_route
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/beta-access/requests")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header("x-chenchess-source-ip", "203.0.113.1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = public_route
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/review-session/commands")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn reserved_subject_or_claim_alone_cannot_authorize_conformance() {
    let application = application(
        staging_auth(),
        BetaAccessRuntime::disabled(),
        AccountDeletionRuntime::disabled(),
    );
    let invalid_firebase_tokens = [
        firebase_token_with_conformance_claim(MCP_CONFORMANCE_PLAYER_ID, "password", Some(true)),
        firebase_token_with_conformance_claim(MCP_CONFORMANCE_PLAYER_ID, "custom", None),
        firebase_token_with_conformance_claim("firebase-player-a", "custom", Some(true)),
    ];
    for token in invalid_firebase_tokens {
        assert_eq!(
            application
                .clone()
                .oneshot(identity_bridge_request(&token))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        application
            .oneshot(authorization_request(&coach_token(
                MCP_CONFORMANCE_PLAYER_ID,
            )))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn account_deletion_marker_precedes_conformance_beta_bypass() {
    let marker_service = Router::new().route(
        "/v1/projects/chenchess-test/databases/coach-app-production/documents/deletedUsers/:document_id",
        get(|Path(document_id): Path<String>| async move {
            Json(serde_json::json!({
                "name": format!(
                    "projects/chenchess-test/databases/coach-app-production/documents/deletedUsers/{document_id}"
                ),
                "fields": {
                    "schemaVersion": { "integerValue": "1" },
                    "playerId": { "stringValue": MCP_CONFORMANCE_PLAYER_ID },
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
    let account_deletion = AccountDeletionRuntime::marker_only(
        crate::firestore::FirestoreDatabase::production_emulator(
            FIREBASE_PROJECT_ID,
            address.to_string(),
        )
        .unwrap(),
    );
    let application = application(
        staging_auth(),
        BetaAccessRuntime::disabled(),
        account_deletion,
    );

    let response = application
        .oneshot(identity_bridge_request(&mcp_conformance_firebase_token(
            MCP_CONFORMANCE_PLAYER_ID,
        )))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    server.abort();
}

fn default_auth() -> AuthConfig {
    AuthConfig::new_firebase(FIREBASE_PROJECT_ID, jwt_jwks())
        .unwrap()
        .with_coach_mcp(jwt_jwks(), COACH_ISSUER, COACH_RESOURCE, COACH_SCOPE)
        .unwrap()
}

fn staging_auth() -> AuthConfig {
    default_auth().with_mcp_conformance_for_test(DeploymentEnvironment::Staging)
}

fn application(
    auth: AuthConfig,
    beta_access: BetaAccessRuntime,
    account_deletion: AccountDeletionRuntime,
) -> Router {
    crate::app(Arc::new(AppState {
        account_deletion,
        auth,
        beta_access,
        daily_coaching: crate::daily_coaching::DailyCoachingRuntime::disabled(),
        imported_games: crate::imported_games::ImportedGamesRuntime::in_memory(),
        opening_analysis: crate::opening_analysis::OpeningAnalysisRuntime::disabled(),
        review_session: ReviewSessionWebBinding::new(Arc::new(NoopExecutor)),
    }))
}

fn identity_bridge_request(token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/internal/v1/oauth/firebase-identity")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "firebaseIdToken": token }).to_string(),
        ))
        .unwrap()
}

fn authorization_request(token: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/beta-access/authorization")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

struct NoopExecutor;

impl ReviewSessionCommandExecutor for NoopExecutor {
    fn submit(
        self: Arc<Self>,
        _principal: ProcessorPrincipal,
        _admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let (_sender, receiver) = mpsc::unbounded_channel();
        receiver
    }
}
