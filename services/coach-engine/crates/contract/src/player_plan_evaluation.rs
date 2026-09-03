use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{canonical_sha256, ArtifactDigest, EngineEvaluation, PositionSnapshot};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerPlanEvaluationContext {
    pub facts_ref: ArtifactDigest,
    pub facts: PlayerPlanEvaluationFacts,
}

impl PlayerPlanEvaluationContext {
    pub fn new(facts: PlayerPlanEvaluationFacts) -> Self {
        Self {
            facts_ref: ArtifactDigest::try_from(canonical_sha256(&facts))
                .expect("Player Plan Evaluation facts have a valid digest"),
            facts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerPlanEvaluationFacts {
    pub position_snapshot: PositionSnapshot,
    pub text_board: String,
    pub reviewed_move_san: String,
    pub objective_counterplay_san: Vec<String>,
    pub best_move_evaluation: EngineEvaluation,
    pub played_move_evaluation: EngineEvaluation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PlayerPlanEvaluationRequest {
    Prepare,
    Admit { draft: PlayerPlanEvaluationDraft },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerPlanEvaluationDraft {
    pub facts_ref: ArtifactDigest,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerPlanEvaluation {
    pub text: String,
}
