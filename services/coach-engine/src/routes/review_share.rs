use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    account_deletion::AccountDeletionError,
    auth::AuthorizedWebPlayer,
    review_session_contract::{
        encode_delivery_frame, CriticalMomentId, GameImportId, MoveSequencePresentationKind,
        ReviewSessionEvent, ReviewSessionEventEnvelope,
    },
    review_session_transport::SharedReviewResource,
    review_share::{ReviewShareAddress, ReviewShareError, ReviewShareGrant},
    types::SharedState,
};

use super::no_store;

// Sharing is its own action: minting and withdrawing a grant are the
// owner's authenticated requests, and only resolving and reading one
// answer a caller who has nothing but the token.
pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route(
            "/api/v1/review-shares",
            get(list_review_shares)
                .post(mint_review_share)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/api/v1/review-shares/:share_id/revoke",
            post(revoke_review_share).layer(DefaultBodyLimit::max(0)),
        )
        .route(
            "/api/v1/review-shares/resolve",
            post(resolve_review_share).layer(DefaultBodyLimit::max(1024)),
        )
        .route(
            "/api/v1/review-shares/read",
            post(read_shared_review).layer(DefaultBodyLimit::max(1024)),
        )
}

/// Mints one Review Share Grant over an address the caller owns.
///
/// Sharing is its own request, made by a Player who chose to make it. Opening a
/// review never mints anything, which is what keeps a pasted address an
/// identifier rather than a key.
async fn mint_review_share(
    player: AuthorizedWebPlayer,
    State(state): State<SharedState>,
    Json(request): Json<MintReviewShareRequest>,
) -> Result<Response, ReviewShareHttpError> {
    let minted = state
        .review_session
        .mint_review_share(
            player.player_id.as_str(),
            ReviewShareAddress {
                game_import_id: request.game_import_id,
                review_moment_id: request.review_moment_id,
                sequence_kind: request.sequence_kind,
            },
        )
        .await
        .map_err(ReviewShareHttpError)?;
    let mut response = no_store(
        Json(MintedReviewShareResponse {
            expires_at: minted.grant.expires_at,
            share_id: minted.grant.share_id,
            share_token: minted.token,
        })
        .into_response(),
    );
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

/// Lists the grants the signed-in Player still has outstanding.
///
/// Withdrawal has to outlive the page that minted a link, so the owner can name
/// their own grants by asking rather than by remembering. This is the only way
/// back to a grant that does not involve holding its token.
async fn list_review_shares(
    player: AuthorizedWebPlayer,
    State(state): State<SharedState>,
) -> Result<Response, ReviewShareHttpError> {
    let grants = state
        .review_session
        .outstanding_review_shares(player.player_id.as_str())
        .await
        .map_err(ReviewShareHttpError)?;
    Ok(no_store(
        Json(OutstandingReviewSharesResponse {
            shares: grants.into_iter().map(outstanding_share).collect(),
        })
        .into_response(),
    ))
}

/// Withdraws one grant. Revoking a link that has already gone succeeds.
async fn revoke_review_share(
    player: AuthorizedWebPlayer,
    State(state): State<SharedState>,
    Path(share_id): Path<String>,
) -> Result<StatusCode, ReviewShareHttpError> {
    state
        .review_session
        .revoke_review_share(player.player_id.as_str(), &share_id)
        .await
        .map_err(ReviewShareHttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Answers a share token with the address it opens.
///
/// Unauthenticated by design — the token is the whole authorization — and
/// checked against expiry and revocation on every call rather than once when
/// the grant was minted.
async fn resolve_review_share(
    State(state): State<SharedState>,
    Json(request): Json<ResolveReviewShareRequest>,
) -> Result<Response, ReviewShareHttpError> {
    let grant = resolved_grant(&state, &request.share_token).await?;
    Ok(no_store(
        Json(ResolvedReviewShareResponse {
            expires_at: grant.expires_at,
            game_import_id: grant.address.game_import_id,
            review_moment_id: grant.address.review_moment_id,
            sequence_kind: grant.address.sequence_kind,
        })
        .into_response(),
    ))
}

/// Reads one resource of a shared review as the Player who shared it.
///
/// The recipient names a resource, never a command, and the grant is resolved
/// again for this read: a link revoked or expired between two reads stops
/// answering at the second one.
async fn read_shared_review(
    State(state): State<SharedState>,
    Json(request): Json<ReadSharedReviewRequest>,
) -> Result<Response, ReviewShareHttpError> {
    let grant = resolved_grant(&state, &request.share_token).await?;
    let mut events = state.review_session.read_shared_review(&grant, {
        match request.resource {
            SharedReviewResourceRequest::GameReview => SharedReviewResource::GameReview,
            SharedReviewResourceRequest::ReviewMoment => SharedReviewResource::ReviewMoment,
        }
    });
    let mut terminal: Option<ReviewSessionEventEnvelope> = None;
    while let Some(event) = events.recv().await {
        if is_terminal(&event.event) {
            terminal = Some(event);
        }
    }
    let Some(terminal) = terminal else {
        return Err(ReviewShareHttpError(ReviewShareError::Unavailable));
    };
    Ok(no_store(
        (
            [(header::CONTENT_TYPE, "application/json")],
            encode_delivery_frame(terminal),
        )
            .into_response(),
    ))
}

/// The grant behind a token, if it is still one the Coach Engine will answer.
///
/// A shared read runs as the Player who shared it, so it has to respect the
/// same account state their own requests do. Every authenticated path checks
/// that the Player is still active before acting; a grant outlives the tab that
/// minted it, so without this check a link would keep serving a Player's review
/// after they asked for their account to be deleted. Deletion eventually
/// removes the grant with the subtree — this closes the window before it does.
async fn resolved_grant(
    state: &SharedState,
    share_token: &str,
) -> Result<ReviewShareGrant, ReviewShareHttpError> {
    let grant = state
        .review_session
        .resolve_review_share(share_token)
        .await
        .map_err(ReviewShareHttpError)?;
    state
        .account_deletion
        .ensure_player_active(&grant.owner)
        .await
        .map_err(|error| {
            ReviewShareHttpError(match error {
                AccountDeletionError::AccountDeleting => ReviewShareError::NotFound,
                _ => ReviewShareError::Unavailable,
            })
        })?;
    Ok(grant)
}

fn is_terminal(event: &ReviewSessionEvent) -> bool {
    matches!(
        event,
        ReviewSessionEvent::Completed { .. }
            | ReviewSessionEvent::Unavailable { .. }
            | ReviewSessionEvent::Cancelled { .. }
            | ReviewSessionEvent::Conflict { .. }
            | ReviewSessionEvent::Rejected { .. }
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MintReviewShareRequest {
    game_import_id: GameImportId,
    review_moment_id: CriticalMomentId,
    #[serde(default)]
    sequence_kind: Option<MoveSequencePresentationKind>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveReviewShareRequest {
    share_token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadSharedReviewRequest {
    share_token: String,
    resource: SharedReviewResourceRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SharedReviewResourceRequest {
    GameReview,
    ReviewMoment,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MintedReviewShareResponse {
    expires_at: DateTime<Utc>,
    share_id: String,
    share_token: String,
}

fn outstanding_share(grant: ReviewShareGrant) -> OutstandingReviewShare {
    OutstandingReviewShare {
        expires_at: grant.expires_at,
        game_import_id: grant.address.game_import_id,
        review_moment_id: grant.address.review_moment_id,
        sequence_kind: grant.address.sequence_kind,
        share_id: grant.share_id,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutstandingReviewSharesResponse {
    shares: Vec<OutstandingReviewShare>,
}

/// One outstanding grant as its owner sees it: where it opens, when it lapses,
/// and the name they withdraw it by. Never its token — that copy left with the
/// link and is not recoverable from anything stored.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutstandingReviewShare {
    expires_at: DateTime<Utc>,
    game_import_id: GameImportId,
    review_moment_id: CriticalMomentId,
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence_kind: Option<MoveSequencePresentationKind>,
    share_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedReviewShareResponse {
    expires_at: DateTime<Utc>,
    game_import_id: GameImportId,
    review_moment_id: CriticalMomentId,
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence_kind: Option<MoveSequencePresentationKind>,
}

/// Why a share request was refused, as the surface has to say it.
///
/// A recipient already holds the token, so telling them the difference between
/// expired, withdrawn, and malformed discloses nothing and is the difference
/// between a page that explains itself and a dead one.
#[derive(Debug)]
pub struct ReviewShareHttpError(ReviewShareError);

impl IntoResponse for ReviewShareHttpError {
    fn into_response(self) -> Response {
        let (status, reason) = match self.0 {
            ReviewShareError::NotOwned => (StatusCode::FORBIDDEN, "notOwned"),
            ReviewShareError::InvalidToken => (StatusCode::BAD_REQUEST, "invalidToken"),
            ReviewShareError::UnknownAddress => (StatusCode::NOT_FOUND, "unknownAddress"),
            ReviewShareError::TooManyReads => (StatusCode::TOO_MANY_REQUESTS, "tooManyReads"),
            ReviewShareError::NotFound => (StatusCode::NOT_FOUND, "notFound"),
            ReviewShareError::Expired => (StatusCode::GONE, "expired"),
            ReviewShareError::Configuration(_)
            | ReviewShareError::Unavailable
            | ReviewShareError::InvalidRecord => {
                tracing::error!(category = "review_share", error = %self.0, "review share request failed");
                (StatusCode::SERVICE_UNAVAILABLE, "unavailable")
            }
        };
        let mut response = no_store(Json(ReviewShareErrorResponse { reason }).into_response());
        *response.status_mut() = status;
        response
    }
}

#[derive(Serialize)]
struct ReviewShareErrorResponse {
    reason: &'static str,
}

#[cfg(test)]
#[path = "tests/review_share_http.rs"]
mod tests;
