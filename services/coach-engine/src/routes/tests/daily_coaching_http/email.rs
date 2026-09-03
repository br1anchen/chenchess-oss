use ring::hmac;

use super::*;
use crate::{
    beta_access::{BetaAccessRuntime, InMemoryBetaAccessStore, NormalizedEmail},
    daily_coaching::{
        delivery::{
            DigestEmailDelivery, DigestEmailDeliveryError, DigestEmailReceipt, DigestEmailRequest,
            DigestEmailStore, EmailDeliveryFuture, InMemoryDigestEmailStore,
        },
        CheckPlayingProfileOutcome, CheckPlayingProfileRequest, DailyCoachingMutationOutcome,
        DailyCoachingMutationRejectionReason, DailyCoachingOwnerKey, DailyCoachingProvider,
    },
    profile_game_feed::ChessProfileProvider,
    routes::firebase_token_test_support::{
        administrator_token, firebase_token_with_email, verified_firebase_token,
    },
};
use axum::{extract::Path, response::IntoResponse, routing::get, Json};

const BETA_TEST_KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn forced_digest_regeneration_requires_an_administrator() {
    let RedeemedBetaPlayerFixture {
        application,
        request_id,
        ..
    } = redeemed_beta_player_fixture(AccountDeletionRuntime::disabled()).await;
    let player_token = administrator_token("firebase-player", true, Some(false));

    let response = application
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/admin/beta-access/requests/{request_id}/daily-coaching/digest/regenerate"
                ))
                .header(header::AUTHORIZATION, format!("Bearer {player_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // A non-administrator token is rejected before the route resolves anything, matching the
    // sibling Beta Access admin commands.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn forced_digest_regeneration_conflicts_without_a_terminal_window() {
    let RedeemedBetaPlayerFixture {
        application,
        request_id,
        ..
    } = redeemed_beta_player_fixture(AccountDeletionRuntime::disabled()).await;
    let token = administrator_token("firebase-administrator", true, Some(true));

    // The Player has never had a Run reach a terminal digest outcome, so there is nothing to
    // rebuild and the action reports unavailable rather than inventing a window.
    let response = application
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/admin/beta-access/requests/{request_id}/daily-coaching/digest/regenerate"
                ))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn forced_digest_regeneration_rejects_an_unknown_beta_access_request() {
    let RedeemedBetaPlayerFixture { application, .. } =
        redeemed_beta_player_fixture(AccountDeletionRuntime::disabled()).await;
    let token = administrator_token("firebase-administrator", true, Some(true));

    let response = application
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/admin/beta-access/requests/{}/daily-coaching/digest/regenerate",
                    "0".repeat(64)
                ))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // An unknown Beta Access Request is resolved before any Daily Coaching state is read, and
    // reports not-found exactly as the sibling administration commands do.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn administrator_listing_projects_the_redeemed_players_latest_digest() {
    let RedeemedBetaPlayerFixture { application, .. } = redeemed_beta_digest_fixture().await;
    let token = administrator_token("firebase-administrator", true, Some(true));

    let response = request_with_token(
        &application,
        Method::GET,
        "/api/v1/admin/beta-access/requests",
        json!({}),
        &token,
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(
        response.1["requests"][0]["dailyCoaching"],
        json!({
            "latestDigest": {
                "coverageDate": response.1["requests"][0]["dailyCoaching"]["latestDigest"]["coverageDate"],
                "gameCount": 2,
                "learningPathCount": 4,
                "publishedAt": response.1["requests"][0]["dailyCoaching"]["latestDigest"]["publishedAt"],
            },
            "status": "ready",
        })
    );
    assert!(response.1["requests"][0].get("playerId").is_none());
}

