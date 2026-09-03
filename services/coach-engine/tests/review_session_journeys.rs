use std::{
    future::{pending, Future},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use chen_chess_coach_engine::{
    app,
    auth::AuthConfig,
    engine_analysis::{
        EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer, EngineProvenance,
        PositionEvaluation,
    },
    evaluation_recording::{
        PINNED_STOCKFISH_BINARY_DIGEST, PINNED_STOCKFISH_DEPTH, PINNED_STOCKFISH_HASH_MIB,
        PINNED_STOCKFISH_THREADS, PINNED_STOCKFISH_VERSION,
    },
    game_import_store::ReviewSessionGame,
    game_import_store::{
        GameImportLookup, GameImportRecord, GameImportReference, GameImportReferenceLookup,
        GameImportStore, GameImportStoreFuture, InMemoryGameImportStore,
    },
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction},
    imported_games::ImportedGameCard,
    operating_limits::{ALTERNATIVE_MOVE_DEADLINE_MILLISECONDS, CANCELLATION_BUDGET_MILLISECONDS},
    quality_capture::{InMemoryQualityCaptureStore, RetentionPreference},
    review_analysis_cache::{
        InMemoryReviewAnalysisCache, ReviewAnalysisCacheError, ReviewAnalysisCacheFuture,
        ReviewAnalysisCacheStore, ReviewAnalysisEntries, ReviewAnalysisEntry,
        ReviewAnalysisMutation,
    },
    review_session_contract::*,
    review_session_processor::{ProcessorPrincipal, ReviewSessionProcessor},
    review_session_transport::{ReviewSessionCommandExecutor, ReviewSessionWebBinding},
    types::{AppState, HumanMoveCandidate},
};
use chrono::Utc;
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};
use tokio::sync::watch;
use tower::ServiceExt;

use crate::{processor_support, transport_support};

#[path = "review_session_journeys/residency.rs"]
mod residency;

#[path = "review_session_journeys/readiness.rs"]
mod readiness;

#[path = "review_session_journeys/player_selected_decision.rs"]
mod player_selected_decision;

#[path = "review_session_journeys/coaching.rs"]
mod coaching;

#[path = "review_session_journeys/replay.rs"]
mod replay;

const FIREBASE_PROJECT_ID: &str = "chenchess-test";

/// The generated key pair these journeys sign and verify with
/// (`bun run keys:test`); see `services/coach-engine/src/certification_keys.rs`
/// for why it is not checked in.
fn certification_fixture(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("certification-fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}. Run `bun run keys:test` to generate the Coach Engine test keys.",
            path.display()
        )
    })
}

fn jwt_jwks() -> String {
    certification_fixture("auth-jwks.json")
}

/// The Player every journey acts as, signed fresh rather than pinned: the
/// pinned token existed to match the deployment certification harness, which
/// this snapshot does not carry.
fn player_token() -> String {
    #[derive(serde::Serialize)]
    struct Claims {
        sub: &'static str,
        exp: u64,
        iat: u64,
        auth_time: u64,
        iss: &'static str,
        aud: &'static str,
    }

    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some("firebase-test-key".to_string());
    jsonwebtoken::encode(
        &header,
        &Claims {
            sub: "firebase-player-a",
            exp: 4_102_444_800,
            iat: 1_700_000_000,
            auth_time: 1_700_000_000,
            iss: "https://securetoken.google.com/chenchess-test",
            aud: FIREBASE_PROJECT_ID,
        },
        &jsonwebtoken::EncodingKey::from_rsa_pem(
            certification_fixture("auth-private-key.pem").as_bytes(),
        )
        .expect("valid test private key"),
    )
    .expect("test token signs")
}

fn idempotency_key(label: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(format!("idempotency-key:journey:{label}")).unwrap()
}

#[tokio::test]
async fn http_player_journey_preserves_visible_review_outcomes() {
    let application = http_application(None);

    let journey = run_player_journey(&mut JourneySurface::http(application)).await;

    assert_visible_outcomes(journey);
}

