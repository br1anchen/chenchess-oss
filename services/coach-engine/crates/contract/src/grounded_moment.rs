//! What a surface and a host model need to speak about one Review Moment.
//!
//! The proof aggregate behind a Review Moment is audit-only and dominates a
//! review's bytes, and almost all of it is content hashes that resolve to
//! nothing outside the Coach Engine. So a Review Moment is delivered twice, at
//! two different addresses: resolved here for reading and reasoning, and whole
//! at the audit address for reproduction.
//!
//! Every reference this module *chose* to follow is followed: concepts, moves,
//! positions, candidates, and the facts a path rests on. What remains are the
//! refs nested inside `AtomicChessFactData` itself — a fact names the position
//! or line step it was observed at, and those names stay content hashes. That
//! is deliberate: the fact's own payload is what a reader reasons from, and
//! resolving the identifiers it carries would pull the position graph back onto
//! the wire, which is the cost this projection exists to avoid. A reader treats
//! a nested ref as opaque provenance and reaches for the audit address if it
//! ever needs to follow one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    AtomicChessFactData, Color, CriticalMomentComment, CriticalMomentId, CurriculumLearningConcept,
    DecisionCandidateOrigin, DecisionExplanationRef, DecisionLearningOutcome,
    EngineAssessmentScore, ExplanationPathAttribution, ExplanationPathRef, GameImportId,
    GameReviewObjectiveLines, MaterialValuePolicyVersion, PieceAtSquare, PieceRole, PositionGoal,
    ProofCapability, ReviewSide, SemanticOutcomeData, Square,
};

/// One Review Moment's grounded detail, addressed by its Game Import and its ID.
///
/// The Game Review that contains the moment is a separate read, so nothing here
/// restates the review, the imported Game, or the other moments. What is left is
/// the two things a moment read alone can answer: the continuations this moment
/// offers, the resolved proof behind what the coach may say about it, and the
/// published Review Moment Comment when the Review Annotation Store has one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroundedReviewMomentDetail {
    pub game_import_id: GameImportId,
    pub review_moment_id: CriticalMomentId,
    pub ply: u16,
    pub continuation: MoveSequenceOrigin,
    /// The frozen engine best line and played-move refutation, already SAN
    /// rendered. Both continuations come from here rather than from candidate
    /// evidence, so a rendered sequence and a coached one are the same line.
    pub objective_lines: Option<GameReviewObjectiveLines>,
    /// Addresses the audit-only proof aggregate without carrying it.
    pub explanation_ref: Option<DecisionExplanationRef>,
    pub decision_learning_outcome: DecisionLearningOutcome,
    pub explanation: Option<GroundedExplanation>,
    /// The published Review Moment Comment for this Player and Review Moment.
    ///
    /// Absence means none has been published yet, not that authoring failed.
    /// The comment is the only canonical place a Coach Intent Hypothesis may
    /// appear (ADR 0026).
    pub comment: Option<CriticalMomentComment>,
}

/// Where a Review Moment's continuations start and which way they are read.
///
/// The engine line continues from the position the Player faced; the refutation
/// continues from the position their move produced. `reviewed_move_uci` is what
/// separates the two, so a surface can place both without re-deriving the Game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveSequenceOrigin {
    pub fen: String,
    pub side_to_move: Color,
    pub review_side: ReviewSide,
    pub reviewed_move_uci: Option<String>,
}

/// The proof behind one Review Moment, resolved into names and moves.
///
/// `capability` governs what a host model may claim — whether it may say a
/// candidate is better than the played move or only that a concept is soundly
/// present — so it stays on the wire even though nothing renders it.
///
/// Every field below is resolved or the whole aggregate is withheld. A proof
/// delivered with its concept or its moves missing still carries a capability
/// that licenses a claim, and a reader would have nothing to make the claim
/// out of — which is exactly the shape a model invents into.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroundedExplanation {
    pub explanation_ref: DecisionExplanationRef,
    pub capability: ProofCapability,
    pub paths: Vec<GroundedExplanationPath>,
    pub candidates: Vec<GroundedCandidate>,
}

/// One selected Explanation Path with every reference resolved.
///
/// A path that cannot be resolved whole is not delivered at all. Optional
/// fields mean the durable proof did not establish that kind of evidence, not
/// that grounding lost it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroundedExplanationPath {
    pub path_ref: ExplanationPathRef,
    pub attribution: ExplanationPathAttribution,
    /// The activated concept under its curriculum name.
    pub concept: CurriculumLearningConcept,
    pub candidate: GroundedCandidate,
    /// The proof-backed goal that generated this candidate, when one exists.
    /// It is candidate-discovery evidence, never a claim about Engine intent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_goal: Option<PositionGoal>,
    /// Every material exchange in the selected candidate's complete retained
    /// line, including events beyond the short variation a coach recites.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_transaction: Option<GroundedMaterialTransaction>,
    /// The move that creates the threat.
    pub causal_step: GroundedStep,
    /// The move that collects on it.
    pub payoff_step: GroundedStep,
    pub supporting_facts: Vec<AtomicChessFactData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroundedMaterialTransaction {
    pub perspective: Color,
    pub events: Vec<GroundedMaterialEvent>,
    pub net_conventional_value_delta: i16,
    pub value_policy_version: MaterialValuePolicyVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum GroundedMaterialEvent {
    Capture {
        line_ply: u16,
        uci: String,
        san: String,
        mover: Color,
        captured: PieceAtSquare,
        conventional_value_delta: i16,
    },
    Promotion {
        line_ply: u16,
        uci: String,
        san: String,
        mover: Color,
        pawn_from_square: Square,
        promotion_role: PieceRole,
        conventional_value_delta: i16,
    },
    CaptureAndPromotion {
        line_ply: u16,
        uci: String,
        san: String,
        mover: Color,
        captured: PieceAtSquare,
        pawn_from_square: Square,
        promotion_role: PieceRole,
        conventional_value_delta: i16,
    },
}

impl GroundedMaterialEvent {
    /// What this one event moved, signed from the transaction's perspective.
    pub fn conventional_value_delta(&self) -> i16 {
        match self {
            Self::Capture {
                conventional_value_delta,
                ..
            }
            | Self::Promotion {
                conventional_value_delta,
                ..
            }
            | Self::CaptureAndPromotion {
                conventional_value_delta,
                ..
            } => *conventional_value_delta,
        }
    }
}

/// One Decision Candidate as a reader can speak about it.
///
/// `line_steps` is deliberately absent: ordinary per-ply machine detail is
/// replaced by the short spoken variation, while the path-local material
/// transaction preserves every later exchange relevant to coaching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroundedCandidate {
    pub root_move_uci: String,
    pub san: String,
    pub origins: Vec<DecisionCandidateOrigin>,
    pub evaluation: EngineAssessmentScore,
    pub outcomes: Vec<SemanticOutcomeData>,
    /// The candidate's line in SAN, truncated to the plies a coach would
    /// actually recite. SAN rather than UCI because prose quotes it verbatim.
    pub retained_variation: Vec<String>,
}

/// One move inside a proof, named the way a coach would name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroundedStep {
    pub uci: String,
    pub san: String,
    /// The position the step produces.
    pub fen: String,
}
