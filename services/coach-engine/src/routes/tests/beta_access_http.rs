use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use serde_json::Value;
use tokio::sync::{mpsc, Notify};
use tower::ServiceExt;

use crate::{
    account_deletion::AccountDeletionRuntime,
    auth::AuthConfig,
    beta_access::{
        BetaAccessRuntime, InMemoryBetaAccessStore, InvitationDeliveryError,
        InvitationDeliveryReceipt, InvitationDeliveryRequest, InvitationEmailDelivery,
    },
    review_session_contract::ReviewSessionEventEnvelope,
    review_session_processor::{ProcessorCommandAdmission, ProcessorPrincipal},
    review_session_transport::{ReviewSessionCommandExecutor, ReviewSessionWebBinding},
    types::AppState,
};

const TEST_KEY: &[u8] = b"0123456789abcdef0123456789abcdef";
const GENERIC_MESSAGE: &str = "Thanks. Your beta access request has been received.";

use super::firebase_token_test_support::{
    administrator_token, coach_token, firebase_token, firebase_token_with_email, jwt_jwks,
    COACH_ISSUER, COACH_RESOURCE, COACH_SCOPE, FIREBASE_PROJECT_ID,
};

#[tokio::test]
async fn new_duplicate_and_handled_requests_have_one_public_outcome() {
    let (application, store) = application_with_store();

    let first = submit(&application, "Player@Example.COM", "203.0.113.1").await;
    store.grant("player@example.com");
    let duplicate = submit(&application, " player@example.com ", "203.0.113.1").await;

    assert_eq!(first, duplicate);
    assert_eq!(first, (StatusCode::ACCEPTED, GENERIC_MESSAGE.to_string()));
    assert_eq!(store.request_count(), 1);
}

