use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use super::{
    CriticalMomentId, DeliverySurface, ExplanationPathRef, GameRef, LearningPathRef,
    LearningResourceId, LearningResourceMappingId, OpeningMetadata, PositionPhase,
};

pub const LEARNING_PLAN_SELECTION_POLICY_VERSION: LearningPlanSelectionPolicyVersion =
    LearningPlanSelectionPolicyVersion::V1;
pub const LEARNING_RESOURCE_CATALOG_VERSION: LearningResourceCatalogVersion =
    LearningResourceCatalogVersion::V2026_08_03;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningPlan {
    pub selection_policy_version: LearningPlanSelectionPolicyVersion,
    pub resource_catalog_version: LearningResourceCatalogVersion,
    /// Recommendations in pedagogical order: evidence rank, `Refines`
    /// clustering, then hard `Prerequisite` constraints.
    pub tracks: Vec<LearningTrack>,
}

impl LearningPlan {
    pub fn empty() -> Self {
        Self {
            selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
            resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
            tracks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewMomentLearningMaterial {
    pub selection_policy_version: LearningPlanSelectionPolicyVersion,
    pub resource_catalog_version: LearningResourceCatalogVersion,
    #[schemars(length(max = 2))]
    pub tracks: Vec<LearningTrack>,
}

impl ReviewMomentLearningMaterial {
    pub fn empty() -> Self {
        Self {
            selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
            resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
            tracks: Vec::new(),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
pub enum LearningPlanSelectionPolicyVersion {
    #[serde(rename = "learning-plan-selection/v1")]
    V1,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
pub enum LearningResourceCatalogVersion {
    #[serde(rename = "learning-resources/2026-07-25")]
    V2026_07_25,
    #[serde(rename = "learning-resources/2026-08-03")]
    V2026_08_03,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningTrack {
    pub key: LearningTrackKey,
    #[schemars(length(min = 1))]
    pub support: Vec<LearningTrackSupport>,
    #[schemars(length(min = 1))]
    pub resources: Vec<LearningResource>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LearningTrackKey {
    Curriculum {
        concept: CurriculumLearningConcept,
    },
    Opening {
        resource_mapping_id: LearningResourceMappingId,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum CurriculumLearningConcept {
    PieceCheckmates,
    CheckmatePatterns,
    KnightAndBishopMate,
    Pin,
    Skewer,
    Fork,
    HangingPiece,
    DiscoveredAttack,
    DoubleCheck,
    OverloadedPiece,
    Intermezzo,
    XRayAttack,
    Zugzwang,
    Interference,
    GreekGift,
    Deflection,
    Attraction,
    Underpromotion,
    Desperado,
    CounterCheck,
    CapturingDefender,
    Clearance,
    KeySquares,
    Opposition,
    SeventhRankRookPawn,
    PassiveRookDefense,
    Lucena,
    Philidor,
    IntermediateRookEndings,
    PracticalRookEndings,
    AdvancedPawn,
    AttackingF2F7,
    ExposedKing,
    KingsideAttack,
    QueensideAttack,
    Sacrifice,
    TrappedPiece,
    CollinearMove,
    DiscoveredCheck,
    DefensiveMove,
    QuietMove,
    AnastasiaMate,
    ArabianMate,
    BackRankMate,
    BalestraMate,
    BlindSwineMate,
    BodenMate,
    CornerMate,
    DoubleBishopMate,
    DovetailMate,
    EpauletteMate,
    HookMate,
    KillBoxMate,
    PillsburysMate,
    MorphysMate,
    OperaMate,
    SwallowstailMate,
    TriangleMate,
    VukovicMate,
    SmotheredMate,
    Castling,
    EnPassant,
    Promotion,
    RookEndgame,
    BishopEndgame,
    PawnEndgame,
    KnightEndgame,
    QueenEndgame,
    QueenAndRookEndgame,
    Equality,
    Advantage,
    CrushingAdvantage,
    Checkmate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "purpose",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LearningTrackSupport {
    Improvement {
        learning_path_ref: LearningPathRef,
        critical_moment_id: CriticalMomentId,
        ply: u16,
        basis: LearningTrackSupportBasis,
    },
    Reinforcement {
        learning_path_ref: LearningPathRef,
        critical_moment_id: CriticalMomentId,
        ply: u16,
        basis: LearningTrackSupportBasis,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum LearningTrackPurpose {
    Improvement,
    Reinforcement,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum LearningPathVote {
    ThumbsUp,
    ThumbsDown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningPathFeedbackState {
    pub learning_path_ref: LearningPathRef,
    pub current_vote: Option<LearningPathVote>,
    pub exposed_surfaces: Vec<DeliverySurface>,
}

impl LearningPathRef {
    pub fn for_selected_support(
        game_ref: &GameRef,
        critical_moment_id: &CriticalMomentId,
        key: &LearningTrackKey,
        purpose: LearningTrackPurpose,
        basis: &LearningTrackSupportBasis,
        selection_policy_version: LearningPlanSelectionPolicyVersion,
        resource_catalog_version: LearningResourceCatalogVersion,
    ) -> Self {
        let canonical = serde_json_canonicalizer::to_vec(&(
            "learning-path/v1",
            game_ref,
            critical_moment_id,
            key,
            purpose,
            basis,
            selection_policy_version,
            resource_catalog_version,
        ))
        .expect("selected Learning Path identity has an infallible canonical representation");
        let digest = format!("{:x}", Sha256::digest(canonical));
        Self::try_from(format!("learning-path:{digest}"))
            .expect("a digest-derived Learning Path reference is a valid semantic ID")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LearningTrackSupportBasis {
    DecisionExplanation {
        explanation_path_ref: ExplanationPathRef,
    },
    Opening {
        evidence: OpeningLearningEvidence,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningLearningEvidence {
    pub position_phase: PositionPhase,
    pub opening_identification: OpeningMetadata,
    pub resource_mapping_id: LearningResourceMappingId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningResource {
    pub resource_id: LearningResourceId,
    pub role: LearningResourceRole,
    pub kind: LearningResourceKind,
    pub title: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum LearningResourceRole {
    Learn,
    Drill,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum LearningResourceKind {
    PracticeModule,
    PuzzleStream,
    OpeningReference,
    OpeningPuzzleStream,
}

#[cfg(test)]
mod tests {
    use schemars::schema_for;
    use serde_json::json;

    use super::{LearningPlan, ReviewMomentLearningMaterial};

    #[test]
    fn empty_plan_and_moment_material_share_the_pinned_versions() {
        let plan = LearningPlan::empty();
        let material = ReviewMomentLearningMaterial::empty();
        let expected = json!({
            "selectionPolicyVersion": "learning-plan-selection/v1",
            "resourceCatalogVersion": "learning-resources/2026-08-03",
            "tracks": [],
        });

        assert_eq!(serde_json::to_value(&plan).unwrap(), expected);
        assert_eq!(serde_json::to_value(&material).unwrap(), expected);
        assert_eq!(
            plan.selection_policy_version,
            material.selection_policy_version
        );
        assert_eq!(
            plan.resource_catalog_version,
            material.resource_catalog_version
        );
        assert!(material.tracks.is_empty());
    }

    #[test]
    fn schema_caps_only_moment_local_tracks() {
        let plan = serde_json::to_value(schema_for!(LearningPlan)).unwrap();
        let material = serde_json::to_value(schema_for!(ReviewMomentLearningMaterial)).unwrap();

        assert_eq!(plan.pointer("/properties/tracks/maxItems"), None);
        assert_eq!(
            material.pointer("/properties/tracks/maxItems"),
            Some(&json!(2))
        );
    }
}
