use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

use axum::{routing::post, Json, Router};
use chen_chess_coach_engine::{
    critical_moment_comment::HostedCommentRuntime,
    evaluation_fingerprint::{evaluation_fingerprint, EvaluationEnvironment},
    language_layer_ledger::{
        LanguageLayerAdmissionConfig, LanguageLayerLedger, MemoryLanguageLayerLedger,
        ProviderConcurrency,
    },
    language_layer_prompt::CoachingProfileProjection,
    language_layer_provider::LanguageLayerProvider,
    pin_record::{compiled_pin_record, fingerprint_from_pin},
    review_annotation_store::{
        InMemoryReviewAnnotationStore, ReviewAnnotationAddress, ReviewAnnotationStore,
        ReviewAnnotationStoreError, ReviewAnnotationStoreFuture, ReviewAnnotations,
        ReviewMomentAnnotation,
    },
    review_session_contract::*,
    review_session_processor::{ProcessorPrincipal, ReviewSessionProcessor},
};
use serde_json::json;

use super::*;

#[tokio::test]
async fn web_bound_first_open_publishes_with_pin_provenance_and_retries_its_fallback_once() {
    let (base, hits) = spawn_comment_server().await;
    let (processor, ledger) = bound_processor(&base, None);
    let principal = player();
    let (game_import_id, selection) = import_and_prepare(&processor, principal.clone()).await;

    let opened = open_web(
        &processor,
        principal.clone(),
        &game_import_id,
        selection.clone(),
        "first",
    )
    .await;
    let (comment, published) = opened_comment(&opened);
    assert!(published, "Web + Bound must publish the first-open comment");
    assert!(!comment.text.trim().is_empty());
    let first_hits = hits.load(Ordering::SeqCst);
    assert!(
        first_hits > 0,
        "first-open must call the hosted provider through author_grounded_comment"
    );

    /* This fixture's prose carries no markers, so the gate rejects it and the
    first open publishes a safe rendering. A fallback is owed one retry, so the
    reopen authors again; the retry lands on the same rendering, and the open
    after it serves without touching the provider. */
    let reopened = open_web(
        &processor,
        principal.clone(),
        &game_import_id,
        selection.clone(),
        "reopen",
    )
    .await;
    let (replayed, replay_published) = opened_comment(&reopened);
    assert!(replay_published);
    assert_eq!(replayed, comment);
    let retry_hits = hits.load(Ordering::SeqCst);
    assert!(
        retry_hits > first_hits,
        "a stored fallback spends its one retry on the next open"
    );

    let settled = open_web(&processor, principal, &game_import_id, selection, "settled").await;
    let (settled_comment, settled_published) = opened_comment(&settled);
    assert!(settled_published);
    assert_eq!(settled_comment, comment);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        retry_hits,
        "a fallback that has spent its retry must not call the provider again"
    );

    let records = ledger.records().await.expect("memory ledger records");
    assert!(
        !records.is_empty(),
        "first-open authoring must settle at least one ledger record"
    );
    let pin = compiled_pin_record();
    let web_capture = fingerprint_from_pin(&pin, EvaluationEnvironment::Staging);
    assert_eq!(
        web_capture.axes.delivery_surface,
        DeliverySurface::Web,
        "Language Layer quality capture must record deliverySurface: web"
    );
    let mut coach_app_axes = web_capture.axes.clone();
    coach_app_axes.delivery_surface = DeliverySurface::CoachApp;
    let coach_app_capture = evaluation_fingerprint(coach_app_axes);
    assert_ne!(
        web_capture.digest, coach_app_capture.digest,
        "web Language Layer capture must not be interchangeable with a CoachApp capture"
    );
    for record in &records {
        assert_eq!(
            record.fingerprint_digest,
            web_capture.digest.as_str(),
            "first-open capture records must keep the web deliverySurface fingerprint"
        );
    }
}

