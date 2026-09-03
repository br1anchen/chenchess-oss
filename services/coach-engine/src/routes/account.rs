use axum::{body::Bytes, extract::State, response::Response, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{
    account_deletion::{AccountDeletionError, ACCOUNT_DELETION_CONFIRMATION},
    auth::FirebaseAccountDeletionPrincipal,
    types::SharedState,
};

const MAX_ACCOUNT_DELETION_BODY_BYTES: usize = 1024;

pub(crate) fn router() -> Router<SharedState> {
    Router::new().route("/api/v1/account/deletion", post(delete_account))
}

async fn delete_account(
    principal: FirebaseAccountDeletionPrincipal,
    State(state): State<SharedState>,
    body: Bytes,
) -> Result<axum::http::StatusCode, AccountDeletionHttpError> {
    if body.len() > MAX_ACCOUNT_DELETION_BODY_BYTES {
        return Err(AccountDeletionHttpError::InvalidRequest);
    }
    let request: DeleteAccountRequest =
        serde_json::from_slice(&body).map_err(|_| AccountDeletionHttpError::InvalidRequest)?;
    state
        .account_deletion
        .delete_account(
            principal.player_id,
            principal.authenticated_at,
            &request.confirmation,
        )
        .await
        .map_err(AccountDeletionHttpError::Deletion)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteAccountRequest {
    confirmation: String,
}

#[derive(Debug)]
enum AccountDeletionHttpError {
    InvalidRequest,
    Deletion(AccountDeletionError),
}

impl axum::response::IntoResponse for AccountDeletionHttpError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::InvalidRequest => (
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid account deletion request".to_string(),
            ),
            Self::Deletion(AccountDeletionError::ConfirmationRequired) => (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Confirmation must exactly equal {ACCOUNT_DELETION_CONFIRMATION}"),
            ),
            Self::Deletion(AccountDeletionError::RecentAuthenticationRequired) => (
                axum::http::StatusCode::UNAUTHORIZED,
                "Recent Firebase authentication required".to_string(),
            ),
            Self::Deletion(AccountDeletionError::UnavailableInEnvironment) => (
                axum::http::StatusCode::NOT_FOUND,
                "Account deletion is unavailable".to_string(),
            ),
            Self::Deletion(_) => (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Account deletion could not complete; retry safely".to_string(),
            ),
        };
        (
            status,
            Json(AccountDeletionErrorResponse { error: message }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct AccountDeletionErrorResponse {
    error: String,
}
