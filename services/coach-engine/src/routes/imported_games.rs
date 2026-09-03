use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::{auth::AuthorizedPlayer, imported_games::ImportedGamesError, types::SharedState};

use super::no_store;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/imported-games", get(imported_games))
        .route("/api/v1/openings/played", get(played_openings))
}

/// The played-opening aggregate the opening surface starts from. Returned
/// whole so an agent never derives it from a truncated search page.
async fn played_openings(player: AuthorizedPlayer, State(state): State<SharedState>) -> Response {
    match state
        .imported_games
        .played_openings(&player.player_id)
        .await
    {
        Ok(result) => no_store(Json(result).into_response()),
        Err(error) => {
            tracing::error!(category = "imported_games", %error, "failed to aggregate played openings");
            no_store(StatusCode::SERVICE_UNAVAILABLE.into_response())
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportedGamesQuery {
    cursor: Option<String>,
}

async fn imported_games(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
    Query(query): Query<ImportedGamesQuery>,
) -> Response {
    match state
        .imported_games
        .page(&player.player_id, query.cursor.as_deref())
        .await
    {
        Ok(page) => no_store(Json(page).into_response()),
        Err(ImportedGamesError::InvalidCursor) => no_store(StatusCode::BAD_REQUEST.into_response()),
        Err(error) => {
            tracing::error!(category = error.diagnostic_category(), %error, "failed to list Imported Games");
            no_store(StatusCode::SERVICE_UNAVAILABLE.into_response())
        }
    }
}
