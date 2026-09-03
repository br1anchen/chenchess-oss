use std::collections::BTreeMap;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use tokio::sync::Mutex;

use super::*;
use crate::{
    engine_analysis::EngineProvenance,
    evaluation_fingerprint::{
        evaluation_fingerprint, CaptureOrigin, CaptureOutcome, CaptureTrigger,
        EvaluationEnvironment, EvaluationFingerprint, EvaluationFingerprintAxes,
        EvaluationGenerationSettings, LanguageLayerAttestation, PinVerificationVerdict,
        StructuredOutputMode,
    },
    game_import_store::GameImportRecord,
    language_layer_provider::{CompletionAttempt, CompletionOutcome},
    pin_record::compiled_pin_record,
    quality_capture::{
        hosted_language_layer_capture, HostedGenerationInput, HostedLanguageLayerTask,
        QualityCaptureAppender,
    },
    review_session_contract::{
        ArtifactDigest, DeliverySurface, GameImportId, ImportedGame, OperationCompletion,
        ReviewSessionEvent, ReviewSessionEventEnvelope,
    },
    review_session_processor::ProcessorPrincipal,
};
use std::time::Duration;

struct FakeFirestoreState {
    application_database: &'static str,
    preference_fields: Option<Value>,
    quality_capture: Option<Value>,
    withdrawal: Option<Value>,
    application_commits: Vec<Value>,
    quality_commits: Vec<Value>,
}

#[tokio::test]
async fn production_gate_prepares_one_atomic_outbox_append_with_hashed_paths() {
    let (address, state, server) = fake_firestore().await;
    let database = FirestoreDatabase::production_emulator("chenchess-test", address).unwrap();
    let player_id = player();
    let capture = fixture_capture();

    let writes = prepare_outbox_writes(&database, &player_id, &capture)
        .await
        .unwrap();
    let encoded = serde_json::to_value(&writes).unwrap();
    let serialized = serde_json::to_string(&encoded).unwrap();

    assert_eq!(writes.len(), 2);
    assert_eq!(
        encoded[0]["update"]["name"],
        format!(
            "projects/chenchess-test/databases/coach-app-production/documents/users/{}",
            user_document_id(&player_id)
        )
    );
    assert_eq!(
        encoded[1]["update"]["name"],
        format!(
            "projects/chenchess-test/databases/coach-app-production/documents/users/{}/qualityOutbox/{}",
            user_document_id(&player_id),
            capture_document_id(&capture.capture_id)
        )
    );
    assert_eq!(
        encoded[1]["update"]["fields"]["status"]["stringValue"],
        PENDING_EXPORT_STATUS
    );
    assert!(encoded[1]["update"]["fields"]["payload"]["stringValue"].is_string());
    for forbidden in [
        player_id.as_str(),
        "https://lichess.org",
        "synthetic-white",
        "synthetic-white",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "outbox contains {forbidden}"
        );
    }
    assert!(state.lock().await.application_commits.is_empty());
    server.abort();
}

#[tokio::test]
async fn production_gate_requires_acknowledged_enabled_preference() {
    let (address, state, server) = fake_firestore().await;
    let database = FirestoreDatabase::production_emulator("chenchess-test", address).unwrap();
    let player_id = player();
    let capture = fixture_capture();

    state.lock().await.preference_fields = None;
    assert!(prepare_outbox_writes(&database, &player_id, &capture)
        .await
        .unwrap()
        .is_empty());

    state.lock().await.preference_fields =
        Some(preference_fields(false, CURRENT_DISCLOSURE_VERSION));
    assert!(prepare_outbox_writes(&database, &player_id, &capture)
        .await
        .unwrap()
        .is_empty());

    state.lock().await.preference_fields = Some(preference_fields(true, 0));
    assert!(prepare_outbox_writes(&database, &player_id, &capture)
        .await
        .unwrap()
        .is_empty());
    server.abort();
}

#[tokio::test]
async fn staging_gate_prepares_one_atomic_outbox_append() {
    let (address, _state, server) = fake_firestore_for("coach-app-staging").await;
    let database = FirestoreDatabase::emulator("chenchess-test", address).unwrap();
    let player_id = player();
    let capture = fixture_capture();

    let writes = prepare_outbox_writes(&database, &player_id, &capture)
        .await
        .unwrap();
    let encoded = serde_json::to_value(&writes).unwrap();
    assert_eq!(writes.len(), 2);
    assert!(encoded[1]["update"]["name"]
        .as_str()
        .unwrap()
        .contains("coach-app-staging"));
    assert_eq!(
        encoded[1]["update"]["fields"]["status"]["stringValue"],
        PENDING_EXPORT_STATUS
    );
    server.abort();
}

