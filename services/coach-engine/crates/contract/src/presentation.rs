use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    Color, CriticalMomentId, DecisionExplanationRef, DecisionLearningOutcome, EloRating,
    EngineEvaluation, GameImportId, GameReviewEvaluationDisplay, GroundedExplanation,
    IdempotencyKey, MoveSequenceRef, OpeningMetadata, Piece, PositionRef,
    ReviewMomentLearningMaterial, ReviewMomentSelection, ReviewSide, Square,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentation {
    pub version: ReviewSessionPresentationVersion,
    pub game_import_id: GameImportId,
    pub session_revision: u64,
    pub presentation_revision: u64,
    pub source: ReviewSessionPresentationSource,
    pub opening: OpeningMetadata,
    pub review_side: ReviewSide,
    pub elo_rating: EloRating,
    pub orientation: Color,
    pub selected_moment_id: Option<CriticalMomentId>,
    pub max_ply: u16,
    pub evaluation_timeline: Vec<ReviewSessionPresentationEvaluationPoint>,
    pub moments: Vec<ReviewSessionPresentationMoment>,
    pub handoff_state: ReviewSessionPresentationHandoffState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<ReviewSessionPresentationAnimation>,
}

/// Everything one Game Review renders as, addressed by its Game Import ID.
///
/// A surface holding this needs nothing else and negotiates nothing: no Review
/// Session, no revision to reconcile, no recovery tier. Reading it is the whole
/// of rehydration, so first paint, a page refresh, and a year-old conversation
/// all render from the same bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewSnapshot {
    pub version: GameReviewSnapshotVersion,
    pub game_import_id: GameImportId,
    pub source: ReviewSessionPresentationSource,
    pub opening: OpeningMetadata,
    pub review_side: ReviewSide,
    pub elo_rating: EloRating,
    pub orientation: Color,
    pub max_ply: u16,
    pub evaluation_timeline: Vec<ReviewSessionPresentationEvaluationPoint>,
    /// Ordered by ply, so "the next Critical Moment" is an index step.
    pub moments: Vec<GameReviewSnapshotMoment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameReviewSnapshotVersion {
    V1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewSnapshotMoment {
    /// Position in the ordered set, so a surface can step without searching.
    pub index: u16,
    pub moment_id: CriticalMomentId,
    pub ply: u16,
    pub move_label: String,
    pub kind: ReviewSessionPresentationMomentKind,
    pub tone: ReviewSessionPresentationMomentTone,
    pub glyph: String,
    pub title: String,
    pub summary: String,
    pub selection: ReviewMomentSelection,
    pub decision_learning_outcome: DecisionLearningOutcome,
    pub learning_material: ReviewMomentLearningMaterial,
    pub board: ReviewSessionPresentationBoard,
    pub arrows: Vec<ReviewSessionPresentationArrow>,
    pub played_evaluation: GameReviewEvaluationDisplay,
    pub best_evaluation: GameReviewEvaluationDisplay,
    /// Which continuations this moment offers, named and nothing more.
    ///
    /// The selector renders a moment's board, arrows, and evaluations; it never
    /// renders a line. Carrying each line's title, length, and SAN here would
    /// put every moment's moves in the payload of a card that shows none of
    /// them, so the moves stay where they are read: `ReviewMomentSnapshot` for
    /// the descriptors, `MoveSequenceSnapshot` for the line played out.
    pub sequence_kinds: Vec<MoveSequencePresentationKind>,
}

/// One canonical continuation a Review Moment offers.
///
/// The moment and the kind are the reference: a Review Moment offers at most
/// one line of each kind, so nothing has to be minted, handed out, or kept
/// alive for a surface to name the line it wants played out. The moves
/// themselves are a separate read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewSnapshotSequence {
    pub kind: MoveSequencePresentationKind,
    pub title: String,
    pub move_count: u16,
    pub san: Vec<String>,
}

/// One Review Moment's detail, addressed under the review that contains it.
///
/// A surface holding the snapshot already has how this moment looks; what it
/// reads here is what the moment can say — the continuations it offers played
/// out, and the resolved proof a host model may speak from. The proof aggregate
/// itself is a third address, never this one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewMomentSnapshot {
    pub version: GameReviewSnapshotVersion,
    pub game_import_id: GameImportId,
    pub review_moment_id: CriticalMomentId,
    pub ply: u16,
    pub orientation: Color,
    pub sequences: Vec<GameReviewSnapshotSequence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation_ref: Option<DecisionExplanationRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<GroundedExplanation>,
}

