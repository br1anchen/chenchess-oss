use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    daily_coaching::DailyCoachingRunStoreError,
    game_import_store::GameImportStoreError,
    imported_games::{
        ImportedGameOpening, ImportedGameOutcome, ImportedGameProvider, ImportedGameReviewSide,
        ImportedGameTimeControlClass,
    },
    review_session_contract::{GameImportId, LearningTrackKey},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedGameSearchRequest {
    #[ts(optional)]
    pub played_from: Option<String>,
    #[ts(optional)]
    pub played_to: Option<String>,
    #[ts(optional)]
    pub provider: Option<ImportedGameProvider>,
    #[ts(optional)]
    pub opening_eco_prefix: Option<String>,
    #[ts(optional)]
    pub opening_name: Option<String>,
    #[ts(optional)]
    pub outcome: Option<ImportedGameOutcome>,
    #[ts(optional)]
    pub review_side: Option<ImportedGameReviewSide>,
    #[ts(optional)]
    pub time_control_class: Option<ImportedGameTimeControlClass>,
    #[ts(optional)]
    pub opponent_name: Option<String>,
    #[ts(optional)]
    pub opponent_rating_min: Option<u16>,
    #[ts(optional)]
    pub opponent_rating_max: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedGameSearchResult {
    pub games: Vec<ReviewedGameSearchCard>,
    pub coverage: ReviewedGameSearchCoverage,
    pub truncation: ReviewedGameSearchTruncation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedGameSearchCoverage {
    pub reviewed_game_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earliest_played_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_played_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReviewedGameSearchTruncation {
    Complete {
        total_match_count: u32,
    },
    Truncated {
        total_match_count: u32,
        oldest_returned_at: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedGameSearchCard {
    pub reviewed_game_key: String,
    pub game_import_id: GameImportId,
    pub provider: ImportedGameProvider,
    pub review_side: ImportedGameReviewSide,
    pub outcome: Option<ImportedGameOutcome>,
    pub opening: Option<ImportedGameOpening>,
    pub opponent_name: Option<String>,
    pub opponent_rating: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ended_at: Option<String>,
    pub time_control_class: Option<ImportedGameTimeControlClass>,
    pub learning_path_count: u16,
    pub learning_track_keys: Vec<LearningTrackKey>,
    pub digested: bool,
    pub imported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest_date: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewedGameSearchError {
    #[error("invalid reviewed-Game search request")]
    InvalidRequest,
    #[error(transparent)]
    DailyCoachingStore(#[from] DailyCoachingRunStoreError),
    #[error(transparent)]
    GameImportStore(#[from] GameImportStoreError),
}
