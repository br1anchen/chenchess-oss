use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::FirebaseAdministrator,
    beta_access::{
        BetaAccessAuthorizationRevokeResult, BetaAccessGrantResult, BetaAccessInvitationError,
        BetaAccessRequest, BetaAccessRequestFilter, BetaAccessRequestStatus, BetaAccessRetryResult,
        BetaAccessRevokeResult, BetaAccessStoreError,
    },
    daily_coaching::{
        DailyCoachingDigestEmailAdminProjection, DailyCoachingDigestEmailAdminStatus,
    },
    review_session_contract::PlayerId,
    types::SharedState,
};

use super::{ensure_admin_daily_coaching_target, no_store, AdminDailyCoachingTargetError};

pub(super) fn router() -> Router<SharedState> {
    Router::new()
        .route(
            "/api/v1/admin/beta-access/requests",
            get(list_beta_access_requests),
        )
        .route(
            "/api/v1/admin/beta-access/requests/:request_id/grant",
            post(grant_beta_access_request),
        )
        .route(
            "/api/v1/admin/beta-access/requests/:request_id/retry-delivery",
            post(retry_beta_invitation_delivery),
        )
        .route(
            "/api/v1/admin/beta-access/requests/:request_id/revoke",
            post(revoke_beta_invitation),
        )
        .route(
            "/api/v1/admin/beta-access/requests/:request_id/revoke-access",
            post(revoke_beta_access),
        )
        .route(
            "/api/v1/admin/beta-access/requests/:request_id/daily-coaching/digest/trigger",
            post(start_beta_access_player_manual_digest_run).layer(DefaultBodyLimit::max(0)),
        )
        .route(
            "/api/v1/admin/beta-access/requests/:request_id/daily-coaching/digest/regenerate",
            post(regenerate_beta_access_player_digest).layer(DefaultBodyLimit::max(0)),
        )
}

const UNAVAILABLE: &str = "Beta back office is temporarily unavailable.";

async fn list_beta_access_requests(
    _administrator: FirebaseAdministrator,
    State(state): State<SharedState>,
    Query(query): Query<BetaAccessRequestQuery>,
) -> Response {
    let Ok(filter) = query.into_filter() else {
        return error_response(StatusCode::BAD_REQUEST, "Invalid request filter.");
    };
    match state.beta_access.list(filter).await {
        Ok(requests) => {
            let mut projections = Vec::with_capacity(requests.len());
            for request in requests {
                let access_is_active = request.request.access_is_active();
                let daily_coaching = match request.redeemed_player_id {
                    Some(player_id) => {
                        match ensure_admin_daily_coaching_target(&state, &player_id).await {
                            Ok(()) => Some(
                                match state
                                    .daily_coaching
                                    .inspect_latest_digest_email(&player_id)
                                    .await
                                {
                                    Ok(projection) => projection,
                                    Err(error) => {
                                        tracing::error!(
                                            category = "beta_access_admin_digest",
                                            error = %error,
                                            "latest digest inspection failed"
                                        );
                                        unavailable_daily_coaching_projection()
                                    }
                                },
                            ),
                            Err(AdminDailyCoachingTargetError::AccessRequired) => {
                                access_is_active.then(unavailable_daily_coaching_projection)
                            }
                            Err(
                                AdminDailyCoachingTargetError::AccountDeleting
                                | AdminDailyCoachingTargetError::Unavailable,
                            ) => Some(unavailable_daily_coaching_projection()),
                        }
                    }
                    None => None,
                };
                projections.push(BetaAccessRequestProjection {
                    request: request.request,
                    daily_coaching,
                });
            }
            no_store(
                Json(BetaAccessRequestList {
                    requests: projections,
                })
                .into_response(),
            )
        }
        Err(error) => {
            tracing::error!(category = "beta_access_admin", error = %error, "access request listing failed");
            error_response(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE)
        }
    }
}

async fn start_beta_access_player_manual_digest_run(
    _administrator: FirebaseAdministrator,
    State(state): State<SharedState>,
    Path(request_id): Path<String>,
) -> Response {
    let player_id = match active_redeemed_player(&state, &request_id, "trigger_digest").await {
        Ok(player_id) => player_id,
        Err(response) => return response,
    };
    match state
        .daily_coaching
        .start_manual_digest_run(&player_id, chrono::Utc::now())
        .await
    {
        Ok(true) => no_store(StatusCode::ACCEPTED.into_response()),
        Ok(false) => error_response(
            StatusCode::CONFLICT,
            "The Manual Digest Run is not available.",
        ),
        Err(error) => {
            tracing::error!(category = "beta_access_admin_digest", %error, "failed to start the Manual Digest Run");
            error_response(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE)
        }
    }
}

