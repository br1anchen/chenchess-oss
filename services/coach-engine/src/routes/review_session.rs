use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, HeaderMap},
    response::Response,
    routing::post,
    Router,
};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};

use crate::{auth::AuthorizedPlayer, types::SharedState};

const COACH_TRACE_HEADER: &str = "x-chenchess-trace-id";

pub(crate) fn router() -> Router<SharedState> {
    Router::new().route(
        "/api/v1/review-session/commands",
        post(review_session_commands),
    )
}

async fn review_session_commands(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let trace_id = headers
        .get(COACH_TRACE_HEADER)
        .and_then(|value| value.to_str().ok());
    let events = state
        .review_session
        .submit_with_trace(player.player_id.as_str(), &body, trace_id);
    let stream = UnboundedReceiverStream::new(events).map(|event| {
        Ok::<_, std::convert::Infallible>(crate::review_session_contract::encode_delivery_frame(
            event,
        ))
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .body(Body::from_stream(stream))
        .expect("the Review Session response has valid static headers")
}
