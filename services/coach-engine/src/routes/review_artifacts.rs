use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::{auth::AuthorizedPlayer, types::SharedState};

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route(
            "/api/v1/review-artifacts/preference",
            get(retention_preference).put(update_retention_preference),
        )
        .route(
            "/api/v1/review-artifacts/feedback",
            post(record_quality_feedback),
        )
}

async fn retention_preference(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
) -> Result<Json<crate::quality_capture::RetentionPreference>, axum::http::StatusCode> {
    state
        .review_session
        .retention_preference(player.player_id.as_str())
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(%error, "failed to read quality capture preference");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn update_retention_preference(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
    Json(request): Json<UpdateRetentionPreference>,
) -> Result<Json<crate::quality_capture::RetentionPreference>, axum::http::StatusCode> {
    state
        .review_session
        .set_retention_preference(player.player_id.as_str(), request.enabled)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(%error, "failed to update quality capture preference");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn record_quality_feedback(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
    Json(request): Json<QualityFeedbackRequest>,
) -> Result<axum::http::StatusCode, axum::http::StatusCode> {
    state
        .review_session
        .record_feedback(player.player_id.as_str(), request.reason_codes)
        .await
        .map(|()| axum::http::StatusCode::NO_CONTENT)
        .map_err(|error| {
            tracing::error!(%error, "failed to persist quality capture feedback");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateRetentionPreference {
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualityFeedbackRequest {
    reason_codes: Vec<crate::quality_capture::ReviewFeedbackReason>,
}
