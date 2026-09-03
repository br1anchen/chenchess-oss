use crate::profile_game_feed::lichess_moves;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use chrono::{DateTime, TimeDelta, Timelike, Utc};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tower::ServiceExt;

use crate::{
    account_deletion::AccountDeletionRuntime,
    auth::AuthConfig,
    beta_access::BetaAccessRuntime,
    daily_coaching::{
        ConnectPlayingProfileOutcome, ConnectPlayingProfileRequest, DailyCoachingRuntime,
    },
    profile_game_feed::{
        ProfileGameClient, ProfileGameFetchError, ProfileGameRequest, ProfileGameResponse,
        ProfileValidationError, PublicChessProfile, PublicProfileValidator,
        ValidatedPublicChessProfile,
    },
    review_session_contract::{
        CanonicalGameId, CommandRejectionReason, CriticalMomentId, CurriculumLearningConcept,
        DeliverySurface, ExplanationPathRef, GameImportId, GameInputSource, GameReview,
        ImportProvenance, ImportedGame, LearningPathRef, LearningPlan, LearningResource,
        LearningResourceId, LearningResourceKind, LearningResourceRole, LearningTrack,
        LearningTrackKey, LearningTrackSupport, LearningTrackSupportBasis, OperationCompletion,
        OperationKind, PlayerId, RejectionRecovery, RequestedEloProfile, RequestedReviewSide,
        ReviewSessionCommand, ReviewSessionCommandEnvelope, ReviewSessionEvent,
        ReviewSessionEventEnvelope, ReviewSide, LEARNING_PLAN_SELECTION_POLICY_VERSION,
        LEARNING_RESOURCE_CATALOG_VERSION,
    },
    review_session_processor::{ProcessorCommandAdmission, ProcessorPrincipal},
    review_session_transport::{ReviewSessionCommandExecutor, ReviewSessionWebBinding},
    types::AppState,
};

use super::firebase_token_test_support::{
    coach_token, firebase_token, jwt_jwks, COACH_ISSUER, COACH_RESOURCE, COACH_SCOPE,
    FIREBASE_PROJECT_ID,
};

#[path = "daily_coaching_http/email.rs"]
mod email;

#[path = "daily_coaching_http/initial_backfill.rs"]
mod initial_backfill;

#[path = "daily_coaching_http/merged_connections.rs"]
mod merged_connections;

#[path = "daily_coaching_http/conformance.rs"]
mod conformance;

#[path = "daily_coaching_http/recent_profile_games.rs"]
mod recent_profile_games;

#[path = "daily_coaching_http/opening_lines.rs"]
mod opening_lines;

#[path = "daily_coaching_http/opening_analysis.rs"]
mod opening_analysis;

#[tokio::test]
async fn reviewed_game_search_accepts_empty_filters_and_rejects_silent_near_misses() {
    let application = application(Arc::new(FakeProfileValidator::default()));

    let empty = request(
        &application,
        Method::POST,
        "/api/v1/reviewed-games/search",
        json!({}),
    )
    .await;
    assert_eq!(empty.0, StatusCode::OK);
    assert_eq!(
        empty.1,
        json!({
            "coverage": { "reviewedGameCount": 0 },
            "games": [],
            "truncation": { "kind": "complete", "totalMatchCount": 0 }
        })
    );

    for invalid in [
        json!({ "outcome": "won" }),
        json!({ "playedFrom": "last Tuesday" }),
        json!({ "opponentRatingMin": 99 }),
        json!({ "learningTrackKey": { "kind": "curriculum" } }),
        json!({ "unknownOnly": true }),
    ] {
        let rejected = request(
            &application,
            Method::POST,
            "/api/v1/reviewed-games/search",
            invalid,
        )
        .await;
        assert!(rejected.0.is_client_error());
    }

    let inverted = request(
        &application,
        Method::POST,
        "/api/v1/reviewed-games/search",
        json!({ "playedFrom": "2026-08-12", "playedTo": "2026-08-01" }),
    )
    .await;
    assert_eq!(inverted.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn connects_aliases_idempotently_and_captures_only_the_first_timezone() {
    let validator = Arc::new(FakeProfileValidator::default());
    let application = application(validator.clone());

    let connected = request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections",
        json!({
            "profileUrl": "https://lichess.org/@/PlayerOne/all/",
            "timezone": "not/a-timezone"
        }),
    )
    .await;
    assert_eq!(connected.0, StatusCode::OK);
    assert_eq!(
        connected.1,
        json!({
            "outcome": "completed",
            "provider": "lichess",
            "username": "PlayerOne",
            "canonicalUrl": "https://lichess.org/@/PlayerOne",
            "status": "connected"
        })
    );

    let retry = request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections",
        json!({
            "profileUrl": "https://lichess.org/@/playerone/",
            "timezone": "America/New_York"
        }),
    )
    .await;
    assert_eq!(retry.0, StatusCode::OK);
    assert_eq!(validator.calls.load(Ordering::Relaxed), 1);

    let state = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching",
        Value::Null,
    )
    .await;
    assert_eq!(state.0, StatusCode::OK);
    assert_eq!(state.1["enabled"], true);
    assert_eq!(state.1["timezone"], "UTC");
    assert_eq!(
        state.1["connections"][0]["canonicalUrl"],
        "https://lichess.org/@/PlayerOne"
    );
}

