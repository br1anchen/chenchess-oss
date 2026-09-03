use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{NaiveDate, Utc};
use serde::Deserialize;

use crate::{
    auth::{AuthorizedPlayer, AuthorizedWebPlayer, VerifiedAccountEmailObservation},
    daily_coaching::{
        CheckPlayingProfileOutcome, CheckPlayingProfileRequest, ConnectPlayingProfileOutcome,
        ConnectPlayingProfileRejectionReason, ConnectPlayingProfileRequest,
        DailyCoachingMutationOutcome, DailyCoachingMutationRejectionReason, DailyCoachingProvider,
        DigestWebhookError, RecentPlayingProfileGamesOutcome, RemovePlayingProfileRequest,
        ReplacePlayingProfileRequest, SetDailyCoachingEnabledRequest,
    },
    review_session_contract::PlayerId,
    types::SharedState,
};

use super::no_store;

pub(crate) fn router() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/daily-coaching", get(daily_coaching_state))
        .route(
            "/api/v1/daily-coaching/dashboard",
            get(daily_coaching_dashboard),
        )
        .route(
            "/api/v1/daily-coaching/recent-profile-games",
            get(recent_playing_profile_games),
        )
        .route(
            "/api/v1/daily-coaching/digests/:digest_id",
            get(daily_coaching_digest),
        )
        .route(
            "/api/v1/daily-coaching/connections",
            post(connect_playing_profile).layer(DefaultBodyLimit::max(2048)),
        )
        .route(
            "/api/v1/daily-coaching/connections/:provider",
            put(replace_playing_profile)
                .delete(remove_playing_profile)
                .layer(DefaultBodyLimit::max(2048)),
        )
        .route(
            "/api/v1/daily-coaching/connections/:provider/check",
            post(check_playing_profile).layer(DefaultBodyLimit::max(256)),
        )
        .route(
            "/api/v1/daily-coaching/enabled",
            put(set_daily_coaching_enabled).layer(DefaultBodyLimit::max(256)),
        )
        .route(
            "/api/v1/daily-coaching/email",
            put(set_digest_email_enabled).layer(DefaultBodyLimit::max(256)),
        )
        .route(
            "/api/v1/daily-coaching/email/unsubscribe",
            get(preview_digest_email_unsubscribe)
                .post(unsubscribe_digest_email)
                .layer(DefaultBodyLimit::max(256)),
        )
        .route(
            "/api/v1/daily-coaching/email/webhooks/resend",
            post(ingest_digest_email_webhook).layer(DefaultBodyLimit::max(32 * 1024)),
        )
}

async fn daily_coaching_state(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
) -> Response {
    if let Err(response) =
        observe_account_email(&state, &player.player_id, &player.verified_email).await
    {
        return response;
    }
    match state.daily_coaching.state(&player.player_id).await {
        Ok(setup) => {
            if let Err(error) = state
                .daily_coaching
                .promote_due_window(&player.player_id, Utc::now())
                .await
            {
                tracing::error!(category = "daily_coaching_nudge", %error, "Daily Coaching arrival promotion failed");
            }
            no_store(Json(setup).into_response())
        }
        Err(error) => {
            tracing::error!(category = "daily_coaching", %error, "failed to read Daily Coaching state");
            no_store(StatusCode::SERVICE_UNAVAILABLE.into_response())
        }
    }
}

async fn daily_coaching_dashboard(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
) -> Response {
    if let Err(response) =
        observe_account_email(&state, &player.player_id, &player.verified_email).await
    {
        return response;
    }
    match state.daily_coaching.dashboard(&player.player_id).await {
        Ok(dashboard) => {
            if let Err(error) = state
                .daily_coaching
                .promote_due_window(&player.player_id, Utc::now())
                .await
            {
                tracing::error!(category = "daily_coaching_nudge", %error, "Daily Coaching arrival promotion failed");
            }
            no_store(Json(dashboard).into_response())
        }
        Err(error) => {
            tracing::error!(category = "daily_coaching", %error, "failed to read Daily Coaching dashboard");
            no_store(StatusCode::SERVICE_UNAVAILABLE.into_response())
        }
    }
}

async fn recent_playing_profile_games(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
) -> Response {
    if let Err(response) =
        observe_account_email(&state, &player.player_id, &player.verified_email).await
    {
        return response;
    }
    let outcome = state
        .daily_coaching
        .recent_playing_profile_games(&player.player_id)
        .await;
    let status = match outcome {
        RecentPlayingProfileGamesOutcome::Found { .. }
        | RecentPlayingProfileGamesOutcome::NoPlayingProfile => StatusCode::OK,
        RecentPlayingProfileGamesOutcome::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
    };
    no_store((status, Json(outcome)).into_response())
}