async fn regenerate_beta_access_player_digest(
    _administrator: FirebaseAdministrator,
    State(state): State<SharedState>,
    Path(request_id): Path<String>,
) -> Response {
    let player_id = match active_redeemed_player(&state, &request_id, "regenerate_digest").await {
        Ok(player_id) => player_id,
        Err(response) => return response,
    };
    match state
        .daily_coaching
        .force_regenerate_last_digest(&player_id, chrono::Utc::now())
        .await
    {
        Ok(true) => no_store(StatusCode::ACCEPTED.into_response()),
        Ok(false) => error_response(
            StatusCode::CONFLICT,
            "Forced Digest Regeneration is not available.",
        ),
        Err(error) => {
            tracing::error!(category = "beta_access_admin_digest", %error, "failed to start the Forced Digest Regeneration");
            error_response(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE)
        }
    }
}

async fn active_redeemed_player(
    state: &SharedState,
    request_id: &str,
    operation: &'static str,
) -> Result<PlayerId, Response> {
    let player_id = match state.beta_access.redeemed_player_id(request_id).await {
        Ok(Some(player_id)) => player_id,
        Ok(None) => {
            return Err(error_response(
                StatusCode::CONFLICT,
                "This Player has not redeemed a Beta Invitation.",
            ));
        }
        Err(error) => return Err(mutation_error(error, operation)),
    };
    match ensure_admin_daily_coaching_target(state, &player_id).await {
        Ok(()) => Ok(player_id),
        Err(
            AdminDailyCoachingTargetError::AccountDeleting
            | AdminDailyCoachingTargetError::AccessRequired,
        ) => Err(error_response(
            StatusCode::CONFLICT,
            "This Player does not have active Beta Access.",
        )),
        Err(AdminDailyCoachingTargetError::Unavailable) => {
            Err(error_response(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE))
        }
    }
}

fn unavailable_daily_coaching_projection() -> DailyCoachingDigestEmailAdminProjection {
    DailyCoachingDigestEmailAdminProjection {
        status: DailyCoachingDigestEmailAdminStatus::Unavailable,
        latest_digest: None,
    }
}

async fn retry_beta_invitation_delivery(
    _administrator: FirebaseAdministrator,
    State(state): State<SharedState>,
    Path(request_id): Path<String>,
) -> Response {
    match state.beta_access.retry_delivery(&request_id).await {
        Ok(result) => {
            let outcome = match result {
                BetaAccessRetryResult::Delivered => BetaAccessRetryOutcome::Delivered,
                BetaAccessRetryResult::DeliveryFailed => BetaAccessRetryOutcome::DeliveryFailed,
                BetaAccessRetryResult::NotIssued => BetaAccessRetryOutcome::NotIssued,
                BetaAccessRetryResult::NotRetryable => BetaAccessRetryOutcome::NotRetryable,
                BetaAccessRetryResult::Revoked => BetaAccessRetryOutcome::Revoked,
                BetaAccessRetryResult::Redeemed => BetaAccessRetryOutcome::Redeemed,
            };
            no_store(Json(BetaAccessRetryResponse { outcome }).into_response())
        }
        Err(error) => mutation_error(error, "retry_delivery"),
    }
}

async fn revoke_beta_invitation(
    _administrator: FirebaseAdministrator,
    State(state): State<SharedState>,
    Path(request_id): Path<String>,
) -> Response {
    match state.beta_access.revoke(&request_id).await {
        Ok(result) => {
            let outcome = match result {
                BetaAccessRevokeResult::Revoked => BetaAccessRevokeOutcome::Revoked,
                BetaAccessRevokeResult::NotIssued => BetaAccessRevokeOutcome::NotIssued,
                BetaAccessRevokeResult::AlreadyRevoked => BetaAccessRevokeOutcome::AlreadyRevoked,
                BetaAccessRevokeResult::AlreadyRedeemed => BetaAccessRevokeOutcome::AlreadyRedeemed,
            };
            no_store(Json(BetaAccessRevokeResponse { outcome }).into_response())
        }
        Err(error) => mutation_error(error, "revoke"),
    }
}