#[tokio::test]
async fn access_requests_reject_a_missing_bearer_as_unauthorized() {
    let (application, store) = application_with_store();
    let response = application
        .oneshot(access_request(None, "203.0.113.1"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(store.request_count(), 0);
}

#[tokio::test]
async fn access_requests_forbid_an_inadmissible_firebase_identity() {
    let (application, store) = application_with_store();
    for request in [
        access_request(
            Some(&firebase_token_with_email(
                "unverified-player",
                "player@example.com",
                false,
                "password",
            )),
            "203.0.113.1",
        ),
        access_request(
            Some(&firebase_token_with_email(
                "unsupported-player",
                "player@example.com",
                true,
                "github.com",
            )),
            "203.0.113.1",
        ),
    ] {
        let response = application.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json_message(response).await,
            "Confirm your email address, then request Beta Access again."
        );
    }
    assert_eq!(store.request_count(), 0);
}

#[tokio::test]
async fn access_requests_reject_a_body_before_submission() {
    let (application, store) = application_with_store();
    let token = firebase_token_with_email("request-player", "player@example.com", true, "password");
    let mut request = access_request(Some(&token), "203.0.113.1");
    *request.body_mut() = Body::from(r#"{"email":"other@example.test"}"#);

    let response = application.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(store.request_count(), 0);
}

#[tokio::test]
async fn normalized_email_and_source_ip_limits_do_not_change_the_public_outcome() {
    let (email_application, email_store) = application_with_store();
    for attempt in 0..=super::super::beta_access::EMAIL_ATTEMPT_LIMIT {
        let email = if attempt % 2 == 0 {
            "Player@Example.COM"
        } else {
            " player@example.com "
        };
        assert_eq!(
            submit(&email_application, email, "203.0.113.2").await,
            (StatusCode::ACCEPTED, GENERIC_MESSAGE.to_string())
        );
    }
    assert_eq!(email_store.request_count(), 1);

    let (ip_application, ip_store) = application_with_store();
    for attempt in 0..=super::super::beta_access::IP_ATTEMPT_LIMIT {
        let email = format!("player-{attempt}@example.com");
        assert_eq!(
            submit(&ip_application, &email, "203.0.113.3").await,
            (StatusCode::ACCEPTED, GENERIC_MESSAGE.to_string())
        );
    }
    assert_eq!(
        ip_store.request_count(),
        usize::from(super::super::beta_access::IP_ATTEMPT_LIMIT)
    );
}

#[tokio::test]
async fn invalid_source_and_unavailable_storage_fail_without_exposing_identity() {
    let (available_application, _) = application_with_store();
    let token =
        firebase_token_with_email("request-player", "private@example.com", true, "password");
    let response = available_application
        .oneshot(access_request(Some(&token), "invalid"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_message(response).await,
        "Beta access request could not be accepted."
    );

    let unavailable = application(BetaAccessRuntime::unavailable_store(TEST_KEY))
        .oneshot(access_request(Some(&token), "203.0.113.4"))
        .await
        .unwrap();
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_message(unavailable).await,
        "Beta access requests are temporarily unavailable. Please try again later."
    );
}

#[tokio::test]
async fn administrator_lists_and_filters_only_the_bounded_request_projection() {
    let (application, store) = application_with_store();
    submit(&application, "pending@example.com", "203.0.113.10").await;
    submit(&application, "granted@example.com", "203.0.113.11").await;
    store.grant("granted@example.com");
    let token = administrator_token("firebase-administrator", true, Some(true));

    let response = application
        .clone()
        .oneshot(admin_request(
            Method::GET,
            "/api/v1/admin/beta-access/requests",
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let requests = body["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests {
        let fields = request
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        if request["status"] == "granted" {
            assert_eq!(
                fields,
                [
                    "createdAt",
                    "deliveryStatus",
                    "email",
                    "id",
                    "invitationStatus",
                    "status",
                ]
            );
        } else {
            assert_eq!(fields, ["createdAt", "email", "id", "status"]);
        }
        let id = request["id"].as_str().unwrap();
        assert_eq!(id.len(), 64);
        assert!(!id.contains("example.com"));
    }

    let filtered = application
        .oneshot(admin_request(
            Method::GET,
            "/api/v1/admin/beta-access/requests?status=pending&email=PENDING",
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(filtered.status(), StatusCode::OK);
    let filtered = json_body(filtered).await;
    assert_eq!(filtered["requests"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["requests"][0]["email"], "pending@example.com");
    assert_eq!(filtered["requests"][0]["status"], "pending");
}

#[tokio::test]
async fn administrator_listing_denies_unauthenticated_player_and_stale_tokens() {
    let (application, _) = application_with_store();
    submit(&application, "private@example.com", "203.0.113.12").await;
    let denied_tokens = [
        None,
        Some(firebase_token("ordinary-player")),
        Some(administrator_token(
            "unverified-administrator",
            false,
            Some(true),
        )),
        Some(administrator_token("stale-administrator", true, None)),
    ];

    for token in denied_tokens {
        let response = application
            .clone()
            .oneshot(admin_request(
                Method::GET,
                "/api/v1/admin/beta-access/requests",
                token.as_deref(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("private@example.com"));
    }
}

#[tokio::test]
async fn beta_back_office_has_no_role_or_identity_mutation_route() {
    let (application, _) = application_with_store();
    let token = administrator_token("firebase-administrator", true, Some(true));

    let mutation = application
        .clone()
        .oneshot(admin_request(
            Method::POST,
            "/api/v1/admin/beta-access/requests",
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(mutation.status(), StatusCode::METHOD_NOT_ALLOWED);

    let firebase_users = application
        .oneshot(admin_request(
            Method::GET,
            "/api/v1/admin/firebase/users",
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(firebase_users.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn administrator_grant_creates_and_delivers_exactly_one_secret_invitation() {
    let (application, store, delivery) = application_with_delivery(None);
    submit(&application, "invited@example.com", "203.0.113.20").await;
    let token = administrator_token("firebase-administrator", true, Some(true));
    let request_id = listed_request_id(&application, &token).await;

    let first = application
        .clone()
        .oneshot(admin_request(
            Method::POST,
            &format!("/api/v1/admin/beta-access/requests/{request_id}/grant"),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(first.headers()[header::CACHE_CONTROL], "no-store");
    let first_body = json_body(first).await;
    assert_eq!(first_body, serde_json::json!({ "outcome": "delivered" }));

    let code = {
        let captured = delivery.requests.lock().unwrap();
        assert_eq!(captured.len(), 1);
        let code = captured[0].code.clone();
        assert_eq!(code.len(), 32);
        assert!(code.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(captured[0].email.as_str(), "invited@example.com");
        code
    };
    assert!(!store.serialized_invitations().contains(&code));

    let listed = listed_requests(&application, &token).await;
    assert_eq!(listed["requests"][0]["status"], "granted");
    assert_eq!(listed["requests"][0]["deliveryStatus"], "sent");
    assert!(!listed.to_string().contains(&code));

    let duplicate = application
        .oneshot(admin_request(
            Method::POST,
            &format!("/api/v1/admin/beta-access/requests/{request_id}/grant"),
            Some(&token),
        ))
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::OK);
    assert_eq!(
        json_body(duplicate).await,
        serde_json::json!({ "outcome": "alreadyGranted" })
    );
    assert_eq!(delivery.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn provider_rejection_and_timeout_record_safe_failed_delivery() {
    for (failure, retryable) in [
        (InvitationDeliveryError::Rejected, false),
        (InvitationDeliveryError::Retryable, true),
    ] {
        let (application, store, delivery) = application_with_delivery(Some(failure));
        submit(&application, "failed@example.com", "203.0.113.21").await;
        let token = administrator_token("firebase-administrator", true, Some(true));
        let request_id = listed_request_id(&application, &token).await;

        let response = application
            .clone()
            .oneshot(admin_request(
                Method::POST,
                &format!("/api/v1/admin/beta-access/requests/{request_id}/grant"),
                Some(&token),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await,
            serde_json::json!({ "outcome": "deliveryFailed" })
        );
        let code = delivery.requests.lock().unwrap()[0].code.clone();
        let listed = listed_requests(&application, &token).await;
        assert_eq!(listed["requests"][0]["deliveryStatus"], "failed");
        assert!(!listed.to_string().contains(&code));
        assert!(!store.serialized_invitations().contains(&code));
        assert_eq!(store.invitation_retryable(), Some(retryable));
        if !retryable {
            assert_eq!(
                admin_mutation(&application, &token, &request_id, "retry-delivery").await,
                serde_json::json!({ "outcome": "notRetryable" })
            );
            assert_eq!(delivery.requests.lock().unwrap().len(), 1);
        }
    }
}

#[tokio::test]
async fn retry_reuses_one_code_and_invitation_before_stable_revocation() {
    let (application, store, delivery) =
        application_with_delivery(Some(InvitationDeliveryError::Retryable));
    submit(&application, "retry@example.com", "203.0.113.23").await;
    let token = administrator_token("firebase-administrator", true, Some(true));
    let request_id = listed_request_id(&application, &token).await;

    assert_eq!(
        admin_mutation(&application, &token, &request_id, "grant").await,
        serde_json::json!({ "outcome": "deliveryFailed" })
    );
    let (invitation_id, code) = {
        let requests = delivery.requests.lock().unwrap();
        assert_eq!(requests[0].delivery_attempt, 1);
        (requests[0].invitation_id.clone(), requests[0].code.clone())
    };
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "retry-delivery").await,
        serde_json::json!({ "outcome": "deliveryFailed" })
    );
    {
        let requests = delivery.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].delivery_attempt, 2);
        assert_eq!(requests[1].invitation_id, invitation_id);
        assert_eq!(requests[1].code, code);
    }
    *delivery.failure.lock().unwrap() = None;
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "retry-delivery").await,
        serde_json::json!({ "outcome": "delivered" })
    );
    {
        let requests = delivery.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[2].delivery_attempt, 3);
        assert_eq!(requests[2].invitation_id, invitation_id);
        assert_eq!(requests[2].code, code);
    }
    assert_eq!(store.invitation_count(), 1);
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "retry-delivery").await,
        serde_json::json!({ "outcome": "notRetryable" })
    );
    assert_eq!(delivery.requests.lock().unwrap().len(), 3);

    assert_eq!(
        admin_mutation(&application, &token, &request_id, "revoke").await,
        serde_json::json!({ "outcome": "revoked" })
    );
    let listed = listed_requests(&application, &token).await;
    assert_eq!(listed["requests"][0]["invitationStatus"], "revoked");
    assert_eq!(listed["requests"][0]["deliveryStatus"], "sent");
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "revoke").await,
        serde_json::json!({ "outcome": "alreadyRevoked" })
    );
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "retry-delivery").await,
        serde_json::json!({ "outcome": "revoked" })
    );
}

#[tokio::test]
async fn pending_and_redeemed_invitations_reject_retry_and_revocation_stably() {
    let (application, store, _) = application_with_delivery(None);
    submit(&application, "terminal@example.com", "203.0.113.24").await;
    let token = administrator_token("firebase-administrator", true, Some(true));
    let request_id = listed_request_id(&application, &token).await;

    assert_eq!(
        admin_mutation(&application, &token, &request_id, "revoke").await,
        serde_json::json!({ "outcome": "notIssued" })
    );
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "grant").await,
        serde_json::json!({ "outcome": "delivered" })
    );
    store.mark_invitation_redeemed();

    assert_eq!(
        admin_mutation(&application, &token, &request_id, "retry-delivery").await,
        serde_json::json!({ "outcome": "redeemed" })
    );
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "revoke").await,
        serde_json::json!({ "outcome": "alreadyRedeemed" })
    );
    let listed = listed_requests(&application, &token).await;
    assert_eq!(listed["requests"][0]["invitationStatus"], "redeemed");
    assert_eq!(
        listed["requests"][0]["dailyCoaching"],
        serde_json::json!({ "status": "noDigest" })
    );
}

#[tokio::test]
async fn concurrent_retry_and_revoke_never_reopen_the_revoked_invitation() {
    let (application, _, delivery) =
        application_with_delivery_hold(Some(InvitationDeliveryError::Retryable), Some(2));
    submit(&application, "race@example.com", "203.0.113.25").await;
    let token = administrator_token("firebase-administrator", true, Some(true));
    let request_id = listed_request_id(&application, &token).await;
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "grant").await,
        serde_json::json!({ "outcome": "deliveryFailed" })
    );
    *delivery.failure.lock().unwrap() = None;

    let retry_application = application.clone();
    let retry_token = token.clone();
    let retry_request_id = request_id.clone();
    let retry = tokio::spawn(async move {
        admin_mutation(
            &retry_application,
            &retry_token,
            &retry_request_id,
            "retry-delivery",
        )
        .await
    });
    delivery.started.notified().await;
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "revoke").await,
        serde_json::json!({ "outcome": "revoked" })
    );
    delivery.release.notify_one();
    assert_eq!(
        retry.await.unwrap(),
        serde_json::json!({ "outcome": "delivered" })
    );

    let listed = listed_requests(&application, &token).await;
    assert_eq!(listed["requests"][0]["invitationStatus"], "revoked");
    assert_eq!(listed["requests"][0]["deliveryStatus"], "sent");
    assert_eq!(
        admin_mutation(&application, &token, &request_id, "retry-delivery").await,
        serde_json::json!({ "outcome": "revoked" })
    );
}