#[tokio::test]
async fn dashboard_projects_setup_and_hides_unknown_digests() {
    let application = application(Arc::new(FakeProfileValidator::default()));

    let empty = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    assert_eq!(empty.0, StatusCode::OK);
    assert_eq!(
        empty.1,
        json!({
            "archive": [],
            "hostConnections": [],
            "kind": "notConnected"
        })
    );

    request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections",
        json!({
            "profileUrl": "https://lichess.org/@/PlayerOne",
            "timezone": "Europe/Oslo"
        }),
    )
    .await;
    let preparing = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    assert_eq!(preparing.0, StatusCode::OK);
    assert_eq!(preparing.1["kind"], "connected");
    assert_eq!(preparing.1["enabled"], true);
    assert_eq!(preparing.1["timezone"], "Europe/Oslo");
    assert_eq!(preparing.1["lead"]["kind"], "preparingFirstDigest");
    assert_eq!(preparing.1["archive"], json!([]));

    let missing = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/digests/daily-2026-08-09",
        Value::Null,
    )
    .await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dashboard_carries_injected_host_connections() {
    use crate::daily_coaching::{CoachingHost, CoachingHostConnection};

    let application = application_with_runtime(
        DailyCoachingRuntime::in_memory(Arc::new(FakeProfileValidator::default()), "UTC")
            .with_host_connections(vec![CoachingHostConnection {
                host: CoachingHost::Claude,
            }]),
        Arc::new(NoopExecutor),
    );

    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    assert_eq!(dashboard.0, StatusCode::OK);
    assert_eq!(
        dashboard.1,
        json!({
            "archive": [],
            "hostConnections": [{ "host": "claude" }],
            "kind": "notConnected"
        })
    );
}