#[tokio::test]
async fn preference_off_holds_a_language_layer_generation() {
    let (address, state, server) = fake_firestore().await;
    let database = FirestoreDatabase::production_emulator("chenchess-test", address).unwrap();
    state.lock().await.preference_fields =
        Some(preference_fields(false, CURRENT_DISCLOSURE_VERSION));
    let capture = fixture_hosted_capture(CaptureTrigger::Preference, CaptureOutcome::Published);
    let writes = prepare_outbox_writes(&database, &player(), &capture)
        .await
        .unwrap();
    let encoded = serde_json::to_value(&writes).unwrap();
    assert_eq!(writes.len(), 2);
    assert!(encoded[1]["update"]["name"]
        .as_str()
        .unwrap()
        .contains("/heldQualityCaptures/"));
    server.abort();
}

#[tokio::test]
async fn feedback_induced_writes_the_outbox_when_preference_is_off() {
    let (address, state, server) = fake_firestore().await;
    let database = FirestoreDatabase::production_emulator("chenchess-test", address).unwrap();
    state.lock().await.preference_fields =
        Some(preference_fields(false, CURRENT_DISCLOSURE_VERSION));
    let capture =
        fixture_hosted_capture(CaptureTrigger::FeedbackInduced, CaptureOutcome::Published);
    let writes = prepare_outbox_writes(&database, &player(), &capture)
        .await
        .unwrap();
    let encoded = serde_json::to_value(&writes).unwrap();
    assert_eq!(writes.len(), 2);
    assert!(encoded[1]["update"]["name"]
        .as_str()
        .unwrap()
        .contains("/qualityOutbox/"));
    assert_eq!(
        encoded[1]["update"]["fields"]["status"]["stringValue"],
        PENDING_EXPORT_STATUS
    );
    server.abort();
}

#[tokio::test]
async fn failed_and_rejected_language_layer_generations_use_the_same_preference_gate() {
    let (address, _state, server) = fake_firestore().await;
    let database = FirestoreDatabase::production_emulator("chenchess-test", address).unwrap();
    for outcome in [CaptureOutcome::Failed, CaptureOutcome::Rejected] {
        let capture = fixture_hosted_capture(CaptureTrigger::Preference, outcome);
        let writes = prepare_outbox_writes(&database, &player(), &capture)
            .await
            .unwrap();
        assert_eq!(writes.len(), 2, "{outcome:?} must share the enabled gate");
        let encoded = serde_json::to_value(&writes).unwrap();
        assert!(encoded[1]["update"]["name"]
            .as_str()
            .unwrap()
            .contains("/qualityOutbox/"));
    }
    server.abort();
}

#[tokio::test]
async fn language_layer_outbox_excludes_identity_and_raw_payloads() {
    let (address, _state, server) = fake_firestore().await;
    let database = FirestoreDatabase::production_emulator("chenchess-test", address).unwrap();
    let capture = fixture_hosted_capture(CaptureTrigger::Preference, CaptureOutcome::Published);
    let writes = prepare_outbox_writes(&database, &player(), &capture)
        .await
        .unwrap();
    let serialized = serde_json::to_string(&writes).unwrap();
    for forbidden in [
        player().as_str(),
        "gen-secret",
        "15:04:05",
        "latency",
        "Keep the rook.",
        "requestId",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "outbox leaked {forbidden}: {serialized}"
        );
    }
    server.abort();
}

/// Export clears the payload, so the Evaluation Fingerprint digest on the row
/// is all a later Review Feedback Report has to point at.
#[test]
fn an_exported_outbox_row_still_anchors_feedback() {
    let capture = fixture_hosted_capture(CaptureTrigger::Preference, CaptureOutcome::Published);
    let expected = capture
        .feedback_anchor()
        .expect("a language-layer generation anchors feedback")
        .fingerprint_digest;
    let mut document = QualityOutboxDocument::pending(capture);
    assert_eq!(document.fingerprint_digest.as_ref(), Some(&expected));

    document.mark_exported();

    assert!(
        document.payload.is_none(),
        "export leaves no generation content in the product database"
    );
    assert_eq!(
        document.fingerprint_digest.as_ref(),
        Some(&expected),
        "feedback after export must still resolve the fingerprint"
    );
    assert!(
        document.into_withdrawal().fingerprint_digest.is_none(),
        "a withdrawn capture is never an anchor"
    );
}

#[test]
fn a_game_analysis_row_is_not_a_feedback_anchor() {
    assert!(
        QualityOutboxDocument::pending(fixture_capture())
            .fingerprint_digest
            .is_none(),
        "only Language Layer generations carry an Evaluation Fingerprint"
    );
}