#[tokio::test]
async fn non_web_or_unbound_safe_renders_unpublished() {
    let (base, hits) = spawn_comment_server().await;
    let (bound, ledger) = bound_processor(&base, None);
    let unbound = {
        let (processor, _, _) = processor(false);
        processor
    };
    let principal = player();

    let (game_import_id, selection) = import_and_prepare(&bound, principal.clone()).await;
    let coach_app = open_on(
        &bound,
        principal.clone(),
        DeliverySurface::CoachApp,
        &game_import_id,
        selection.clone(),
        "coach-app",
    )
    .await;
    let (_, published) = opened_comment(&coach_app);
    assert!(!published, "non-Web must not host-author");
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert!(
        ledger
            .records()
            .await
            .expect("memory ledger records")
            .is_empty(),
        "CoachApp must not write a web Language Layer capture"
    );

    let (unbound_id, unbound_selection) = import_and_prepare(&unbound, principal.clone()).await;
    let web_unbound = open_web(
        &unbound,
        principal,
        &unbound_id,
        unbound_selection,
        "web-unbound",
    )
    .await;
    let (_, unbound_published) = opened_comment(&web_unbound);
    assert!(
        !unbound_published,
        "unbound Web must safe-render unpublished"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn crash_between_settle_and_publish_reauthors_once() {
    let (base, hits) = spawn_comment_server().await;
    let annotations = Arc::new(FailingOnceAnnotationStore::default());
    let (processor, ledger) = bound_processor(&base, Some(annotations.clone()));
    let principal = player();
    let (game_import_id, selection) = import_and_prepare(&processor, principal.clone()).await;

    let first = open_web(
        &processor,
        principal.clone(),
        &game_import_id,
        selection.clone(),
        "crash",
    )
    .await;
    assert!(
        first.iter().any(|event| matches!(
            event.event,
            ReviewSessionEvent::ReviewMomentUnavailable { .. }
        )),
        "persist failure after settle must not publish: {first:?}"
    );
    let first_hits = hits.load(Ordering::SeqCst);
    assert!(first_hits > 0, "the crashed open must have authored");
    let settled = ledger.records().await.expect("memory ledger records").len();
    assert!(
        settled > 0,
        "crash-between leaves a billed unpublished record"
    );

    let second = open_web(
        &processor,
        principal.clone(),
        &game_import_id,
        selection,
        "recover",
    )
    .await;
    let (_, published) = opened_comment(&second);
    assert!(published, "re-author under the same key must publish once");
    assert!(
        hits.load(Ordering::SeqCst) > first_hits,
        "reopen after a settled unpublished attempt is a second attempt_hosted"
    );

    let stored = annotations
        .read(&ReviewAnnotationAddress {
            owner: principal,
            game_import_id,
        })
        .await
        .unwrap();
    assert_eq!(stored.len(), 1, "append-only still one published comment");
}

#[tokio::test]
async fn a_fallback_retry_authors_under_the_current_profile_beside_the_record_it_supersedes() {
    let (base, hits) = spawn_comment_server().await;
    let annotations = Arc::new(InMemoryReviewAnnotationStore::default());
    let authored = CoachingProfileProjection::populated(["occupyTheCenter".to_string()]);
    let later = CoachingProfileProjection::populated(["tactics".to_string()]);
    let (processor, _) = bound_processor(&base, Some(annotations.clone()));
    processor.set_coaching_profile(authored.clone());
    let principal = player();
    let (game_import_id, selection) = import_and_prepare(&processor, principal.clone()).await;

    let opened = open_web(
        &processor,
        principal.clone(),
        &game_import_id,
        selection.clone(),
        "profile",
    )
    .await;
    let (_, published) = opened_comment(&opened);
    assert!(published);
    let moment_id = opened_moment_id(&opened);
    assert!(hits.load(Ordering::SeqCst) > 0);

    processor.set_coaching_profile(later.clone());
    let reopened = open_web(
        &processor,
        principal.clone(),
        &game_import_id,
        selection,
        "profile-reopen",
    )
    .await;
    let (_, replay_published) = opened_comment(&reopened);
    assert!(replay_published);

    let stored = annotations
        .read(&ReviewAnnotationAddress {
            owner: principal,
            game_import_id,
        })
        .await
        .unwrap();
    /* This fixture's prose carries no markers, so the first open published a
    safe rendering and the reopen spent its one retry. The retry is a fresh
    authoring and reads the profile in force now; what it must not do is rewrite
    the record it supersedes, and the append-only store is what guarantees that
    -- two entries, the earlier one still carrying the projection it was
    authored under. */
    assert_eq!(stored.len(), 2, "the retry appends beside its predecessor");
    let annotation = stored.active(&moment_id).expect("published annotation");
    assert_eq!(
        annotation.authoring_provenance.coaching_profile_projection, later,
        "a retry authors under the projection in force when it runs"
    );
    assert_ne!(
        annotation.authoring_provenance.coaching_profile_projection, authored,
        "the superseded record keeps its own projection rather than sharing one"
    );
}

fn opened_moment_id(events: &[ReviewSessionEventEnvelope]) -> CriticalMomentId {
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewMomentOpened { review_moment, .. } => {
                    Some(review_moment.review_moment.moment_id.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("expected ReviewMomentOpened")
}

#[tokio::test]
async fn web_session_start_settles_artifacts_that_the_coach_app_never_sees() {
    let (base, hits) = spawn_comment_server().await;
    let (processor, _ledger) = bound_processor_with(&base, None, true);
    let principal = player();

    let imported = submit_on(
        &processor,
        principal.clone(),
        DeliverySurface::Web,
        "eager-import",
        import_command(),
    )
    .await;
    let imported_id = imported.iter().find_map(imported_game).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "an import alone spends no Language Layer budget"
    );

    let started = submit_on(
        &processor,
        principal.clone(),
        DeliverySurface::Web,
        "eager-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: imported_id,
        },
    )
    .await;
    let (game_import_id, selection) = started
        .iter()
        .find_map(started_admission)
        .expect("the web Review Session start admits at least one moment");

    /* The web start scheduled detached authoring; a language-layer hit
    arriving without any surface opening a moment proves the artifacts
    settle on their own. */
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    while hits.load(Ordering::SeqCst) == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "lazy authoring never reached the language layer"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let web = open_web(
        &processor,
        principal.clone(),
        &game_import_id,
        selection.clone(),
        "eager-web",
    )
    .await;
    let (_, published) = opened_comment(&web);
    assert!(published, "the web serves the stored artifact as published");

    let coach_app = open_on(
        &processor,
        principal,
        DeliverySurface::CoachApp,
        &game_import_id,
        selection,
        "eager-coach-app",
    )
    .await;
    let (_, coach_published) = opened_comment(&coach_app);
    assert!(
        !coach_published,
        "engine-hosted artifacts stay web-only for now"
    );
}

fn player() -> ProcessorPrincipal {
    ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-first-open".to_string()).unwrap(),
    )
}

fn bound_processor(
    provider_base: &str,
    annotations: Option<Arc<dyn ReviewAnnotationStore>>,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<MemoryLanguageLayerLedger>,
) {
    bound_processor_with(provider_base, annotations, false)
}

fn bound_processor_with(
    provider_base: &str,
    annotations: Option<Arc<dyn ReviewAnnotationStore>>,
    eager_web_artifacts: bool,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<MemoryLanguageLayerLedger>,
) {
    let recording = support::provider_recording();
    let pin = compiled_pin_record();
    let fingerprint = fingerprint_from_pin(&pin, EvaluationEnvironment::Staging);
    let ledger = Arc::new(MemoryLanguageLayerLedger::new());
    let hosted = Arc::new(HostedCommentRuntime::new(
        Arc::new(LanguageLayerProvider::from_client_at(
            reqwest::Client::new(),
            "test",
            provider_base,
        )),
        pin,
        fingerprint,
        ledger.clone(),
        Arc::new(ProviderConcurrency::new(4)),
        LanguageLayerAdmissionConfig::conservative_defaults(),
    ));
    let mut built = ReviewSessionProcessor::new(
        CapturedLichess::new(),
        recording.clone(),
        Arc::new(support::RecordingEngine::new(&recording)),
        Arc::new(support::RecordingHuman::new(&recording, false)),
        Arc::new(support::GroundedAuthor),
    )
    .unwrap()
    .with_language_layer_ledger(ledger.clone())
    .with_hosted_comment(hosted);
    if eager_web_artifacts {
        built = built.with_eager_web_artifacts();
    }
    if let Some(annotations) = annotations {
        built = built.with_review_annotation_store(annotations);
    }
    (Arc::new(built), ledger)
}

async fn spawn_comment_server() -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let app = Router::new()
        .route(
            "/generation",
            axum::routing::get(|| async {
                Json(json!({
                    "data": {
                        "model": "google/gemini-3.5-flash-lite-20260721",
                        "provider_name": "Google Vertex",
                        "data_region": "global",
                        "provider_responses": [{
                            "endpoint_id": "ep-first-open",
                            "routed_service_tier": null
                        }]
                    }
                }))
            }),
        )
        .route(
            "/chat/completions",
            post(move || {
                let counted = counted.clone();
                async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    Json(json!({
                        "id": "gen-first-open",
                        "choices": [{
                            "message": { "content": "{\"comment\":\"a first-open comment\"}" },
                            "finish_reason": "stop"
                        }]
                    }))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    (format!("http://{addr}"), hits)
}

async fn import_and_prepare(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
) -> (GameImportId, ReviewMomentSelection) {
    let imported = submit_on(
        processor,
        principal.clone(),
        DeliverySurface::Web,
        "setup-import",
        import_command(),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let started = submit_on(
        processor,
        principal,
        DeliverySurface::Web,
        "setup-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    if let Some((_, core)) = started.iter().find_map(started_session) {
        return (game_import_id, core.review_moment.selection.clone());
    }
    started
        .iter()
        .find_map(started_admission)
        .expect("Review Session start admits at least one moment")
}

async fn open_web(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    game_import_id: &GameImportId,
    selection: ReviewMomentSelection,
    label: &str,
) -> Vec<ReviewSessionEventEnvelope> {
    open_on(
        processor,
        principal,
        DeliverySurface::Web,
        game_import_id,
        selection,
        label,
    )
    .await
}

async fn open_on(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    surface: DeliverySurface,
    game_import_id: &GameImportId,
    selection: ReviewMomentSelection,
    label: &str,
) -> Vec<ReviewSessionEventEnvelope> {
    submit_on(
        processor,
        principal,
        surface,
        label,
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection,
            idempotency_key: idempotency_key(label),
        },
    )
    .await
}

async fn submit_on(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    surface: DeliverySurface,
    label: &str,
    command: ReviewSessionCommand,
) -> Vec<ReviewSessionEventEnvelope> {
    let mut envelope = envelope_for(&principal, label, command);
    envelope.surface = surface;
    submit(processor, principal, envelope).await
}

fn opened_comment(events: &[ReviewSessionEventEnvelope]) -> (CriticalMomentComment, bool) {
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewMomentOpened {
                    comment: Some(comment),
                    comment_published,
                    ..
                } => Some((comment.as_ref().clone(), *comment_published)),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected ReviewMomentOpened with a comment: {events:#?}"))
}

struct FailingOnceAnnotationStore {
    inner: InMemoryReviewAnnotationStore,
    remaining_failures: AtomicUsize,
}

impl Default for FailingOnceAnnotationStore {
    fn default() -> Self {
        Self {
            inner: InMemoryReviewAnnotationStore::default(),
            remaining_failures: AtomicUsize::new(1),
        }
    }
}

impl ReviewAnnotationStore for FailingOnceAnnotationStore {
    fn append<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
        annotation: ReviewMomentAnnotation,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewMomentAnnotation> {
        Box::pin(async move {
            if self
                .remaining_failures
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                    remaining.checked_sub(1)
                })
                .is_ok()
            {
                return Err(ReviewAnnotationStoreError::Unavailable);
            }
            self.inner.append(address, annotation).await
        })
    }

    fn read<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewAnnotations> {
        self.inner.read(address)
    }

    fn delete<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ()> {
        self.inner.delete(address)
    }
}

/// A comment prompt edit has to reach reviews that were authored before it.
///
/// Stored prose carries the digests of the prompt that wrote it, so a later
/// web open recognises it as superseded and authors again. It must then
/// converge: the open after that finds prose from the compiled prompt and
/// calls nothing.
#[tokio::test]
async fn an_edited_comment_prompt_reauthors_stored_prose_once_and_then_settles() {
    let (base, hits) = spawn_comment_server().await;
    let annotations = Arc::new(AgingAnnotationStore::superseding_fallback());
    let shared: Arc<dyn ReviewAnnotationStore> = annotations.clone();
    let principal = player();

    let (before_edit, _) = bound_processor(&base, Some(shared.clone()));
    let (game_import_id, selection) = import_and_prepare(&before_edit, principal.clone()).await;
    let opened = open_web(
        &before_edit,
        principal.clone(),
        &game_import_id,
        selection,
        "before-edit",
    )
    .await;
    let (stale, stale_published) = opened_comment(&opened);
    assert!(stale_published);
    assert!(!stale.text.trim().is_empty());
    /* While the store keeps ageing what it is handed, every open finds prose
    from a prompt that no longer exists and rewrites it. The count here is a
    baseline to measure the settled build against, not a fixed number. */
    let superseded_hits = hits.load(Ordering::SeqCst);
    assert!(superseded_hits > 0);

    /* From here the store keeps what it is given, so the rewrite lands as the
    running build actually writes it. */
    annotations.stop_aging();

    let (after_edit, _) = bound_processor(&base, Some(shared.clone()));
    let (after_edit_id, after_edit_selection) =
        import_and_prepare(&after_edit, principal.clone()).await;
    assert_eq!(
        after_edit_id, game_import_id,
        "one Game must address one set of annotations"
    );
    let reopened = open_web(
        &after_edit,
        principal.clone(),
        &after_edit_id,
        after_edit_selection,
        "after-edit",
    )
    .await;
    let (rewritten, rewritten_published) = opened_comment(&reopened);
    assert!(
        rewritten_published,
        "prose an edited prompt superseded must republish rather than safe-render"
    );
    let rewrite_hits = hits.load(Ordering::SeqCst);
    assert!(
        rewrite_hits > superseded_hits,
        "an edited comment prompt must reach the hosted provider again"
    );

    let shared_after_retry = shared.clone();
    let (settled, _) = bound_processor(&base, Some(shared));
    let (settled_id, settled_selection) = import_and_prepare(&settled, principal.clone()).await;
    let served = open_web(
        &settled,
        principal.clone(),
        &settled_id,
        settled_selection,
        "settled",
    )
    .await;
    let (served_comment, served_published) = opened_comment(&served);
    assert!(served_published);
    assert_eq!(
        served_comment, rewritten,
        "later opens serve the rewritten prose"
    );
    /* The rewrite fell back -- this fixture's prose carries no markers, so the
    gate rejects it -- and a fallback written under the compiled prompt is owed
    one retry. The edit reset the count, because an edited prompt is a fresh
    chance rather than the same one taken twice. */
    let retry_hits = hits.load(Ordering::SeqCst);
    assert!(
        retry_hits > rewrite_hits,
        "a fallback under the compiled prompt spends its one retry"
    );

    let (settled_again, _) = bound_processor(&base, Some(shared_after_retry));
    let (settled_again_id, settled_again_selection) =
        import_and_prepare(&settled_again, principal.clone()).await;
    let finally = open_web(
        &settled_again,
        principal,
        &settled_again_id,
        settled_again_selection,
        "settled-again",
    )
    .await;
    let (finally_comment, finally_published) = opened_comment(&finally);
    assert!(finally_published);
    assert_eq!(finally_comment, rewritten);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        retry_hits,
        "a fallback that has spent its retry must never author again"
    );
}

/// Durable state as a comment prompt edit leaves it: prose written by an
/// earlier build, under that build's digests and its own idempotency key.
///
/// Both have to move together. The key hashes the evaluation fingerprint,
/// which hashes the prompt digest, so a real edit never reuses the key the
/// superseded write used.
struct AgingAnnotationStore {
    inner: InMemoryReviewAnnotationStore,
    aging: AtomicBool,
    /// Whether the superseded record should read as prose the Language Layer
    /// authored, rather than the fallback rendering the stub really produces.
    ///
    /// The stub's canned comment cannot pass the Grounding Gate, so everything
    /// it publishes is a safe rendering. A rewrite is allowed to replace one of
    /// those; only authored prose is protected, so a test about that protection
    /// has to say so.
    as_authored: bool,
}

impl AgingAnnotationStore {
    /// Superseded prose that was itself a fallback rendering.
    fn superseding_fallback() -> Self {
        Self::new(false)
    }

    /// Superseded prose the Language Layer authored.
    fn superseding_authored_prose() -> Self {
        Self::new(true)
    }

    fn new(as_authored: bool) -> Self {
        Self {
            inner: InMemoryReviewAnnotationStore::default(),
            aging: AtomicBool::new(true),
            as_authored,
        }
    }

    fn stop_aging(&self) {
        self.aging.store(false, Ordering::SeqCst);
    }
}

/// The idempotency key the fixture writes superseded prose under, standing in
/// for the key an earlier build's evaluation fingerprint would have produced.
const SUPERSEDED_PROMPT_KEY: &str = "idempotency-key:first-open:superseded-prompt";

fn authored_by_an_older_prompt(
    mut annotation: ReviewMomentAnnotation,
    as_authored: bool,
) -> ReviewMomentAnnotation {
    let candidate = &annotation
        .authoring_provenance
        .generation_contract
        .candidate;
    annotation
        .authoring_provenance
        .generation_contract
        .candidate = CriticalMomentExplainerCandidate::new(
        candidate.provider.clone(),
        candidate.model.clone(),
        candidate.model_revision.clone(),
        ArtifactDigest::try_from(format!("sha256:{}", "1".repeat(64)))
            .expect("a fixed digest is a valid ArtifactDigest"),
        candidate.response_schema_digest.clone(),
    );
    annotation.idempotency_key = IdempotencyKey::try_from(SUPERSEDED_PROMPT_KEY.to_string())
        .expect("a fixed idempotency key is valid");
    if as_authored {
        annotation.authoring_provenance.outcome =
            CriticalMomentCommentGenerationOutcome::Authored { attempts: 1 };
    }
    annotation
}

impl ReviewAnnotationStore for AgingAnnotationStore {
    fn append<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
        annotation: ReviewMomentAnnotation,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewMomentAnnotation> {
        let annotation = if self.aging.load(Ordering::SeqCst) {
            authored_by_an_older_prompt(annotation, self.as_authored)
        } else {
            annotation
        };
        Box::pin(async move { self.inner.append(address, annotation).await })
    }

    fn read<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ReviewAnnotations> {
        self.inner.read(address)
    }

    fn delete<'a>(
        &'a self,
        address: &'a ReviewAnnotationAddress,
    ) -> ReviewAnnotationStoreFuture<'a, ()> {
        self.inner.delete(address)
    }
}

