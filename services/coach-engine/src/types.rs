use std::sync::Arc;

use crate::{
    account_deletion::AccountDeletionRuntime, auth::AuthConfig, beta_access::BetaAccessRuntime,
    daily_coaching::DailyCoachingRuntime, imported_games::ImportedGamesRuntime,
    opening_analysis::OpeningAnalysisRuntime, review_session_transport::ReviewSessionWebBinding,
};

pub use coach_engine_pipeline::domain::*;

#[derive(Clone)]
pub struct AppState {
    pub account_deletion: AccountDeletionRuntime,
    pub auth: AuthConfig,
    pub beta_access: BetaAccessRuntime,
    pub daily_coaching: DailyCoachingRuntime,
    pub imported_games: ImportedGamesRuntime,
    pub opening_analysis: OpeningAnalysisRuntime,
    pub review_session: ReviewSessionWebBinding,
}

pub type SharedState = Arc<AppState>;
