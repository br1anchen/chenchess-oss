use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, Notify};
use tracing::Instrument;

use crate::{
    critical_moment_comment::HostedCommentRuntime,
    digested_games::{DigestedGameIndex, NoDigestedGames},
    engine_analysis::EngineAnalyzer,
    evaluation_recording::{RecordingError, ReviewSessionProviderRecording},
    game_analysis_store::{GameAnalysisStore, InMemoryGameAnalysisStore},
    game_import::GameImporter,
    game_import_store::{GameImportStore, InMemoryGameImportStore},
    human_move_model::HumanMoveModel,
    language_layer_ledger::LanguageLayerLedger,
    learning_path_feedback::{InMemoryLearningPathFeedbackStore, LearningPathFeedbackStore},
    lichess::LichessExportClient,
    profile_game_feed::DailyGameReviewRequest,
    request_single_flight::SingleFlight,
    request_trace::ReviewSessionTraceId,
    review_analysis_cache::{
        FirestoreFirstOpenPublication, InMemoryReviewAnalysisCache, ReviewAnalysisCacheStore,
    },
    review_annotation_store::{
        InMemoryReviewAnnotationStore, ReviewAnnotationAddress, ReviewAnnotationLog,
        ReviewAnnotationStore, ReviewAnnotationStoreError,
    },
    review_session_cancellation::ReviewSessionCancellation,
    review_session_coaching::{AlternativeMoveAssessmentAuthor, CoachTurnActivity},
    review_session_contract::*,
    review_session_exploration::{AlternativeMoveCancellation, ExploreAlternativeMoveRequest},
    review_share::{InMemoryReviewShareStore, ReviewShareStore},
};

use admission::{CoachAdmission, EngineAdmission};
use events::EventEmitter;
use ingress::DecodedCommand;
use session::ProcessorSession;

mod addressed_reads;
mod admission;
mod coaching;
mod deletion;
mod eager_authoring;
mod events;
mod exploration;
mod feedback;
mod host_capabilities;
mod host_turn;
mod ingress;
mod lifecycle;
mod live;
mod mutation;
mod player_plan;
mod prefetch;
mod publication;
mod readiness;
mod residency;
mod session;
mod terminal;

pub(crate) use admission::PlayerTrafficPolicy;
#[cfg(test)]
pub(crate) use admission::{
    ControllableTrafficClock, PLAYER_COMMAND_LIMIT, PLAYER_COMMAND_WINDOW_MS, PLAYER_IMPORT_LIMIT,
};
pub use ingress::ProcessorCommandAdmission;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "playerId", rename_all = "camelCase")]
pub enum ProcessorPrincipal {
    LocalCoach,
    Player(PlayerId),
}

impl ProcessorPrincipal {
    fn permits(&self, surface: DeliverySurface) -> bool {
        matches!(
            (self, surface),
            (Self::LocalCoach, DeliverySurface::CoachSkill)
                | (
                    Self::Player(_),
                    DeliverySurface::Web | DeliverySurface::CoachApp
                )
        )
    }
}