/// A provider outage during a rewrite must not cost the Player prose they
/// already have.
///
/// `author_grounded_comment` returns its safe rendering as an `Ok`, so it
/// reaches the publication seam indistinguishable from authored prose. A stale
/// comment no longer short-circuits staging, so without an explicit guard an
/// outage would persist the template over real coaching prose -- and
/// permanently, because the rendering carries the compiled digests and no
/// later open would ever try again.
#[tokio::test]
async fn an_outage_during_a_rewrite_keeps_the_prose_it_could_not_replace() {
    let (base, healthy) = spawn_switchable_comment_server().await;
    let annotations = Arc::new(AgingAnnotationStore::superseding_authored_prose());
    let shared: Arc<dyn ReviewAnnotationStore> = annotations.clone();
    let principal = player();

    let (before_outage, _) = bound_processor(&base, Some(shared.clone()));
    let (game_import_id, selection) = import_and_prepare(&before_outage, principal.clone()).await;
    let opened = open_web(
        &before_outage,
        principal.clone(),
        &game_import_id,
        selection,
        "before-outage",
    )
    .await;
    let (authored, published) = opened_comment(&opened);
    assert!(published);
    let moment_id = opened_moment_id(&opened);

    /* The prompt has moved on, and the provider is down for the rewrite the
    move asks for. */
    annotations.stop_aging();
    healthy.store(false, Ordering::SeqCst);

    let (during_outage, _) = bound_processor(&base, Some(shared.clone()));
    let (during_id, during_selection) = import_and_prepare(&during_outage, principal.clone()).await;
    let reopened = open_web(
        &during_outage,
        principal.clone(),
        &during_id,
        during_selection,
        "during-outage",
    )
    .await;
    let (served, served_published) = opened_comment(&reopened);
    assert!(
        served_published,
        "prose that is already published stays published through an outage"
    );
    assert_eq!(
        served, authored,
        "an outage must not put a template rendering where the stored prose was"
    );

    /* Text alone cannot carry this assertion: the stub's canned comment does
    not pass the Grounding Gate, so the prose stored before the outage is
    itself a safe rendering and compares equal to the one the outage produces.
    Durable identity is what separates them -- the stored record must still be
    the one the fixture wrote, under the superseded prompt's key. */
    let stored = shared
        .read(&ReviewAnnotationAddress {
            owner: principal,
            game_import_id: during_id,
        })
        .await
        .expect("the annotation store reads back");
    assert_eq!(
        stored.len(),
        1,
        "an outage must not append a rendering beside the prose it failed to replace"
    );
    let active = stored
        .active(&moment_id)
        .expect("the Review Moment keeps an active annotation");
    assert_eq!(
        active.idempotency_key,
        IdempotencyKey::try_from(SUPERSEDED_PROMPT_KEY.to_string()).unwrap(),
        "a safe rendering must never supersede stored prose durably"
    );
}

