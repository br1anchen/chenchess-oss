use axum::{
    extract::{DefaultBodyLimit, State},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::Deserialize;

use crate::auth::AuthorizedPlayer;
use crate::opening_analysis::{
    analyze_opening_line, OpeningAnalysisOutcome, OpeningAnalysisRequest, OpeningLineIdentity,
    ResolveOpeningLineOutcome,
};
use crate::opening_identification::{
    self, opening_line_reference, resolve_opening_line, FindOpeningLinesRequest,
};
use crate::types::SharedState;

use super::no_store;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route(
            "/api/v1/opening-lines/resolve",
            post(resolve_opening_lines).layer(DefaultBodyLimit::max(1024)),
        )
        .route(
            "/api/v1/opening-lines/analysis",
            post(analyze_opening_lines).layer(DefaultBodyLimit::max(8192)),
        )
        .route(
            "/api/v1/opening-lines/find",
            post(find_opening_lines).layer(DefaultBodyLimit::max(4096)),
        )
}

async fn find_opening_lines(Json(request): Json<FindOpeningLinesRequest>) -> impl IntoResponse {
    no_store(
        Json(opening_identification::find_opening_lines(
            &request.query,
            &request.played,
        ))
        .into_response(),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolveOpeningLineRequest {
    opening_line_ref: String,
}

/// A public catalog read, like find: opening a line is navigation, so
/// resolving its address creates nothing and needs no Player.
async fn resolve_opening_lines(
    Json(request): Json<ResolveOpeningLineRequest>,
) -> impl IntoResponse {
    let outcome = match resolve_opening_line(&request.opening_line_ref) {
        Some(line) => ResolveOpeningLineOutcome::Resolved {
            line: OpeningLineIdentity {
                opening_line_ref: opening_line_reference(&line.eco, &line.name, &line.path),
                eco: line.eco.clone(),
                name: line.name.clone(),
                path: line.path.clone(),
            },
        },
        None => ResolveOpeningLineOutcome::UnknownOpeningLine,
    };
    no_store(Json(outcome).into_response())
}

async fn analyze_opening_lines(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
    Json(request): Json<OpeningAnalysisRequest>,
) -> impl IntoResponse {
    let outcome = analyze_opening_line(&state.opening_analysis, &player.player_id, &request).await;
    let status = match &outcome {
        OpeningAnalysisOutcome::Unavailable { .. } => axum::http::StatusCode::SERVICE_UNAVAILABLE,
        _ => axum::http::StatusCode::OK,
    };
    no_store((status, Json(outcome)).into_response())
}
