use std::net::IpAddr;

use axum::{
    http::{header, HeaderMap},
    response::Response,
};

use crate::{
    account_deletion::AccountDeletionError, beta_access::BetaAccessAuthorizationError,
    review_session_contract::PlayerId, types::SharedState,
};

pub(crate) mod account;
pub(crate) mod beta_access;
mod beta_admin;
mod beta_redemption;
pub(crate) mod daily_coaching;
pub(crate) mod health;
pub(crate) mod imported_games;
pub(crate) mod oauth;
pub(crate) mod opening_lines;
pub(crate) mod review_artifacts;
pub(crate) mod review_session;
pub(crate) mod review_share;
pub(crate) mod reviewed_games;

const TRUSTED_SOURCE_IP_HEADER: &str = "x-chenchess-source-ip";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdminDailyCoachingTargetError {
    AccountDeleting,
    AccessRequired,
    Unavailable,
}

async fn ensure_admin_daily_coaching_target(
    state: &SharedState,
    player_id: &PlayerId,
) -> Result<(), AdminDailyCoachingTargetError> {
    state
        .account_deletion
        .ensure_player_active(player_id)
        .await
        .map_err(|error| match error {
            AccountDeletionError::AccountDeleting => AdminDailyCoachingTargetError::AccountDeleting,
            error => {
                tracing::error!(
                    category = "admin_daily_coaching_target",
                    %error,
                    "account deletion authorization failed before Daily Coaching admin action"
                );
                AdminDailyCoachingTargetError::Unavailable
            }
        })?;
    state
        .beta_access
        .require_access(player_id)
        .await
        .map_err(|error| match error {
            BetaAccessAuthorizationError::Required => AdminDailyCoachingTargetError::AccessRequired,
            BetaAccessAuthorizationError::Unavailable => AdminDailyCoachingTargetError::Unavailable,
            BetaAccessAuthorizationError::Store(error) => {
                tracing::error!(
                    category = "admin_daily_coaching_target",
                    %error,
                    "Beta Access authorization failed before Daily Coaching admin action"
                );
                AdminDailyCoachingTargetError::Unavailable
            }
        })
}

fn trusted_source_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get(TRUSTED_SOURCE_IP_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

#[cfg(test)]
#[path = "routes/tests/review_session_http.rs"]
mod tests;

#[cfg(test)]
#[path = "routes/tests/beta_access_http.rs"]
mod beta_access_tests;

#[cfg(test)]
#[path = "routes/tests/firebase_token.rs"]
mod firebase_token_test_support;

#[cfg(test)]
#[path = "routes/tests/mcp_conformance_http.rs"]
mod mcp_conformance_tests;

#[cfg(test)]
#[path = "routes/tests/daily_coaching_http.rs"]
mod daily_coaching_http_tests;

#[cfg(test)]
#[path = "routes/tests/player_traffic_http.rs"]
mod player_traffic_http_tests;
