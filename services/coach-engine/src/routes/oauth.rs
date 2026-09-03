use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{
    account_deletion::AccountDeletionError,
    auth::{ensure_beta_access, AuthError, AuthenticationPurpose},
    types::SharedState,
};

const MAX_FIREBASE_ID_TOKEN_BYTES: usize = 16 * 1024;

pub(crate) fn router() -> Router<SharedState> {
    Router::new().route(
        "/internal/v1/oauth/firebase-identity",
        post(verify_firebase_identity),
    )
}

async fn verify_firebase_identity(
    State(state): State<SharedState>,
    Json(request): Json<VerifyFirebaseIdentityRequest>,
) -> Result<Json<VerifiedFirebaseIdentity>, AuthError> {
    if request.firebase_id_token.len() > MAX_FIREBASE_ID_TOKEN_BYTES {
        return Err(AuthError::InvalidToken);
    }
    let player = state
        .auth
        .authenticate_firebase_token(&request.firebase_id_token)
        .await?;
    let authorization_kind = match player.purpose() {
        AuthenticationPurpose::McpConformance => FirebaseAuthorizationKind::McpConformance,
        AuthenticationPurpose::Player
            if player.email_verified
                && matches!(
                    player.sign_in_provider.as_deref(),
                    Some("password" | "google.com")
                ) =>
        {
            FirebaseAuthorizationKind::Player
        }
        AuthenticationPurpose::Player => return Err(AuthError::InvalidToken),
    };
    state
        .account_deletion
        .ensure_player_active(&player.player_id)
        .await
        .map_err(|error| match error {
            AccountDeletionError::AccountDeleting => AuthError::AccountDeleting,
            _ => AuthError::AuthenticationUnavailable,
        })?;
    ensure_beta_access(&state, &player.player_id, player.purpose()).await?;
    Ok(Json(VerifiedFirebaseIdentity {
        authorization_kind,
        player_id: player.player_id.as_str().to_string(),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VerifyFirebaseIdentityRequest {
    firebase_id_token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifiedFirebaseIdentity {
    authorization_kind: FirebaseAuthorizationKind,
    player_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum FirebaseAuthorizationKind {
    Player,
    McpConformance,
}
