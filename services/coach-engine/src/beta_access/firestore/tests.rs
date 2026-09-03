use std::collections::BTreeMap;

use axum::{
    body::Bytes,
    extract::State,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json, Router,
};
use serde_json::Value;
use tokio::sync::Mutex;

use super::grant::{BETA_ACCESS_COLLECTION, BETA_ACCESS_GRANT_ID, USERS_COLLECTION};
use super::redemption::INVITATION_LOOKUPS_COLLECTION;
use super::*;
use crate::beta_access::{BetaAccessRedemptionCommit, BetaAccessRedemptionTarget};
use crate::review_durability::path::hashed_path_segment;

#[derive(Default)]
struct FakeFirestore {
    commits: Vec<Value>,
    documents: BTreeMap<String, Value>,
}

#[tokio::test]
async fn firestore_submission_is_atomic_idempotent_and_uses_only_keyed_path_ids() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let application = Router::new()
        .fallback(fake_firestore_request)
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, application).await });
    let store = FirestoreBetaAccessStore {
        database: FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
    };
    let submission = BetaAccessSubmission {
        email: NormalizedEmail::parse("player@example.com").unwrap(),
        email_rate_key: "a".repeat(64),
        ip_rate_key: "b".repeat(64),
        now: "2026-08-02T10:00:00Z".parse().unwrap(),
    };

    assert_eq!(
        store.submit(submission.clone()).await.unwrap(),
        BetaAccessStoreOutcome::Recorded
    );
    assert_eq!(
        store.submit(submission).await.unwrap(),
        BetaAccessStoreOutcome::Duplicate
    );
    let state = state.lock().await;
    assert_eq!(
        state
            .documents
            .keys()
            .filter(|path| path.starts_with(ACCESS_REQUESTS_COLLECTION))
            .count(),
        1
    );
    assert!(state
        .documents
        .keys()
        .all(|path| !path.contains("player@example.com")));
    assert_eq!(state.commits.len(), 2);
    assert!(state
        .commits
        .iter()
        .all(|commit| { commit["transaction"].as_str() == Some("fixture-transaction") }));
    let serialized = serde_json::to_string(&state.documents).unwrap();
    assert!(serialized.contains("2026-08-03T10:00:00"));
    server.abort();
}

