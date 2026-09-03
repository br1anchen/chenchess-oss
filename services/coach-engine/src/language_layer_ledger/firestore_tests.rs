use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::State,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio::sync::Mutex;

use std::time::Duration;

use super::*;
use crate::account_deletion::application_data_document_path;
use crate::firestore::FirestoreDatabase;
use crate::review_durability::path::hashed_path_segment;
use crate::review_session_contract::PlayerId;

#[derive(Default)]
struct FakeFirestore {
    documents: BTreeMap<String, Value>,
    commits: Vec<Vec<String>>,
    rollbacks: usize,
    transactions: usize,
}

#[tokio::test]
async fn settle_commits_record_player_day_and_global_month_together() {
    let (ledger, state, server) = serve().await;
    let record = billed("ll-settle-1", 1_500);

    ledger.settle(record.clone()).await.unwrap();

    let locked = state.lock().await;
    assert_eq!(locked.commits.len(), 1, "settle must be one transaction");
    let writes = &locked.commits[0];
    assert_eq!(writes.len(), 3, "record + player day + global month");
    let player = application_data_document_path(&record.player_id);
    let player_prefix = format!("{}/{}", player[0], player[1]);
    let day = super::day_key(record.settled_at);
    let month = super::month_key(record.settled_at);
    let record_path = format!(
        "{player_prefix}/languageLayerRecords/{}",
        hashed_path_segment(&record.request_id)
    );
    let day_path = format!("{player_prefix}/languageLayerSpend/{day}");
    let global_path = format!("languageLayerGlobalSpend/{month}");
    assert!(writes.contains(&record_path), "{writes:?}");
    assert!(writes.contains(&day_path), "{writes:?}");
    assert!(writes.contains(&global_path), "{writes:?}");
    assert!(
        !global_path.starts_with("users/"),
        "global month must not live under the player"
    );
    assert!(
        writes
            .iter()
            .filter(|path| path.starts_with(&player_prefix))
            .count()
            == 2
    );
    drop(locked);

    assert_eq!(
        ledger
            .player_rolling_30_day(&record.player_id, record.settled_at)
            .await
            .unwrap(),
        1_500
    );
    assert_eq!(
        ledger
            .global_calendar_month(record.settled_at)
            .await
            .unwrap(),
        1_500
    );
    server.abort();
}

#[tokio::test]
async fn settle_writes_a_new_record_for_each_distinct_request_id() {
    let (ledger, state, server) = serve().await;
    ledger.settle(billed("ll-1", 800)).await.unwrap();
    let fresh = billed(&next_request_id(), 900);
    ledger.settle(fresh.clone()).await.unwrap();

    let locked = state.lock().await;
    let record_writes = locked
        .commits
        .iter()
        .flatten()
        .filter(|path| path.contains("/languageLayerRecords/"))
        .count();
    assert_eq!(
        record_writes, 2,
        "distinct request ids must each write a record: {:?}",
        locked.commits
    );
    assert!(locked
        .commits
        .iter()
        .flatten()
        .any(|path| { path.ends_with(&hashed_path_segment(&fresh.request_id)) }));
    drop(locked);
    server.abort();
}

#[tokio::test]
async fn settle_is_idempotent_when_the_record_already_exists() {
    let (ledger, state, server) = serve().await;
    let record = billed("ll-settle-dup", 2_000);

    ledger.settle(record.clone()).await.unwrap();
    ledger.settle(record.clone()).await.unwrap();

    let locked = state.lock().await;
    assert_eq!(locked.commits.len(), 1, "replay must not write again");
    assert!(
        locked.rollbacks >= 1,
        "existing record must roll the txn back"
    );
    drop(locked);

    assert_eq!(
        ledger
            .player_rolling_30_day(&record.player_id, record.settled_at)
            .await
            .unwrap(),
        2_000
    );
    assert_eq!(
        ledger
            .global_calendar_month(record.settled_at)
            .await
            .unwrap(),
        2_000
    );
    server.abort();
}

#[tokio::test]
async fn settle_bills_only_when_admitted_and_cost_is_positive() {
    let (ledger, state, server) = serve().await;
    let player_id = player();
    let as_of = as_of();

    ledger
        .settle(LanguageLayerOperationalRecord {
            request_id: "ll-denied".into(),
            player_id: player_id.clone(),
            settled_at: as_of,
            latency: Duration::ZERO,
            cost_micros: 4_000,
            prompt_tokens: None,
            completion_tokens: None,
            budget_decision: BudgetDecision::Denied,
            denial_reason: Some(DenialReason::PlayerCeiling),
            error_class: None,
            pin_verification: crate::evaluation_fingerprint::PinVerificationVerdict::NotApplicable,
            pin_cause: None,
            fingerprint_digest: "sha256:denied".into(),
            capture_outcome: None,
            provider_cooldown: None,
            steps: Vec::new(),
        })
        .await
        .unwrap();
    ledger
        .settle(LanguageLayerOperationalRecord {
            request_id: "ll-zero".into(),
            player_id: player_id.clone(),
            settled_at: as_of,
            latency: Duration::ZERO,
            cost_micros: 0,
            prompt_tokens: None,
            completion_tokens: None,
            budget_decision: BudgetDecision::Admitted,
            denial_reason: None,
            error_class: None,
            pin_verification: crate::evaluation_fingerprint::PinVerificationVerdict::NotApplicable,
            pin_cause: None,
            fingerprint_digest: "sha256:zero".into(),
            capture_outcome: None,
            provider_cooldown: None,
            steps: Vec::new(),
        })
        .await
        .unwrap();

    let locked = state.lock().await;
    assert_eq!(locked.commits.len(), 2);
    assert!(
        locked.commits.iter().all(|writes| writes.len() == 1),
        "denied and zero-cost must write only the operational record: {:?}",
        locked.commits
    );
    assert!(locked
        .commits
        .iter()
        .flatten()
        .all(|path| path.contains("/languageLayerRecords/")));
    drop(locked);

    assert_eq!(
        ledger
            .player_rolling_30_day(&player_id, as_of)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ledger.global_calendar_month(as_of).await.unwrap(), 0);
    server.abort();
}

