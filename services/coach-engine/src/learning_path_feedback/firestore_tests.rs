use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::State,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json, Router,
};
use serde_json::Value;
use tokio::sync::Mutex;

use super::*;
use crate::review_session_contract::{
    CurriculumLearningConcept, LearningPlanSelectionPolicyVersion, LearningResourceCatalogVersion,
};

#[derive(Default)]
struct FakeFirestore {
    commits: Vec<Value>,
    documents: BTreeMap<String, Value>,
    next_transaction: u64,
    revision: u64,
    transaction_revisions: BTreeMap<String, u64>,
}

#[tokio::test]
async fn concurrent_exposure_retries_preserve_one_sample_and_aggregate_count() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let application = Router::new()
        .fallback(fake_firestore_request)
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, application).await });
    let store = Arc::new(FirestoreLearningPathFeedbackStore {
        database: FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
    });
    let player = player("concurrent");
    let sample = sample("concurrent");

    let first = {
        let store = store.clone();
        let player = player.clone();
        let sample = sample.clone();
        tokio::spawn(async move {
            store
                .record_exposure(&player, sample, DeliverySurface::Web)
                .await
        })
    };
    let second = {
        let store = store.clone();
        let player = player.clone();
        let sample = sample.clone();
        tokio::spawn(async move {
            store
                .record_exposure(&player, sample, DeliverySurface::Web)
                .await
        })
    };

    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    let analytics = store.analytics().await.unwrap();
    assert_eq!(analytics.slices.len(), 1);
    assert_eq!(analytics.slices[0].exposure_count, 1);
    assert_eq!(analytics.slices[0].vote_count, 0);
    let state = state.lock().await;
    assert_eq!(
        state
            .documents
            .keys()
            .filter(|path| path.starts_with(ANALYTICS_COLLECTION))
            .count(),
        1
    );
    assert!(state.commits.len() >= 2);
    server.abort();
}

#[tokio::test]
async fn transactional_vote_retry_replaces_and_removes_aggregate_counts() {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let application = Router::new()
        .fallback(fake_firestore_request)
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, application).await });
    let store = FirestoreLearningPathFeedbackStore {
        database: FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
    };
    let player = player("replace");
    let sample = sample("replace");
    for surface in [DeliverySurface::Web, DeliverySurface::CoachApp] {
        store
            .record_exposure(&player, sample.clone(), surface)
            .await
            .unwrap();
    }
    store
        .update_vote(
            &player,
            sample.clone(),
            DeliverySurface::Web,
            Some(LearningPathVote::ThumbsUp),
        )
        .await
        .unwrap();
    store
        .update_vote(
            &player,
            sample.clone(),
            DeliverySurface::CoachApp,
            Some(LearningPathVote::ThumbsDown),
        )
        .await
        .unwrap();
    store
        .update_vote(&player, sample, DeliverySurface::Web, None)
        .await
        .unwrap();

    let analytics = store.analytics().await.unwrap();
    assert_eq!(analytics.slices.len(), 2);
    assert!(analytics.slices.iter().all(|slice| slice.vote_count == 0));
    assert!(analytics
        .slices
        .iter()
        .all(|slice| slice.exposure_count == 1));
    server.abort();
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
        let mut state = state.lock().await;
        state.next_transaction += 1;
        let transaction = format!("fixture-transaction-{}", state.next_transaction);
        let revision = state.revision;
        state
            .transaction_revisions
            .insert(transaction.clone(), revision);
        return Json(serde_json::json!({ "transaction": transaction })).into_response();
    }
    if method == Method::POST && path.ends_with("documents:commit") {
        let commit: Value = serde_json::from_slice(&body).unwrap();
        let mut state = state.lock().await;
        let transaction = commit["transaction"].as_str().unwrap();
        let transaction_revision = state.transaction_revisions.remove(transaction).unwrap();
        if transaction_revision != state.revision {
            return StatusCode::CONFLICT.into_response();
        }
        for write in commit["writes"].as_array().unwrap() {
            let update = &write["update"];
            let name = update["name"].as_str().unwrap();
            let document_path = name.split("/documents/").nth(1).unwrap().to_string();
            state.documents.insert(document_path, update.clone());
        }
        state.revision += 1;
        state.commits.push(commit);
        return StatusCode::OK.into_response();
    }
    if method == Method::POST && path.ends_with("documents:rollback") {
        return StatusCode::OK.into_response();
    }
    if method == Method::GET {
        let document_path = path.split("/documents/").nth(1).unwrap_or_default();
        let state = state.lock().await;
        if document_path == ANALYTICS_COLLECTION {
            let documents = state
                .documents
                .iter()
                .filter(|(path, _)| path.starts_with(&format!("{ANALYTICS_COLLECTION}/")))
                .map(|(_, document)| document.clone())
                .collect::<Vec<_>>();
            return Json(serde_json::json!({ "documents": documents })).into_response();
        }
        return state
            .documents
            .get(document_path)
            .cloned()
            .map(Json)
            .map(IntoResponse::into_response)
            .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response());
    }
    StatusCode::BAD_REQUEST.into_response()
}

fn sample(seed: &str) -> LearningPathSample {
    LearningPathSample {
        learning_path_ref: LearningPathRef::try_from(format!("learning-path:{seed}")).unwrap(),
        game_ref: GameRef::try_from(format!("sha256:{}", "1".repeat(64))).unwrap(),
        critical_moment_id: CriticalMomentId::try_from("moment:1".to_string()).unwrap(),
        key: LearningTrackKey::Curriculum {
            concept: CurriculumLearningConcept::Pin,
        },
        purpose: LearningTrackPurpose::Improvement,
        proof_family: LearningProofFamily::DecisionExplanation,
        selection_policy_version: LearningPlanSelectionPolicyVersion::V1,
        resource_catalog_version: LearningResourceCatalogVersion::V2026_08_03,
        resource_ids: Vec::new(),
    }
}

fn player(seed: &str) -> PlayerId {
    PlayerId::try_from(format!("player:{seed}")).unwrap()
}
