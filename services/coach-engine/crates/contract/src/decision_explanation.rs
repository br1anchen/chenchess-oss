mod facts;
mod identity;
mod proof;

pub use facts::*;
pub use identity::*;
pub use proof::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{CriticalMomentId, GameRef, PositionSnapshot};

pub const DECISION_EXPLANATION_GENERATION: DecisionExplanationGeneration =
    DecisionExplanationGeneration::V1;
pub const CHESS_KNOWLEDGE_GRAPH_VERSION: ChessKnowledgeGraphVersion =
    ChessKnowledgeGraphVersion::V1;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
pub enum DecisionExplanationGeneration {
    #[serde(rename = "decision-explanation/v1")]
    V1,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
pub enum ChessKnowledgeGraphVersion {
    #[serde(rename = "chess-knowledge/v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionExplanation {
    pub decision_explanation_ref: DecisionExplanationRef,
    pub generation: DecisionExplanationGeneration,
    pub knowledge_graph_version: ChessKnowledgeGraphVersion,
    pub game_ref: GameRef,
    pub critical_moment_id: CriticalMomentId,
    pub position_snapshot: PositionSnapshot,
    /// The complete normalized engine input is durable so the transient
    /// construction graph, including rejected matches, can be recomputed.
    pub candidate_evidence: CandidateEvidence,
    pub snapshots: Vec<DecisionPositionSnapshot>,
    pub facts: Vec<AtomicChessFact>,
    pub candidates: Vec<DecisionCandidate>,
    pub selected_paths: Vec<ExplanationPath>,
    pub preference: Option<PreferenceProof>,
    pub capability: ProofCapability,
}