#[tokio::test]
async fn jsonl_player_journey_preserves_visible_review_outcomes() {
    let (processor, _, _) = transport_support::processor(false);
    let journey = run_player_journey(&mut JourneySurface::jsonl(processor)).await;

    assert_visible_outcomes(journey);
}

/// Starting is a pure function of the address, so every retry — concurrent,
/// sequential, or after a re-import of the same Game — names the same review and
/// pays for its analysis exactly once.
#[tokio::test]
async fn start_retries_are_idempotent_and_a_reimport_reuses_the_same_review() {
    let checkpoints = Arc::new(CountingCheckpointStore::available());
    let processor = processor_with_checkpoint_store(checkpoints.clone());
    let mut transport = JourneySurface::jsonl(processor.clone());
    let imported = transport
        .submit("concurrent-start-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let start_bytes = serde_json::to_vec(&transport_support::envelope(
        DeliverySurface::CoachSkill,
        "concurrent-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    ))
    .unwrap();
    let first = chen_chess_coach_engine::review_session_processor::ReviewSessionProcessor::submit(
        &processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &start_bytes,
    );
    let second = chen_chess_coach_engine::review_session_processor::ReviewSessionProcessor::submit(
        &processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &start_bytes,
    );
    let (first, second) = tokio::join!(
        transport_support::collect_receiver(first),
        transport_support::collect_receiver(second),
    );
    let (first_id, first_moments) = started_session(&first);
    let (second_id, second_moments) = started_session(&second);

    assert_eq!(first_id, second_id);
    assert_eq!(first_moments, second_moments);
    assert_eq!(checkpoints.seeds.load(Ordering::SeqCst), 1);

    let repeated =
        chen_chess_coach_engine::review_session_processor::ReviewSessionProcessor::submit(
            &processor,
            chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
            &start_bytes,
        );
    let repeated = transport_support::collect_receiver(repeated).await;
    assert_eq!(started_session(&repeated).0, first_id);
    assert_eq!(
        checkpoints.seeds.load(Ordering::SeqCst),
        1,
        "a resident session must not seed its analysis again"
    );

    let reimported = transport
        .submit("concurrent-start-reimport", import_command())
        .await;
    let reimported_id = match completion(&reimported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    assert_eq!(reimported_id, game_import_id);

    let next = transport
        .submit(
            "concurrent-start-reimported",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: reimported_id,
            },
        )
        .await;
    let (next_id, _) = started_session(&next);
    assert_eq!(next_id, first_id);
    assert_eq!(
        checkpoints.seeds.load(Ordering::SeqCst),
        1,
        "a re-import of the same Game reuses the cached analysis"
    );
}

/// A cache that cannot be written at all exposes no session and stays retryable.
///
/// Reads are optional, but a Review Moment whose prepared analysis could not be
/// persisted is not admitted: the Player is told to retry rather than handed a
/// review whose state exists only in one process's memory.
#[tokio::test]
async fn a_cache_that_cannot_be_written_exposes_no_session_and_remains_retryable() {
    let checkpoints = Arc::new(CountingCheckpointStore::unavailable());
    let processor = processor_with_checkpoint_store(checkpoints.clone());
    let mut transport = JourneySurface::jsonl(processor);
    let imported = transport
        .submit("checkpoint-failure-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };

    for label in ["checkpoint-failure-first", "checkpoint-failure-retry"] {
        let events = transport
            .submit(
                label,
                ReviewSessionCommand::StartReviewSession {
                    game_import_id: game_import_id.clone(),
                },
            )
            .await;
        assert!(
            !events.iter().any(|event| matches!(
                event.event,
                ReviewSessionEvent::Completed { ref result }
                    if matches!(
                        result.as_ref(),
                        OperationCompletion::ReviewSessionStarted { .. }
                    )
            )),
            "no session may be exposed: {events:#?}"
        );
    }
    assert!(checkpoints.replaces.load(Ordering::SeqCst) >= 1);
}

#[tokio::test]
async fn zero_automatic_moments_commit_an_empty_ready_session() {
    let checkpoints = Arc::new(CountingCheckpointStore::available());
    let processor = processor_with_stores(
        Arc::new(EmptyAutomaticMomentsStore::default()),
        checkpoints.clone(),
    );
    let mut transport = JourneySurface::jsonl(processor);
    let imported = transport
        .submit("empty-review-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = transport
        .submit(
            "empty-review-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (_, review_moments) = started_session(&started);

    assert!(review_moments.is_empty());
    assert_eq!(checkpoints.seeds.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn invalid_imported_game_commits_no_checkpoint_or_partial_session() {
    let checkpoints = Arc::new(CountingCheckpointStore::available());
    let processor = processor_with_stores(
        Arc::new(CorruptingGameImportStore::default()),
        checkpoints.clone(),
    );
    let mut transport = JourneySurface::jsonl(processor);
    let imported = transport
        .submit("hard-failure-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };

    for label in ["hard-failure-start", "hard-failure-retry"] {
        let events = transport
            .submit(
                label,
                ReviewSessionCommand::StartReviewSession {
                    game_import_id: game_import_id.clone(),
                },
            )
            .await;
        assert!(matches!(
            events.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Rejected {
                operation: OperationKind::ReviewSessionStart,
                reason: CommandRejectionReason::InvalidCommand,
                recovery: RejectionRecovery::CorrectInput,
            })
        ));
    }
    assert_eq!(checkpoints.seeds.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancelled_start_execution_commits_no_checkpoint() {
    let checkpoints = Arc::new(BlockingCheckpointStore::new());
    let observed_checkpoints = checkpoints.clone();
    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let recording = processor_support::provider_recording();
            let processor = Arc::new(
                ReviewSessionProcessor::new(
                    processor_support::CapturedLichess::new(),
                    recording.clone(),
                    Arc::new(processor_support::RecordingEngine::new(&recording)),
                    Arc::new(processor_support::RecordingHuman::new(&recording, false)),
                    Arc::new(processor_support::GroundedAuthor),
                )
                .unwrap()
                .with_review_analysis_cache(checkpoints.clone()),
            );
            let mut transport = JourneySurface::jsonl(processor.clone());
            let imported = transport
                .submit("cancelled-start-import", import_command())
                .await;
            let game_import_id = match completion(&imported) {
                OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
                completion => panic!("expected Game Import completion, got {completion:?}"),
            };
            let _cancelled_execution =
                chen_chess_coach_engine::review_session_processor::ReviewSessionProcessor::submit(
                    &processor,
                chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
                &serde_json::to_vec(&transport_support::envelope(
                    DeliverySurface::CoachSkill,
                    "cancelled-start",
                    ReviewSessionCommand::StartReviewSession { game_import_id },
                ))
                .unwrap(),
            );

            checkpoints.wait_until_started().await;
            assert_eq!(checkpoints.active_creates.load(Ordering::SeqCst), 1);
        });
    })
    .await
    .unwrap();

    assert_eq!(
        observed_checkpoints.active_creates.load(Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn quality_capture_preference_requires_disclosure_and_supports_opt_out() {
    let store = Arc::new(InMemoryQualityCaptureStore::default());
    let binding = retention_binding(store.clone());

    assert_eq!(
        binding
            .retention_preference(transport_support::WEB_SUBJECT)
            .await
            .unwrap(),
        RetentionPreference {
            available: true,
            enabled: true,
            disclosure_required: true,
            deleted_review_snapshots: 0,
        }
    );
    assert_eq!(
        binding
            .set_retention_preference(transport_support::WEB_SUBJECT, true)
            .await
            .unwrap(),
        RetentionPreference {
            available: true,
            enabled: true,
            disclosure_required: false,
            deleted_review_snapshots: 0,
        }
    );

    import_and_start_review_session(&binding, "retention-enabled").await;
    assert_eq!(
        binding
            .set_retention_preference(transport_support::WEB_SUBJECT, false)
            .await
            .unwrap(),
        RetentionPreference {
            available: true,
            enabled: false,
            disclosure_required: false,
            deleted_review_snapshots: 0,
        }
    );
    import_and_start_review_session(&binding, "retention-disabled").await;
}

struct JourneyResult {
    automatic_moment_plies: Vec<u16>,
}

enum JourneySurface {
    Http { application: Router },
    Jsonl(transport_support::TransportHarness),
}

impl JourneySurface {
    fn http(application: Router) -> Self {
        Self::Http { application }
    }

    fn jsonl(
        processor: Arc<
            chen_chess_coach_engine::review_session_processor::ReviewSessionProcessor<
                transport_support::CapturedLichess,
            >,
        >,
    ) -> Self {
        Self::Jsonl(transport_support::TransportHarness::local(processor))
    }

    async fn submit(
        &mut self,
        label: &str,
        command: ReviewSessionCommand,
    ) -> Vec<ReviewSessionEventEnvelope> {
        match self {
            Self::Http { application } => http_events(application, label, command).await,
            Self::Jsonl(transport) => transport.submit(label, command).await,
        }
    }
}

async fn run_player_journey(transport: &mut JourneySurface) -> JourneyResult {
    let imported = transport.submit("journey-import", import_command()).await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported {
            game_import_id,
            review,
            ..
        } => {
            assert!(!review.critical_moments.is_empty());
            game_import_id.clone()
        }
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };

    let started = transport
        .submit(
            "journey-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let automatic_moment_plies = match completion(&started) {
        OperationCompletion::ReviewSessionStarted { review_moments, .. } => review_moments
            .iter()
            .map(|core| core.review_moment.ply)
            .collect(),
        completion => panic!("expected Review Session start, got {completion:?}"),
    };
    JourneyResult {
        automatic_moment_plies,
    }
}

fn assert_visible_outcomes(journey: JourneyResult) {
    assert!(!journey.automatic_moment_plies.is_empty());
    assert!(journey
        .automatic_moment_plies
        .windows(2)
        .all(|window| window[0] < window[1]));
}

fn started_session(
    events: &[ReviewSessionEventEnvelope],
) -> (GameImportId, Vec<ReviewSessionCoreContract>) {
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewSessionStarted {
                    game_import_id,
                    review_moments,
                    ..
                } => Some((
                    game_import_id.clone(),
                    review_moments
                        .iter()
                        .map(|moment| {
                            moment
                                .prepared_core()
                                .expect("Coach Skill starts return a complete prepared batch")
                                .clone()
                        })
                        .collect(),
                )),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("Review Session start should complete: {events:#?}"))
}

fn http_application(quality_capture: Option<Arc<InMemoryQualityCaptureStore>>) -> Router {
    let (processor, _, _) = transport_support::processor(false);
    let executor: Arc<dyn ReviewSessionCommandExecutor> = processor;
    application_with_executor(executor, quality_capture)
}

fn processor_with_checkpoint_store(
    checkpoints: Arc<dyn ReviewAnalysisCacheStore>,
) -> Arc<ReviewSessionProcessor<processor_support::CapturedLichess>> {
    processor_with_stores(Arc::new(InMemoryGameImportStore::default()), checkpoints)
}

fn processor_with_stores(
    game_imports: Arc<dyn GameImportStore>,
    checkpoints: Arc<dyn ReviewAnalysisCacheStore>,
) -> Arc<ReviewSessionProcessor<processor_support::CapturedLichess>> {
    let recording = processor_support::provider_recording();
    processor_with_engine_and_stores(
        Arc::new(processor_support::RecordingEngine::new(&recording)),
        game_imports,
        checkpoints,
    )
}

fn processor_with_engine_and_stores(
    engine: Arc<dyn EngineAnalyzer>,
    game_imports: Arc<dyn GameImportStore>,
    checkpoints: Arc<dyn ReviewAnalysisCacheStore>,
) -> Arc<ReviewSessionProcessor<processor_support::CapturedLichess>> {
    let recording = processor_support::provider_recording();
    Arc::new(
        ReviewSessionProcessor::new(
            processor_support::CapturedLichess::new(),
            recording.clone(),
            engine,
            Arc::new(processor_support::RecordingHuman::new(&recording, false)),
            Arc::new(processor_support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(game_imports)
        .with_review_analysis_cache(checkpoints),
    )
}

fn live_processor_with_engine_and_stores(
    engine: Arc<dyn EngineAnalyzer>,
    game_imports: Arc<dyn GameImportStore>,
    checkpoints: Arc<dyn ReviewAnalysisCacheStore>,
) -> Arc<ReviewSessionProcessor<processor_support::CapturedLichess>> {
    let recording = processor_support::provider_recording();
    Arc::new(
        ReviewSessionProcessor::new_live_with_authors(
            processor_support::CapturedLichess::new(),
            engine,
            Arc::new(processor_support::RecordingHuman::new(&recording, false)),
            Arc::new(processor_support::GroundedAuthor),
        )
        .with_game_import_store(game_imports)
        .with_review_analysis_cache(checkpoints),
    )
}

struct BlockingAfterImportEngine {
    recording: processor_support::RecordingEngine,
    remaining_import_calls: AtomicUsize,
    started: watch::Sender<bool>,
    released: watch::Sender<bool>,
}

impl BlockingAfterImportEngine {
    fn new() -> Self {
        let recording = processor_support::provider_recording();
        let (started, _) = watch::channel(false);
        let (released, _) = watch::channel(false);
        Self {
            recording: processor_support::RecordingEngine::new(&recording),
            remaining_import_calls: AtomicUsize::new(processor_support::canonical_game_plies()),
            started,
            released,
        }
    }

    async fn wait_until_started(&self) {
        let mut started = self.started.subscribe();
        while !*started.borrow_and_update() {
            started.changed().await.unwrap();
        }
    }

    fn release(&self) {
        self.released.send_replace(true);
    }
}

impl EngineAnalyzer for BlockingAfterImportEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            if self
                .remaining_import_calls
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                    remaining.checked_sub(1)
                })
                .is_ok()
            {
                return self.recording.analyze(input).await;
            }
            self.started.send_replace(true);
            let mut released = self.released.subscribe();
            while !*released.borrow_and_update() {
                released.changed().await.unwrap();
            }
            self.recording.analyze(input).await
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        self.recording.provenance()
    }
}

struct CountingAfterImportEngine {
    recording: processor_support::RecordingEngine,
    remaining_import_calls: AtomicUsize,
    preparation_calls: AtomicUsize,
}

struct CountingEngine {
    recording: processor_support::RecordingEngine,
    calls: AtomicUsize,
}

impl CountingEngine {
    fn new() -> Self {
        let recording = processor_support::provider_recording();
        Self {
            recording: processor_support::RecordingEngine::new(&recording),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl EngineAnalyzer for CountingEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.recording.analyze(input).await
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        self.recording.provenance()
    }
}

struct CountingHuman {
    recording: processor_support::RecordingHuman,
    calls: AtomicUsize,
}

impl CountingHuman {
    fn new() -> Self {
        let recording = processor_support::provider_recording();
        Self {
            recording: processor_support::RecordingHuman::new(&recording, false),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl HumanMoveModel for CountingHuman {
    fn predict<'a>(
        &'a self,
        input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.recording.predict(input).await
        })
    }
}

impl CountingAfterImportEngine {
    fn new() -> Self {
        let recording = processor_support::provider_recording();
        Self {
            recording: processor_support::RecordingEngine::new(&recording),
            remaining_import_calls: AtomicUsize::new(processor_support::canonical_game_plies()),
            preparation_calls: AtomicUsize::new(0),
        }
    }
}

impl EngineAnalyzer for CountingAfterImportEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            if self
                .remaining_import_calls
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                    remaining.checked_sub(1)
                })
                .is_ok()
            {
                return self.recording.analyze(input).await;
            }
            self.preparation_calls.fetch_add(1, Ordering::SeqCst);
            self.recording.analyze(input).await
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        self.recording.provenance()
    }
}

