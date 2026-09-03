use std::{sync::Arc, time::Duration};

use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};

use crate::{
    firestore::{
        ServiceAccountTokenSource, ACCOUNT_LIFECYCLE_SERVICE_ACCOUNT_ENV, IDENTITY_TOOLKIT_SCOPE,
    },
    review_session_contract::PlayerId,
};

use super::{required_env, unix_timestamp, AccountDeletionError};

const IDENTITY_TOOLKIT_ENDPOINT: &str = "https://identitytoolkit.googleapis.com/v1";
const RESPONSE_LIMIT_BYTES: u64 = 64 * 1024;

pub(super) struct FirebaseIdentityAdmin {
    client: Client,
    project_id: String,
    endpoint: String,
    token_source: Arc<ServiceAccountTokenSource>,
}

impl FirebaseIdentityAdmin {
    pub(super) fn from_env() -> Result<Self, AccountDeletionError> {
        let project_id = required_env("FIREBASE_PROJECT_ID")?;
        let service_account = required_env(ACCOUNT_LIFECYCLE_SERVICE_ACCOUNT_ENV)?;
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| {
                AccountDeletionError::Configuration(
                    "could not construct the Firebase Auth HTTP client".to_string(),
                )
            })?;
        let token_source = ServiceAccountTokenSource::new_with_scope(
            &project_id,
            &service_account,
            ACCOUNT_LIFECYCLE_SERVICE_ACCOUNT_ENV,
            IDENTITY_TOOLKIT_SCOPE,
            client.clone(),
        )?;
        Ok(Self {
            client,
            project_id,
            endpoint: IDENTITY_TOOLKIT_ENDPOINT.to_string(),
            token_source: Arc::new(token_source),
        })
    }

    pub(super) async fn revoke_refresh_tokens(
        &self,
        player_id: &PlayerId,
    ) -> Result<(), AccountDeletionError> {
        let valid_since = unix_timestamp()?.to_string();
        let response = self
            .client
            .post(format!("{}/accounts:update", self.endpoint))
            .bearer_auth(self.token_source.access_token().await?)
            .json(&UpdateAccountRequest {
                local_id: player_id.as_str(),
                target_project_id: &self.project_id,
                valid_since: &valid_since,
            })
            .send()
            .await
            .map_err(transport_error)?;
        require_success(response, false).await
    }

    pub(super) async fn delete_identity(
        &self,
        player_id: &PlayerId,
    ) -> Result<(), AccountDeletionError> {
        let response = self
            .client
            .post(format!(
                "{}/projects/{}/accounts:delete",
                self.endpoint, self.project_id
            ))
            .bearer_auth(self.token_source.access_token().await?)
            .json(&DeleteAccountRequest {
                local_id: player_id.as_str(),
            })
            .send()
            .await
            .map_err(transport_error)?;
        require_success(response, true).await
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAccountRequest<'a> {
    local_id: &'a str,
    target_project_id: &'a str,
    valid_since: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteAccountRequest<'a> {
    local_id: &'a str,
}

#[derive(Deserialize)]
struct IdentityToolkitErrorEnvelope {
    error: IdentityToolkitError,
}

#[derive(Deserialize)]
struct IdentityToolkitError {
    message: String,
}

async fn require_success(
    mut response: Response,
    user_not_found_is_success: bool,
) -> Result<(), AccountDeletionError> {
    if response.status().is_success() {
        return Ok(());
    }
    if response
        .content_length()
        .is_some_and(|size| size > RESPONSE_LIMIT_BYTES)
    {
        return Err(AccountDeletionError::Unavailable);
    }
    let status = response.status();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if body.len().saturating_add(chunk.len()) as u64 > RESPONSE_LIMIT_BYTES {
            return Err(AccountDeletionError::Unavailable);
        }
        body.extend_from_slice(&chunk);
    }
    let user_not_found = serde_json::from_slice::<IdentityToolkitErrorEnvelope>(&body)
        .is_ok_and(|error| error.error.message == "USER_NOT_FOUND");
    if user_not_found_is_success && user_not_found {
        return Ok(());
    }
    if status.is_server_error() || status.as_u16() == 429 {
        Err(AccountDeletionError::Unavailable)
    } else {
        Err(AccountDeletionError::InvalidRecord)
    }
}

fn transport_error(error: reqwest::Error) -> AccountDeletionError {
    if error.is_timeout() || error.is_connect() {
        AccountDeletionError::Unavailable
    } else {
        AccountDeletionError::Transport
    }
}
