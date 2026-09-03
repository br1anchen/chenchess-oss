use axum::{routing::get, Json, Router};
use serde::Serialize;

use crate::types::SharedState;

pub(crate) fn router() -> Router<SharedState> {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
}
