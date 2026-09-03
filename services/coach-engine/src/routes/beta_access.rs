use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::{
    auth::{AuthError, AuthenticatedFirebasePlayer, AuthorizedPlayer},
    beta_access::NormalizedEmail,
    types::SharedState,
};

use super::{no_store, trusted_source_ip};

/// Every Beta Access route, including the redemption and back-office surfaces
/// their own modules build, so one place answers where a beta-access route goes.
pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route(
            "/api/v1/beta-access/requests",
            post(request_beta_access).layer(DefaultBodyLimit::max(0)),
        )
        .route(
            "/api/v1/beta-access/authorization",
            get(authorize_beta_access),
        )
        .merge(super::beta_redemption::router())
        .merge(super::beta_admin::router())
}

async fn authorize_beta_access(player: AuthorizedPlayer) -> Response {
    no_store(
        Json(AuthorizedBetaAccess {
            player_id: player.player_id.as_str().to_string(),
        })
        .into_response(),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthorizedBetaAccess {
    player_id: String,
}

const ACCEPTED: &str = "Thanks. Your beta access request has been received.";
const INVALID: &str = "Beta access request could not be accepted.";
const INADMISSIBLE: &str = "Confirm your email address, then request Beta Access again.";
const UNAVAILABLE: &str =
    "Beta access requests are temporarily unavailable. Please try again later.";

async fn request_beta_access(
    player: AuthenticatedFirebasePlayer,
    State(state): State<SharedState>,
    headers: HeaderMap,
    _empty_body: Bytes,
) -> Result<Response, AuthError> {
    let email = match verified_email(&player) {
        Ok(email) => email,
        Err(reason) => {
            tracing::warn!(
                category = "beta_access",
                reason = reason.as_str(),
                "beta access request rejected an inadmissible identity"
            );
            return Ok(response(StatusCode::FORBIDDEN, INADMISSIBLE));
        }
    };
    let Some(source_ip) = trusted_source_ip(&headers) else {
        tracing::warn!(
            category = "beta_access",
            "beta access request lacked a trusted source IP"
        );
        return Ok(response(StatusCode::BAD_REQUEST, INVALID));
    };

    Ok(
        match state
            .beta_access
            .submit(email, source_ip, chrono::Utc::now())
            .await
        {
            Ok(()) => response(StatusCode::ACCEPTED, ACCEPTED),
            Err(error) => {
                tracing::error!(category = "beta_access", error = %error, "beta access request failed");
                response(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE)
            }
        },
    )
}

fn verified_email(
    player: &AuthenticatedFirebasePlayer,
) -> Result<NormalizedEmail, InadmissibleIdentity> {
    if !player.email_verified {
        return Err(InadmissibleIdentity::UnverifiedEmail);
    }
    if !matches!(
        player.sign_in_provider.as_deref(),
        Some("password" | "google.com")
    ) {
        return Err(InadmissibleIdentity::UnsupportedProvider);
    }
    player
        .email
        .as_deref()
        .and_then(|email| NormalizedEmail::parse(email).ok())
        .ok_or(InadmissibleIdentity::InvalidEmail)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InadmissibleIdentity {
    InvalidEmail,
    UnsupportedProvider,
    UnverifiedEmail,
}

impl InadmissibleIdentity {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidEmail => "invalid_email",
            Self::UnsupportedProvider => "unsupported_provider",
            Self::UnverifiedEmail => "unverified_email",
        }
    }
}

fn response(status: StatusCode, message: &'static str) -> Response {
    let mut response = Json(BetaAccessResponse { message }).into_response();
    *response.status_mut() = status;
    no_store(response)
}

#[derive(Serialize)]
struct BetaAccessResponse {
    message: &'static str,
}
