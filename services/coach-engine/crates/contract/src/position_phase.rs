use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const POSITION_PHASE_POLICY_VERSION: PositionPhasePolicyVersion =
    PositionPhasePolicyVersion::V1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PositionPhase {
    pub policy_version: PositionPhasePolicyVersion,
    pub phase: PositionPhaseKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub enum PositionPhasePolicyVersion {
    #[serde(rename = "position-phase/v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum PositionPhaseKind {
    Opening,
    Middlegame,
    Endgame,
}