struct CountingCheckpointStore {
    inner: InMemoryReviewAnalysisCache,
    seeds: AtomicUsize,
    replaces: AtomicUsize,
    unavailable: bool,
}

impl CountingCheckpointStore {
    fn available() -> Self {
        Self {
            inner: InMemoryReviewAnalysisCache::default(),
            seeds: AtomicUsize::new(0),
            replaces: AtomicUsize::new(0),
            unavailable: false,
        }
    }

    fn unavailable() -> Self {
        Self {
            inner: InMemoryReviewAnalysisCache::default(),
            seeds: AtomicUsize::new(0),
            replaces: AtomicUsize::new(0),
            unavailable: true,
        }
    }
}

impl ReviewAnalysisCacheStore for CountingCheckpointStore {
    fn seed<'a>(&'a self, entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            self.seeds.fetch_add(1, Ordering::SeqCst);
            if self.unavailable {
                Err(ReviewAnalysisCacheError::Unavailable)
            } else {
                self.inner.seed(entries).await
            }
        })
    }

    fn load<'a>(
        &'a self,
        game_import_id: &'a GameImportId,
        game: &'a ReviewSessionGame,
        now: chrono::DateTime<Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>> {
        if self.unavailable {
            Box::pin(async { Err(ReviewAnalysisCacheError::Unavailable) })
        } else {
            self.inner.load(game_import_id, game, now)
        }
    }

    fn replace_moment<'a>(
        &'a self,
        mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a> {
        self.replaces.fetch_add(1, Ordering::SeqCst);
        if self.unavailable {
            Box::pin(async { Err(ReviewAnalysisCacheError::Unavailable) })
        } else {
            self.inner.replace_moment(mutation)
        }
    }
}