#[tokio::test]
async fn dashboard_and_digest_archive_admit_web_and_coach_identities() {
    let application = application(Arc::new(FakeProfileValidator::default()));
    let coach = coach_token("daily-coaching-player");

    assert_eq!(
        request_with_token(
            &application,
            Method::GET,
            "/api/v1/daily-coaching/dashboard",
            Value::Null,
            &coach,
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request_with_token(
            &application,
            Method::GET,
            "/api/v1/daily-coaching/digests/daily-2026-08-09",
            Value::Null,
            &coach,
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        request(
            &application,
            Method::GET,
            "/api/v1/daily-coaching/dashboard",
            Value::Null,
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &application,
            Method::GET,
            "/api/v1/daily-coaching/digests/daily-2026-08-09",
            Value::Null,
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn malformed_digest_paths_are_rejected_at_the_http_boundary() {
    let application = application(Arc::new(FakeProfileValidator::default()));
    let overlong = format!("/api/v1/daily-coaching/digests/daily-{}", "9".repeat(256));
    let malformed = [
        "/api/v1/daily-coaching/digests/daily-%25".to_string(),
        "/api/v1/daily-coaching/digests/daily-%20".to_string(),
        "/api/v1/daily-coaching/digests/%2E".to_string(),
        "/api/v1/daily-coaching/digests/%2E%2E".to_string(),
        overlong,
    ];

    for uri in malformed {
        assert_eq!(
            request(&application, Method::GET, &uri, Value::Null)
                .await
                .0,
            StatusCode::NOT_FOUND,
            "malformed digest path must not reach storage: {uri}"
        );
    }
}

#[tokio::test]
async fn app_router_serves_the_published_digest_contract_in_canonical_order() {
    let executor = Arc::new(PublishingExecutor);
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(TwoGameWindowClient),
        executor.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    let connected = runtime
        .connect_at(
            &player_id,
            ConnectPlayingProfileRequest {
                profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                timezone: Some(midday_fixed_timezone(&now)),
            },
            now - TimeDelta::days(1),
        )
        .await;
    assert!(matches!(
        connected,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    let report = runtime.tick(now).await.unwrap();
    assert_eq!(report.published, 1);
    let application = application_with_runtime(runtime, executor);

    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    assert_eq!(dashboard.0, StatusCode::OK);
    assert_eq!(dashboard.1["kind"], "connected");
    assert_eq!(dashboard.1["lead"]["kind"], "digest");
    assert_eq!(dashboard.1["archive"].as_array().unwrap().len(), 1);
    assert_eq!(dashboard.1["archive"][0]["gameCount"], 2);
    assert_eq!(dashboard.1["archive"][0]["learningPathCount"], 4);
    let digest_id = dashboard.1["lead"]["digestId"].as_str().unwrap();

    let digest = request(
        &application,
        Method::GET,
        &format!("/api/v1/daily-coaching/digests/{digest_id}"),
        Value::Null,
    )
    .await;
    assert_eq!(digest.0, StatusCode::OK);
    assert_eq!(digest.1["gameCount"], 2);
    assert_eq!(digest.1["learningPathCount"], 4);
    assert_eq!(
        digest.1["games"]
            .as_array()
            .unwrap()
            .iter()
            .map(|game| game["gameImportId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "game-import:daily-http:Synthet1",
            "game-import:daily-http:Synthet2",
        ]
    );
    assert_eq!(digest.1["games"][0]["learningPathCount"], 2);
    assert_eq!(digest.1["games"][1]["learningPathCount"], 2);
    assert_eq!(digest.1["priorities"].as_array().unwrap().len(), 2);
    assert_eq!(digest.1["priorities"][0]["title"], "Learn fork");
    assert_eq!(digest.1["priorities"][0]["purpose"], "improvement");
    assert_eq!(digest.1["priorities"][0]["supportingGameCount"], 2);
    assert_eq!(
        digest.1["priorities"][0]["supportingGameImportIds"],
        json!([
            "game-import:daily-http:Synthet1",
            "game-import:daily-http:Synthet2"
        ])
    );
    assert_eq!(
        digest.1["priorities"][0]["resources"],
        json!([
            {
                "canonicalUrl": "https://lichess.org/practice/http-fork",
                "kind": "practiceModule",
                "resourceId": "resource:http-fork:learn",
                "role": "learn",
                "title": "Learn fork"
            },
            {
                "canonicalUrl": "https://lichess.org/training/http-fork",
                "kind": "puzzleStream",
                "resourceId": "resource:http-fork:drill",
                "role": "drill",
                "title": "Drill fork"
            }
        ])
    );

    let unknown = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/digests/daily-2026-08-08",
        Value::Null,
    )
    .await;
    let wrong_owner = request_with_token(
        &application,
        Method::GET,
        &format!("/api/v1/daily-coaching/digests/{digest_id}"),
        Value::Null,
        &firebase_token("another-player"),
    )
    .await;
    assert_eq!(wrong_owner, unknown);
    assert_eq!(wrong_owner.0, StatusCode::NOT_FOUND);

    let removed = request(
        &application,
        Method::DELETE,
        "/api/v1/daily-coaching/connections/lichess",
        json!({ "expectedUsername": "PlayerOne" }),
    )
    .await;
    assert_eq!(removed.0, StatusCode::OK);
    let disconnected = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    assert_eq!(disconnected.1["kind"], "notConnected");
    assert_eq!(disconnected.1["archive"][0]["digestId"], digest_id);
}

#[tokio::test]
async fn explicit_disablement_survives_replace_and_stale_removal_conflicts() {
    let application = application(Arc::new(FakeProfileValidator::default()));
    request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections",
        json!({
            "profileUrl": "https://lichess.org/@/PlayerOne",
            "timezone": "Europe/Oslo"
        }),
    )
    .await;
    let disabled = request(
        &application,
        Method::PUT,
        "/api/v1/daily-coaching/enabled",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(disabled.0, StatusCode::OK);
    let paused_dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    assert_eq!(paused_dashboard.1["lead"]["kind"], "disabled");

    let missing = request(
        &application,
        Method::PUT,
        "/api/v1/daily-coaching/connections/lichess",
        json!({
            "expectedUsername": "PlayerOne",
            "profileUrl": "https://lichess.org/@/missing"
        }),
    )
    .await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    assert_eq!(missing.1["reason"], "profileNotFound");

    let malformed = request(
        &application,
        Method::PUT,
        "/api/v1/daily-coaching/connections/lichess",
        json!({
            "expectedUsername": "PlayerOne",
            "profileUrl": "not a profile URL"
        }),
    )
    .await;
    assert_eq!(malformed.0, StatusCode::BAD_REQUEST);
    assert_eq!(malformed.1["reason"], "unparseableProfileUrl");

    let replaced = request(
        &application,
        Method::PUT,
        "/api/v1/daily-coaching/connections/lichess",
        json!({
            "expectedUsername": "PlayerOne",
            "profileUrl": "https://lichess.org/@/PlayerTwo"
        }),
    )
    .await;
    assert_eq!(replaced.0, StatusCode::OK);
    assert_eq!(replaced.1["state"]["enabled"], false);
    assert_eq!(replaced.1["state"]["timezone"], "Europe/Oslo");

    let stale = request(
        &application,
        Method::DELETE,
        "/api/v1/daily-coaching/connections/lichess",
        json!({ "expectedUsername": "PlayerOne" }),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT);
    assert_eq!(stale.1["reason"], "stalePlayingProfile");

    let removed = request(
        &application,
        Method::DELETE,
        "/api/v1/daily-coaching/connections/lichess",
        json!({ "expectedUsername": "PlayerTwo" }),
    )
    .await;
    assert_eq!(removed.0, StatusCode::OK);
    assert_eq!(removed.1["state"]["kind"], "notConnected");
    let exact_retry = request(
        &application,
        Method::DELETE,
        "/api/v1/daily-coaching/connections/lichess",
        json!({ "expectedUsername": "PlayerTwo" }),
    )
    .await;
    assert_eq!(exact_retry.0, StatusCode::OK);
}

#[tokio::test]
async fn invalid_not_found_and_unreachable_profiles_persist_nothing() {
    let application = application(Arc::new(FakeProfileValidator::default()));
    let cases = [
        (
            "not a URL",
            StatusCode::BAD_REQUEST,
            "unparseableProfileUrl",
        ),
        (
            "https://example.test/@/Player",
            StatusCode::BAD_REQUEST,
            "unsupportedProvider",
        ),
        (
            "https://lichess.org/@/missing",
            StatusCode::NOT_FOUND,
            "profileNotFound",
        ),
        (
            "https://lichess.org/@/unreachable",
            StatusCode::SERVICE_UNAVAILABLE,
            "providerUnreachable",
        ),
    ];
    for (profile_url, expected_status, expected_reason) in cases {
        let response = request(
            &application,
            Method::POST,
            "/api/v1/daily-coaching/connections",
            json!({ "profileUrl": profile_url }),
        )
        .await;
        assert_eq!(response.0, expected_status, "{profile_url}");
        assert_eq!(response.1["reason"], expected_reason, "{profile_url}");
    }

    let state = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching",
        Value::Null,
    )
    .await;
    assert_eq!(state.1, json!({ "kind": "notConnected" }));
}

#[tokio::test]
async fn profile_check_reports_each_feed_class_and_persists_only_health_changes() {
    let profile_client = Arc::new(ControllableProfileGameClient(AtomicUsize::new(0)));
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        profile_client.clone(),
        Arc::new(NoopExecutor),
    );
    let application = application_with_runtime(runtime, Arc::new(NoopExecutor));
    request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections",
        json!({
            "profileUrl": "https://lichess.org/@/PlayerOne",
            "timezone": "UTC"
        }),
    )
    .await;

    let missing = request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections/lichess/check",
        json!({ "expectedUsername": "PlayerOne" }),
    )
    .await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    assert_eq!(
        missing.1,
        json!({ "outcome": "profileUnavailable", "provider": "lichess" })
    );
    let unavailable = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    assert_eq!(unavailable.1["lead"]["kind"], "profileUnavailable");
    assert_eq!(
        unavailable.1["connections"][0]["status"],
        "profileUnavailable"
    );

    profile_client.set_outcome(1);
    let transient = request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections/lichess/check",
        json!({ "expectedUsername": "PlayerOne" }),
    )
    .await;
    assert_eq!(transient.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(transient.1["outcome"], "providerUnavailable");
    assert_eq!(
        request(
            &application,
            Method::GET,
            "/api/v1/daily-coaching/dashboard",
            Value::Null,
        )
        .await
        .1["connections"][0]["status"],
        "profileUnavailable"
    );

    profile_client.set_outcome(2);
    let reachable = request(
        &application,
        Method::POST,
        "/api/v1/daily-coaching/connections/lichess/check",
        json!({ "expectedUsername": "PlayerOne" }),
    )
    .await;
    assert_eq!(reachable.0, StatusCode::OK);
    assert_eq!(
        reachable.1,
        json!({ "outcome": "reachable", "provider": "lichess" })
    );
    assert_eq!(
        request(
            &application,
            Method::GET,
            "/api/v1/daily-coaching/dashboard",
            Value::Null,
        )
        .await
        .1["connections"][0]["status"],
        "connected"
    );
}

#[tokio::test]
async fn arrival_nudge_submits_two_single_game_imports_sequentially_to_the_shared_executor() {
    let (submission_sender, mut submissions) = mpsc::unbounded_channel();
    let executor = Arc::new(RecordingExecutor {
        submissions: submission_sender,
    });
    let daily_executor: Arc<dyn ReviewSessionCommandExecutor> = executor.clone();
    let review_session_executor: Arc<dyn ReviewSessionCommandExecutor> = executor;
    assert!(Arc::ptr_eq(&daily_executor, &review_session_executor));

    let runtime = DailyCoachingRuntime::in_memory_with_pipeline(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(TwoGameWindowClient),
        daily_executor,
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    let connected = runtime
        .connect_at(
            &player_id,
            ConnectPlayingProfileRequest {
                profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                timezone: Some(midday_fixed_timezone(&now)),
            },
            now - TimeDelta::days(1),
        )
        .await;
    assert!(matches!(
        connected,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    let application = application_with_runtime(runtime, review_session_executor);

    let response = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching",
        Value::Null,
    )
    .await;
    assert_eq!(response.0, StatusCode::OK);

    let first = next_submission(&mut submissions).await;
    assert_daily_import(&first, &player_id, "https://lichess.org/Synthet1");
    assert!(matches!(
        submissions.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));

    first.reject();
    let second = next_submission(&mut submissions).await;
    assert!(first.events.is_closed());
    assert_daily_import(&second, &player_id, "https://lichess.org/Synthet2");
    assert_ne!(
        first.envelope.operation_id.as_str(),
        second.envelope.operation_id.as_str()
    );
    assert!(matches!(
        submissions.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));

    second.reject();
    tokio::time::timeout(Duration::from_secs(5), second.events.closed())
        .await
        .expect("the second one-Game review should finish");
}

fn application(validator: Arc<FakeProfileValidator>) -> Router {
    application_with_runtime(
        DailyCoachingRuntime::in_memory(validator, "UTC"),
        Arc::new(NoopExecutor),
    )
}

fn application_with_runtime(
    daily_coaching: DailyCoachingRuntime,
    review_session_executor: Arc<dyn ReviewSessionCommandExecutor>,
) -> Router {
    application_with_runtimes(
        BetaAccessRuntime::disabled(),
        daily_coaching,
        review_session_executor,
    )
}

fn application_with_runtimes(
    beta_access: BetaAccessRuntime,
    daily_coaching: DailyCoachingRuntime,
    review_session_executor: Arc<dyn ReviewSessionCommandExecutor>,
) -> Router {
    application_with_all_runtimes(
        AccountDeletionRuntime::disabled(),
        beta_access,
        daily_coaching,
        review_session_executor,
    )
}

fn application_with_all_runtimes(
    account_deletion: AccountDeletionRuntime,
    beta_access: BetaAccessRuntime,
    daily_coaching: DailyCoachingRuntime,
    review_session_executor: Arc<dyn ReviewSessionCommandExecutor>,
) -> Router {
    application_with_opening_analysis_runtimes(
        account_deletion,
        beta_access,
        daily_coaching,
        crate::opening_analysis::OpeningAnalysisRuntime::disabled(),
        review_session_executor,
    )
}

fn application_with_opening_analysis(
    opening_analysis: crate::opening_analysis::OpeningAnalysisRuntime,
) -> Router {
    application_with_opening_analysis_runtimes(
        AccountDeletionRuntime::disabled(),
        BetaAccessRuntime::disabled(),
        DailyCoachingRuntime::disabled(),
        opening_analysis,
        Arc::new(NoopExecutor),
    )
}

fn application_with_opening_analysis_runtimes(
    account_deletion: AccountDeletionRuntime,
    beta_access: BetaAccessRuntime,
    daily_coaching: DailyCoachingRuntime,
    opening_analysis: crate::opening_analysis::OpeningAnalysisRuntime,
    review_session_executor: Arc<dyn ReviewSessionCommandExecutor>,
) -> Router {
    crate::app(Arc::new(AppState {
        account_deletion,
        auth: AuthConfig::new_firebase(FIREBASE_PROJECT_ID, jwt_jwks())
            .unwrap()
            .with_coach_mcp(jwt_jwks(), COACH_ISSUER, COACH_RESOURCE, COACH_SCOPE)
            .unwrap(),
        beta_access,
        daily_coaching,
        imported_games: crate::imported_games::ImportedGamesRuntime::in_memory(),
        opening_analysis,
        review_session: ReviewSessionWebBinding::new(review_session_executor),
    }))
}

async fn request(
    application: &Router,
    method: Method,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let token = firebase_token("daily-coaching-player");
    request_with_token(application, method, uri, body, &token).await
}

async fn request_with_token(
    application: &Router,
    method: Method,
    uri: &str,
    body: Value,
    token: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method.clone())
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let body = if method == Method::GET {
        Body::empty()
    } else {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    };
    let response = application
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, value)
}

#[derive(Default)]
struct FakeProfileValidator {
    calls: AtomicUsize,
}

impl PublicProfileValidator for FakeProfileValidator {
    fn validate<'a>(
        &'a self,
        profile: &'a PublicChessProfile,
    ) -> crate::profile_game_feed::ProfileValidationFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            match profile.username().to_ascii_lowercase().as_str() {
                "missing" => Err(ProfileValidationError::ProfileNotFound),
                "unreachable" => Err(ProfileValidationError::ProviderUnavailable {
                    retry_after_seconds: None,
                }),
                _ => ValidatedPublicChessProfile::from_provider_username(
                    profile.provider(),
                    profile.username(),
                )
                .map_err(|_| ProfileValidationError::MalformedProviderResponse),
            }
        })
    }
}