#[tokio::test]
async fn administrator_starts_a_manual_digest_run_and_its_normal_email_pipeline() {
    let RedeemedBetaPlayerFixture {
        application,
        runtime,
        email_delivery,
        request_id,
        now,
        ..
    } = redeemed_beta_player_fixture(AccountDeletionRuntime::disabled()).await;
    let token = administrator_token("firebase-administrator", true, Some(true));
    let listed = request_with_token(
        &application,
        Method::GET,
        "/api/v1/admin/beta-access/requests",
        json!({}),
        &token,
    )
    .await;

    assert_eq!(
        listed.1["requests"][0]["dailyCoaching"],
        json!({ "status": "noDigest" })
    );

    let response = application
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/admin/beta-access/requests/{request_id}/daily-coaching/digest/trigger"
                ))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let delivery = tokio::time::timeout(Duration::from_secs(5), async {
        while email_delivery.sent.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        delivery.is_ok(),
        "the Manual Digest Run should publish and hand off the due Daily Window; dashboard: {:?}",
        runtime
            .dashboard(&PlayerId::try_from("test-redeemed-player".to_string()).unwrap())
            .await
            .unwrap()
    );
    assert_eq!(
        email_delivery.sent.lock().unwrap()[0].delivery_id,
        format!("daily-{}", (now - TimeDelta::days(1)).date_naive())
    );
}

