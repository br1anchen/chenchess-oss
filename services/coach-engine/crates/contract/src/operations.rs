use std::time::Duration;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{de, Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use super::*;
use crate::lichess::LichessGameUrl;

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionCommandEnvelope {
    pub request_id: RequestId,
    pub operation_id: OperationId,
    pub surface: DeliverySurface,
    pub command: ReviewSessionCommand,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewSessionCommandEnvelopeWire {
    request_id: RequestId,
    operation_id: OperationId,
    surface: DeliverySurface,
    command: ReviewSessionCommand,
}

impl<'de> Deserialize<'de> for ReviewSessionCommandEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ReviewSessionCommandEnvelopeWire::deserialize(deserializer)?;
        let envelope = Self {
            request_id: wire.request_id,
            operation_id: wire.operation_id,
            surface: wire.surface,
            command: wire.command,
        };
        if envelope.has_valid_surface_ownership()
            && envelope.has_valid_text_limits()
            && envelope.has_valid_import_size()
            && envelope.has_consistent_command_identity()
        {
            Ok(envelope)
        } else {
            Err(de::Error::custom(
                "ReviewSession command violates surface or text policy",
            ))
        }
    }
}

impl ReviewSessionCommandEnvelope {
    fn has_valid_surface_ownership(&self) -> bool {
        match (&self.surface, &self.command) {
            (
                DeliverySurface::Web | DeliverySurface::CoachApp,
                ReviewSessionCommand::ImportGame {
                    source: GameInputSource::LocalPgnFile { .. },
                    ..
                },
            ) => false,
            (
                DeliverySurface::CoachSkill,
                ReviewSessionCommand::ImportGame {
                    source: GameInputSource::PastedPgn { .. } | GameInputSource::LocalPgnFile { .. },
                    review_side,
                    ..
                },
            ) => matches!(review_side, RequestedReviewSide::Selected { .. }),
            (_, ReviewSessionCommand::PublishReviewMomentComment { .. }) => {
                matches!(self.surface, DeliverySurface::CoachApp)
            }
            (DeliverySurface::Web, ReviewSessionCommand::EvaluatePlayerPlan { .. }) => false,
            (_, ReviewSessionCommand::StartHostTurn { .. }) => {
                matches!(self.surface, DeliverySurface::Web)
            }
            /* Destructive account actions route to the web dashboard, the way
            account deletion does, so a host model never holds a Player's
            records in one hand and a delete in the other. */
            (_, ReviewSessionCommand::DeleteGameImport { .. }) => {
                matches!(self.surface, DeliverySurface::Web)
            }
            (_, ReviewSessionCommand::StartCoachTurn { .. }) => {
                !matches!(self.surface, DeliverySurface::Web)
            }
            (
                _,
                ReviewSessionCommand::ImportGame {
                    source: GameInputSource::LichessUrl { url },
                    review_side: RequestedReviewSide::FromQualifiedUrl,
                    ..
                },
            ) => is_side_qualified_lichess_url(url),
            (
                _,
                ReviewSessionCommand::ImportGame {
                    source: GameInputSource::ChessComUrl { .. } | GameInputSource::PastedPgn { .. },
                    review_side: RequestedReviewSide::FromQualifiedUrl,
                    ..
                },
            ) => false,
            _ => true,
        }
    }

