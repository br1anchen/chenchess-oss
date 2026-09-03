use axum::{
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};

use crate::{
    auth::AuthorizedPlayer,
    reviewed_games::{self, ReviewedGameSearchError, ReviewedGameSearchRequest},
    types::SharedState,
};

use super::no_store;

pub(crate) fn router() -> Router<SharedState> {
    Router::new().route(
        "/api/v1/reviewed-games/search",
        post(search_reviewed_games).layer(DefaultBodyLimit::max(4096)),
    )
}

async fn search_reviewed_games(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
    Json(request): Json<ReviewedGameSearchRequest>,
) -> Response {
    match reviewed_games::search_reviewed_games(
        &state.daily_coaching,
        &state.imported_games,
        &player.player_id,
        request,
    )
    .await
    {
        Ok(result) => no_store(Json(result).into_response()),
        Err(ReviewedGameSearchError::InvalidRequest) => {
            no_store(StatusCode::BAD_REQUEST.into_response())
        }
        Err(error) => {
            tracing::error!(category = "reviewed_game_search", %error, "failed to search reviewed Games");
            no_store(StatusCode::SERVICE_UNAVAILABLE.into_response())
        }
    }
}