#[tokio::test]
async fn firestore_grant_retry_delivery_and_revoke_are_atomic_and_secret_free() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let application = Router::new()
        .fallback(fake_firestore_request)
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, application).await });
    let store = FirestoreBetaAccessStore {
        database: FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
    };
    let request_id = "a".repeat(64);
    let email = NormalizedEmail::parse("player@example.com").unwrap();
    store
        .submit(BetaAccessSubmission {
            email: email.clone(),
            email_rate_key: request_id.clone(),
            ip_rate_key: "b".repeat(64),
            now: "2026-08-02T10:00:00Z".parse().unwrap(),
        })
        .await
        .unwrap();
    let invitation_id = "c".repeat(32);
    let plaintext_code = "must-never-be-persisted";
    let invitation = StoredInvitation {
        authenticator: "d".repeat(64),
        authenticator_version: 1,
        ciphertext: "e".repeat(96),
        created_at: "2026-08-02T10:01:00Z".parse().unwrap(),
        delivery_attempt: 1,
        delivery_retryable: None,
        delivery_status: InvitationDeliveryStatus::Pending,
        email,
        encryption_nonce: "f".repeat(24),
        encryption_version: 1,
        id: invitation_id.clone(),
        lookup_id: "1".repeat(64),
        provider_message_id: None,
        record_version: 1,
        redeemed_at: None,
        redeemed_by: None,
        request_id: request_id.clone(),
        status: InvitationStatus::Issued,
    };

    assert_eq!(
        store.commit_grant(invitation).await.unwrap(),
        BetaAccessGrantCommit::Issued
    );
    store
        .record_delivery(
            &invitation_id,
            &request_id,
            1,
            InvitationDeliveryAttempt::Failed { retryable: true },
        )
        .await
        .unwrap();
    assert!(matches!(
        store.invitation_target(&request_id).await.unwrap(),
        BetaAccessInvitationTarget::Invitation(_)
    ));
    assert!(matches!(
        store
            .begin_retry(&invitation_id, &request_id, 1)
            .await
            .unwrap(),
        BetaAccessRetryCommit::Started {
            delivery_attempt: 2
        }
    ));
    store
        .record_delivery(
            &invitation_id,
            &request_id,
            2,
            InvitationDeliveryAttempt::Sent {
                provider_message_id: "email_123".to_string(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.revoke(&request_id).await.unwrap(),
        BetaAccessRevokeResult::Revoked
    );
    assert_eq!(
        store.revoke(&request_id).await.unwrap(),
        BetaAccessRevokeResult::AlreadyRevoked
    );
    assert!(matches!(
        store
            .begin_retry(&invitation_id, &request_id, 2)
            .await
            .unwrap(),
        BetaAccessRetryCommit::Revoked
    ));

    let state = state.lock().await;
    assert_eq!(state.commits.len(), 6);
    assert_eq!(state.commits[1]["writes"].as_array().unwrap().len(), 3);
    assert!(state.commits[2..]
        .iter()
        .all(|commit| commit["writes"].as_array().unwrap().len() == 2));
    assert!(state
        .documents
        .contains_key(&format!("{INVITATIONS_COLLECTION}/{invitation_id}")));
    let serialized = serde_json::to_string(&state.documents).unwrap();
    assert!(!serialized.contains(plaintext_code));
    assert!(serialized.contains("sent"));
    assert!(serialized.contains("revoked"));
    server.abort();
}

#[tokio::test]
async fn firestore_redemption_rate_limits_lookup_and_grant_in_atomic_transactions() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let application = Router::new()
        .fallback(fake_firestore_request)
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, application).await });
    let store = FirestoreBetaAccessStore {
        database: FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
    };
    let player_id = PlayerId::try_from("firebase-player".to_string()).unwrap();
    assert!(!store.player_has_access(&player_id).await.unwrap());
    let now: DateTime<Utc> = "2026-08-02T10:00:00Z".parse().unwrap();
    let request_id = "a".repeat(64);
    let invitation_id = "c".repeat(32);
    let lookup_id = "d".repeat(64);
    let email = NormalizedEmail::parse("player@example.com").unwrap();
    store
        .submit(BetaAccessSubmission {
            email: email.clone(),
            email_rate_key: request_id.clone(),
            ip_rate_key: "b".repeat(64),
            now,
        })
        .await
        .unwrap();
    let invitation = StoredInvitation {
        authenticator: "e".repeat(64),
        authenticator_version: 1,
        ciphertext: "f".repeat(96),
        created_at: now,
        delivery_attempt: 1,
        delivery_retryable: None,
        delivery_status: InvitationDeliveryStatus::Pending,
        email,
        encryption_nonce: "0".repeat(24),
        encryption_version: 1,
        id: invitation_id.clone(),
        lookup_id: lookup_id.clone(),
        provider_message_id: None,
        record_version: 1,
        redeemed_at: None,
        redeemed_by: None,
        request_id: request_id.clone(),
        status: InvitationStatus::Issued,
    };
    assert_eq!(
        store.commit_grant(invitation.clone()).await.unwrap(),
        BetaAccessGrantCommit::Issued
    );

    let target = store
        .redemption_target(BetaAccessRedemptionAttempt {
            ip_rate_key: "1".repeat(64),
            lookup_id: Some(lookup_id.clone()),
            now,
            player_rate_key: "2".repeat(64),
        })
        .await
        .unwrap();
    let BetaAccessRedemptionTarget::Candidate(stored) = target else {
        panic!("expected the issued invitation")
    };
    assert!(matches!(
        store
            .commit_redemption(
                BetaAccessRedemptionCandidate::from(stored.as_ref()),
                player_id.clone(),
                now,
            )
            .await
            .unwrap(),
        BetaAccessRedemptionCommit::Granted
    ));
    assert!(store.player_has_access(&player_id).await.unwrap());
    assert!(matches!(
        store
            .commit_redemption(
                BetaAccessRedemptionCandidate::from(stored.as_ref()),
                player_id.clone(),
                now,
            )
            .await
            .unwrap(),
        BetaAccessRedemptionCommit::AlreadyHandled
    ));
    assert_eq!(
        store.revoke_access(&request_id).await.unwrap(),
        BetaAccessAuthorizationRevokeResult::Revoked
    );
    assert!(!store.player_has_access(&player_id).await.unwrap());
    assert_eq!(
        store.revoke_access(&request_id).await.unwrap(),
        BetaAccessAuthorizationRevokeResult::AlreadyRevoked
    );
    let request = store
        .database
        .get_document::<BetaAccessRequestDocument>(&[ACCESS_REQUESTS_COLLECTION, &request_id])
        .await
        .unwrap()
        .unwrap()
        .into_request(request_id.clone())
        .unwrap();
    assert_eq!(
        request.access_status,
        Some(BetaAccessAuthorizationStatus::Revoked)
    );
    assert_eq!(request.invitation_status, Some(InvitationStatus::Redeemed));

    let state = state.lock().await;
    assert!(state
        .documents
        .contains_key(&format!("{INVITATION_LOOKUPS_COLLECTION}/{lookup_id}")));
    let player_path = hashed_path_segment("firebase-player");
    assert!(!state.documents.contains_key(&format!(
        "{USERS_COLLECTION}/{player_path}/{BETA_ACCESS_COLLECTION}/{BETA_ACCESS_GRANT_ID}"
    )));
    assert_eq!(
        state.documents[&format!("{INVITATIONS_COLLECTION}/{invitation_id}")]["fields"]["status"]
            ["stringValue"],
        "redeemed"
    );
    assert_eq!(state.commits[2]["writes"].as_array().unwrap().len(), 2);
    assert_eq!(state.commits[3]["writes"].as_array().unwrap().len(), 3);
    assert_eq!(state.commits[4]["writes"].as_array().unwrap().len(), 2);
    server.abort();
}