struct BlockingCheckpointStore {
    started: tokio::sync::watch::Sender<bool>,
    active_creates: AtomicUsize,
}

impl BlockingCheckpointStore {
    fn new() -> Self {
        let (started, _) = tokio::sync::watch::channel(false);
        Self {
            started,
            active_creates: AtomicUsize::new(0),
        }
    }

    async fn wait_until_started(&self) {
        let mut started = self.started.subscribe();
        while !*started.borrow_and_update() {
            started.changed().await.unwrap();
        }
    }
}

impl ReviewAnalysisCacheStore for BlockingCheckpointStore {
    fn seed<'a>(&'a self, _entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            self.active_creates.fetch_add(1, Ordering::SeqCst);
            let _active_create = ActiveCheckpointCreate(&self.active_creates);
            self.started.send_replace(true);
            pending().await
        })
    }

    fn load<'a>(
        &'a self,
        _game_import_id: &'a GameImportId,
        _game: &'a ReviewSessionGame,
        _now: chrono::DateTime<Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn replace_moment<'a>(
        &'a self,
        _mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async { Err(ReviewAnalysisCacheError::Unavailable) })
    }
}

struct ActiveCheckpointCreate<'a>(&'a AtomicUsize);

impl Drop for ActiveCheckpointCreate<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct EmptyAutomaticMomentsStore {
    inner: InMemoryGameImportStore,
}