struct TwoGameWindowClient;

struct ControllableProfileGameClient(AtomicUsize);

impl ControllableProfileGameClient {
    fn set_outcome(&self, outcome: usize) {
        self.0.store(outcome, Ordering::SeqCst);
    }
}

impl ProfileGameClient for ControllableProfileGameClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        let outcome = self.0.load(Ordering::SeqCst);
        Box::pin(async move {
            match outcome {
                0 => Err(ProfileGameFetchError::Status {
                    provider: request.provider(),
                    code: 404,
                    retry_after_seconds: None,
                }),
                1 => Err(ProfileGameFetchError::Status {
                    provider: request.provider(),
                    code: 503,
                    retry_after_seconds: Some(120),
                }),
                _ => Ok(ProfileGameResponse {
                    body: Vec::new(),
                    content_type: request.accept().to_string(),
                }),
            }
        })
    }
}

impl ProfileGameClient for TwoGameWindowClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = reqwest::Url::parse(request.url()).expect("the profile request URL is valid");
            let until = url
                .query_pairs()
                .find_map(|(name, value)| (name == "until").then_some(value))
                .map(|value| {
                    value
                        .parse::<u64>()
                        .expect("the window upper bound is milliseconds")
                })
                .unwrap_or_else(|| u64::try_from(Utc::now().timestamp_millis()).unwrap());
            let body = [
                ("Synthet1Demo", until - 60 * 60 * 1_000),
                ("Synthet2Demo", until - 2 * 60 * 60 * 1_000),
            ]
            .into_iter()
            .map(|(id, ended_at)| {
                json!({
                    "id": id,
                    "variant": "standard",
                    "status": "mate",
                    "speed": "rapid",
                    "clock": { "initial": 600, "increment": 0 },
                    "moves": lichess_moves(90),
                    "lastMoveAt": ended_at,
                    "players": {
                        "white": { "userId": "Opponent" },
                        "black": { "userId": "PlayerOne" }
                    }
                })
                .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes();
            Ok(ProfileGameResponse {
                body,
                content_type: request.accept().to_string(),
            })
        })
    }
}

