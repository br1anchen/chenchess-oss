use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::AuthenticatedFirebasePlayer,
    beta_access::{BetaAccessRedemptionIdentity, BetaAccessRedemptionResult, NormalizedEmail},
    types::SharedState,
};

use super::{no_store, trusted_source_ip};

pub(super) fn router() -> Router<SharedState> {
    Router::new().route(
        "/api/v1/beta-access/invitations/redeem",
        post(redeem_beta_invitation).layer(DefaultBodyLimit::max(1024)),
    )
}

async fn redeem_beta_invitation(
    player: AuthenticatedFirebasePlayer,
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !is_json(&headers) {
        return error_response(StatusCode::BAD_REQUEST, "Invalid redemption request");
    }
    let Ok(request) = serde_json::from_slice::<BetaRedemptionRequest>(&body) else {
        return error_response(StatusCode::BAD_REQUEST, "Invalid redemption request");
    };
    let Some(source_ip) = trusted_source_ip(&headers) else {
        tracing::warn!(
            category = "beta_access",
            "beta invitation redemption lacked a trusted source IP"
        );
        return error_response(StatusCode::BAD_REQUEST, "Invalid redemption request");
    };
    let identity = verified_identity(&player);
    match state
        .beta_access
        .redeem(identity, &request.code, source_ip, chrono::Utc::now())
        .await
    {
        Ok(result) => no_store(
            Json(BetaRedemptionResponse {
                outcome: result.into(),
            })
            .into_response(),
        ),
        Err(error) => {
            tracing::error!(
                category = "beta_access",
                error = %error,
                "beta invitation redemption failed"
            );
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Redemption is temporarily unavailable",
            )
        }
    }
}

fn verified_identity(player: &AuthenticatedFirebasePlayer) -> BetaAccessRedemptionIdentity {
    if player.email_verified
        && matches!(
            player.sign_in_provider.as_deref(),
            Some("password" | "google.com")
        )
    {
        if let Some(email) = player
            .email
            .as_deref()
            .and_then(|email| NormalizedEmail::parse(email).ok())
        {
            return BetaAccessRedemptionIdentity::Verified {
                player_id: player.player_id.clone(),
                email,
            };
        }
    }
    BetaAccessRedemptionIdentity::VerificationRequired {
        player_id: player.player_id.clone(),
    }
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
}

fn error_response(status: StatusCode, message: &'static str) -> Response {
    let mut response = Json(BetaRedemptionError { error: message }).into_response();
    *response.status_mut() = status;
    no_store(response)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BetaRedemptionRequest {
    code: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BetaRedemptionResponse {
    outcome: BetaRedemptionOutcome,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum BetaRedemptionOutcome {
    Granted,
    WrongAccount,
    VerificationRequired,
    Revoked,
    Invalid,
    AlreadyHandled,
    TryLater,
}

impl From<BetaAccessRedemptionResult> for BetaRedemptionOutcome {
    fn from(result: BetaAccessRedemptionResult) -> Self {
        match result {
            BetaAccessRedemptionResult::Granted => Self::Granted,
            BetaAccessRedemptionResult::WrongAccount => Self::WrongAccount,
            BetaAccessRedemptionResult::VerificationRequired => Self::VerificationRequired,
            BetaAccessRedemptionResult::Revoked => Self::Revoked,
            BetaAccessRedemptionResult::Invalid => Self::Invalid,
            BetaAccessRedemptionResult::AlreadyHandled => Self::AlreadyHandled,
            BetaAccessRedemptionResult::RateLimited => Self::TryLater,
        }
    }
}

#[derive(Serialize)]
struct BetaRedemptionError {
    error: &'static str,
}