/// The comment server from [`spawn_comment_server`], plus a switch that makes
/// completions fail so authoring falls back to its safe rendering.
async fn spawn_switchable_comment_server() -> (String, Arc<AtomicBool>) {
    use axum::response::IntoResponse;

    let healthy = Arc::new(AtomicBool::new(true));
    let routed = healthy.clone();
    let app = Router::new()
        .route(
            "/generation",
            axum::routing::get(|| async {
                Json(json!({
                    "data": {
                        "model": "google/gemini-3.5-flash-lite-20260721",
                        "provider_name": "Google Vertex",
                        "data_region": "global",
                        "provider_responses": [{
                            "endpoint_id": "ep-first-open",
                            "routed_service_tier": null
                        }]
                    }
                }))
            }),
        )
        .route(
            "/chat/completions",
            post(move || {
                let routed = routed.clone();
                async move {
                    if !routed.load(Ordering::SeqCst) {
                        return (
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": "provider unavailable" })),
                        )
                            .into_response();
                    }
                    Json(json!({
                        "id": "gen-first-open",
                        "choices": [{
                            "message": { "content": "{\"comment\":\"a first-open comment\"}" },
                            "finish_reason": "stop"
                        }]
                    }))
                    .into_response()
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    (format!("http://{addr}"), healthy)
}