struct RecordingExecutor {
    submissions: mpsc::UnboundedSender<RecordedSubmission>,
}

struct PublishingExecutor;

impl ReviewSessionCommandExecutor for PublishingExecutor {
    fn submit(
        self: Arc<Self>,
        _principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let envelope = admission
            .envelope()
            .expect("Daily Coaching submits a valid import command")
            .clone();
        let ReviewSessionCommand::ImportGame {
            source: GameInputSource::LichessUrl { url },
            ..
        } = &envelope.command
        else {
            panic!("the publishing fixture only accepts Lichess imports")
        };
        let game_id = url.rsplit('/').next().unwrap();
        let game_import_id =
            GameImportId::try_from(format!("game-import:daily-http:{game_id}")).unwrap();
        let (sender, receiver) = mpsc::unbounded_channel();
        sender
            .send(ReviewSessionEventEnvelope {
                request_id: envelope.request_id,
                operation_id: envelope.operation_id,
                sequence: 0,
                event: ReviewSessionEvent::Completed {
                    result: Box::new(OperationCompletion::GameImported {
                        game_import_id,
                        review: Box::new(published_review(game_id)),
                        timing: None,
                        imported_game: Some(Box::new(published_imported_game(game_id))),
                    }),
                },
            })
            .unwrap();
        receiver
    }
}