#[tokio::test]
async fn administrator_mutations_reject_unauthorized_missing_and_malformed_requests_without_data() {
    let (application, _, _) = application_with_delivery(None);
    submit(&application, "private@example.com", "203.0.113.22").await;
    let token = administrator_token("firebase-administrator", true, Some(true));
    let player_token = firebase_token("ordinary-player");
    let request_id = listed_request_id(&application, &token).await;
    let missing_request_id = "f".repeat(64);

    for operation in ["grant", "retry-delivery", "revoke", "revoke-access"] {
        for (id, presented_token, status) in [
            (request_id.as_str(), None, StatusCode::UNAUTHORIZED),
            (
                request_id.as_str(),
                Some(player_token.as_str()),
                StatusCode::UNAUTHORIZED,
            ),
            (
                "not-an-opaque-id",
                Some(token.as_str()),
                StatusCode::BAD_REQUEST,
            ),
            (
                missing_request_id.as_str(),
                Some(token.as_str()),
                StatusCode::NOT_FOUND,
            ),
        ] {
            let response = application
                .clone()
                .oneshot(admin_request(
                    Method::POST,
                    &format!("/api/v1/admin/beta-access/requests/{id}/{operation}"),
                    presented_token,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert!(!String::from_utf8_lossy(&body).contains("private@example.com"));
        }
    }
}

#[tokio::test]
async fn verified_invitee_redeems_once_and_binds_beta_access_to_the_player() {
    let (application, store, code, _) = redeemable_invitation("invited@example.com").await;
    let token =
        firebase_token_with_email("invited-player", "Invited@Example.COM", true, "password");

    assert_eq!(
        redeem(&application, Some(&token), &code, "203.0.113.40").await,
        (StatusCode::OK, serde_json::json!({ "outcome": "granted" }))
    );
    assert_eq!(store.access_grant_count(), 1);
    assert!(store.has_access("invited-player"));
    assert_eq!(
        redeem(&application, Some(&token), &code, "203.0.113.40").await,
        (
            StatusCode::OK,
            serde_json::json!({ "outcome": "alreadyHandled" })
        )
    );
    assert_eq!(store.access_grant_count(), 1);
}

#[tokio::test]
async fn an_administrator_revokes_redeemed_access_across_web_oauth_and_mcp() {
    let (application, store, code, request_id) = redeemable_invitation("invited@example.com").await;
    let firebase =
        firebase_token_with_email("shared-player", "invited@example.com", true, "password");
    let coach = coach_token("shared-player");
    let administrator = administrator_token("firebase-administrator", true, Some(true));

    for token in [&firebase, &coach] {
        assert_eq!(
            authorization(&application, token).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            protected_command_status(&application, token).await,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        identity_bridge_status(&application, &firebase).await,
        StatusCode::FORBIDDEN
    );

    assert_eq!(
        redeem(&application, Some(&firebase), &code, "203.0.113.48").await,
        (StatusCode::OK, serde_json::json!({ "outcome": "granted" }))
    );

    for token in [&firebase, &coach] {
        let (status, response, cache_control) = authorization(&application, token).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response, serde_json::json!({ "playerId": "shared-player" }));
        assert_eq!(cache_control.as_deref(), Some("no-store"));
        assert_eq!(
            protected_command_status(&application, token).await,
            StatusCode::OK
        );
    }
    assert_eq!(
        identity_bridge_status(&application, &firebase).await,
        StatusCode::OK
    );

    let listed = listed_requests(&application, &administrator).await;
    assert_eq!(listed["requests"][0]["invitationStatus"], "redeemed");
    assert_eq!(listed["requests"][0]["accessStatus"], "active");
    assert_eq!(
        admin_mutation(&application, &administrator, &request_id, "revoke-access").await,
        serde_json::json!({ "outcome": "revoked" })
    );
    assert_eq!(store.access_grant_count(), 0);
    assert_eq!(
        redeem(&application, Some(&firebase), &code, "203.0.113.49").await,
        (
            StatusCode::OK,
            serde_json::json!({ "outcome": "alreadyHandled" })
        )
    );
    assert_eq!(store.access_grant_count(), 0);

    for token in [&firebase, &coach] {
        assert_eq!(
            authorization(&application, token).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            protected_command_status(&application, token).await,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        identity_bridge_status(&application, &firebase).await,
        StatusCode::FORBIDDEN
    );
    let listed = listed_requests(&application, &administrator).await;
    assert_eq!(listed["requests"][0]["invitationStatus"], "redeemed");
    assert_eq!(listed["requests"][0]["accessStatus"], "revoked");
    assert_eq!(
        admin_mutation(&application, &administrator, &request_id, "revoke-access").await,
        serde_json::json!({ "outcome": "alreadyRevoked" })
    );
}

#[tokio::test]
async fn production_bypass_never_requires_beta_persistence_and_staging_fails_closed() {
    let token = firebase_token_with_email(
        "production-player",
        "player@example.com",
        true,
        "google.com",
    );
    let production = application(BetaAccessRuntime::disabled());
    assert_eq!(authorization(&production, &token).await.0, StatusCode::OK);
    assert_eq!(
        identity_bridge_status(&production, &token).await,
        StatusCode::OK
    );

    let unavailable = application(BetaAccessRuntime::unavailable_store(TEST_KEY));
    assert_eq!(
        authorization(&unavailable, &token).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        identity_bridge_status(&unavailable, &token).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn redemption_failures_never_consume_or_grant_the_invitation() {
    for (token, code_transform, expected) in [
        (
            firebase_token_with_email("wrong-player", "wrong@example.com", true, "google.com"),
            CodeTransform::Exact,
            "wrongAccount",
        ),
        (
            firebase_token_with_email(
                "unverified-player",
                "invited@example.com",
                false,
                "password",
            ),
            CodeTransform::Exact,
            "verificationRequired",
        ),
        (
            firebase_token_with_email("malformed-player", "invited@example.com", true, "password"),
            CodeTransform::Malformed,
            "invalid",
        ),
        (
            firebase_token_with_email("incorrect-player", "invited@example.com", true, "password"),
            CodeTransform::Incorrect,
            "invalid",
        ),
    ] {
        let (application, store, code, _) = redeemable_invitation("invited@example.com").await;
        let presented_code = code_transform.apply(&code);
        assert_eq!(
            redeem(&application, Some(&token), &presented_code, "203.0.113.41").await,
            (StatusCode::OK, serde_json::json!({ "outcome": expected }))
        );
        assert_eq!(store.access_grant_count(), 0);
        let invited =
            firebase_token_with_email("eventual-player", "invited@example.com", true, "password");
        assert_eq!(
            redeem(&application, Some(&invited), &code, "203.0.113.42").await,
            (StatusCode::OK, serde_json::json!({ "outcome": "granted" }))
        );
    }

    let (application, store, code, request_id) = redeemable_invitation("revoked@example.com").await;
    let admin = administrator_token("firebase-administrator", true, Some(true));
    assert_eq!(
        admin_mutation(&application, &admin, &request_id, "revoke").await,
        serde_json::json!({ "outcome": "revoked" })
    );
    let token =
        firebase_token_with_email("revoked-player", "revoked@example.com", true, "password");
    assert_eq!(
        redeem(&application, Some(&token), &code, "203.0.113.43").await,
        (StatusCode::OK, serde_json::json!({ "outcome": "revoked" }))
    );
    assert_eq!(store.access_grant_count(), 0);
}

#[tokio::test]
async fn concurrent_redemption_has_one_winner_and_rate_limits_rejected_attempts() {
    let (application, store, code, _) = redeemable_invitation("race@example.com").await;
    let first_token = firebase_token_with_email("race-a", "race@example.com", true, "password");
    let second_token = firebase_token_with_email("race-b", "race@example.com", true, "password");
    let first = redeem(&application, Some(&first_token), &code, "203.0.113.44");
    let second = redeem(&application, Some(&second_token), &code, "203.0.113.45");
    let (first, second) = tokio::join!(first, second);
    let outcomes = [first.1["outcome"].as_str(), second.1["outcome"].as_str()];
    assert!(outcomes.contains(&Some("granted")));
    assert!(outcomes.contains(&Some("alreadyHandled")));
    assert_eq!(store.access_grant_count(), 1);

    let (application, store, code, _) = redeemable_invitation("limited@example.com").await;
    let token = firebase_token_with_email("limited-player", "wrong@example.com", true, "password");
    for _ in 0..super::super::beta_access::REDEMPTION_PLAYER_ATTEMPT_LIMIT {
        assert_eq!(
            redeem(&application, Some(&token), &code, "203.0.113.46")
                .await
                .1,
            serde_json::json!({ "outcome": "wrongAccount" })
        );
    }
    assert_eq!(
        redeem(&application, Some(&token), &code, "203.0.113.46")
            .await
            .1,
        serde_json::json!({ "outcome": "tryLater" })
    );
    assert_eq!(store.access_grant_count(), 0);
}

#[tokio::test]
async fn redemption_requires_firebase_auth_json_and_a_trusted_source_ip() {
    let (application, _, code, _) = redeemable_invitation("private@example.com").await;
    let token =
        firebase_token_with_email("private-player", "private@example.com", true, "password");
    let unauthenticated = redeem(&application, None, &code, "203.0.113.47").await;
    assert_eq!(unauthenticated.0, StatusCode::UNAUTHORIZED);

    let invalid_source = application
        .clone()
        .oneshot(redemption_request(Some(&token), &code, "not-an-ip"))
        .await
        .unwrap();
    assert_eq!(invalid_source.status(), StatusCode::BAD_REQUEST);

    let invalid_json = application
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/beta-access/invitations/redeem")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "text/plain")
                .header("x-chenchess-source-ip", "203.0.113.47")
                .body(Body::from(code))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_json.status(), StatusCode::BAD_REQUEST);
}

#[derive(Clone, Copy)]
enum CodeTransform {
    Exact,
    Malformed,
    Incorrect,
}

impl CodeTransform {
    fn apply(self, code: &str) -> String {
        match self {
            Self::Exact => code.to_string(),
            Self::Malformed => "not-an-invitation-code".to_string(),
            Self::Incorrect => {
                let replacement = if code.starts_with('0') { '1' } else { '0' };
                format!("{replacement}{}", &code[1..])
            }
        }
    }
}

struct FakeInvitationDelivery {
    failure: Mutex<Option<InvitationDeliveryError>>,
    hold_attempt: Option<u32>,
    release: Notify,
    requests: Mutex<Vec<InvitationDeliveryRequest>>,
    started: Notify,
}

impl InvitationEmailDelivery for FakeInvitationDelivery {
    fn deliver<'a>(
        &'a self,
        request: InvitationDeliveryRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<InvitationDeliveryReceipt, InvitationDeliveryError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let delivery_attempt = request.delivery_attempt;
            self.requests.lock().unwrap().push(request);
            let failure = *self.failure.lock().unwrap();
            if self.hold_attempt == Some(delivery_attempt) {
                self.started.notify_one();
                self.release.notified().await;
            }
            match failure {
                Some(error) => Err(error),
                None => Ok(InvitationDeliveryReceipt {
                    provider_message_id: "provider-message-1".to_string(),
                }),
            }
        })
    }
}

fn application_with_delivery(
    failure: Option<InvitationDeliveryError>,
) -> (
    Router,
    Arc<InMemoryBetaAccessStore>,
    Arc<FakeInvitationDelivery>,
) {
    application_with_delivery_hold(failure, None)
}

fn application_with_delivery_hold(
    failure: Option<InvitationDeliveryError>,
    hold_attempt: Option<u32>,
) -> (
    Router,
    Arc<InMemoryBetaAccessStore>,
    Arc<FakeInvitationDelivery>,
) {
    let store = Arc::new(InMemoryBetaAccessStore::default());
    let delivery = Arc::new(FakeInvitationDelivery {
        failure: Mutex::new(failure),
        hold_attempt,
        release: Notify::new(),
        requests: Mutex::new(Vec::new()),
        started: Notify::new(),
    });
    let runtime =
        BetaAccessRuntime::in_memory_with_delivery(store.clone(), TEST_KEY, delivery.clone())
            .unwrap();
    (application(runtime), store, delivery)
}

async fn listed_request_id(application: &Router, token: &str) -> String {
    listed_requests(application, token).await["requests"][0]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn listed_requests(application: &Router, token: &str) -> Value {
    let response = application
        .clone()
        .oneshot(admin_request(
            Method::GET,
            "/api/v1/admin/beta-access/requests",
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn admin_mutation(
    application: &Router,
    token: &str,
    request_id: &str,
    operation: &str,
) -> Value {
    let response = application
        .clone()
        .oneshot(admin_request(
            Method::POST,
            &format!("/api/v1/admin/beta-access/requests/{request_id}/{operation}"),
            Some(token),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn redeemable_invitation(
    email: &str,
) -> (Router, Arc<InMemoryBetaAccessStore>, String, String) {
    let (application, store, delivery) = application_with_delivery(None);
    submit(&application, email, "203.0.113.39").await;
    let administrator = administrator_token("firebase-administrator", true, Some(true));
    let request_id = listed_request_id(&application, &administrator).await;
    assert_eq!(
        admin_mutation(&application, &administrator, &request_id, "grant").await,
        serde_json::json!({ "outcome": "delivered" })
    );
    let code = delivery.requests.lock().unwrap()[0].code.clone();
    (application, store, code, request_id)
}

async fn redeem(
    application: &Router,
    token: Option<&str>,
    code: &str,
    source_ip: &str,
) -> (StatusCode, Value) {
    let response = application
        .clone()
        .oneshot(redemption_request(token, code, source_ip))
        .await
        .unwrap();
    let status = response.status();
    let body = json_body(response).await;
    (status, body)
}

async fn authorization(application: &Router, token: &str) -> (StatusCode, Value, Option<String>) {
    let response = application
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/beta-access/authorization")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let cache_control = response
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = json_body(response).await;
    (status, body, cache_control)
}

async fn protected_command_status(application: &Router, token: &str) -> StatusCode {
    application
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/review-session/commands")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn identity_bridge_status(application: &Router, token: &str) -> StatusCode {
    application
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/internal/v1/oauth/firebase-identity")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "firebaseIdToken": token }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

fn redemption_request(token: Option<&str>, code: &str, source_ip: &str) -> Request<Body> {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/beta-access/invitations/redeem")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-chenchess-source-ip", source_ip);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    request
        .body(Body::from(serde_json::json!({ "code": code }).to_string()))
        .unwrap()
}

fn application_with_store() -> (Router, Arc<InMemoryBetaAccessStore>) {
    let store = Arc::new(InMemoryBetaAccessStore::default());
    let runtime = BetaAccessRuntime::in_memory(store.clone(), TEST_KEY).unwrap();
    (application(runtime), store)
}

fn application(beta_access: BetaAccessRuntime) -> Router {
    crate::app(Arc::new(AppState {
        account_deletion: AccountDeletionRuntime::disabled(),
        auth: AuthConfig::new_firebase(FIREBASE_PROJECT_ID, jwt_jwks())
            .unwrap()
            .with_coach_mcp(jwt_jwks(), COACH_ISSUER, COACH_RESOURCE, COACH_SCOPE)
            .unwrap(),
        beta_access,
        daily_coaching: crate::daily_coaching::DailyCoachingRuntime::disabled(),
        imported_games: crate::imported_games::ImportedGamesRuntime::in_memory(),
        opening_analysis: crate::opening_analysis::OpeningAnalysisRuntime::disabled(),
        review_session: ReviewSessionWebBinding::new(Arc::new(NoopExecutor)),
    }))
}

async fn submit(application: &Router, email: &str, source_ip: &str) -> (StatusCode, String) {
    let token = firebase_token_with_email("request-player", email, true, "password");
    let response = application
        .clone()
        .oneshot(access_request(Some(&token), source_ip))
        .await
        .unwrap();
    let status = response.status();
    (status, json_message(response).await)
}

fn access_request(token: Option<&str>, source_ip: &str) -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/beta-access/requests")
        .header(header::ACCEPT, "application/json")
        .header("x-chenchess-source-ip", source_ip);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    request.body(Body::empty()).unwrap()
}

fn admin_request(method: Method, uri: &str, token: Option<&str>) -> Request<Body> {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    request.body(Body::empty()).unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

async fn json_message(response: axum::response::Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice::<Value>(&body).unwrap()["message"]
        .as_str()
        .unwrap()
        .to_string()
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