#[tokio::test]
async fn manual_digest_run_requires_a_verified_email() {
    let RedeemedBetaPlayerFixture {
        application,
        runtime,
        email_delivery,
        request_id,
        ..
    } = redeemed_beta_player_fixture(AccountDeletionRuntime::disabled()).await;
    let player_id = PlayerId::try_from("test-redeemed-player".to_string()).unwrap();
    runtime
        .observe_verified_email(&player_id, None)
        .await
        .unwrap();

    let response = application
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/admin/beta-access/requests/{request_id}/daily-coaching/digest/trigger"
                ))
                .header(
                    header::AUTHORIZATION,
                    format!(
                        "Bearer {}",
                        administrator_token("firebase-administrator", true, Some(true))
                    ),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(email_delivery.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn manual_digest_run_requires_an_empty_digest_archive() {
    let RedeemedBetaPlayerFixture {
        application,
        email_delivery,
        request_id,
        ..
    } = redeemed_beta_digest_fixture().await;

    let response = application
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/admin/beta-access/requests/{request_id}/daily-coaching/digest/trigger"
                ))
                .header(
                    header::AUTHORIZATION,
                    format!(
                        "Bearer {}",
                        administrator_token("firebase-administrator", true, Some(true))
                    ),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(email_delivery.sent.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn administrator_cannot_rebuild_a_digest_after_beta_access_is_revoked() {
    let RedeemedBetaPlayerFixture {
        application,
        email_delivery,
        request_id,
        beta_access,
        ..
    } = redeemed_beta_digest_fixture().await;
    assert_eq!(
        beta_access.revoke_access(&request_id).await.unwrap(),
        crate::beta_access::BetaAccessAuthorizationRevokeResult::Revoked
    );

    let response = application
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/admin/beta-access/requests/{request_id}/daily-coaching/digest/regenerate"
                ))
                .header(
                    header::AUTHORIZATION,
                    format!(
                        "Bearer {}",
                        administrator_token("firebase-administrator", true, Some(true))
                    ),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(email_delivery.sent.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn administrator_listing_survives_a_purged_live_access_grant() {
    let RedeemedBetaPlayerFixture {
        application,
        beta_store,
        ..
    } = redeemed_beta_digest_fixture().await;
    beta_store.remove_access_grant("test-redeemed-player");

    let response = request_with_token(
        &application,
        Method::GET,
        "/api/v1/admin/beta-access/requests",
        json!({}),
        &administrator_token("firebase-administrator", true, Some(true)),
    )
    .await;

    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.1["requests"][0]["accessStatus"], "active");
    assert_eq!(
        response.1["requests"][0]["dailyCoaching"],
        json!({ "status": "unavailable" })
    );
}

#[tokio::test]
async fn administrator_cannot_rebuild_a_digest_after_account_deletion_starts() {
    let target_document_id =
        crate::review_durability::path::hashed_path_segment("test-redeemed-player");
    let marker_service = Router::new().route(
        "/v1/projects/chenchess-test/databases/coach-app-production/documents/deletedUsers/:document_id",
        get(move |Path(document_id): Path<String>| {
            let target_document_id = target_document_id.clone();
            async move {
                if document_id != target_document_id {
                    return StatusCode::NOT_FOUND.into_response();
                }
                Json(json!({
                "name": format!(
                    "projects/chenchess-test/databases/coach-app-production/documents/deletedUsers/{document_id}"
                ),
                "fields": {
                    "schemaVersion": { "integerValue": "1" },
                    "playerId": { "stringValue": "test-redeemed-player" },
                    "startedAt": { "timestampValue": "2026-08-01T10:00:00Z" },
                    "phase": { "stringValue": "markersWritten" },
                },
                }))
                .into_response()
            }
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
    let RedeemedBetaPlayerFixture {
        application,
        email_delivery,
        request_id,
        ..
    } = redeemed_beta_digest_fixture_with_account_deletion(account_deletion).await;
    let listed = request_with_token(
        &application,
        Method::GET,
        "/api/v1/admin/beta-access/requests",
        json!({}),
        &administrator_token("firebase-administrator", true, Some(true)),
    )
    .await;

    assert_eq!(listed.0, StatusCode::OK);
    assert_eq!(
        listed.1["requests"][0]["dailyCoaching"],
        json!({ "status": "unavailable" })
    );

    let response = application
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/admin/beta-access/requests/{request_id}/daily-coaching/digest/regenerate"
                ))
                .header(
                    header::AUTHORIZATION,
                    format!(
                        "Bearer {}",
                        administrator_token("firebase-administrator", true, Some(true))
                    ),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(email_delivery.sent.lock().unwrap().len(), 1);
    server.abort();
}

struct RedeemedBetaPlayerFixture {
    application: Router,
    runtime: DailyCoachingRuntime,
    email_delivery: Arc<RecordingEmailDelivery>,
    request_id: String,
    beta_access: BetaAccessRuntime,
    beta_store: Arc<InMemoryBetaAccessStore>,
    now: DateTime<Utc>,
}

async fn redeemed_beta_digest_fixture() -> RedeemedBetaPlayerFixture {
    redeemed_beta_digest_fixture_with_account_deletion(AccountDeletionRuntime::disabled()).await
}

async fn redeemed_beta_digest_fixture_with_account_deletion(
    account_deletion: AccountDeletionRuntime,
) -> RedeemedBetaPlayerFixture {
    let fixture = redeemed_beta_player_fixture(account_deletion).await;
    assert_eq!(
        fixture.runtime.tick(fixture.now).await.unwrap().published,
        1
    );
    fixture
}

async fn redeemed_beta_player_fixture(
    account_deletion: AccountDeletionRuntime,
) -> RedeemedBetaPlayerFixture {
    let executor = Arc::new(PublishingExecutor);
    let email_delivery = Arc::new(RecordingEmailDelivery::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline_and_email(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(TwoGameWindowClient),
        executor.clone(),
        Arc::new(InMemoryDigestEmailStore::default()),
        email_delivery.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("test-redeemed-player".to_string()).unwrap();
    assert!(matches!(
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                    timezone: Some(midday_fixed_timezone(&now)),
                },
                now - TimeDelta::days(1),
            )
            .await,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    let email = NormalizedEmail::parse("player@example.com").unwrap();
    runtime
        .observe_verified_email(&player_id, Some(&email))
        .await
        .unwrap();

    let beta_store = Arc::new(InMemoryBetaAccessStore::default());
    let beta_access = BetaAccessRuntime::in_memory(beta_store.clone(), BETA_TEST_KEY).unwrap();
    beta_access
        .submit(email, "203.0.113.80".parse().unwrap(), now)
        .await
        .unwrap();
    beta_store.grant("player@example.com");
    beta_store.mark_invitation_redeemed();
    let request_id = beta_store.request_id();

    let application = application_with_all_runtimes(
        account_deletion,
        beta_access.clone(),
        runtime.clone(),
        executor,
    );
    RedeemedBetaPlayerFixture {
        application,
        runtime,
        email_delivery,
        request_id,
        beta_access,
        beta_store,
        now,
    }
}

#[tokio::test]
async fn profile_unavailable_email_sends_once_per_health_epoch_and_honors_opt_out() {
    let email_store = Arc::new(InMemoryDigestEmailStore::default());
    let email_delivery = Arc::new(RecordingEmailDelivery::default());
    let profile_client = Arc::new(ControllableProfileGameClient(AtomicUsize::new(0)));
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline_and_email(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        profile_client.clone(),
        Arc::new(NoopExecutor),
        email_store,
        email_delivery.clone(),
    );
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    let email = crate::beta_access::NormalizedEmail::parse("player@example.com").unwrap();
    let connected_at = DateTime::parse_from_rfc3339("2026-08-09T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    assert!(matches!(
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                    timezone: Some("UTC".to_string()),
                },
                connected_at,
            )
            .await,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    runtime
        .observe_verified_email(&player_id, Some(&email))
        .await
        .unwrap();
    let request = || CheckPlayingProfileRequest {
        expected_username: "PlayerOne".to_string(),
    };

    assert!(matches!(
        runtime
            .check_profile_at(
                &player_id,
                DailyCoachingProvider::Lichess,
                request(),
                connected_at + TimeDelta::minutes(1),
            )
            .await,
        CheckPlayingProfileOutcome::ProfileUnavailable { .. }
    ));
    runtime
        .check_profile_at(
            &player_id,
            DailyCoachingProvider::Lichess,
            request(),
            connected_at + TimeDelta::minutes(2),
        )
        .await;
    {
        let sent = email_delivery.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].rendered.text.contains("Daily Coaching is paused"));
        assert!(sent[0]
            .rendered
            .text
            .contains("Update your profile link: https://beta.chenchess.test/dashboard/"));
        assert!(!sent[0]
            .rendered
            .text
            .lines()
            .next()
            .unwrap()
            .contains("404"));
    }

    profile_client.set_outcome(2);
    runtime
        .check_profile_at(
            &player_id,
            DailyCoachingProvider::Lichess,
            request(),
            connected_at + TimeDelta::minutes(3),
        )
        .await;
    profile_client.set_outcome(0);
    runtime
        .check_profile_at(
            &player_id,
            DailyCoachingProvider::Lichess,
            request(),
            connected_at + TimeDelta::minutes(4),
        )
        .await;
    {
        let sent = email_delivery.sent.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_ne!(sent[0].delivery_id, sent[1].delivery_id);
    }

    assert!(matches!(
        runtime
            .set_digest_email_enabled(&player_id, Some(&email), false)
            .await,
        DailyCoachingMutationOutcome::Completed { .. }
    ));
    profile_client.set_outcome(2);
    runtime
        .check_profile_at(
            &player_id,
            DailyCoachingProvider::Lichess,
            request(),
            connected_at + TimeDelta::minutes(5),
        )
        .await;
    profile_client.set_outcome(0);
    runtime
        .check_profile_at(
            &player_id,
            DailyCoachingProvider::Lichess,
            request(),
            connected_at + TimeDelta::minutes(6),
        )
        .await;
    assert_eq!(email_delivery.sent.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn profile_unavailable_email_waits_for_all_feeds_then_sends_once_per_connection() {
    let email_delivery = Arc::new(RecordingEmailDelivery::default());
    let profile_client = Arc::new(PerProviderProfileGameClient::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline_and_email(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        profile_client.clone(),
        Arc::new(NoopExecutor),
        Arc::new(InMemoryDigestEmailStore::default()),
        email_delivery.clone(),
    );
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    let connected_at = DateTime::parse_from_rfc3339("2026-08-09T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    for profile_url in [
        "https://lichess.org/@/PlayerOne",
        "https://www.chess.com/member/PlayerOne",
    ] {
        assert!(matches!(
            runtime
                .connect_at(
                    &player_id,
                    ConnectPlayingProfileRequest {
                        profile_url: profile_url.to_string(),
                        timezone: Some("UTC".to_string()),
                    },
                    connected_at,
                )
                .await,
            ConnectPlayingProfileOutcome::Completed { .. }
        ));
    }
    runtime
        .observe_verified_email(
            &player_id,
            Some(&crate::beta_access::NormalizedEmail::parse("player@example.com").unwrap()),
        )
        .await
        .unwrap();
    let check = || CheckPlayingProfileRequest {
        expected_username: "PlayerOne".to_string(),
    };

    profile_client.fail(DailyCoachingProvider::Lichess);
    assert!(matches!(
        runtime
            .check_profile_at(
                &player_id,
                DailyCoachingProvider::Lichess,
                check(),
                connected_at + TimeDelta::minutes(1),
            )
            .await,
        CheckPlayingProfileOutcome::ProfileUnavailable { .. }
    ));
    assert!(email_delivery.sent.lock().unwrap().is_empty());

    profile_client.fail(DailyCoachingProvider::ChessCom);
    assert!(matches!(
        runtime
            .check_profile_at(
                &player_id,
                DailyCoachingProvider::ChessCom,
                check(),
                connected_at + TimeDelta::minutes(2),
            )
            .await,
        CheckPlayingProfileOutcome::ProfileUnavailable { .. }
    ));
    runtime
        .check_profile_at(
            &player_id,
            DailyCoachingProvider::ChessCom,
            check(),
            connected_at + TimeDelta::minutes(3),
        )
        .await;

    let sent = email_delivery.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert_ne!(sent[0].delivery_id, sent[1].delivery_id);
    assert!(sent
        .iter()
        .any(|message| message.rendered.text.contains("Lichess profile")));
    assert!(sent
        .iter()
        .any(|message| message.rendered.text.contains("Chess.com profile")));
}

#[tokio::test]
async fn verified_account_digest_email_is_learning_first_exactly_once_and_unsubscribable() {
    let executor = Arc::new(PublishingExecutor);
    let email_store = Arc::new(InMemoryDigestEmailStore::default());
    let email_delivery = Arc::new(RecordingEmailDelivery::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline_and_email(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(TwoGameWindowClient),
        executor.clone(),
        email_store.clone(),
        email_delivery.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    assert!(matches!(
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                    timezone: Some(midday_fixed_timezone(&now)),
                },
                now - TimeDelta::days(1),
            )
            .await,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    runtime
        .observe_verified_email(
            &player_id,
            Some(&crate::beta_access::NormalizedEmail::parse("Player@Example.COM").unwrap()),
        )
        .await
        .unwrap();
    let application = application_with_runtime(runtime.clone(), executor);
    let coach = coach_token("daily-coaching-player");
    assert_eq!(
        request_with_token(
            &application,
            Method::GET,
            "/api/v1/daily-coaching",
            Value::Null,
            &coach,
        )
        .await
        .0,
        StatusCode::OK
    );
    let owner = DailyCoachingOwnerKey::for_player(&player_id);
    assert!(email_store
        .preference(&owner)
        .await
        .unwrap()
        .unwrap()
        .can_receive());

    assert_eq!(runtime.tick(now).await.unwrap().published, 1);
    assert_eq!(runtime.tick(now).await.unwrap().published, 0);
    let unsubscribe_url = {
        let sent = email_delivery.sent.lock().unwrap();
        assert_eq!(sent.len(), 1, "a completed Run must never send twice");
        let message = &sent[0];
        assert_eq!(message.recipient.as_str(), "player@example.com");
        assert!(message.rendered.text.contains("1. Learn fork — Improve"));
        assert!(message
            .rendered
            .text
            .contains("2. Learn hanging pieces — Reinforce"));
        assert!(
            message.rendered.text.find("1. Learn fork").unwrap()
                < message
                    .rendered
                    .text
                    .find("2. Learn hanging pieces")
                    .unwrap()
        );
        assert!(message
            .rendered
            .text
            .contains("Learn: Learn fork — https://lichess.org/practice/http-fork"));
        assert!(message
            .rendered
            .text
            .contains("Drill: Drill fork — https://lichess.org/training/http-fork"));
        assert!(message.rendered.text.contains(
            "Learn: Learn hanging pieces — https://lichess.org/practice/http-hanging-piece"
        ));
        assert!(message.rendered.text.contains(
            "Drill: Drill hanging pieces — https://lichess.org/training/http-hanging-piece"
        ));
        assert!(message.rendered.text.contains("Spotted in 2 of your games"));
        assert!(message
            .rendered
            .text
            .contains(&format!("/dashboard/#digest={}", message.delivery_id)));
        assert!(!message.rendered.text.contains("/game-reviews/"));
        message.unsubscribe_url.clone()
    };

    let unsubscribe_path = unsubscribe_url
        .strip_prefix("https://beta.chenchess.test")
        .unwrap();
    assert_eq!(
        request_public_status(&application, Method::GET, unsubscribe_path).await,
        StatusCode::OK
    );
    assert!(email_store
        .preference(&owner)
        .await
        .unwrap()
        .unwrap()
        .can_receive());
    assert_eq!(
        request_public_status(&application, Method::POST, unsubscribe_path).await,
        StatusCode::OK
    );
    assert!(!email_store
        .preference(&owner)
        .await
        .unwrap()
        .unwrap()
        .can_receive());
}

#[tokio::test]
async fn retryable_handoff_is_redriven_after_lease_without_duplicate_delivery() {
    let executor = Arc::new(PublishingExecutor);
    let email_store = Arc::new(InMemoryDigestEmailStore::default());
    let email_delivery = Arc::new(RetryOnceEmailDelivery::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline_and_email(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(TwoGameWindowClient),
        executor,
        email_store,
        email_delivery.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    assert!(matches!(
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                    timezone: Some(midday_fixed_timezone(&now)),
                },
                now - TimeDelta::days(1),
            )
            .await,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    runtime
        .observe_verified_email(
            &player_id,
            Some(&crate::beta_access::NormalizedEmail::parse("player@example.com").unwrap()),
        )
        .await
        .unwrap();

    assert_eq!(runtime.tick(now).await.unwrap().published, 1);
    assert_eq!(email_delivery.attempts.lock().unwrap().len(), 1);
    runtime
        .tick(now + TimeDelta::minutes(5) - TimeDelta::milliseconds(1))
        .await
        .unwrap();
    assert_eq!(email_delivery.attempts.lock().unwrap().len(), 1);
    runtime.tick(now + TimeDelta::minutes(5)).await.unwrap();
    runtime.tick(now + TimeDelta::minutes(10)).await.unwrap();

    let attempts = email_delivery.attempts.lock().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].owner_key, attempts[1].owner_key);
    assert_eq!(attempts[0].delivery_id, attempts[1].delivery_id);
    assert_eq!(attempts[0].recipient, attempts[1].recipient);
}

#[tokio::test]
async fn email_preference_rejects_missing_verified_email_at_route_and_runtime_boundaries() {
    let executor = Arc::new(PublishingExecutor);
    let email_store = Arc::new(InMemoryDigestEmailStore::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline_and_email(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(TwoGameWindowClient),
        executor.clone(),
        email_store,
        Arc::new(RecordingEmailDelivery::default()),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    assert!(matches!(
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                    timezone: Some(midday_fixed_timezone(&now)),
                },
                now,
            )
            .await,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    for enabled in [true, false] {
        assert_eq!(
            runtime
                .set_digest_email_enabled(&player_id, None, enabled)
                .await,
            DailyCoachingMutationOutcome::Rejected {
                reason: DailyCoachingMutationRejectionReason::NoVerifiedAccountEmail,
            }
        );
    }

    let application = application_with_runtime(runtime, executor);
    let token = verified_firebase_token("daily-coaching-player", true, "password");
    for enabled in [true, false] {
        let response = request_with_token(
            &application,
            Method::PUT,
            "/api/v1/daily-coaching/email",
            json!({ "enabled": enabled }),
            &token,
        )
        .await;
        assert_eq!(response.0, StatusCode::CONFLICT);
        assert_eq!(response.1["reason"], "noVerifiedAccountEmail");
    }
}

#[tokio::test]
async fn deployment_disabled_email_is_absent_and_rejects_preference_mutation() {
    let runtime = DailyCoachingRuntime::in_memory(Arc::new(FakeProfileValidator::default()), "UTC");
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    assert!(matches!(
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                    timezone: Some(midday_fixed_timezone(&now)),
                },
                now,
            )
            .await,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    let executor = Arc::new(PublishingExecutor);
    let application = application_with_runtime(runtime, executor);
    let token = firebase_token_with_email(
        "daily-coaching-player",
        "player@example.com",
        true,
        "password",
    );

    let dashboard = request_with_token(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
        &token,
    )
    .await;
    assert_eq!(dashboard.0, StatusCode::OK);
    assert!(dashboard.1.get("digestEmailEnabled").is_none());
    for enabled in [true, false] {
        let response = request_with_token(
            &application,
            Method::PUT,
            "/api/v1/daily-coaching/email",
            json!({ "enabled": enabled }),
            &token,
        )
        .await;
        assert_eq!(response.0, StatusCode::CONFLICT);
        assert_eq!(response.1["reason"], "digestEmailUnavailable");
    }
}

#[tokio::test]
async fn connection_without_verified_email_silently_publishes_without_sending() {
    let executor = Arc::new(PublishingExecutor);
    let email_store = Arc::new(InMemoryDigestEmailStore::default());
    let email_delivery = Arc::new(RecordingEmailDelivery::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline_and_email(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(TwoGameWindowClient),
        executor.clone(),
        email_store,
        email_delivery.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    assert!(matches!(
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                    timezone: Some(midday_fixed_timezone(&now)),
                },
                now - TimeDelta::days(1),
            )
            .await,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    runtime
        .observe_verified_email(
            &player_id,
            Some(&crate::beta_access::NormalizedEmail::parse("old@example.com").unwrap()),
        )
        .await
        .unwrap();
    runtime
        .observe_verified_email(&player_id, None)
        .await
        .unwrap();
    let application = application_with_runtime(runtime.clone(), executor);
    assert_eq!(runtime.tick(now).await.unwrap().published, 1);
    assert!(email_delivery.sent.lock().unwrap().is_empty());
    let dashboard = request(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
    )
    .await;
    assert_eq!(dashboard.0, StatusCode::OK);
    assert!(dashboard.1.get("digestEmailEnabled").is_none());
}

#[tokio::test]
async fn signed_bounce_suppresses_email_without_changing_coaching_or_archive() {
    let executor = Arc::new(PublishingExecutor);
    let email_store = Arc::new(InMemoryDigestEmailStore::default());
    let email_delivery = Arc::new(RecordingEmailDelivery::default());
    let runtime = DailyCoachingRuntime::in_memory_with_pipeline_and_email(
        Arc::new(FakeProfileValidator::default()),
        "UTC",
        Arc::new(TwoGameWindowClient),
        executor.clone(),
        email_store.clone(),
        email_delivery.clone(),
    );
    let now = Utc::now();
    let player_id = PlayerId::try_from("daily-coaching-player".to_string()).unwrap();
    let email = crate::beta_access::NormalizedEmail::parse("player@example.com").unwrap();
    assert!(matches!(
        runtime
            .connect_at(
                &player_id,
                ConnectPlayingProfileRequest {
                    profile_url: "https://lichess.org/@/PlayerOne".to_string(),
                    timezone: Some(midday_fixed_timezone(&now)),
                },
                now - TimeDelta::days(1),
            )
            .await,
        ConnectPlayingProfileOutcome::Completed { .. }
    ));
    runtime
        .observe_verified_email(&player_id, Some(&email))
        .await
        .unwrap();
    assert_eq!(runtime.tick(now).await.unwrap().published, 1);
    let application = application_with_runtime(runtime, executor);
    let delivery = tokio::time::timeout(Duration::from_secs(5), async {
        while email_delivery.sent.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(delivery.is_ok());
    // The scheduled digest send carries the digest's own delivery identity.
    let digest_id = email_delivery.sent.lock().unwrap()[0].delivery_id.clone();
    assert!(digest_id.starts_with("daily-"));
    let owner = DailyCoachingOwnerKey::for_player(&player_id);
    let body = json!({
        "type": "email.bounced",
        "created_at": now.to_rfc3339(),
        "data": {
            "email_id": "provider-digest-message-1",
            "to": ["player@example.com"],
            "tags": {
                "coaching_owner": owner.as_str(),
                "digest_id": digest_id.clone(),
            }
        }
    })
    .to_string();

    let unknown = json!({
        "type": "email.bounced",
        "created_at": now.to_rfc3339(),
        "data": {
            "email_id": "unknown-provider-message",
            "to": ["player@example.com"],
            "tags": {
                "coaching_owner": owner.as_str(),
                "digest_id": digest_id,
            }
        }
    })
    .to_string();

    assert_eq!(
        signed_webhook_request(&application, "webhook-event-unknown", &unknown, now).await,
        StatusCode::NO_CONTENT
    );
    assert!(email_store
        .preference(&owner)
        .await
        .unwrap()
        .unwrap()
        .can_receive());

    assert_eq!(
        signed_webhook_request(&application, "webhook-event-1", &body, now).await,
        StatusCode::NO_CONTENT
    );
    assert!(!email_store
        .preference(&owner)
        .await
        .unwrap()
        .unwrap()
        .can_receive());

    let token = firebase_token_with_email(
        "daily-coaching-player",
        "player@example.com",
        true,
        "password",
    );
    assert_eq!(
        request_with_token(
            &application,
            Method::PUT,
            "/api/v1/daily-coaching/email",
            json!({ "enabled": true }),
            &token,
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(!email_store
        .preference(&owner)
        .await
        .unwrap()
        .unwrap()
        .can_receive());
    let dashboard = request_with_token(
        &application,
        Method::GET,
        "/api/v1/daily-coaching/dashboard",
        Value::Null,
        &token,
    )
    .await;
    assert_eq!(dashboard.0, StatusCode::OK);
    assert_eq!(dashboard.1["kind"], "connected");
    assert_eq!(dashboard.1["digestEmailEnabled"], true);
    assert_eq!(dashboard.1["archive"].as_array().unwrap().len(), 1);
    assert!(dashboard.1.get("email").is_none());
    assert!(dashboard.1.get("suppression").is_none());
}

async fn request_public_status(application: &Router, method: Method, uri: &str) -> StatusCode {
    application
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn signed_webhook_request(
    application: &Router,
    event_id: &str,
    body: &str,
    now: DateTime<Utc>,
) -> StatusCode {
    let timestamp = now.timestamp().to_string();
    let signed = format!("{event_id}.{timestamp}.{body}");
    let key = hmac::Key::new(hmac::HMAC_SHA256, &[0x42; 32]);
    let signature = base64(hmac::sign(&key, signed.as_bytes()).as_ref());
    application
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/daily-coaching/email/webhooks/resend")
                .header("svix-id", event_id)
                .header("svix-timestamp", timestamp)
                .header("svix-signature", format!("v1,{signature}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or_default();
        let third = chunk.get(2).copied().unwrap_or_default();
        output.push(ALPHABET[usize::from(first >> 2)] as char);
        output.push(ALPHABET[usize::from((first & 0b11) << 4 | second >> 4)] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[usize::from((second & 0b1111) << 2 | third >> 6)] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[usize::from(third & 0b11_1111)] as char
        } else {
            '='
        });
    }
    output
}

#[derive(Default)]
struct RecordingEmailDelivery {
    sent: std::sync::Mutex<Vec<DigestEmailRequest>>,
}

#[derive(Default)]
struct PerProviderProfileGameClient {
    lichess: AtomicUsize,
    chess_com: AtomicUsize,
}

impl PerProviderProfileGameClient {
    fn fail(&self, provider: DailyCoachingProvider) {
        match provider {
            DailyCoachingProvider::Lichess => self.lichess.store(1, Ordering::SeqCst),
            DailyCoachingProvider::ChessCom => self.chess_com.store(1, Ordering::SeqCst),
        }
    }
}

impl ProfileGameClient for PerProviderProfileGameClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            let unavailable = match request.provider() {
                ChessProfileProvider::Lichess => self.lichess.load(Ordering::SeqCst) == 1,
                ChessProfileProvider::ChessCom => self.chess_com.load(Ordering::SeqCst) == 1,
            };
            if unavailable {
                return Err(ProfileGameFetchError::Status {
                    provider: request.provider(),
                    code: 404,
                    retry_after_seconds: None,
                });
            }
            let body = match request.provider() {
                ChessProfileProvider::Lichess => Vec::new(),
                ChessProfileProvider::ChessCom => br#"{"games":[]}"#.to_vec(),
            };
            Ok(ProfileGameResponse {
                body,
                content_type: request.accept().to_string(),
            })
        })
    }
}

#[derive(Default)]
struct RetryOnceEmailDelivery {
    attempts: std::sync::Mutex<Vec<DigestEmailRequest>>,
}

impl DigestEmailDelivery for RetryOnceEmailDelivery {
    fn deliver<'a>(&'a self, request: DigestEmailRequest) -> EmailDeliveryFuture<'a> {
        Box::pin(async move {
            let mut attempts = self.attempts.lock().unwrap();
            attempts.push(request);
            if attempts.len() == 1 {
                return Err(DigestEmailDeliveryError::Retryable);
            }
            Ok(DigestEmailReceipt {
                provider_message_id: "provider-digest-message-1".to_string(),
            })
        })
    }
}

impl DigestEmailDelivery for RecordingEmailDelivery {
    fn deliver<'a>(&'a self, request: DigestEmailRequest) -> EmailDeliveryFuture<'a> {
        Box::pin(async move {
            self.sent.lock().unwrap().push(request);
            Ok(DigestEmailReceipt {
                provider_message_id: "provider-digest-message-1".to_string(),
            })
        })
    }
}