async fn daily_coaching_digest(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
    Path(digest_id): Path<String>,
) -> Response {
    if let Err(response) =
        observe_account_email(&state, &player.player_id, &player.verified_email).await
    {
        return response;
    }
    let Some(digest_id) = parse_digest_id(&digest_id) else {
        return no_store(StatusCode::NOT_FOUND.into_response());
    };
    match state
        .daily_coaching
        .digest(&player.player_id, digest_id)
        .await
    {
        Ok(Some(digest)) => no_store(Json(digest).into_response()),
        Ok(None) => no_store(StatusCode::NOT_FOUND.into_response()),
        Err(error) => {
            tracing::error!(category = "daily_coaching", %error, "failed to read Daily Coaching digest");
            no_store(StatusCode::SERVICE_UNAVAILABLE.into_response())
        }
    }
}

async fn connect_playing_profile(
    player: AuthorizedPlayer,
    State(state): State<SharedState>,
    Json(request): Json<ConnectPlayingProfileRequest>,
) -> Response {
    let outcome = match &player.verified_email {
        VerifiedAccountEmailObservation::NotObserved => {
            state
                .daily_coaching
                .connect(&player.player_id, request)
                .await
        }
        VerifiedAccountEmailObservation::Observed(email) => {
            state
                .daily_coaching
                .connect_with_verified_email(&player.player_id, email.as_ref(), request)
                .await
        }
    };
    connect_response(outcome)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SetDigestEmailEnabledRequest {
    enabled: bool,
}

async fn set_digest_email_enabled(
    player: AuthorizedWebPlayer,
    State(state): State<SharedState>,
    Json(request): Json<SetDigestEmailEnabledRequest>,
) -> Response {
    let Some(email) = player.verified_email.email() else {
        return mutation_response(DailyCoachingMutationOutcome::Rejected {
            reason: DailyCoachingMutationRejectionReason::NoVerifiedAccountEmail,
        });
    };
    mutation_response(
        state
            .daily_coaching
            .set_digest_email_enabled(&player.player_id, Some(email), request.enabled)
            .await,
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnsubscribeQuery {
    token: String,
}

async fn preview_digest_email_unsubscribe(
    State(state): State<SharedState>,
    Query(query): Query<UnsubscribeQuery>,
) -> Response {
    if !valid_unsubscribe_token(&query.token)
        || !state
            .daily_coaching
            .can_unsubscribe_digest_email(&query.token)
            .await
    {
        return no_store(StatusCode::NOT_FOUND.into_response());
    }
    no_store(
        (
            StatusCode::OK,
            [("content-type", "text/html; charset=utf-8")],
            "<!doctype html><title>Stop digest email?</title><p>Stop Daily Coaching digest email? Coaching and your digest archive will stay unchanged.</p><form method=\"post\"><button type=\"submit\">Stop digest email</button></form>",
        )
            .into_response(),
    )
}

async fn unsubscribe_digest_email(
    State(state): State<SharedState>,
    Query(query): Query<UnsubscribeQuery>,
) -> Response {
    if !valid_unsubscribe_token(&query.token)
        || !state
            .daily_coaching
            .unsubscribe_digest_email(&query.token)
            .await
    {
        return no_store(StatusCode::NOT_FOUND.into_response());
    }
    no_store(
        (
            StatusCode::OK,
            [("content-type", "text/html; charset=utf-8")],
            "<!doctype html><title>Digest email stopped</title><p>Daily Coaching email is off. Coaching and your digest archive are unchanged.</p>",
        )
            .into_response(),
    )
}

fn valid_unsubscribe_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 512 && token.is_ascii()
}

async fn ingest_digest_email_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(id) = header_value(&headers, "svix-id") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(timestamp) = header_value(&headers, "svix-timestamp") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(signature) = header_value(&headers, "svix-signature") else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match state
        .daily_coaching
        .ingest_digest_email_webhook(
            crate::daily_coaching::WebhookHeaders {
                id,
                timestamp,
                signature,
            },
            &body,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(DigestWebhookError::Invalid) => StatusCode::BAD_REQUEST.into_response(),
        Err(DigestWebhookError::Unavailable | DigestWebhookError::Store(_)) => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

async fn observe_account_email(
    state: &SharedState,
    player_id: &PlayerId,
    observation: &VerifiedAccountEmailObservation,
) -> Result<(), Response> {
    let VerifiedAccountEmailObservation::Observed(email) = observation else {
        return Ok(());
    };
    state
        .daily_coaching
        .observe_verified_email(player_id, email.as_ref())
        .await
        .map_err(|error| {
            tracing::error!(category = "daily_coaching_email", %error, "failed to observe verified account email");
            no_store(StatusCode::SERVICE_UNAVAILABLE.into_response())
        })
}

async fn replace_playing_profile(
    player: AuthorizedWebPlayer,
    State(state): State<SharedState>,
    Path(provider): Path<String>,
    Json(request): Json<ReplacePlayingProfileRequest>,
) -> Response {
    let Some(provider) = parse_provider(&provider) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    mutation_response(
        state
            .daily_coaching
            .replace(&player.player_id, provider, request)
            .await,
    )
}

async fn remove_playing_profile(
    player: AuthorizedWebPlayer,
    State(state): State<SharedState>,
    Path(provider): Path<String>,
    Json(request): Json<RemovePlayingProfileRequest>,
) -> Response {
    let Some(provider) = parse_provider(&provider) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    mutation_response(
        state
            .daily_coaching
            .remove(&player.player_id, provider, request)
            .await,
    )
}

async fn check_playing_profile(
    player: AuthorizedWebPlayer,
    State(state): State<SharedState>,
    Path(provider): Path<String>,
    Json(request): Json<CheckPlayingProfileRequest>,
) -> Response {
    let Some(provider) = parse_provider(&provider) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let outcome = state
        .daily_coaching
        .check_profile(&player.player_id, provider, request)
        .await;
    let status = match outcome {
        CheckPlayingProfileOutcome::Reachable { .. } => StatusCode::OK,
        CheckPlayingProfileOutcome::ProfileUnavailable { .. } => StatusCode::NOT_FOUND,
        CheckPlayingProfileOutcome::ProviderUnavailable { .. }
        | CheckPlayingProfileOutcome::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
        CheckPlayingProfileOutcome::Rejected { .. } => StatusCode::CONFLICT,
    };
    no_store((status, Json(outcome)).into_response())
}

async fn set_daily_coaching_enabled(
    player: AuthorizedWebPlayer,
    State(state): State<SharedState>,
    Json(request): Json<SetDailyCoachingEnabledRequest>,
) -> Response {
    mutation_response(
        state
            .daily_coaching
            .set_enabled(&player.player_id, request.enabled)
            .await,
    )
}

fn parse_provider(value: &str) -> Option<DailyCoachingProvider> {
    match value {
        "lichess" => Some(DailyCoachingProvider::Lichess),
        "chessCom" => Some(DailyCoachingProvider::ChessCom),
        _ => None,
    }
}

fn parse_digest_id(value: &str) -> Option<&str> {
    let date = value.strip_prefix("daily-")?;
    if value.len() != "daily-YYYY-MM-DD".len()
        || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err()
    {
        return None;
    }
    Some(value)
}

fn connect_response(outcome: ConnectPlayingProfileOutcome) -> Response {
    let status = match outcome {
        ConnectPlayingProfileOutcome::Completed { .. } => StatusCode::OK,
        ConnectPlayingProfileOutcome::Rejected {
            reason: ConnectPlayingProfileRejectionReason::ProfileNotFound,
        } => StatusCode::NOT_FOUND,
        ConnectPlayingProfileOutcome::Rejected {
            reason: ConnectPlayingProfileRejectionReason::ProviderAlreadyConnected,
        } => StatusCode::CONFLICT,
        ConnectPlayingProfileOutcome::Rejected { .. } => StatusCode::BAD_REQUEST,
        ConnectPlayingProfileOutcome::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
    };
    no_store((status, Json(outcome)).into_response())
}

fn mutation_response(outcome: DailyCoachingMutationOutcome) -> Response {
    let status = match outcome {
        DailyCoachingMutationOutcome::Completed { .. } => StatusCode::OK,
        DailyCoachingMutationOutcome::Rejected {
            reason: DailyCoachingMutationRejectionReason::ProfileNotFound,
        } => StatusCode::NOT_FOUND,
        DailyCoachingMutationOutcome::Rejected {
            reason:
                DailyCoachingMutationRejectionReason::ProviderMismatch
                | DailyCoachingMutationRejectionReason::UnparseableProfileUrl
                | DailyCoachingMutationRejectionReason::UnsupportedProvider,
        } => StatusCode::BAD_REQUEST,
        DailyCoachingMutationOutcome::Rejected { .. } => StatusCode::CONFLICT,
        DailyCoachingMutationOutcome::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
    };
    no_store((status, Json(outcome)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_paths_are_exact() {
        assert_eq!(
            parse_provider("lichess"),
            Some(DailyCoachingProvider::Lichess)
        );
        assert_eq!(
            parse_provider("chessCom"),
            Some(DailyCoachingProvider::ChessCom)
        );
        assert_eq!(parse_provider("chess.com"), None);
    }

    #[test]
    fn digest_paths_are_canonical_daily_dates() {
        assert_eq!(
            parse_digest_id("daily-2026-08-09"),
            Some("daily-2026-08-09")
        );
        for malformed in [
            "daily-%",
            "daily- ",
            ".",
            "..",
            "daily-2026-8-09",
            "daily-2026-02-30",
            "daily-999999999999999999999999999999999999999999999999",
        ] {
            assert_eq!(parse_digest_id(malformed), None, "{malformed}");
        }
    }
}
