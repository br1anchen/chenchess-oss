pub mod account_deletion;
pub mod auth;
pub mod beta_access;
pub use coach_engine_pipeline::causal_facts;
#[cfg(test)]
pub(crate) use coach_engine_pipeline::critical_moment_selector;
#[cfg(test)]
mod certification_keys;
pub mod chess_com;
pub mod chess_com_import;
mod chess_literal_grounding;
pub mod critical_moment_comment;
pub mod daily_coaching;
pub mod decision_explanation;
mod decision_learning;
mod deployment;
pub mod digested_games;
pub use coach_engine_pipeline::engine_analysis;
pub mod evaluation_fingerprint;
pub use coach_engine_pipeline::evaluation_recording;
mod firestore;
pub mod game_analysis_store;
mod game_eligibility;
pub mod game_import;
pub mod game_import_store;
pub mod grounded_review_moment;
pub use coach_engine_pipeline::human_move_model;
pub mod imported_games;
pub mod language_layer_ledger;
mod language_layer_markers;
pub mod language_layer_prompt;
pub mod language_layer_provider;
pub mod learning_path_feedback;
mod learning_plan;
pub mod lichess;
pub mod lichess_import;
pub mod local_runtime;
pub mod moment_display;
pub mod opening_analysis;
pub mod opening_identification;
pub use coach_engine_pipeline::operating_limits;
pub use coach_engine_pipeline::pgn;
pub mod pin_record;
pub mod pin_verification;
pub mod pipeline_evaluation;
mod player_plan_evaluation;
mod player_selected_decision;
pub use coach_engine_pipeline::position_phase;
pub mod profile_game_feed;
pub mod projected_plan;
pub(crate) use coach_engine_pipeline::provider_concurrency;
pub(crate) use coach_engine_pipeline::provider_provenance;
mod provider_user_agent;
pub mod quality_capture;
pub(crate) use coach_engine_pipeline::request_single_flight;
mod request_trace;
mod retry_after;
pub mod review_analysis_cache;
pub mod review_annotation_store;
mod review_durability;
pub mod review_facts;
pub mod review_session_board;
mod review_session_cancellation;
pub mod review_session_coaching;
pub use coach_engine_contract as review_session_contract;
pub mod review_session_exploration;
pub(crate) mod review_session_game_identity;
pub mod review_session_host;
pub mod review_session_processor;
pub mod review_session_runtime;
pub mod review_session_start;
pub mod review_session_transport;
pub mod review_share;
pub mod review_validation;
pub mod reviewed_games;
pub mod routes;
pub use coach_engine_pipeline::rule_extractor;
pub mod shared_assets;
pub mod types;

use axum::Router;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use types::SharedState;

pub fn app(state: SharedState) -> Router {
    Router::new()
        .merge(routes::account::router())
        .merge(routes::beta_access::router())
        .merge(routes::daily_coaching::router())
        .merge(routes::health::router())
        .merge(routes::imported_games::router())
        .merge(routes::oauth::router())
        .merge(routes::opening_lines::router())
        .merge(routes::review_artifacts::router())
        .merge(routes::review_session::router())
        .merge(routes::review_share::router())
        .merge(routes::reviewed_games::router())
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