async fn revoke_beta_access(
    _administrator: FirebaseAdministrator,
    State(state): State<SharedState>,
    Path(request_id): Path<String>,
) -> Response {
    match state.beta_access.revoke_access(&request_id).await {
        Ok(result) => {
            let outcome = match result {
                BetaAccessAuthorizationRevokeResult::Revoked => {
                    BetaAccessAuthorizationRevokeOutcome::Revoked
                }
                BetaAccessAuthorizationRevokeResult::NotGranted => {
                    BetaAccessAuthorizationRevokeOutcome::NotGranted
                }
                BetaAccessAuthorizationRevokeResult::AlreadyRevoked => {
                    BetaAccessAuthorizationRevokeOutcome::AlreadyRevoked
                }
            };
            no_store(Json(BetaAccessAuthorizationRevokeResponse { outcome }).into_response())
        }
        Err(error) => mutation_error(error, "revoke_access"),
    }
}

async fn grant_beta_access_request(
    _administrator: FirebaseAdministrator,
    State(state): State<SharedState>,
    Path(request_id): Path<String>,
) -> Response {
    match state
        .beta_access
        .grant(&request_id, chrono::Utc::now())
        .await
    {
        Ok(result) => {
            let outcome = match result {
                BetaAccessGrantResult::Delivered => BetaAccessGrantOutcome::Delivered,
                BetaAccessGrantResult::DeliveryFailed => BetaAccessGrantOutcome::DeliveryFailed,
                BetaAccessGrantResult::AlreadyGranted => BetaAccessGrantOutcome::AlreadyGranted,
            };
            no_store(Json(BetaAccessGrantResponse { outcome }).into_response())
        }
        Err(error) => mutation_error(error, "grant"),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BetaAccessRequestQuery {
    email: Option<String>,
    status: Option<BetaAccessRequestStatus>,
}

impl BetaAccessRequestQuery {
    fn into_filter(self) -> Result<BetaAccessRequestFilter, ()> {
        let email_contains = self
            .email
            .map(|email| email.trim().to_ascii_lowercase())
            .filter(|email| !email.is_empty());
        if email_contains.as_ref().is_some_and(|email| {
            email.len() > 254
                || !email.is_ascii()
                || email.bytes().any(|byte| byte.is_ascii_control())
        }) {
            return Err(());
        }
        Ok(BetaAccessRequestFilter {
            email_contains,
            status: self.status,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BetaAccessRequestList {
    requests: Vec<BetaAccessRequestProjection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BetaAccessRequestProjection {
    #[serde(flatten)]
    request: BetaAccessRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    daily_coaching: Option<DailyCoachingDigestEmailAdminProjection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BetaAccessGrantResponse {
    outcome: BetaAccessGrantOutcome,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum BetaAccessGrantOutcome {
    Delivered,
    DeliveryFailed,
    AlreadyGranted,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BetaAccessRetryResponse {
    outcome: BetaAccessRetryOutcome,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum BetaAccessRetryOutcome {
    Delivered,
    DeliveryFailed,
    NotIssued,
    NotRetryable,
    Revoked,
    Redeemed,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BetaAccessRevokeResponse {
    outcome: BetaAccessRevokeOutcome,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum BetaAccessRevokeOutcome {
    Revoked,
    NotIssued,
    AlreadyRevoked,
    AlreadyRedeemed,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BetaAccessAuthorizationRevokeResponse {
    outcome: BetaAccessAuthorizationRevokeOutcome,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum BetaAccessAuthorizationRevokeOutcome {
    Revoked,
    NotGranted,
    AlreadyRevoked,
}

fn mutation_error(error: BetaAccessInvitationError, operation: &'static str) -> Response {
    match error {
        BetaAccessInvitationError::InvalidRequest => {
            error_response(StatusCode::BAD_REQUEST, "Invalid access request.")
        }
        BetaAccessInvitationError::Store(BetaAccessStoreError::NotFound) => {
            error_response(StatusCode::NOT_FOUND, "Access request was not found.")
        }
        error => {
            tracing::error!(
                category = "beta_access_admin",
                operation,
                error = %error,
                "beta access administration operation failed"
            );
            error_response(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE)
        }
    }
}

fn error_response(status: StatusCode, message: &'static str) -> Response {
    let mut response = Json(BetaAccessAdminError { message }).into_response();
    *response.status_mut() = status;
    no_store(response)
}

#[derive(Serialize)]
struct BetaAccessAdminError {
    message: &'static str,
}
