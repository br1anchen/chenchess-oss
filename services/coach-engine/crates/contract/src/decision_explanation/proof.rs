use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    AtomicFactRef, DecisionCandidateRef, DecisionLineStep, EngineAssessmentRef, ExplanationPathRef,
    KnowledgeNodeRef, KnowledgeRuleRef, LineStepRef, PositionGoal, SemanticOutcome,
    SemanticOutcomeRef,
};
use crate::{ArtifactDigest, EngineEvaluation, LearningResource, LearningTrackKey};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CandidateEvidence {
    SinglePv {
        authoritative: EngineCandidateEvidence,
        player_move: PlayerMoveEvidence,
    },
    MultiPv {
        authoritative_single_pv: EngineCandidateEvidence,
        requested_count: u8,
        /// Ranks two and up only. Rank one **is** `authoritative_single_pv`, so
        /// it is absent here rather than restated — a MultiPV rank-one score is
        /// a second, noisier reading of a position the SinglePV search already
        /// scored, and the two disagree (ADR 0041).
        ranked_alternatives: Vec<RankedAlternativeEvidence>,
        player_move: PlayerMoveEvidence,
    },
}

/// An alternative root the MultiPV search ranked below the best move.
///
/// It carries no absolute evaluation. MultiPV's scores are only mutually
/// comparable **within** its own search, so an alternative states how far behind
/// the best move it fell there, and nothing about its own worth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankedAlternativeEvidence {
    pub rank: u8,
    pub root_move_uci: String,
    pub gap: CandidateGap,
    pub variation: Vec<String>,
    pub provenance: DecisionEngineProvenance,
}

/// How far an alternative fell short of the best move, measured inside one
/// MultiPV search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CandidateGap {
    /// Both roots scored in centipawns.
    Centipawns { behind_best: u32 },
    /// Both roots force mate for the mover; this one takes longer.
    SlowerMate { extra_plies: u16 },
    /// The best move forces mate and this alternative does not.
    MissesForcedMate,
    /// This alternative concedes a forced mate the best move avoids.
    ConcedesForcedMate,
    /// The two scores admit no ordered comparison.
    Incommensurable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineCandidateEvidence {
    pub rank: u8,
    pub root_move_uci: String,
    pub evaluation: EngineEvaluation,
    pub variation: Vec<String>,
    pub provenance: DecisionEngineProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerMoveEvidence {
    pub root_move_uci: String,
    pub evaluation: EngineEvaluation,
    pub retained_variation: Vec<String>,
    pub provenance: DecisionEngineProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionEngineProvenance {
    pub engine: String,
    pub binary_digest: ArtifactDigest,
    pub depth: u8,
    pub threads: u8,
    pub hash_mib: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionCandidate {
    pub candidate_ref: DecisionCandidateRef,
    pub root_move_uci: String,
    pub origins: Vec<DecisionCandidateOrigin>,
    pub retained_variation: Vec<String>,
    pub line_steps: Vec<DecisionLineStep>,
    pub fact_refs: Vec<AtomicFactRef>,
    pub outcomes: Vec<SemanticOutcome>,
    pub assessment: EngineAssessment,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum DecisionCandidateOrigin {
    AuthoritativeSinglePv,
    EngineRanked,
    PlayerPlayed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineAssessment {
    pub assessment_ref: EngineAssessmentRef,
    pub rank: Option<u8>,
    pub score: EngineAssessmentScore,
    pub provenance: DecisionEngineProvenance,
}

/// A Decision Candidate is scored either absolutely or relative to the best
/// move, never both, and never absolutely from a search that does not own the
/// absolute (ADR 0041).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EngineAssessmentScore {
    /// The authoritative SinglePV reading of the best move, or the SinglePV
    /// reading of the position after the Player's move.
    Absolute { evaluation: EngineEvaluation },
    /// A MultiPV alternative, stated only as its shortfall against the best move
    /// within that same search.
    BehindBest { gap: CandidateGap },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExplanationPath {
    pub path_ref: ExplanationPathRef,
    pub attribution: ExplanationPathAttribution,
    pub candidate_ref: DecisionCandidateRef,
    pub knowledge_activation: KnowledgeActivation,
    pub candidate_generation_proof: Option<CandidateGenerationProof>,
    pub concept_validation_proof: ConceptValidationProof,
    pub outcome_refs: Vec<SemanticOutcomeRef>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum ExplanationPathAttribution {
    MissedBest,
    ConcededRefutation,
    Reinforcement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeActivation {
    pub concept_node_ref: KnowledgeNodeRef,
    pub recognition_rule_ref: KnowledgeRuleRef,
    pub supporting_fact_refs: Vec<AtomicFactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateGenerationProof {
    pub supporting_fact_refs: Vec<AtomicFactRef>,
    pub concept_node_ref: KnowledgeNodeRef,
    pub position_goal: PositionGoal,
    pub suggested_candidate_ref: DecisionCandidateRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConceptValidationProof {
    pub candidate_ref: DecisionCandidateRef,
    pub causal_step_ref: LineStepRef,
    pub payoff_step_ref: LineStepRef,
    pub recognition_rule_ref: KnowledgeRuleRef,
    pub supporting_fact_refs: Vec<AtomicFactRef>,
    pub outcome_refs: Vec<SemanticOutcomeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreferenceProof {
    pub preferred_candidate_ref: DecisionCandidateRef,
    pub engine_comparisons: Vec<EngineComparison>,
    pub semantic_comparisons: Vec<SemanticComparison>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineComparison {
    pub preferred_candidate_ref: DecisionCandidateRef,
    pub alternative_candidate_ref: DecisionCandidateRef,
    pub preferred_assessment_ref: EngineAssessmentRef,
    pub alternative_assessment_ref: EngineAssessmentRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticComparison {
    pub preferred_outcome_ref: SemanticOutcomeRef,
    pub alternative_outcome_ref: SemanticOutcomeRef,
    pub relation: SemanticComparisonRelation,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum SemanticComparisonRelation {
    Dominates,
    Refutes,
    Tradeoff,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum ProofCapability {
    ValidationOnly,
    EnginePreference,
    SemanticPreference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionLearningTrackProjection {
    pub key: LearningTrackKey,
    pub explanation_path_ref: ExplanationPathRef,
    pub resources: Vec<LearningResource>,
}