impl GameImportStore for EmptyAutomaticMomentsStore {
    fn create<'a>(&'a self, mut record: GameImportRecord) -> GameImportStoreFuture<'a, ()> {
        record.frozen_review.critical_moments.clear();
        self.inner.create(record)
    }

    fn list_game_import_records<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<GameImportRecord>> {
        self.inner.list_game_import_records(owner)
    }

    fn find<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
        game_import_id: &'a GameImportId,
    ) -> GameImportStoreFuture<'a, GameImportLookup> {
        self.inner.find(owner, game_import_id)
    }

    fn retain_for_review_session<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        self.inner.retain_for_review_session(owner, reference)
    }

    fn resolve_review_session_reference<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        self.inner
            .resolve_review_session_reference(owner, reference)
    }
}

#[derive(Default)]
struct CorruptingGameImportStore {
    inner: InMemoryGameImportStore,
}

impl GameImportStore for CorruptingGameImportStore {
    fn create<'a>(&'a self, mut record: GameImportRecord) -> GameImportStoreFuture<'a, ()> {
        let impossible_before = record.imported_game.game.final_position_ref.clone();
        record.imported_game.game.moves[0].before_position_ref = impossible_before;
        self.inner.create(record)
    }

    fn list_game_import_records<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<GameImportRecord>> {
        self.inner.list_game_import_records(owner)
    }

    fn find<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
        game_import_id: &'a GameImportId,
    ) -> GameImportStoreFuture<'a, GameImportLookup> {
        self.inner.find(owner, game_import_id)
    }

    fn retain_for_review_session<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        self.inner.retain_for_review_session(owner, reference)
    }

    fn resolve_review_session_reference<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        self.inner
            .resolve_review_session_reference(owner, reference)
    }
}