#[tokio::test]
async fn export_error_does_not_fail_the_business_command() {
    let appender = QualityCaptureAppender::for_application(
        FirestoreDatabase::production_emulator("chenchess-test", "127.0.0.1:9").unwrap(),
    );
    let writes = appender
        .prepare_firestore_writes(&ProcessorPrincipal::Player(player()), &[fixture_capture()])
        .await;
    assert!(
        writes.is_empty(),
        "a transport failure must drop the capture rather than fail the command"
    );
}

#[tokio::test]
async fn exporter_accepts_an_identical_retry_and_blocks_a_digest_conflict() {
    let (address, state, server) = fake_firestore().await;
    let store = FirestoreQualityCaptureStore::new(
        FirestoreDatabase::production_emulator("chenchess-test", address.clone()).unwrap(),
        FirestoreDatabase::quality_emulator("chenchess-test", address).unwrap(),
    );
    let capture = fixture_capture();
    let pending = QualityOutboxDocument::pending(capture.clone());

    store
        .export_one(
            versioned(pending.clone(), "2026-08-01T10:00:01Z"),
            capture.created_at,
        )
        .await
        .unwrap();
    let first_stored = state.lock().await.quality_capture.clone().unwrap();

    store
        .export_one(
            versioned(pending.clone(), "2026-08-01T10:00:02Z"),
            capture.created_at,
        )
        .await
        .unwrap();
    assert_eq!(
        state.lock().await.quality_capture.as_ref(),
        Some(&first_stored)
    );

    state.lock().await.quality_capture.as_mut().unwrap()["fields"]["contentDigest"]
        ["stringValue"] = Value::String(format!("sha256:{}", "f".repeat(64)));
    let error = store
        .export_one(
            versioned(pending, "2026-08-01T10:00:03Z"),
            capture.created_at,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, QualityCaptureStoreError::Conflict));
    let locked = state.lock().await;
    let last_commit = locked.application_commits.last().unwrap();
    assert_eq!(
        last_commit["writes"][0]["update"]["fields"]["status"]["stringValue"],
        "digestConflict"
    );
    assert!(
        last_commit["writes"][0]["update"]["fields"]["payload"].is_null(),
        "a blocked outbox must discard the quality payload"
    );
    drop(locked);
    server.abort();
}

#[tokio::test]
async fn exporter_rejects_a_tampered_outbox_before_quality_persistence() {
    let (address, state, server) = fake_firestore().await;
    let store = FirestoreQualityCaptureStore::new(
        FirestoreDatabase::production_emulator("chenchess-test", address.clone()).unwrap(),
        FirestoreDatabase::quality_emulator("chenchess-test", address).unwrap(),
    );
    let capture = fixture_capture();
    let mut pending = QualityOutboxDocument::pending(capture.clone());
    pending.content_digest = crate::review_session_contract::ArtifactDigest::try_from(format!(
        "sha256:{}",
        "f".repeat(64)
    ))
    .unwrap();

    let error = store
        .export_one(
            versioned(pending, "2026-08-01T10:00:01Z"),
            capture.created_at,
        )
        .await
        .unwrap_err();

    assert!(matches!(error, QualityCaptureStoreError::InvalidRecord));
    assert!(state.lock().await.quality_capture.is_none());
    server.abort();
}

#[tokio::test]
async fn admitted_opt_out_deletes_content_and_leaves_a_non_content_tombstone() {
    let (address, state, server) = fake_firestore().await;
    let store = FirestoreQualityCaptureStore::new(
        FirestoreDatabase::production_emulator("chenchess-test", address.clone()).unwrap(),
        FirestoreDatabase::quality_emulator("chenchess-test", address).unwrap(),
    );
    let capture = fixture_capture();
    let pending = QualityOutboxDocument::pending(capture.clone());
    store
        .export_one(
            versioned(pending.clone(), "2026-08-01T10:00:01Z"),
            capture.created_at,
        )
        .await
        .unwrap();
    let mut withdrawal = pending.into_withdrawal();
    withdrawal.admitted = true;

    store
        .withdraw_one(versioned(withdrawal, "2026-08-01T10:00:02Z"))
        .await
        .unwrap();

    let locked = state.lock().await;
    assert!(locked.quality_capture.is_none());
    let tombstone = locked.withdrawal.as_ref().unwrap();
    assert_eq!(
        tombstone["fields"]["contentDigest"]["stringValue"],
        capture.content_digest.as_str()
    );
    assert!(tombstone["fields"]["payload"].is_null());
    assert!(locked.application_commits.last().unwrap()["writes"][0]["delete"].is_string());
    drop(locked);
    server.abort();
}