    fn has_valid_text_limits(&self) -> bool {
        match &self.command {
            ReviewSessionCommand::StartCoachTurn { message, .. } => {
                message.len() <= usize::from(ReviewSessionLimits::V1.max_player_message_bytes)
            }
            ReviewSessionCommand::StartHostTurn {
                message,
                prior_turns,
                ..
            } => {
                let max_bytes = ReviewSessionLimits::V1.max_player_message_bytes;
                has_nonempty_text_within_limit(message, max_bytes)
                    && prior_turns.len()
                        <= usize::from(ReviewSessionLimits::V1.max_host_turn_prior_turns)
                    && prior_turns.iter().all(|turn| {
                        has_nonempty_text_within_limit(&turn.message, max_bytes)
                            && has_nonempty_text_within_limit(&turn.answer, max_bytes)
                    })
            }
            ReviewSessionCommand::PublishReviewMomentComment { text, .. } => {
                text.len() <= usize::from(ReviewSessionLimits::V1.max_player_message_bytes)
            }
            ReviewSessionCommand::PublishCoachTurn { assessment, .. } => [
                &assessment.objective_quality,
                &assessment.findability,
                &assessment.resilience,
            ]
            .into_iter()
            .all(|dimension| {
                has_nonempty_json_text_within_limit(
                    &dimension.explanation,
                    ReviewSessionLimits::V1.max_player_message_bytes,
                )
            }),
            ReviewSessionCommand::EvaluatePlayerPlan {
                request: PlayerPlanEvaluationRequest::Admit { draft },
                ..
            } => has_nonempty_text_within_limit(
                &draft.text,
                ReviewSessionLimits::V1.max_player_message_bytes,
            ),
            _ => true,
        }
    }

    fn has_valid_import_size(&self) -> bool {
        match &self.command {
            ReviewSessionCommand::ImportGame {
                source: GameInputSource::PastedPgn { pgn },
                ..
            } => pgn.len() <= usize::try_from(ReviewSessionLimits::V1.max_pgn_bytes).unwrap(),
            _ => true,
        }
    }

    fn has_consistent_command_identity(&self) -> bool {
        match &self.command {
            ReviewSessionCommand::StartCoachTurn {
                coach_turn_id,
                context,
                ..
            } => coach_turn_id == &context.coach_turn_id,
            ReviewSessionCommand::PublishCoachTurn {
                coach_turn_id,
                assessment,
                ..
            } => coach_turn_id == &assessment.coach_turn_id,
            _ => true,
        }
    }
}

fn is_side_qualified_lichess_url(url: &str) -> bool {
    LichessGameUrl::parse(url).is_ok_and(|source| source.has_qualified_side())
}

fn has_nonempty_text_within_limit(value: &str, max_bytes: u16) -> bool {
    !value.trim().is_empty() && value.len() <= usize::from(max_bytes)
}