fn retention_binding(store: Arc<InMemoryQualityCaptureStore>) -> ReviewSessionWebBinding {
    let processor = ReviewSessionProcessor::new_live_with_authors(
        processor_support::CapturedLichess::new(),
        Arc::new(UniversalEngine),
        Arc::new(UniversalHuman),
        Arc::new(processor_support::GroundedAuthor),
    );
    let executor: Arc<dyn ReviewSessionCommandExecutor> = Arc::new(processor);
    ReviewSessionWebBinding::new(executor).with_quality_capture_store(store)
}

fn application_with_executor(
    executor: Arc<dyn ReviewSessionCommandExecutor>,
    quality_capture: Option<Arc<InMemoryQualityCaptureStore>>,
) -> Router {
    let binding = ReviewSessionWebBinding::new(executor);
    let binding = match quality_capture {
        Some(store) => binding.with_quality_capture_store(store),
        None => binding,
    };
    app(Arc::new(AppState {
        account_deletion:
            chen_chess_coach_engine::account_deletion::AccountDeletionRuntime::disabled(),
        auth: AuthConfig::new_firebase(FIREBASE_PROJECT_ID, jwt_jwks()).unwrap(),
        beta_access: chen_chess_coach_engine::beta_access::BetaAccessRuntime::disabled(),
        daily_coaching: chen_chess_coach_engine::daily_coaching::DailyCoachingRuntime::disabled(),
        imported_games: chen_chess_coach_engine::imported_games::ImportedGamesRuntime::in_memory(),
        opening_analysis:
            chen_chess_coach_engine::opening_analysis::OpeningAnalysisRuntime::disabled(),
        review_session: binding,
    }))
}