fn versioned(
    value: QualityOutboxDocument,
    update_time: &str,
) -> FirestoreVersionedDocumentAtPath<QualityOutboxDocument> {
    FirestoreVersionedDocumentAtPath {
        path: vec![
            USERS_COLLECTION.to_string(),
            user_document_id(&player()),
            QUALITY_OUTBOX_COLLECTION.to_string(),
            capture_document_id(&value.capture_id),
        ],
        value,
        update_time: update_time.to_string(),
    }
}

async fn fake_firestore() -> (
    String,
    std::sync::Arc<Mutex<FakeFirestoreState>>,
    tokio::task::JoinHandle<Result<(), std::io::Error>>,
) {
    fake_firestore_for("coach-app-production").await
}

async fn fake_firestore_for(
    application_database: &'static str,
) -> (
    String,
    std::sync::Arc<Mutex<FakeFirestoreState>>,
    tokio::task::JoinHandle<Result<(), std::io::Error>>,
) {
    let state = std::sync::Arc::new(Mutex::new(FakeFirestoreState {
        application_database,
        preference_fields: Some(preference_fields(true, CURRENT_DISCLOSURE_VERSION)),
        quality_capture: None,
        withdrawal: None,
        application_commits: Vec::new(),
        quality_commits: Vec::new(),
    }));
    let user_route =
        format!("/v1/projects/chenchess-test/databases/{application_database}/documents/users/:id");
    let commit_route =
        format!("/v1/projects/chenchess-test/databases/{application_database}/documents:commit");
    let application = Router::new()
        .route(Box::leak(user_route.into_boxed_str()), get(read_preference))
        .route(
            Box::leak(commit_route.into_boxed_str()),
            post(commit_application),
        )
        .route(
            "/v1/projects/chenchess-test/databases/coach-quality/documents/captures",
            post(create_capture),
        )
        .route(
            "/v1/projects/chenchess-test/databases/coach-quality/documents/captures/:id",
            get(read_capture),
        )
        .route(
            "/v1/projects/chenchess-test/databases/coach-quality/documents/withdrawals/:id",
            get(read_withdrawal),
        )
        .route(
            "/v1/projects/chenchess-test/databases/coach-quality/documents:commit",
            post(commit_quality),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move { axum::serve(listener, application).await });
    (address, state, server)
}

async fn read_preference(
    State(state): State<std::sync::Arc<Mutex<FakeFirestoreState>>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let locked = state.lock().await;
    let fields = locked
        .preference_fields
        .clone()
        .ok_or(StatusCode::NOT_FOUND)?;
    let application_database = locked.application_database;
    drop(locked);
    Ok(Json(serde_json::json!({
        "name": format!(
            "projects/chenchess-test/databases/{application_database}/documents/users/{id}"
        ),
        "updateTime": "2026-08-01T09:59:59Z",
        "fields": fields
    })))
}

fn preference_fields(enabled: bool, disclosure_version: u16) -> Value {
    serde_json::json!({
        "schemaVersion": { "integerValue": "1" },
        "createdAt": { "timestampValue": "2026-08-01T09:00:00Z" },
        "updatedAt": { "timestampValue": "2026-08-01T09:00:00Z" },
        "captureEnabled": { "booleanValue": enabled },
        "acknowledgedDisclosureVersion": {
            "integerValue": disclosure_version.to_string()
        }
    })
}

async fn create_capture(
    State(state): State<std::sync::Arc<Mutex<FakeFirestoreState>>>,
    Query(query): Query<BTreeMap<String, String>>,
    Json(mut document): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut state = state.lock().await;
    if state.quality_capture.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": { "status": "ALREADY_EXISTS" } })),
        ));
    }
    let document_id = query.get("documentId").cloned().unwrap();
    document["name"] = Value::String(format!(
        "projects/chenchess-test/databases/coach-quality/documents/captures/{document_id}"
    ));
    document["updateTime"] = Value::String("2026-08-01T10:00:00Z".to_string());
    state.quality_capture = Some(document.clone());
    Ok(Json(document))
}