/// One canonical continuation played out ply by ply, addressed by its kind.
///
/// Nothing is minted and nothing expires: a Review Moment offers at most one
/// line of each kind, so the kind is the reference and the same address answers
/// with the same moves forever.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveSequenceSnapshot {
    pub version: GameReviewSnapshotVersion,
    pub game_import_id: GameImportId,
    pub review_moment_id: CriticalMomentId,
    pub kind: MoveSequencePresentationKind,
    pub title: String,
    pub orientation: Color,
    pub moves: Vec<MoveSequencePresentationMove>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationAddition {
    pub version: ReviewSessionPresentationVersion,
    pub game_import_id: GameImportId,
    pub prior_revision: u64,
    pub resulting_revision: u64,
    pub changed_moment_ids: Vec<CriticalMomentId>,
    pub changed_fields: Vec<ReviewSessionPresentationChangedField>,
    pub full_refresh_required: bool,
    pub moment: ReviewSessionPresentationMoment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<ReviewSessionPresentationAnimation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSessionPresentationChangedField {
    Animation,
    Moment,
    SelectedMomentId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSessionPresentationVersion {
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSessionPresentationSource {
    Lichess,
    ChessCom,
    Pgn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationEvaluationPoint {
    pub ply: u16,
    pub evaluation: EngineEvaluation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationMoment {
    pub moment_id: CriticalMomentId,
    pub ply: u16,
    pub move_label: String,
    pub kind: ReviewSessionPresentationMomentKind,
    pub authoring_readiness: ReviewSessionPresentationAuthoringReadiness,
    pub tone: ReviewSessionPresentationMomentTone,
    pub glyph: String,
    pub title: String,
    pub summary: String,
    pub decision_learning_outcome: DecisionLearningOutcome,
    pub learning_material: ReviewMomentLearningMaterial,
    pub board: ReviewSessionPresentationBoard,
    pub arrows: Vec<ReviewSessionPresentationArrow>,
    pub played_evaluation: GameReviewEvaluationDisplay,
    pub best_evaluation: GameReviewEvaluationDisplay,
    pub handoff: ReviewSessionPresentationHandoffTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSessionPresentationAuthoringReadiness {
    Pending,
    Prepared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSessionPresentationMomentKind {
    Automatic,
    PlayerSelected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSessionPresentationMomentTone {
    Improvement,
    Positive,
    Selected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationBoard {
    pub position_ref: PositionRef,
    pub pieces: Vec<ReviewSessionPresentationPiece>,
    pub last_move: Option<ReviewSessionPresentationMove>,
    pub check_square: Option<Square>,
    pub announcement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationPiece {
    pub piece_id: String,
    pub square: Square,
    pub piece: Piece,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationMove {
    pub from: Square,
    pub to: Square,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationArrow {
    pub kind: ReviewSessionPresentationArrowKind,
    pub from: Square,
    pub to: Square,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSessionPresentationArrowKind {
    EngineBest,
    Maia,
    BestReply,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationHandoffTarget {
    pub game_import_id: GameImportId,
    pub moment_id: CriticalMomentId,
    pub ply: u16,
    pub selection: ReviewMomentSelection,
    pub idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReviewSessionPresentationHandoffState {
    Ready,
    Busy,
    Disabled,
    PassedToChat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationAnimation {
    pub version: ReviewSessionPresentationVersion,
    pub line_id: String,
    pub review_moment_id: CriticalMomentId,
    pub frames: Vec<ReviewSessionPresentationAnimationFrame>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationAnimationFrame {
    pub index: u16,
    pub move_label: String,
    pub duration_ms: u16,
    pub motions: Vec<ReviewSessionPresentationPieceMotion>,
    pub removed_piece_ids: Vec<String>,
    pub placements: Vec<ReviewSessionPresentationPiece>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewSessionPresentationPieceMotion {
    pub piece_id: String,
    pub from: Square,
    pub to: Square,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveSequencePresentation {
    pub version: MoveSequencePresentationVersion,
    pub sequence_ref: MoveSequenceRef,
    pub game_import_id: GameImportId,
    pub review_moment_id: CriticalMomentId,
    pub kind: MoveSequencePresentationKind,
    pub title: String,
    pub orientation: Color,
    pub moves: Vec<MoveSequencePresentationMove>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum MoveSequencePresentationVersion {
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum MoveSequencePresentationKind {
    EngineBest,
    PlayedMoveRefutation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveSequencePresentationMove {
    pub index: u16,
    pub san: String,
    pub board: MoveSequencePresentationBoard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveSequencePresentationBoard {
    pub pieces: Vec<ReviewSessionPresentationPiece>,
    pub last_move: ReviewSessionPresentationMove,
    pub check_square: Option<Square>,
    pub announcement: String,
}