struct UniversalHuman;

struct UniversalEngine;

impl EngineAnalyzer for UniversalEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        let best_move = first_legal_uci(input.position);
        Box::pin(async move {
            Ok(EngineAnalysis {
                principal_variation: vec![best_move.clone()],
                best_move,
                evaluation: PositionEvaluation::Centipawns(50),
                depth: PINNED_STOCKFISH_DEPTH,
            })
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        Some(EngineProvenance {
            version: PINNED_STOCKFISH_VERSION.to_string(),
            binary_sha256: PINNED_STOCKFISH_BINARY_DIGEST
                .strip_prefix("sha256:")
                .expect("pinned Stockfish digest has a prefix")
                .to_string(),
            depth: PINNED_STOCKFISH_DEPTH,
            threads: PINNED_STOCKFISH_THREADS,
            hash_mib: PINNED_STOCKFISH_HASH_MIB,
        })
    }
}

impl HumanMoveModel for UniversalHuman {
    fn predict<'a>(
        &'a self,
        input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        let candidate = first_legal_uci(input.position);
        Box::pin(async move {
            Ok(HumanMovePrediction {
                candidates: vec![HumanMoveCandidate {
                    uci: candidate,
                    probability: 0.5,
                    rank: 1,
                }],
                win_probability: Some(0.5),
            })
        })
    }
}