async fn read_capture(
    State(state): State<std::sync::Arc<Mutex<FakeFirestoreState>>>,
) -> Result<Json<Value>, StatusCode> {
    state
        .lock()
        .await
        .quality_capture
        .clone()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn read_withdrawal(
    State(state): State<std::sync::Arc<Mutex<FakeFirestoreState>>>,
) -> Result<Json<Value>, StatusCode> {
    state
        .lock()
        .await
        .withdrawal
        .clone()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn commit_application(
    State(state): State<std::sync::Arc<Mutex<FakeFirestoreState>>>,
    Json(commit): Json<Value>,
) -> Json<Value> {
    state.lock().await.application_commits.push(commit);
    Json(serde_json::json!({}))
}

async fn commit_quality(
    State(state): State<std::sync::Arc<Mutex<FakeFirestoreState>>>,
    Json(commit): Json<Value>,
) -> Json<Value> {
    let mut state = state.lock().await;
    for write in commit["writes"].as_array().unwrap() {
        if write["delete"]
            .as_str()
            .is_some_and(|name| name.contains("/captures/"))
        {
            state.quality_capture = None;
        }
        if write["update"]["name"]
            .as_str()
            .is_some_and(|name| name.contains("/withdrawals/"))
        {
            let mut document = write["update"].clone();
            document["updateTime"] = Value::String("2026-08-01T10:00:03Z".to_string());
            state.withdrawal = Some(document);
        }
    }
    state.quality_commits.push(commit);
    Json(serde_json::json!({}))
}

fn player() -> PlayerId {
    PlayerId::try_from("firebase-player".to_string()).unwrap()
}

fn fixture_capture() -> QualityCaptureDraft {
    let created_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
    let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/events.json"
    )))
    .unwrap();
    let review = events
        .into_iter()
        .find_map(|event| match event.event {
            ReviewSessionEvent::Completed { result } => match *result {
                OperationCompletion::GameImported { review, .. } => Some(*review),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    let snapshot: ImportedGame = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
    )))
    .unwrap();
    let record = GameImportRecord::new(
        GameImportId::try_from("game-import:quality-fixture".to_string()).unwrap(),
        ProcessorPrincipal::Player(player()),
        snapshot,
        review,
        Vec::new(),
        Some(EngineProvenance {
            version: "Stockfish 18".to_string(),
            binary_sha256: "a".repeat(64),
            depth: 16,
            threads: 1,
            hash_mib: 16,
        }),
        created_at,
    );
    QualityCaptureDraft::game_analysis(&record).unwrap()
}

fn fixture_hosted_capture(trigger: CaptureTrigger, outcome: CaptureOutcome) -> QualityCaptureDraft {
    hosted_language_layer_capture(HostedGenerationInput {
        fingerprint: fixture_fingerprint(),
        attempt: &CompletionAttempt {
            latency: Duration::from_millis(87),
            http_status: Some(200),
            generation_id: Some("gen-secret".to_string()),
            served_model: Some("google/gemini-test".to_string()),
            served_provider: Some("Google Vertex".to_string()),
            prompt_tokens: Some(120),
            completion_tokens: Some(40),
            reasoning_tokens: None,
            cost: Some(0.002),
            finish_reason: Some("stop".to_string()),
            raw_content: Some("Keep the rook.".to_string()),
            outcome: CompletionOutcome::Completed,
        },
        trigger,
        outcome,
        pin_verification: PinVerificationVerdict::Passed,
        served_endpoint: Some("ep-1".to_string()),
        served_region: Some("global".to_string()),
        routed_service_tier: None,
        attempts: 1,
        task: HostedLanguageLayerTask::Comment,
        created_at: "2026-08-19T15:04:05Z".parse().unwrap(),
        steps: Vec::new(),
        rejection: None,
    })
}

fn fixture_fingerprint() -> EvaluationFingerprint {
    let digest = |byte: char| {
        ArtifactDigest::try_from(format!("sha256:{}", byte.to_string().repeat(64))).unwrap()
    };
    evaluation_fingerprint(EvaluationFingerprintAxes {
        evaluation_contract_version: crate::evaluation_fingerprint::EVALUATION_CONTRACT_VERSION
            .to_string(),
        environment: EvaluationEnvironment::Staging,
        capture_origin: CaptureOrigin::QualityCapture,
        delivery_surface: DeliverySurface::Web,
        code_revision: "git:test".to_string(),
        pipeline_revision: "pipeline:test".to_string(),
        language_layer_attestation: LanguageLayerAttestation::Attested {
            pin: compiled_pin_record().model,
            provider_allowlist: vec!["google-vertex/global".to_string()],
            generation_settings: EvaluationGenerationSettings {
                max_output_tokens: 512,
                temperature: false,
                seed: true,
            },
            structured_output_mode: StructuredOutputMode::NativeSchema,
            prompt_digest: digest('a'),
            response_schema_digest: digest('b'),
            evidence_schema_digest: digest('c'),
            coaching_profile_projection_schema_digest: digest('d'),
        },
    })
}
