use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::State,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;
use tokio::sync::Mutex;

use super::*;
use crate::review_session_contract::{ArtifactDigest, CriticalMomentGroundingLedger, PlayerId};

#[derive(Default)]
struct FakeFirestore {
    documents: BTreeMap<String, Value>,
    deletes: usize,
    updates: usize,
}

#[tokio::test]
async fn a_replayed_key_reads_back_its_original_annotation_without_a_second_write() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let (store, server) = serve(state.clone()).await;
    let address = address();

    let first = store
        .append(&address, annotation("once", "first text"))
        .await
        .unwrap();
    let replayed = store
        .append(&address, annotation("once", "rewritten text"))
        .await
        .unwrap();

    assert_eq!(replayed, first);
    let stored = state.lock().await;
    assert_eq!(stored.documents.len(), 1);
    // Append-only: the store never issues an update or a delete.
    assert_eq!((stored.updates, stored.deletes), (0, 0));
    server.abort();
}

#[tokio::test]
async fn every_annotation_lands_under_the_owning_player_subtree() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let (store, server) = serve(state.clone()).await;
    let address = address();

    store
        .append(&address, annotation("first", "earlier"))
        .await
        .unwrap();
    store
        .append(&address, annotation("second", "later"))
        .await
        .unwrap();

    let stored = state.lock().await;
    assert_eq!(stored.documents.len(), 2);
    // Account deletion recursively removes `users/{owner}`, so living inside it
    // is what makes the annotation store erasable.
    let player_subtree = crate::account_deletion::application_data_document_path(
        &PlayerId::try_from("firebase-player-durable".to_string()).unwrap(),
    )
    .join("/");
    assert!(stored
        .documents
        .keys()
        .all(|path| path.starts_with(&format!(
            "{player_subtree}/{REVIEW_ANNOTATIONS_COLLECTION}/"
        ))));
    drop(stored);

    let read = store.read(&address).await.unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(
        read.active(&CriticalMomentId::try_from("moment:1".to_string()).unwrap())
            .map(|active| active.comment.text.as_str()),
        Some("later")
    );
    server.abort();
}

async fn serve(
    state: Arc<Mutex<FakeFirestore>>,
) -> (FirestoreReviewAnnotationStore, tokio::task::JoinHandle<()>) {
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
        FirestoreReviewAnnotationStore {
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
            if write.get("delete").is_some() {
                state.deletes += 1;
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
            let exists_precondition = write["currentDocument"]["exists"].as_bool();
            if exists_precondition == Some(false) && state.documents.contains_key(&document_path) {
                return StatusCode::CONFLICT.into_response();
            }
            if exists_precondition != Some(false) {
                state.updates += 1;
            }
            state.documents.insert(document_path, update.clone());
        }
        return StatusCode::OK.into_response();
    }
    if method == Method::GET {
        let requested = uri.path().split("/documents/").nth(1).unwrap_or_default();
        let state = state.lock().await;
        if let Some(document) = state.documents.get(requested) {
            return Json(document.clone()).into_response();
        }
        let prefix = format!("{requested}/");
        let documents = state
            .documents
            .iter()
            .filter(|(path, _)| path.starts_with(&prefix))
            .map(|(_, document)| document.clone())
            .collect::<Vec<_>>();
        if documents.is_empty() {
            return StatusCode::NOT_FOUND.into_response();
        }
        return Json(serde_json::json!({ "documents": documents })).into_response();
    }
    StatusCode::BAD_REQUEST.into_response()
}

fn address() -> ReviewAnnotationAddress {
    ReviewAnnotationAddress {
        owner: ProcessorPrincipal::Player(
            PlayerId::try_from("firebase-player-durable".to_string()).unwrap(),
        ),
        game_import_id: GameImportId::try_from(format!(
            "game-import:{}:{}",
            "a".repeat(64),
            "b".repeat(32)
        ))
        .unwrap(),
    }
}

fn annotation(key: &str, text: &str) -> ReviewMomentAnnotation {
    ReviewMomentAnnotation {
        moment_id: CriticalMomentId::try_from("moment:1".to_string()).unwrap(),
        idempotency_key: IdempotencyKey::try_from(format!("idempotency-key:test:{key}")).unwrap(),
        comment: CriticalMomentComment {
            text: text.to_string(),
        },
        authoring_provenance: CriticalMomentCommentAuthoringProvenance::hosted_authored(
            CriticalMomentGroundingLedger {
                facts_ref: ArtifactDigest::try_from(format!("sha256:{}", "c".repeat(64))).unwrap(),
                factual_claims: Vec::new(),
            },
            1,
        ),
        published_at: match key {
            "second" => "2026-08-09T12:00:00Z".parse().unwrap(),
            _ => "2026-08-09T10:00:00Z".parse().unwrap(),
        },
    }
}