fn published_imported_game(game_id: &str) -> ImportedGame {
    let mut imported: ImportedGame = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
    )))
    .unwrap();
    let ImportProvenance::Lichess {
        canonical_game_id,
        side_qualified_url,
        canonical_url,
        ..
    } = &mut imported.provenance
    else {
        panic!("the imported Game fixture must use Lichess provenance")
    };
    *canonical_game_id = CanonicalGameId::try_from(game_id.to_string()).unwrap();
    *side_qualified_url = format!("https://lichess.org/{game_id}0000/black");
    *canonical_url = format!("https://lichess.org/{game_id}");
    imported
}

fn published_review(game_id: &str) -> GameReview {
    let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/events.json"
    )))
    .unwrap();
    let mut review = events
        .into_iter()
        .find_map(|envelope| match envelope.event {
            ReviewSessionEvent::Completed { result } => match *result {
                OperationCompletion::GameImported { review, .. } => Some(*review),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    review.learning_plan = LearningPlan {
        selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
        resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
        tracks: vec![
            LearningTrack {
                key: LearningTrackKey::Curriculum {
                    concept: CurriculumLearningConcept::Fork,
                },
                support: vec![LearningTrackSupport::Improvement {
                    learning_path_ref: LearningPathRef::try_from(format!(
                        "learning-path:http:{game_id}"
                    ))
                    .unwrap(),
                    critical_moment_id: CriticalMomentId::try_from(format!(
                        "critical-moment:http:{game_id}"
                    ))
                    .unwrap(),
                    ply: 10,
                    basis: LearningTrackSupportBasis::DecisionExplanation {
                        explanation_path_ref: ExplanationPathRef::from_content(&game_id),
                    },
                }],
                resources: vec![
                    LearningResource {
                        resource_id: LearningResourceId::try_from(
                            "resource:http-fork:learn".to_string(),
                        )
                        .unwrap(),
                        role: LearningResourceRole::Learn,
                        kind: LearningResourceKind::PracticeModule,
                        title: "Learn fork".to_string(),
                        canonical_url: "https://lichess.org/practice/http-fork".to_string(),
                    },
                    LearningResource {
                        resource_id: LearningResourceId::try_from(
                            "resource:http-fork:drill".to_string(),
                        )
                        .unwrap(),
                        role: LearningResourceRole::Drill,
                        kind: LearningResourceKind::PuzzleStream,
                        title: "Drill fork".to_string(),
                        canonical_url: "https://lichess.org/training/http-fork".to_string(),
                    },
                ],
            },
            LearningTrack {
                key: LearningTrackKey::Curriculum {
                    concept: CurriculumLearningConcept::HangingPiece,
                },
                support: vec![LearningTrackSupport::Reinforcement {
                    learning_path_ref: LearningPathRef::try_from(format!(
                        "learning-path:http-hanging-piece:{game_id}"
                    ))
                    .unwrap(),
                    critical_moment_id: CriticalMomentId::try_from(format!(
                        "critical-moment:http-hanging-piece:{game_id}"
                    ))
                    .unwrap(),
                    ply: 12,
                    basis: LearningTrackSupportBasis::DecisionExplanation {
                        explanation_path_ref: ExplanationPathRef::from_content(&format!(
                            "hanging-piece:{game_id}"
                        )),
                    },
                }],
                resources: vec![
                    LearningResource {
                        resource_id: LearningResourceId::try_from(
                            "resource:http-hanging-piece:learn".to_string(),
                        )
                        .unwrap(),
                        role: LearningResourceRole::Learn,
                        kind: LearningResourceKind::PracticeModule,
                        title: "Learn hanging pieces".to_string(),
                        canonical_url: "https://lichess.org/practice/http-hanging-piece"
                            .to_string(),
                    },
                    LearningResource {
                        resource_id: LearningResourceId::try_from(
                            "resource:http-hanging-piece:drill".to_string(),
                        )
                        .unwrap(),
                        role: LearningResourceRole::Drill,
                        kind: LearningResourceKind::PuzzleStream,
                        title: "Drill hanging pieces".to_string(),
                        canonical_url: "https://lichess.org/training/http-hanging-piece"
                            .to_string(),
                    },
                ],
            },
        ],
    };
    review
}

struct RecordedSubmission {
    principal: ProcessorPrincipal,
    envelope: ReviewSessionCommandEnvelope,
    events: mpsc::UnboundedSender<ReviewSessionEventEnvelope>,
}

impl RecordedSubmission {
    fn reject(&self) {
        self.events
            .send(ReviewSessionEventEnvelope {
                request_id: self.envelope.request_id.clone(),
                operation_id: self.envelope.operation_id.clone(),
                sequence: 0,
                event: ReviewSessionEvent::Rejected {
                    operation: OperationKind::GameImport,
                    reason: CommandRejectionReason::InvalidCommand,
                    recovery: RejectionRecovery::None,
                },
            })
            .expect("the Daily Coaching reviewer is awaiting this response");
    }
}

impl ReviewSessionCommandExecutor for RecordingExecutor {
    fn submit(
        self: Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let envelope = admission
            .envelope()
            .expect("Daily Coaching submits a valid command")
            .clone();
        let (events, receiver) = mpsc::unbounded_channel();
        self.submissions
            .send(RecordedSubmission {
                principal,
                envelope,
                events,
            })
            .expect("the route journey is awaiting submissions");
        receiver
    }
}

async fn next_submission(
    submissions: &mut mpsc::UnboundedReceiver<RecordedSubmission>,
) -> RecordedSubmission {
    tokio::time::timeout(Duration::from_secs(5), submissions.recv())
        .await
        .expect("Daily Coaching should submit the next one-Game import")
        .expect("the shared executor should remain available")
}

fn assert_daily_import(submission: &RecordedSubmission, player_id: &PlayerId, expected_url: &str) {
    assert_eq!(
        submission.principal,
        ProcessorPrincipal::Player(player_id.clone())
    );
    assert_eq!(submission.envelope.surface, DeliverySurface::CoachApp);
    assert_eq!(
        submission.envelope.command,
        ReviewSessionCommand::ImportGame {
            source: GameInputSource::LichessUrl {
                url: expected_url.to_string(),
            },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::Black,
            },
            elo_profile: RequestedEloProfile::FromImportedMetadata,
        }
    );
}

fn midday_fixed_timezone(now: &DateTime<Utc>) -> String {
    let offset_hours = 12 - i32::try_from(now.hour()).expect("an hour fits in i32");
    match offset_hours.cmp(&0) {
        std::cmp::Ordering::Less => format!("Etc/GMT+{}", -offset_hours),
        std::cmp::Ordering::Equal => "Etc/GMT".to_string(),
        std::cmp::Ordering::Greater => format!("Etc/GMT-{offset_hours}"),
    }
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