fn has_nonempty_json_text_within_limit(value: &str, max_bytes: u16) -> bool {
    !value.trim().is_empty() && json_text_bytes_within_limit(value, max_bytes)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
/// The complete stable v1 wire vocabulary. Command handlers are added in vertical slices without
/// changing this shared Rust/TypeScript boundary.
pub enum ReviewSessionCommand {
    ImportGame {
        source: GameInputSource,
        review_side: RequestedReviewSide,
        elo_profile: RequestedEloProfile,
    },
    /// Removes one manually imported Game from the Player's own records.
    ///
    /// The address names one Game Import, and the delete spans the Game: every
    /// Game Import of the same Game reviewed from the same side goes, whatever
    /// Elo Profile each was reviewed at. A Game reviewed from the other side is
    /// a different review with different findings and is left standing.
    ///
    /// A Game Daily Coaching digested is not deletable here and is rejected
    /// with `DigestedGameImport`: a published Coaching Digest cites its
    /// supporting Games and is immutable, and the profile connector would
    /// import the Game again on the next tick.
    DeleteGameImport {
        game_import_id: GameImportId,
    },
    StartReviewSession {
        game_import_id: GameImportId,
    },
    OpenGameReview {
        game_import_id: GameImportId,
    },
    /// Reads one immutable Game Review snapshot by its address.
    ///
    /// The Game Import ID is the review address, so this read is the whole of
    /// rehydration: no Review Session, no revision, and nothing the caller has
    /// to negotiate. Distinct from `OpenGameReview`, which answers the model
    /// with a frozen review; this answers a surface with everything it needs to
    /// render one.
    ///
    /// `known_content_digest` is what a caller already holds. When it matches
    /// what this read would answer with, the completion is
    /// `GameReviewSnapshotUnchanged` and the payload is not sent again — the
    /// review is roughly a megabyte, so the saving is transfer and client
    /// parsing, not engine work: the Game Import is still read to compute the
    /// digest. Omitting it always fetches.
    ReadGameReviewSnapshot {
        game_import_id: GameImportId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        known_content_digest: Option<ReviewContentDigest>,
    },
    /// Reads one Review Moment's grounded detail by its address.
    ///
    /// A surface rendering a single moment reads this instead of the whole
    /// review, and a host model reads it to speak about the moment without
    /// inventing anything. Distinct from `OpenReviewMoment`, which admits a
    /// moment into a Review Session; this touches no state at all.
    ///
    /// `known_content_digest` works as it does on the snapshot read, and
    /// matters more here: this is the read that carries a Review Moment
    /// Comment, and prose is the one part of a review that a later build can
    /// rewrite.
    ReadReviewMomentDetail {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        known_content_digest: Option<ReviewContentDigest>,
    },
    /// Opens one Review Moment of an addressed Game Review, however it is named.
    ///
    /// The Game Import ID is the whole address, so this answers the same way on
    /// first paint and on a conversation reopened a year later. Distinct from
    /// `ReadReviewMomentDetail`, which is the immutable resource behind one
    /// exact Review Moment ID: this one resolves a caller's *reference* first,
    /// which is what lets a bare ply the review never flagged open at all, and
    /// what makes "the next Critical Moment" an index step rather than a search
    /// the caller has to run. Distinct from `OpenReviewMoment`, which admits a
    /// moment into a Review Session; this touches no state.
    OpenAddressedReviewMoment {
        game_import_id: GameImportId,
        reference: ReviewMomentReference,
    },
    /// Reads the whole proof aggregate behind one Review Moment, for audit.
    ///
    /// This is the one address where a Decision Explanation is delivered whole.
    /// It exists so removing the proof from every other payload does not cost
    /// reproducibility, and it is never on a rendering path.
    ReadReviewMomentExplanation {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
    },
    OpenGameReviewByIdentity {
        source: GameInputSource,
        review_side: ReviewSide,
        elo_rating: EloRating,
    },
    OpenReviewMoment {
        game_import_id: GameImportId,
        selection: ReviewMomentSelection,
        idempotency_key: IdempotencyKey,
    },
    InspectPosition {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        target: PositionInspectionTarget,
    },
    EvaluatePlayerPlan {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        request: PlayerPlanEvaluationRequest,
    },
    ExploreAlternativeMove {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        parent: BranchParent,
        source_position_ref: PositionRef,
        move_input: MoveInput,
        idempotency_key: IdempotencyKey,
    },
    StartCoachTurn {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        coach_turn_id: CoachTurnId,
        context: Box<CoachTurnContext>,
        message: String,
        idempotency_key: IdempotencyKey,
        prior_turn: PriorCoachTurn,
    },
    /// One Player message on the web Review Session, answered by the pinned
    /// Language Layer as one HostTurn. Screen context is read from the session
    /// actor. At most four prior prose pairs may be resent.
    // Runtime dispatch lands in #435.
    StartHostTurn {
        game_import_id: GameImportId,
        message: String,
        prior_turns: Vec<HostTurnPriorTurn>,
        idempotency_key: IdempotencyKey,
    },
    PublishCoachTurn {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        coach_turn_id: CoachTurnId,
        assessment: Box<AlternativeMoveAssessment>,
        idempotency_key: IdempotencyKey,
    },
    PublishReviewMomentComment {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        text: String,
        grounding_ledger: CriticalMomentGroundingLedger,
        idempotency_key: IdempotencyKey,
    },
    RecordLearningPathExposure {
        game_import_id: GameImportId,
        learning_path_ref: LearningPathRef,
    },
    UpdateLearningPathVote {
        game_import_id: GameImportId,
        learning_path_ref: LearningPathRef,
        #[serde(deserialize_with = "deserialize_required_nullable")]
        #[schemars(required, schema_with = "nullable_learning_path_vote_schema")]
        vote: Option<LearningPathVote>,
    },
    CancelOperation {
        game_import_id: GameImportId,
        operation_id: OperationId,
        idempotency_key: IdempotencyKey,
    },
}

fn nullable_learning_path_vote_schema(generator: &mut SchemaGenerator) -> Schema {
    Option::<LearningPathVote>::json_schema(generator)
}

fn deserialize_required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum DeliverySurface {
    Web,
    CoachSkill,
    CoachApp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum GameInputSource {
    LichessUrl { url: String },
    ChessComUrl { url: String },
    PastedPgn { pgn: String },
    LocalPgnFile { path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RequestedReviewSide {
    Selected { review_side: ReviewSide },
    FromQualifiedUrl,
    Required,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RequestedEloProfile {
    PlayerProvided { rating: EloRating },
    FromImportedMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MoveInput {
    Uci { uci: String },
    San { san: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PriorCoachTurn {
    None,
    Steers { coach_turn_id: CoachTurnId },
    RetriesUnavailable { coach_turn_id: CoachTurnId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionEventEnvelope {
    pub request_id: RequestId,
    pub operation_id: OperationId,
    pub sequence: u32,
    pub event: ReviewSessionEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReviewSessionEvent {
    Accepted {
        operation: OperationKind,
        limits: ReviewSessionLimits,
    },
    Progress {
        stage: OperationProgress,
    },
    Completed {
        result: Box<OperationCompletion>,
    },
    Unavailable {
        operation: OperationKind,
        reason: ProviderUnavailableReason,
        retry: RetryDirective,
    },
    ReviewMomentUnavailable {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        reason: ProviderUnavailableReason,
        retry: RetryDirective,
    },
    Cancelled {
        operation: OperationKind,
    },
    Conflict {
        operation: OperationKind,
        reason: OperationConflictReason,
    },
    Rejected {
        operation: OperationKind,
        reason: CommandRejectionReason,
        recovery: RejectionRecovery,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    CommandAdmission,
    GameImport,
    GameImportDeletion,
    GameReviewOpen,
    ReviewSessionStart,
    ReviewMomentOpen,
    PositionInspection,
    PlayerPlanEvaluation,
    AlternativeMoveEvaluation,
    CoachTurn,
    ReviewMomentCommentPublication,
    LearningPathFeedback,
    Cancellation,
    HostTurn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OperationProgress {
    Import {
        stage: ImportProgressStage,
    },
    ReviewSession {
        stage: ReviewSessionProgressStage,
    },
    ReviewMomentPreparation {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        stage: ReviewMomentPreparationProgressStage,
    },
    AlternativeMove {
        stage: AlternativeMoveProgressStage,
    },
    AlternativeMoveAllowance {
        remaining: u8,
    },
    CoachTurn {
        stage: CoachTurnProgressStage,
    },
    HostTurn {
        label: HostTurnStepLabel,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ImportProgressStage {
    ValidatingSource,
    WaitingForLichess,
    WaitingForChessCom,
    FetchingGame,
    ValidatingGame,
    RunningGameReview,
    BuildingSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSessionProgressStage {
    ResolvingMoment,
    BuildingPosition,
    PreparingEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewMomentPreparationProgressStage {
    WaitingForCapacity,
    PreparingAuthoringContext,
    CommittingAuthoringContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum AlternativeMoveProgressStage {
    ValidatingMove,
    WaitingForStockfish,
    EvaluatingMove,
    CommittingMove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum CoachTurnProgressStage {
    Queued,
    InspectingPosition,
    ProjectingIntent,
    AnalyzingRefutation,
    GeneratingResponse,
    RepairingResponse,
    ValidatingResponse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OperationCompletion {
    GameImported {
        game_import_id: GameImportId,
        review: Box<GameReview>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timing: Option<GameImportTiming>,
        #[serde(skip)]
        #[schemars(skip)]
        #[ts(skip)]
        imported_game: Option<Box<ImportedGame>>,
    },
    GameReviewOpened {
        game_import_id: GameImportId,
        review: Box<GameReview>,
    },
    /// The Game is gone, and `deleted_import_count` says how many reviews went
    /// with it — one Game re-imported at three Elo Profiles is three, so the
    /// count is never zero.
    ///
    /// A Game that is already gone is an address this Player does not own, and
    /// is answered with `UnknownGameImport` rather than with this.
    GameImportDeleted {
        game_import_id: GameImportId,
        deleted_import_count: u16,
    },
    /// Everything a surface needs to render one Game Review, and nothing else.
    ///
    /// `review_moments` is ordered by ply, so "the next Critical Moment" is an
    /// index step rather than a search. The moments carry no authoring core:
    /// the snapshot is a rendering input, and preparing a moment for coaching
    /// is a separate, later act.
    GameReviewSnapshotRead {
        game_import_id: GameImportId,
        review: Box<GameReview>,
        imported_game: Box<ImportedGame>,
        review_moments: Vec<ReviewSessionMoment>,
        content_digest: ReviewContentDigest,
    },
    /// What the caller already holds is still what this read would answer with.
    ///
    /// Carries the digest back so a cache can record that it revalidated, and
    /// deliberately carries nothing else: answering with any part of the review
    /// would defeat the only reason to ask this way.
    GameReviewSnapshotUnchanged {
        game_import_id: GameImportId,
        content_digest: ReviewContentDigest,
    },
    /// One Review Moment resolved for reading, and nothing that contains it.
    ReviewMomentDetailRead {
        detail: Box<GroundedReviewMomentDetail>,
        content_digest: ReviewContentDigest,
    },
    /// The Review Moment a caller holds is still the one this read would answer
    /// with, comment included.
    ReviewMomentDetailUnchanged {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        content_digest: ReviewContentDigest,
    },
    /// The Review Moment a caller's reference resolved to, grounded for speech.
    ///
    /// Carries the same detail as `ReviewMomentDetailRead` and is a separate
    /// kind on purpose: that one answers a resource read, whose model-visible
    /// payload collapses to the address it was asked for, while this one
    /// answers a tool the host model called in order to say something about the
    /// moment. Delivering the resolved proof projection is right for exactly
    /// one of them, and a shared kind would make the two indistinguishable at
    /// the boundary that decides.
    AddressedReviewMomentOpened {
        detail: Box<GroundedReviewMomentDetail>,
    },
    /// The audit-only proof aggregate, delivered whole at its own address.
    ///
    /// Every other completion has its proof dropped on the way out. This one is
    /// the proof, so dropping it would leave the audit path with nothing to
    /// read.
    ReviewMomentExplanationRead {
        game_import_id: GameImportId,
        review_moment_id: CriticalMomentId,
        explanation: Box<DecisionExplanation>,
    },
    ReviewSessionStarted {
        game_import_id: GameImportId,
        session_revision: u64,
        review: Box<GameReview>,
        imported_game: Box<ImportedGame>,
        review_moments: Vec<ReviewSessionMoment>,
    },
    /// Opening a Review Moment ships that moment and nothing else.
    ///
    /// The Game Review, the imported Game, and the admitted Review Moment list
    /// are established when the Review Session starts and never change while it
    /// runs, so re-sending them once per open costs the Player the same bytes
    /// on every widget. `review_moment` carries the moment's deterministic
    /// core, `critical_moment` its Game Review entry, and `revision_delta`
    /// names what a surface holding the session must refresh.
    ///
    /// `game_import_id` is the address of the Game Review this moment belongs
    /// to. A surface reopened by a host has only the replayed arguments and
    /// this result to work from, and the address is what it reads its immutable
    /// snapshot with, so the open has to say which review it opened into.
    ReviewMomentOpened {
        game_import_id: GameImportId,
        session_revision: u64,
        revision_delta: ReviewSessionRevisionDelta,
        review_moment: Box<ReviewSessionCoreContract>,
        critical_moment: Box<GameReviewCriticalMoment>,
        decision_explanation_ref: Option<DecisionExplanationRef>,
        comment: Option<Box<CriticalMomentComment>>,
        comment_published: bool,
        authoring_context: Option<Box<ReviewMomentCommentAuthoringContext>>,
    },
    PositionInspected {
        inspection: Box<PositionInspection>,
    },
    PlayerPlanEvaluationPrepared {
        context: Box<PlayerPlanEvaluationContext>,
    },
    PlayerPlanEvaluated {
        evaluation: PlayerPlanEvaluation,
    },
    AlternativeMoveEvaluated {
        alternative_move: Box<AlternativeMoveResult>,
    },
    CoachTurnCompleted {
        assessment: Box<AlternativeMoveAssessment>,
    },
    HostTurnCompleted {
        answer: String,
        /// Ply (half-move index) to open. Not a CriticalMomentId.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        focus_moment: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show_line: Option<HostTurnShowLine>,
    },
    HostTurnRefused {
        reason: HostTurnRefusalReason,
    },
    CoachTurnPrepared {
        facts: Box<CoachTurnFacts>,
    },
    ReviewMomentCommentPublished {
        comment: Box<CriticalMomentComment>,
    },
    LearningPathFeedbackRecorded {
        feedback: LearningPathFeedbackState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionRevisionDelta {
    pub prior_revision: u64,
    pub resulting_revision: u64,
    pub changed_moment_ids: Vec<CriticalMomentId>,
    pub full_refresh_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameImportTiming {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_startup_milliseconds: Option<u64>,
    pub total_pipeline_milliseconds: u64,
    pub engine_analysis: ProviderTimingSummary,
    pub human_move_model: ProviderTimingSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderTimingSummary {
    pub provider: String,
    pub call_count: u32,
    pub total_milliseconds: u64,
    pub median_milliseconds: u64,
    pub maximum_milliseconds: u64,
}

impl ProviderTimingSummary {
    pub fn from_durations(provider: &str, samples: &[Duration]) -> Self {
        if samples.is_empty() {
            return Self {
                provider: provider.to_string(),
                call_count: 0,
                total_milliseconds: 0,
                median_milliseconds: 0,
                maximum_milliseconds: 0,
            };
        }
        let mut ordered = samples.to_vec();
        ordered.sort_unstable();
        let upper_middle = ordered[ordered.len() / 2];
        let median = if ordered.len().is_multiple_of(2) {
            let lower_middle = ordered[ordered.len() / 2 - 1];
            lower_middle + (upper_middle - lower_middle) / 2
        } else {
            upper_middle
        };
        Self {
            provider: provider.to_string(),
            call_count: u32::try_from(ordered.len()).unwrap_or(u32::MAX),
            total_milliseconds: duration_milliseconds(ordered.iter().copied().sum()),
            median_milliseconds: duration_milliseconds(median),
            maximum_milliseconds: duration_milliseconds(
                *ordered
                    .last()
                    .expect("provider timing samples are nonempty"),
            ),
        }
    }
}

pub fn duration_milliseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod timing_tests {
    use std::time::Duration;

    use super::ProviderTimingSummary;

    #[test]
    fn provider_timing_summary_averages_the_middle_pair() {
        let summary = ProviderTimingSummary::from_durations(
            "test provider",
            &[
                Duration::from_millis(10),
                Duration::from_millis(30),
                Duration::from_millis(50),
                Duration::from_millis(90),
            ],
        );

        assert_eq!(summary.provider, "test provider");
        assert_eq!(summary.call_count, 4);
        assert_eq!(summary.total_milliseconds, 180);
        assert_eq!(summary.median_milliseconds, 40);
        assert_eq!(summary.maximum_milliseconds, 90);
    }
}

#[cfg(test)]
mod command_boundary_tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PositionInspectionTarget {
    ReviewedMove,
    AlternativeMove {
        alternative_move_id: AlternativeMoveId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PositionInspection {
    pub position_snapshot: PositionSnapshot,
    pub text_board: String,
    pub side_to_move: Color,
    pub evaluation: EngineEvaluation,
    pub context: CoachTurnContext,
    pub evidence_packet: ReviewSessionEvidencePacket,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoachTurnFacts {
    pub coach_turn_id: CoachTurnId,
    pub message: String,
    pub context: CoachTurnContext,
    pub alternative_move: AlternativeMoveResult,
    pub ancestor_branch: Vec<AlternativeMoveResult>,
    pub source_position: PositionSnapshot,
    pub evidence_packet: ReviewSessionEvidencePacket,
    pub evidence: AlternativeMoveAssessmentEvidenceRefs,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AlternativeMoveAssessmentEvidenceRefs {
    pub target_branch: EvidenceId,
    pub source_engine: EvidenceId,
    pub resulting_engine: EvidenceId,
    pub source_human: EvidenceId,
    pub resulting_human: EvidenceId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionLimits {
    pub max_committed_alternative_moves: u8,
    pub max_branch_depth_plies: u8,
    pub max_started_coach_turns: u8,
    pub max_player_message_bytes: u16,
    pub max_pgn_bytes: u32,
    pub max_active_coach_turns: u8,
    pub max_active_alternative_move_evaluations: u8,
    pub stockfish_slots: u8,
    pub max_position_inspections_per_turn: u8,
    pub max_host_turn_prior_turns: u8,
}

impl Default for ReviewSessionLimits {
    fn default() -> Self {
        Self::V1
    }
}

impl ReviewSessionLimits {
    pub const V1: Self = Self {
        max_committed_alternative_moves: 24,
        max_branch_depth_plies: 12,
        max_started_coach_turns: 12,
        max_player_message_bytes: 4096,
        max_pgn_bytes: 512 * 1024,
        max_active_coach_turns: 1,
        max_active_alternative_move_evaluations: 1,
        stockfish_slots: 2,
        max_position_inspections_per_turn: 2,
        max_host_turn_prior_turns: 4,
    };
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProviderUnavailableReason {
    LichessTransport,
    ChessComTransport,
    StockfishProcess,
    MaiaTransport,
    LanguageLayer,
    Persistence,
    /// Coach Engine could not be reached at all. Only the Central Host can
    /// observe this: it means the command never arrived, so no Review Session
    /// state changed and an idempotent command stays safe to repeat.
    CoachEngineTransport,
    Timeout {
        provider: ProviderKind,
    },
    RateLimited {
        retry_after_seconds: u32,
    },
    QueueDeadline,
    AdmissionLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Lichess,
    ChessCom,
    Stockfish,
    Maia,
    LanguageLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RetryDirective {
    RetryAllowed,
    RetryAfter { seconds: u32 },
    StartNewOperation,
    NotRetryable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum OperationConflictReason {
    CoachTurnAlreadyActive,
    AlternativeMoveEvaluationAlreadyActive,
    /// One logical write was replayed under a different idempotency key than the
    /// one that owns it.
    ///
    /// This is a mismatch, not staleness: an immutable snapshot has nothing to go
    /// stale against, so the caller re-sends the original key rather than
    /// re-synchronising against server state.
    IdempotencyKeyMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum CommandRejectionReason {
    MalformedInput,
    UnknownCommand,
    InvalidCommand,
    AuthenticationRequired,
    PlayerMismatch,
    UnknownGameImport,
    /// Daily Coaching produced this Game, so it is not the Player's to remove
    /// one at a time: a published Coaching Digest cites its supporting Games,
    /// and the connected playing profile would import it again.
    DigestedGameImport,
    InvalidLichessUrl,
    InvalidChessComUrl,
    ReviewSideRequired,
    EloProfileRequired,
    InvalidPgn,
    GameNotFound,
    PrivateGame,
    OngoingGame,
    AbortedGame,
    UnsupportedVariant,
    MalformedProviderResponse,
    ResponseTooLarge,
    IllegalMove,
    AmbiguousMove,
    TerminalPosition,
    /// The in-memory Review Session for this Player and Game Import is gone —
    /// evicted, or lost with the process. Only transient state is lost: the
    /// review is durable at its Game Import ID and the next command rebuilds
    /// the session over it. There is no expiry reason beside this one, because
    /// no session expiry is Player-visible.
    UnknownSession,
    UnknownMoment,
    UnknownTarget,
    MissingEvidence,
    InvalidEvidenceReceipt,
    AlternativeMoveLimit,
    BranchDepthLimit,
    CoachTurnLimit,
    MessageTooLong,
}

impl ReviewSessionCommand {
    pub fn operation(&self) -> OperationKind {
        match self {
            Self::ImportGame { .. } => OperationKind::GameImport,
            Self::DeleteGameImport { .. } => OperationKind::GameImportDeletion,
            Self::OpenGameReview { .. }
            | Self::OpenGameReviewByIdentity { .. }
            | Self::ReadGameReviewSnapshot { .. } => OperationKind::GameReviewOpen,
            Self::StartReviewSession { .. } => OperationKind::ReviewSessionStart,
            Self::OpenReviewMoment { .. }
            | Self::OpenAddressedReviewMoment { .. }
            | Self::ReadReviewMomentDetail { .. }
            | Self::ReadReviewMomentExplanation { .. } => OperationKind::ReviewMomentOpen,
            Self::InspectPosition { .. } => OperationKind::PositionInspection,
            Self::EvaluatePlayerPlan { .. } => OperationKind::PlayerPlanEvaluation,
            Self::ExploreAlternativeMove { .. } => OperationKind::AlternativeMoveEvaluation,
            Self::StartCoachTurn { .. } | Self::PublishCoachTurn { .. } => OperationKind::CoachTurn,
            Self::StartHostTurn { .. } => OperationKind::HostTurn,
            Self::PublishReviewMomentComment { .. } => {
                OperationKind::ReviewMomentCommentPublication
            }
            Self::RecordLearningPathExposure { .. } | Self::UpdateLearningPathVote { .. } => {
                OperationKind::LearningPathFeedback
            }
            Self::CancelOperation { .. } => OperationKind::Cancellation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RejectionRecovery {
    CorrectInput,
    SelectReviewSide,
    ProvideEloProfile,
    ChooseLegalMove { matching_moves: Vec<String> },
    RetryAfter { seconds: u32 },
    StartNewReviewSession,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AlternativeMoveResult {
    pub alternative_move_id: AlternativeMoveId,
    pub branch_ref: BranchRef,
    pub parent: BranchParent,
    pub source_position_ref: PositionRef,
    pub move_uci: String,
    pub resulting_position: PositionSnapshot,
    pub evaluation: AlternativeMoveEvaluation,
    pub strongest_reply: StrongestReply,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AlternativeMoveEvaluation {
    pub selected_move: EngineEvaluation,
    pub best_move_uci: String,
    pub best_move: EngineEvaluation,
    pub comparison: EvaluationLoss,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EvaluationLoss {
    Centipawns {
        value: u32,
    },
    Mate {
        best: MateComparison,
        selected: MateComparison,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MateComparison {
    NotForced,
    Forced {
        outcome: MateOutcome,
        distance_plies: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StrongestReply {
    Offered { uci: String },
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AlternativeMoveAssessment {
    pub coach_turn_id: CoachTurnId,
    pub alternative_move_id: AlternativeMoveId,
    pub objective_quality: AssessmentDimension,
    pub findability: AssessmentDimension,
    pub resilience: AssessmentDimension,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssessmentDimension {
    pub explanation: String,
    pub evidence_refs: Vec<EvidenceId>,
}
