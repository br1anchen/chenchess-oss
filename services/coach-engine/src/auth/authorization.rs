use axum::{async_trait, extract::FromRequestParts, http::request::Parts};

use crate::{
    beta_access::{BetaAccessAuthorizationError, NormalizedEmail},
    review_session_contract::PlayerId,
    types::SharedState,
};

use super::{
    account_deletion_auth_error, bearer_token, AuthError, AuthenticationPurpose,
    VerifiedAccountEmailObservation,
};

#[derive(Debug, Clone)]
pub struct AuthorizedPlayer {
    pub player_id: PlayerId,
    pub(crate) verified_email: VerifiedAccountEmailObservation,
}

#[async_trait]
impl FromRequestParts<SharedState> for AuthorizedPlayer {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let player = state.auth.authenticate_token(bearer_token(parts)?).await?;
        if player.is_firebase_conformance() {
            return Err(AuthError::InvalidToken);
        }
        state
            .account_deletion
            .ensure_player_active(&player.player_id)
            .await
            .map_err(account_deletion_auth_error)?;
        ensure_beta_access(state, &player.player_id, player.purpose()).await?;
        let verified_email = player.verified_email_observation();
        Ok(Self {
            player_id: player.player_id,
            verified_email,
        })
    }
}

/// A Player acting through the web application in person.
///
/// [`AuthorizedPlayer`] admits both bearer profiles, which is right for
/// coaching operations: the Coach App acts for the Player and the Coach Engine
/// does not care which surface asked. Dashboard-only reads and deliberate
/// account controls — including Review Share grants and digest-email choices —
/// must not work that way. Requiring the Firebase identity keeps those surfaces
/// off the model-visible tool boundary by construction.
#[derive(Debug, Clone)]
pub struct AuthorizedWebPlayer {
    pub player_id: PlayerId,
    pub(crate) verified_email: VerifiedAccountEmailObservation,
}

#[async_trait]
impl FromRequestParts<SharedState> for AuthorizedWebPlayer {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let player = super::AuthenticatedFirebasePlayer::from_request_parts(parts, state).await?;
        ensure_beta_access(state, &player.player_id, player.purpose()).await?;
        let verified_email = player
            .email_verified
            .then(|| {
                player
                    .email
                    .as_deref()
                    .and_then(|email| NormalizedEmail::parse(email).ok())
            })
            .flatten();
        Ok(Self {
            player_id: player.player_id,
            verified_email: VerifiedAccountEmailObservation::Observed(verified_email),
        })
    }
}

pub(crate) async fn ensure_beta_access(
    state: &SharedState,
    player_id: &PlayerId,
    purpose: AuthenticationPurpose,
) -> Result<(), AuthError> {
    if state.auth.bypasses_beta_access(player_id, purpose) {
        return Ok(());
    }
    state
        .beta_access
        .require_access(player_id)
        .await
        .map_err(|error| match error {
            BetaAccessAuthorizationError::Required => AuthError::BetaAccessRequired,
            BetaAccessAuthorizationError::Unavailable => AuthError::AuthenticationUnavailable,
            BetaAccessAuthorizationError::Store(error) => {
                tracing::error!(category = "beta_access", %error, "beta access authorization failed");
                AuthError::AuthenticationUnavailable
            }
        })
}