fn first_legal_uci(fen: &str) -> String {
    let position: Chess = Fen::from_ascii(fen.as_bytes())
        .expect("test positions are valid FEN")
        .into_position(CastlingMode::Standard)
        .expect("test positions are legal");
    let chess_move = position
        .legal_moves()
        .into_iter()
        .next()
        .expect("test positions have a legal move");
    UciMove::from_standard(&chess_move).to_string()
}

async fn import_and_start_review_session(binding: &ReviewSessionWebBinding, label: &str) {
    let imported = binding_events(binding, &format!("{label}-import"), import_command()).await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = binding_events(
        binding,
        &format!("{label}-start"),
        ReviewSessionCommand::StartReviewSession { game_import_id },
    )
    .await;
    assert!(matches!(
        completion(&started),
        OperationCompletion::ReviewSessionStarted { .. }
    ));
}

async fn http_events(
    application: &Router,
    label: &str,
    command: ReviewSessionCommand,
) -> Vec<ReviewSessionEventEnvelope> {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/review-session/commands")
        .header(header::AUTHORIZATION, format!("Bearer {}", player_token()))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&ReviewSessionCommandEnvelope {
                request_id: RequestId::try_from(format!("request:journey:{label}")).unwrap(),
                operation_id: OperationId::try_from(format!("operation:journey:{label}")).unwrap(),
                surface: DeliverySurface::Web,
                command,
            })
            .unwrap(),
        ))
        .unwrap();
    let response = application.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}

async fn binding_events(
    binding: &ReviewSessionWebBinding,
    label: &str,
    command: ReviewSessionCommand,
) -> Vec<ReviewSessionEventEnvelope> {
    let envelope = transport_support::envelope(DeliverySurface::Web, label, command);
    transport_support::collect_receiver(binding.submit(
        transport_support::WEB_SUBJECT,
        &serde_json::to_vec(&envelope).unwrap(),
    ))
    .await
}

fn import_command() -> ReviewSessionCommand {
    ReviewSessionCommand::ImportGame {
        source: GameInputSource::LichessUrl {
            url: "https://lichess.org/Synthet1Demo/black".to_string(),
        },
        review_side: RequestedReviewSide::FromQualifiedUrl,
        elo_profile: RequestedEloProfile::FromImportedMetadata,
    }
}

async fn surface_events(
    processor: &Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    surface: DeliverySurface,
    label: &str,
    command: ReviewSessionCommand,
) -> Vec<ReviewSessionEventEnvelope> {
    let principal = match surface {
        DeliverySurface::CoachSkill => ProcessorPrincipal::LocalCoach,
        DeliverySurface::Web | DeliverySurface::CoachApp => {
            ProcessorPrincipal::Player(PlayerId::try_from("journey-player".to_string()).unwrap())
        }
    };
    transport_support::collect_receiver(ReviewSessionProcessor::submit(
        processor,
        principal,
        &serde_json::to_vec(&transport_support::envelope(surface, label, command)).unwrap(),
    ))
    .await
}

fn imported_game_id(events: &[ReviewSessionEventEnvelope]) -> GameImportId {
    match completion(events) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    }
}

fn completion(events: &[ReviewSessionEventEnvelope]) -> &OperationCompletion {
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => Some(result.as_ref()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("command did not complete: {events:?}"))
}