fn player() -> PlayerId {
    PlayerId::try_from("firebase-player-ledger-settle".to_string()).unwrap()
}

fn as_of() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-18T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn billed(request_id: &str, cost_micros: i64) -> LanguageLayerOperationalRecord {
    LanguageLayerOperationalRecord {
        request_id: request_id.to_string(),
        player_id: player(),
        settled_at: as_of(),
        latency: Duration::from_millis(12),
        cost_micros,
        prompt_tokens: Some(80),
        completion_tokens: Some(40),
        budget_decision: BudgetDecision::Admitted,
        denial_reason: None,
        error_class: None,
        pin_verification: crate::evaluation_fingerprint::PinVerificationVerdict::NotApplicable,
        pin_cause: None,
        fingerprint_digest: "sha256:ledger-settle".into(),
        capture_outcome: None,
        provider_cooldown: None,
        steps: Vec::new(),
    }
}

async fn serve() -> (
    FirestoreLanguageLayerLedger,
    Arc<Mutex<FakeFirestore>>,
    tokio::task::JoinHandle<()>,
) {
    let state = Arc::new(Mutex::new(FakeFirestore::default()));
    let application = axum::Router::new()
        .fallback(fake_firestore_request)
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, application).await;
    });
    let ledger = FirestoreLanguageLayerLedger::new(
        FirestoreDatabase::emulator("chenchess-test", address.to_string()).unwrap(),
    );
    (ledger, state, server)
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
    if method == Method::POST && uri.path().ends_with("documents:beginTransaction") {
        let mut state = state.lock().await;
        state.transactions += 1;
        let token = format!("txn-{}", state.transactions);
        return Json(serde_json::json!({ "transaction": token })).into_response();
    }
    if method == Method::POST && uri.path().ends_with("documents:rollback") {
        state.lock().await.rollbacks += 1;
        return StatusCode::OK.into_response();
    }
    if method == Method::POST && uri.path().ends_with("documents:commit") {
        let commit: Value = serde_json::from_slice(&body).unwrap();
        let mut state = state.lock().await;
        let mut written = Vec::new();
        for write in commit["writes"].as_array().unwrap() {
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
            state
                .documents
                .insert(document_path.clone(), update.clone());
            written.push(document_path);
        }
        state.commits.push(written);
        return StatusCode::OK.into_response();
    }
    if method == Method::GET {
        let requested = uri.path().split("/documents/").nth(1).unwrap_or_default();
        let state = state.lock().await;
        if let Some(document) = state.documents.get(requested) {
            return Json(document.clone()).into_response();
        }
        return StatusCode::NOT_FOUND.into_response();
    }
    StatusCode::BAD_REQUEST.into_response()
}

async fn real_emulator_ledger() -> Option<FirestoreLanguageLayerLedger> {
    let host = std::env::var("FIRESTORE_EMULATOR_HOST").ok()?;
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    let database = FirestoreDatabase::emulator("chenchess-test", host).ok()?;
    match database
        .get_document::<serde_json::Value>(&["languageLayerGlobalSpend", "__372-probe"])
        .await
    {
        Ok(_) => Some(FirestoreLanguageLayerLedger::new(database)),
        Err(_) => {
            eprintln!("skipping: FIRESTORE_EMULATOR_HOST is down");
            None
        }
    }
}

#[tokio::test]
async fn production_emulator_settle_skips_when_emulator_is_down() {
    let Some(ledger) = real_emulator_ledger().await else {
        eprintln!("skipping: FIRESTORE_EMULATOR_HOST is unset or down");
        return;
    };
    let record = billed(
        &format!(
            "ll-emu-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ),
        1_250,
    );
    ledger.settle(record.clone()).await.unwrap();
    ledger.settle(record.clone()).await.unwrap();
    assert_eq!(
        ledger
            .player_rolling_30_day(&record.player_id, record.settled_at)
            .await
            .unwrap(),
        1_250
    );
    assert_eq!(
        ledger
            .global_calendar_month(record.settled_at)
            .await
            .unwrap(),
        1_250
    );

    ledger
        .settle(LanguageLayerOperationalRecord {
            request_id: format!("{}-denied", record.request_id),
            player_id: record.player_id.clone(),
            settled_at: record.settled_at,
            latency: Duration::ZERO,
            cost_micros: 9_000,
            prompt_tokens: None,
            completion_tokens: None,
            budget_decision: BudgetDecision::Denied,
            denial_reason: Some(DenialReason::GlobalCeiling),
            error_class: None,
            pin_verification: crate::evaluation_fingerprint::PinVerificationVerdict::NotApplicable,
            pin_cause: None,
            fingerprint_digest: "sha256:emu-denied".into(),
            capture_outcome: None,
            provider_cooldown: None,
            steps: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(
        ledger
            .player_rolling_30_day(&record.player_id, record.settled_at)
            .await
            .unwrap(),
        1_250,
        "denied settle must not bill"
    );
}