pub struct ReviewSessionProcessor<C> {
    importer: GameImporter<C>,
    recording: Option<Arc<ReviewSessionProviderRecording>>,
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
    coaching_author: Arc<dyn AlternativeMoveAssessmentAuthor>,
    game_imports: Arc<dyn GameImportStore>,
    /// Whether Daily Coaching produced a Game, which decides whether the Player
    /// may delete it.
    digested_games: Arc<dyn DigestedGameIndex>,
    review_shares: Arc<dyn ReviewShareStore>,
    game_analysis: Arc<dyn GameAnalysisStore>,
    analysis_cache: Arc<dyn ReviewAnalysisCacheStore>,
    annotations: Arc<dyn ReviewAnnotationStore>,
    learning_path_feedback: Arc<dyn LearningPathFeedbackStore>,
    sessions: Mutex<BTreeMap<GameImportId, Arc<ProcessorSession>>>,
    coach_turn_activity: Mutex<BTreeMap<CoachTurnActivityScope, Weak<CoachTurnActivity>>>,
    starting_sessions: Mutex<BTreeMap<GameImportId, Arc<SessionStartSignal>>>,
    live: Mutex<BTreeMap<OperationId, LiveOperation>>,
    coach_admission: CoachAdmission,
    engine_admission: Arc<EngineAdmission>,
    player_traffic: Arc<PlayerTrafficPolicy>,
    runtime_startup: Option<Duration>,
    language_layer_ledger: Option<Arc<dyn LanguageLayerLedger>>,
    hosted_comment: Option<Arc<HostedCommentRuntime>>,
    /// Author web coaching artifacts in the background after every import.
    /// Off by default so a test-constructed processor stays a pure
    /// request-response machine; the deployed runtime turns it on.
    eager_web_artifacts: bool,
    first_open_flights: SingleFlight<(GameImportId, CriticalMomentId, String)>,
    first_open_persist: Option<Arc<FirestoreFirstOpenPublication>>,
    quality_capture: crate::quality_capture::QualityCaptureAppender,
    coaching_profile: std::sync::Mutex<crate::language_layer_prompt::CoachingProfileProjection>,
}

#[derive(Clone)]
enum LiveOperation {
    ReviewMomentPreparation {
        owner: ProcessorPrincipal,
        game_import_id: GameImportId,
        idempotency_key: IdempotencyKey,
        cancellation: ReviewSessionCancellation,
    },
    AlternativeMove {
        owner: ProcessorPrincipal,
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        idempotency_key: IdempotencyKey,
        cancellation: AlternativeMoveCancellation,
    },
    CoachTurn {
        owner: ProcessorPrincipal,
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        idempotency_key: IdempotencyKey,
        coach_turn_id: CoachTurnId,
        coaching: Arc<crate::review_session_coaching::AlternativeMoveCoaching>,
    },
    HostTurn {
        owner: ProcessorPrincipal,
        game_import_id: GameImportId,
        idempotency_key: IdempotencyKey,
        cancellation: crate::review_session_cancellation::ReviewSessionCancellation,
    },
}

/// Addresses the at-most-one active Coach Turn rule.
///
/// The rule belongs to the Player and the reviewed Game, so every Review
/// Session over one Game Import resolves to the same scope and no session
/// identifier takes part.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CoachTurnActivityScope {
    player: ProcessorPrincipal,
    game_import_id: GameImportId,
}

struct SessionStartSignal {
    complete: AtomicBool,
    changed: Notify,
}

impl SessionStartSignal {
    fn new() -> Self {
        Self {
            complete: AtomicBool::new(false),
            changed: Notify::new(),
        }
    }