#[test]
fn listed_request_requires_a_normalized_record_and_opaque_id() {
    let document = || BetaAccessRequestDocument {
        schema_version: SCHEMA_VERSION,
        email: "player@example.com".to_string(),
        status: BetaAccessRequestStatus::Pending,
        created_at: "2026-08-02T10:00:00Z".parse().unwrap(),
        delivery_status: None,
        delivery_retryable: None,
        invitation_id: None,
        invitation_status: None,
        access_status: None,
    };

    assert!(document().into_request("a".repeat(64)).is_ok());
    assert!(matches!(
        document().into_request("player@example.com".to_string()),
        Err(BetaAccessStoreError::InvalidRecord)
    ));
}

#[test]
fn a_redeemed_request_from_before_access_status_tracking_projects_active_access() {
    let request = BetaAccessRequestDocument {
        schema_version: SCHEMA_VERSION,
        email: "player@example.com".to_string(),
        status: BetaAccessRequestStatus::Granted,
        created_at: "2026-08-02T10:00:00Z".parse().unwrap(),
        delivery_status: Some(InvitationDeliveryStatus::Sent),
        delivery_retryable: None,
        invitation_id: Some("b".repeat(32)),
        invitation_status: Some(InvitationStatus::Redeemed),
        access_status: None,
    }
    .into_request("a".repeat(64))
    .unwrap();

    assert_eq!(
        request.access_status,
        Some(BetaAccessAuthorizationStatus::Active)
    );
}

async fn fake_firestore_request(
    State(state): State<Arc<Mutex<FakeFirestore>>>,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> Response {
    // A transactional read arrives as `:batchGet`, which is the documented
    // transactional read and the one the Firebase emulator answers.
    if method == Method::POST && uri.path().ends_with("documents:batchGet") {
        let request: Value = serde_json::from_slice(&body).unwrap();
        let name = request["documents"][0].as_str().unwrap().to_string();
        let document_path = name.split("/documents/").nth(1).unwrap().to_string();
        let state = state.lock().await;
        return match state.documents.get(&document_path) {
            Some(document) => {
                Json(serde_json::json!([{ "found": document.clone() }])).into_response()
            }
            None => Json(serde_json::json!([{ "missing": name }])).into_response(),
        };
    }
    let path = uri.path();
    if method == Method::POST && path.ends_with("documents:beginTransaction") {
        return Json(serde_json::json!({ "transaction": "fixture-transaction" })).into_response();
    }
    if method == Method::POST && path.ends_with("documents:commit") {
        let commit: Value = serde_json::from_slice(&body).unwrap();
        let mut state = state.lock().await;
        for write in commit["writes"].as_array().unwrap() {
            if let Some(update) = write.get("update") {
                let name = update["name"].as_str().unwrap();
                let path = name.split("/documents/").nth(1).unwrap().to_string();
                state.documents.insert(path, update.clone());
            } else {
                let name = write["delete"].as_str().unwrap();
                let path = name.split("/documents/").nth(1).unwrap();
                state.documents.remove(path);
            }
        }
        state.commits.push(commit);
        return StatusCode::OK.into_response();
    }
    if method == Method::POST && path.ends_with("documents:rollback") {
        return StatusCode::OK.into_response();
    }
    if method == Method::GET {
        let document_path = path.split("/documents/").nth(1).unwrap_or_default();
        return state
            .lock()
            .await
            .documents
            .get(document_path)
            .cloned()
            .map(Json)
            .map(IntoResponse::into_response)
            .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response());
    }
    StatusCode::BAD_REQUEST.into_response()
}
