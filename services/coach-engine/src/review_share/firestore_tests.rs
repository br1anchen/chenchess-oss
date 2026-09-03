use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::State,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json,
};
use chrono::TimeZone;
use serde_json::Value;
use tokio::sync::Mutex;

use super::*;

#[derive(Default)]
struct FakeFirestore {
    documents: BTreeMap<String, Value>,
}

#[tokio::test]
async fn a_grant_lands_in_the_owner_subtree_and_reads_back_whole() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let (store, server) = serve(state.clone()).await;
    let owner = player();
    let owner_segment = player_subtree_owner(&owner);
    let grant = grant(&owner);

    store.put(&owner_segment, grant.clone()).await.unwrap();

    let stored = state.lock().await;
    // Account deletion recursively removes `users/{owner}`, so living inside it
    // is what makes an outstanding link die with the account.
    let subtree = crate::account_deletion::application_data_document_path(&owner).join("/");
    assert!(stored
        .documents
        .keys()
        .all(|path| path == &format!("{subtree}/{REVIEW_SHARES_COLLECTION}/{}", grant.share_id)));
    drop(stored);

    assert_eq!(
        store.get(&owner_segment, &grant.share_id).await.unwrap(),
        Some(grant)
    );
    server.abort();
}

#[tokio::test]
async fn nothing_stored_can_reconstruct_the_link() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let (store, server) = serve(state.clone()).await;
    let owner = player();
    let secret = "e".repeat(64);
    let stored_grant = ReviewShareGrant {
        share_id: share_id(&secret),
        ..grant(&owner)
    };

    store
        .put(&player_subtree_owner(&owner), stored_grant)
        .await
        .unwrap();

    let stored = state.lock().await;
    let serialized = serde_json::to_string(&stored.documents).unwrap();
    assert!(
        !serialized.contains(&secret),
        "the durable record must never carry the secret that resolves it"
    );
    server.abort();
}

#[tokio::test]
async fn revoking_removes_the_document_and_repeats_harmlessly() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let (store, server) = serve(state.clone()).await;
    let owner = player();
    let owner_segment = player_subtree_owner(&owner);
    let grant = grant(&owner);
    store.put(&owner_segment, grant.clone()).await.unwrap();

    store.delete(&owner_segment, &grant.share_id).await.unwrap();
    store.delete(&owner_segment, &grant.share_id).await.unwrap();

    assert!(state.lock().await.documents.is_empty());
    assert_eq!(
        store.get(&owner_segment, &grant.share_id).await.unwrap(),
        None
    );
    server.abort();
}

async fn serve(
    state: Arc<Mutex<FakeFirestore>>,
) -> (FirestoreReviewShareStore, tokio::task::JoinHandle<()>) {
    let application = axum::Router::new()
        .fallback(fake_firestore_request)
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, application).await;
    });
    (
        FirestoreReviewShareStore {
            database: FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
        },
        server,
    )
}

async fn fake_firestore_request(
    State(state): State<Arc<Mutex<FakeFirestore>>>,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> Response {
    if method == Method::POST && uri.path().ends_with("documents:commit") {
        let commit: Value = serde_json::from_slice(&body).unwrap();
        let mut state = state.lock().await;
        for write in commit["writes"].as_array().unwrap() {
            if let Some(deleted) = write.get("delete").and_then(Value::as_str) {
                state
                    .documents
                    .remove(deleted.split("/documents/").nth(1).unwrap());
                continue;
            }
            let update = &write["update"];
            let document_path = update["name"]
                .as_str()
                .unwrap()
                .split("/documents/")
                .nth(1)
                .unwrap()
                .to_string();
            if write["currentDocument"]["exists"].as_bool() == Some(false)
                && state.documents.contains_key(&document_path)
            {
                return StatusCode::CONFLICT.into_response();
            }
            state.documents.insert(document_path, update.clone());
        }
        return StatusCode::OK.into_response();
    }
    if method == Method::GET {
        let requested = uri.path().split("/documents/").nth(1).unwrap_or_default();
        let state = state.lock().await;
        return match state.documents.get(requested) {
            Some(document) => Json(document.clone()).into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }
    StatusCode::BAD_REQUEST.into_response()
}

fn player() -> PlayerId {
    PlayerId::try_from("firebase-player-durable".to_string()).unwrap()
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap()
}

fn grant(owner: &PlayerId) -> ReviewShareGrant {
    ReviewShareGrant {
        share_id: "d".repeat(64),
        owner: owner.clone(),
        address: ReviewShareAddress {
            game_import_id: GameImportId::try_from(format!(
                "game-import:{}:{}",
                "a".repeat(64),
                "b".repeat(32)
            ))
            .unwrap(),
            review_moment_id: CriticalMomentId::try_from("moment:1".to_string()).unwrap(),
            sequence_kind: Some(MoveSequencePresentationKind::PlayedMoveRefutation),
        },
        expires_at: now(),
    }
}