    async fn wait(&self) {
        while !self.complete.load(Ordering::Acquire) {
            let notified = self.changed.notified();
            if self.complete.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    fn finish(&self) {
        self.complete.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub fn new(
        lichess: C,
        recording: ReviewSessionProviderRecording,
        engine: Arc<dyn EngineAnalyzer>,
        human: Arc<dyn HumanMoveModel>,
        coaching_author: Arc<dyn AlternativeMoveAssessmentAuthor>,
    ) -> Result<Self, RecordingError> {
        recording.verify()?;
        Ok(Self::build(
            lichess,
            Some(Arc::new(recording)),
            engine,
            human,
            coaching_author,
        ))
    }

    pub fn new_with_authors(
        lichess: C,
        recording: ReviewSessionProviderRecording,
        engine: Arc<dyn EngineAnalyzer>,
        human: Arc<dyn HumanMoveModel>,
        coaching_author: Arc<dyn AlternativeMoveAssessmentAuthor>,
    ) -> Result<Self, RecordingError> {
        recording.verify()?;
        Ok(Self::build(
            lichess,
            Some(Arc::new(recording)),
            engine,
            human,
            coaching_author,
        ))
    }

    pub fn new_live_with_authors(
        lichess: C,
        engine: Arc<dyn EngineAnalyzer>,
        human: Arc<dyn HumanMoveModel>,
        coaching_author: Arc<dyn AlternativeMoveAssessmentAuthor>,
    ) -> Self {
        Self::build(lichess, None, engine, human, coaching_author)
    }

    fn build(
        lichess: C,
        recording: Option<Arc<ReviewSessionProviderRecording>>,
        engine: Arc<dyn EngineAnalyzer>,
        human: Arc<dyn HumanMoveModel>,
        coaching_author: Arc<dyn AlternativeMoveAssessmentAuthor>,
    ) -> Self {
        Self {
            importer: GameImporter::new(lichess),
            recording,
            engine,
            human,
            coaching_author,
            game_imports: Arc::new(InMemoryGameImportStore::default()),
            digested_games: Arc::new(NoDigestedGames),
            review_shares: Arc::new(InMemoryReviewShareStore::default()),
            game_analysis: Arc::new(InMemoryGameAnalysisStore::default()),
            analysis_cache: Arc::new(InMemoryReviewAnalysisCache::default()),
            annotations: Arc::new(InMemoryReviewAnnotationStore::default()),
            learning_path_feedback: Arc::new(InMemoryLearningPathFeedbackStore::default()),
            sessions: Mutex::new(BTreeMap::new()),
            coach_turn_activity: Mutex::new(BTreeMap::new()),
            starting_sessions: Mutex::new(BTreeMap::new()),
            live: Mutex::new(BTreeMap::new()),
            coach_admission: CoachAdmission::v1(),
            engine_admission: Arc::new(EngineAdmission::v1()),
            player_traffic: Arc::new(PlayerTrafficPolicy::v1()),
            runtime_startup: None,
            language_layer_ledger: None,
            hosted_comment: None,
            eager_web_artifacts: false,
            first_open_flights: SingleFlight::default(),
            first_open_persist: None,
            quality_capture: crate::quality_capture::QualityCaptureAppender::Inert,
            coaching_profile: std::sync::Mutex::new(
                crate::language_layer_prompt::CoachingProfileProjection::cold_start(),
            ),
        }
    }

    pub fn with_runtime_startup(mut self, duration: Duration) -> Self {
        self.runtime_startup = Some(duration);
        self
    }

    #[cfg(test)]
    pub fn with_player_traffic(mut self, player_traffic: Arc<PlayerTrafficPolicy>) -> Self {
        self.player_traffic = player_traffic;
        self
    }

    pub fn with_language_layer_ledger(mut self, ledger: Arc<dyn LanguageLayerLedger>) -> Self {
        self.language_layer_ledger = Some(ledger);
        self
    }

    pub fn with_eager_web_artifacts(mut self) -> Self {
        self.eager_web_artifacts = true;
        self
    }

    pub fn with_hosted_comment(mut self, hosted: Arc<HostedCommentRuntime>) -> Self {
        self.hosted_comment = Some(hosted);
        self
    }

    pub fn with_coaching_profile(
        self,
        profile: crate::language_layer_prompt::CoachingProfileProjection,
    ) -> Self {
        *self.coaching_profile.lock().expect("coaching profile lock") = profile;
        self
    }

    pub fn set_coaching_profile(
        &self,
        profile: crate::language_layer_prompt::CoachingProfileProjection,
    ) {
        *self.coaching_profile.lock().expect("coaching profile lock") = profile;
    }

    pub(super) fn current_coaching_profile(
        &self,
    ) -> crate::language_layer_prompt::CoachingProfileProjection {
        self.coaching_profile
            .lock()
            .expect("coaching profile lock")
            .clone()
    }

    pub(crate) fn with_first_open_persist(
        mut self,
        persist: Arc<FirestoreFirstOpenPublication>,
    ) -> Self {
        self.first_open_persist = Some(persist);
        self
    }

    pub(crate) fn with_quality_capture_appender(
        mut self,
        appender: crate::quality_capture::QualityCaptureAppender,
    ) -> Self {
        self.quality_capture = appender;
        self
    }

    pub fn with_in_memory_quality_capture(
        self,
        store: Arc<crate::quality_capture::InMemoryQualityCaptureStore>,
    ) -> Self {
        self.with_quality_capture_appender(crate::quality_capture::QualityCaptureAppender::memory(
            store,
        ))
    }

    pub fn with_hosted_language_layer(
        mut self,
        binding: crate::pin_record::HostedLanguageLayerBinding,
    ) -> Self {
        let crate::pin_record::HostedLanguageLayerBinding::Bound {
            provider,
            fingerprint,
            pin,
        } = binding
        else {
            return self;
        };
        let Some(ledger) = self.language_layer_ledger.clone() else {
            return self;
        };
        let config =
            crate::language_layer_ledger::LanguageLayerAdmissionConfig::conservative_defaults();
        let concurrency = Arc::new(crate::language_layer_ledger::ProviderConcurrency::new(
            config.max_concurrent_provider_calls,
        ));
        self.hosted_comment = Some(Arc::new(HostedCommentRuntime::new(
            Arc::new(provider),
            pin,
            fingerprint,
            ledger,
            concurrency,
            config,
        )));
        self
    }

    pub fn with_game_import_store(mut self, game_imports: Arc<dyn GameImportStore>) -> Self {
        self.game_imports = game_imports;
        self
    }

    pub fn with_game_analysis_store(mut self, game_analysis: Arc<dyn GameAnalysisStore>) -> Self {
        self.game_analysis = game_analysis;
        self
    }

    pub fn with_review_analysis_cache(
        mut self,
        analysis_cache: Arc<dyn ReviewAnalysisCacheStore>,
    ) -> Self {
        self.analysis_cache = analysis_cache;
        self
    }

    pub fn with_review_annotation_store(
        mut self,
        annotations: Arc<dyn ReviewAnnotationStore>,
    ) -> Self {
        self.annotations = annotations;
        self
    }

    pub fn with_digested_games(mut self, digested_games: Arc<dyn DigestedGameIndex>) -> Self {
        self.digested_games = digested_games;
        self
    }

    pub fn with_review_share_store(mut self, shares: Arc<dyn ReviewShareStore>) -> Self {
        self.review_shares = shares;
        self
    }

    /// Opens this Player's durable notes on one imported Game.
    ///
    /// Read once per Review Session rather than once per Review Moment: the
    /// annotations of a whole review are small and every moment answers from
    /// the same snapshot.
    pub(crate) async fn review_annotation_log(
        &self,
        owner: &ProcessorPrincipal,
        game_import_id: &GameImportId,
    ) -> Result<Arc<ReviewAnnotationLog>, ReviewAnnotationStoreError> {
        ReviewAnnotationLog::load(
            self.annotations.clone(),
            ReviewAnnotationAddress {
                owner: owner.clone(),
                game_import_id: game_import_id.clone(),
            },
        )
        .await
        .map(Arc::new)
    }

    pub fn with_learning_path_feedback_store(
        mut self,
        learning_path_feedback: Arc<dyn LearningPathFeedbackStore>,
    ) -> Self {
        self.learning_path_feedback = learning_path_feedback;
        self
    }

    /// The Coach Turn activity scope one Player shares across every Review
    /// Session over one Game Import.
    ///
    /// Scopes are held weakly: the last session on a Game drops the entry with
    /// it, and a fresh session on the same Game starts a fresh scope. Like every
    /// other transient Review Session state here the map is per process, so the
    /// rule binds the Player's sessions on one Coach Engine and not across a
    /// replicated deployment.
    async fn coach_turn_activity(
        &self,
        player: &ProcessorPrincipal,
        game_import_id: &GameImportId,
    ) -> Arc<CoachTurnActivity> {
        let scope = CoachTurnActivityScope {
            player: player.clone(),
            game_import_id: game_import_id.clone(),
        };
        let mut scopes = self.coach_turn_activity.lock().await;
        if let Some(shared) = scopes.get(&scope).and_then(Weak::upgrade) {
            return shared;
        }
        scopes.retain(|_, held| held.strong_count() > 0);
        let activity = Arc::new(CoachTurnActivity::default());
        scopes.insert(scope, Arc::downgrade(&activity));
        activity
    }

    pub fn submit(
        self: &Arc<Self>,
        principal: ProcessorPrincipal,
        serialized_command: &[u8],
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        self.submit_admitted(
            principal,
            ProcessorCommandAdmission::parse(serialized_command),
        )
    }

    pub(crate) fn submit_admitted(
        self: &Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        self.submit_admitted_with_trace(principal, admission, None)
    }

    pub(crate) fn submit_admitted_with_trace(
        self: &Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
        trace_id: Option<ReviewSessionTraceId>,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        self.dispatch_admitted(principal, admission, trace_id, true)
    }

    pub(crate) fn submit_admitted_unmetered(
        self: &Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        self.dispatch_admitted(principal, admission, None, false)
    }

    fn dispatch_admitted(
        self: &Arc<Self>,
        principal: ProcessorPrincipal,
        admission: ProcessorCommandAdmission,
        trace_id: Option<ReviewSessionTraceId>,
        meter_player_traffic: bool,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let (decoded, validation_milliseconds) = admission.into_decoded();
        match decoded {
            DecodedCommand::Rejected {
                request_id,
                operation_id,
                operation,
                reason,
            } => {
                let (emitter, receiver) = EventEmitter::new(
                    request_id,
                    operation_id,
                    operation,
                    trace_id,
                    validation_milliseconds,
                );
                emitter.rejected(operation, reason, RejectionRecovery::CorrectInput);
                receiver
            }
            DecodedCommand::Ready(envelope) => {
                let operation = envelope.command.operation();
                let (emitter, receiver) = EventEmitter::new(
                    envelope.request_id.clone(),
                    envelope.operation_id.clone(),
                    operation,
                    trace_id.clone(),
                    validation_milliseconds,
                );
                if meter_player_traffic {
                    if let ProcessorPrincipal::Player(player_id) = &principal {
                        if let Err(retry_after_seconds) =
                            self.player_traffic.admit_command(player_id)
                        {
                            emit_player_rate_limited(&emitter, operation, retry_after_seconds);
                            return receiver;
                        }
                    }
                }
                let processor = self.clone();
                let span = tracing::info_span!(
                    "review_session_operation",
                    operation = ?operation,
                    operation_id = envelope.operation_id.as_str(),
                    request_id = envelope.request_id.as_str(),
                    trace_id = trace_id
                        .as_ref()
                        .map(ReviewSessionTraceId::as_str)
                        .unwrap_or("unavailable"),
                );
                tokio::spawn(
                    async move {
                        processor.execute(principal, *envelope, emitter).await;
                    }
                    .instrument(span),
                );
                receiver
            }
        }
    }

    pub(crate) fn submit_daily_game(
        self: &Arc<Self>,
        player_id: PlayerId,
        request_id: RequestId,
        operation_id: OperationId,
        request: DailyGameReviewRequest,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let (emitter, receiver) = EventEmitter::new(
            request_id.clone(),
            operation_id.clone(),
            OperationKind::GameImport,
            None,
            0.0,
        );
        emitter.accepted(OperationKind::GameImport);
        let principal = ProcessorPrincipal::Player(player_id);
        let processor = self.clone();
        let span = tracing::info_span!(
            "daily_coaching_game_import",
            operation = ?OperationKind::GameImport,
            operation_id = operation_id.as_str(),
            request_id = request_id.as_str(),
        );
        tokio::spawn(
            async move {
                processor
                    .import_daily_game(principal, request, emitter)
                    .await;
            }
            .instrument(span),
        );
        receiver
    }

    async fn execute(
        self: &Arc<Self>,
        principal: ProcessorPrincipal,
        envelope: ReviewSessionCommandEnvelope,
        emitter: Arc<EventEmitter>,
    ) {
        let operation = envelope.command.operation();
        if !principal.permits(envelope.surface) {
            emitter.rejected(
                operation,
                CommandRejectionReason::AuthenticationRequired,
                RejectionRecovery::None,
            );
            return;
        }
        if let ReviewSessionCommand::ImportGame {
            source,
            review_side,
            ..
        } = &envelope.command
        {
            if let Err(error) = crate::game_import::validate_import_boundary(source, *review_side) {
                emitter.event(error.terminal().event().clone());
                return;
            }
            if let ProcessorPrincipal::Player(player_id) = &principal {
                if let Err(retry_after_seconds) = self
                    .player_traffic
                    .admit_import(player_id, &envelope.operation_id)
                {
                    emit_player_rate_limited(&emitter, operation, retry_after_seconds);
                    return;
                }
            }
        }
        let defers_acceptance = matches!(
            &envelope.command,
            ReviewSessionCommand::ExploreAlternativeMove { .. }
                | ReviewSessionCommand::StartCoachTurn { .. }
                | ReviewSessionCommand::StartHostTurn { .. }
        );
        if !defers_acceptance {
            emitter.accepted(operation);
        }

        match envelope.command {
            ReviewSessionCommand::ImportGame {
                source,
                review_side,
                elo_profile,
            } => {
                self.import_game(principal, source, review_side, elo_profile, emitter)
                    .await
            }
            ReviewSessionCommand::StartReviewSession { game_import_id } => {
                self.start_session(principal, envelope.surface, game_import_id, emitter)
                    .await
            }
            ReviewSessionCommand::OpenGameReview { game_import_id } => {
                self.open_game_review(principal, game_import_id, emitter)
                    .await
            }
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id,
                known_content_digest,
            } => {
                self.read_game_review_snapshot(
                    principal,
                    game_import_id,
                    known_content_digest,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::ReadReviewMomentDetail {
                game_import_id,
                review_moment_id,
                known_content_digest,
            } => {
                self.read_review_moment_detail(
                    principal,
                    game_import_id,
                    review_moment_id,
                    known_content_digest,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::OpenAddressedReviewMoment {
                game_import_id,
                reference,
            } => {
                self.open_addressed_review_moment(principal, game_import_id, reference, emitter)
                    .await
            }
            ReviewSessionCommand::ReadReviewMomentExplanation {
                game_import_id,
                review_moment_id,
            } => {
                self.read_review_moment_explanation(
                    principal,
                    game_import_id,
                    review_moment_id,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::OpenGameReviewByIdentity {
                source,
                review_side,
                elo_rating,
            } => {
                self.open_game_review_by_identity(
                    principal,
                    source,
                    review_side,
                    elo_rating,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id,
                selection,
                idempotency_key,
            } => {
                self.open_review_moment(
                    readiness::OpenReviewMomentRequest {
                        principal,
                        operation_id: envelope.operation_id,
                        game_import_id,
                        selection,
                        idempotency_key,
                        surface: envelope.surface,
                    },
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::InspectPosition {
                game_import_id,
                review_moment_id,
                target,
            } => {
                self.inspect_position(
                    &principal,
                    game_import_id,
                    review_moment_id,
                    target,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::EvaluatePlayerPlan {
                game_import_id,
                review_moment_id,
                request,
            } => {
                self.evaluate_player_plan(
                    &principal,
                    game_import_id,
                    review_moment_id,
                    request,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id,
                review_moment_id,
                parent,
                source_position_ref,
                move_input,
                idempotency_key,
            } => {
                self.explore_move(
                    principal,
                    envelope.operation_id,
                    game_import_id,
                    review_moment_id,
                    ExploreAlternativeMoveRequest {
                        parent,
                        source_position_ref,
                        move_input,
                        idempotency_key,
                    },
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::StartCoachTurn {
                game_import_id,
                review_moment_id,
                coach_turn_id,
                context,
                message,
                idempotency_key,
                prior_turn,
            } => {
                self.start_coach_turn(
                    principal,
                    envelope.surface,
                    envelope.operation_id,
                    game_import_id,
                    review_moment_id,
                    coach_turn_id,
                    *context,
                    message,
                    idempotency_key,
                    prior_turn,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::PublishCoachTurn {
                game_import_id,
                review_moment_id,
                coach_turn_id,
                assessment,
                idempotency_key,
            } => {
                self.publish_coach_turn(
                    &principal,
                    game_import_id,
                    review_moment_id,
                    coach_turn_id,
                    *assessment,
                    idempotency_key,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::PublishReviewMomentComment {
                game_import_id,
                review_moment_id,
                text,
                grounding_ledger,
                idempotency_key,
            } => {
                self.publish_review_moment_comment(
                    &principal,
                    publication::ReviewMomentCommentPublicationInput {
                        game_import_id,
                        review_moment_id,
                        text,
                        grounding_ledger,
                        idempotency_key,
                    },
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::RecordLearningPathExposure {
                game_import_id,
                learning_path_ref,
            } => {
                self.record_learning_path_exposure(
                    principal,
                    envelope.surface,
                    game_import_id,
                    learning_path_ref,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::UpdateLearningPathVote {
                game_import_id,
                learning_path_ref,
                vote,
            } => {
                self.update_learning_path_vote(
                    principal,
                    envelope.surface,
                    game_import_id,
                    learning_path_ref,
                    vote,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::StartHostTurn {
                game_import_id,
                message,
                prior_turns,
                idempotency_key,
            } => {
                self.start_host_turn(
                    principal,
                    envelope.operation_id,
                    game_import_id,
                    message,
                    prior_turns,
                    idempotency_key,
                    emitter,
                )
                .await
            }
            ReviewSessionCommand::DeleteGameImport { game_import_id } => {
                self.delete_game_import(&principal, game_import_id, emitter)
                    .await
            }
            ReviewSessionCommand::CancelOperation {
                game_import_id,
                operation_id,
                idempotency_key,
            } => {
                self.cancel_operation(
                    &principal,
                    &game_import_id,
                    &operation_id,
                    &idempotency_key,
                    emitter,
                )
                .await
            }
        }
    }
}

fn emit_player_rate_limited(
    emitter: &EventEmitter,
    operation: OperationKind,
    retry_after_seconds: u32,
) {
    emitter.unavailable(
        operation,
        ProviderUnavailableReason::RateLimited {
            retry_after_seconds,
        },
        RetryDirective::RetryAfter {
            seconds: retry_after_seconds,
        },
    );
}

impl LiveOperation {
    fn owner(&self) -> &ProcessorPrincipal {
        match self {
            Self::ReviewMomentPreparation { owner, .. }
            | Self::AlternativeMove { owner, .. }
            | Self::CoachTurn { owner, .. }
            | Self::HostTurn { owner, .. } => owner,
        }
    }

    fn game_import_id(&self) -> &GameImportId {
        match self {
            Self::ReviewMomentPreparation { game_import_id, .. }
            | Self::AlternativeMove { game_import_id, .. }
            | Self::CoachTurn { game_import_id, .. }
            | Self::HostTurn { game_import_id, .. } => game_import_id,
        }
    }

    fn idempotency_key(&self) -> &IdempotencyKey {
        match self {
            Self::ReviewMomentPreparation {
                idempotency_key, ..
            }
            | Self::AlternativeMove {
                idempotency_key, ..
            }
            | Self::CoachTurn {
                idempotency_key, ..
            }
            | Self::HostTurn {
                idempotency_key, ..
            } => idempotency_key,
        }
    }
}
