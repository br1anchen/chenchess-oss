use std::time::Duration;

use reqwest::Client;
use serde::Serialize;

use crate::review_session_contract::PlayerId;

use super::{required_env, AccountDeletionError};

const INTERNAL_TOKEN_ENV: &str = "COACH_ACCOUNT_LIFECYCLE_INTERNAL_TOKEN";
const INTERNAL_BASE_URL_ENV: &str = "COACH_OAUTH_INTERNAL_BASE_URL";

pub(super) struct OAuthGrantRevoker {
    client: Client,
    endpoint: String,
    internal_token: String,
}

impl OAuthGrantRevoker {
    pub(super) fn from_env() -> Result<Self, AccountDeletionError> {
        let base_url = required_env(INTERNAL_BASE_URL_ENV)?;
        let url = reqwest::Url::parse(&base_url).map_err(|_| {
            AccountDeletionError::Configuration(format!(
                "{INTERNAL_BASE_URL_ENV} must be a valid URL"
            ))
        })?;
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
        let railway_private = url
            .host_str()
            .is_some_and(|host| host.ends_with(".railway.internal"));
        if !matches!(url.scheme(), "http" | "https")
            || (!loopback && !railway_private)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AccountDeletionError::Configuration(format!(
                "{INTERNAL_BASE_URL_ENV} must be a loopback or Railway-private origin"
            )));
        }
        let internal_token = required_env(INTERNAL_TOKEN_ENV)?;
        if internal_token.len() < 32 || !internal_token.is_ascii() {
            return Err(AccountDeletionError::Configuration(format!(
                "{INTERNAL_TOKEN_ENV} must be at least 32 ASCII characters"
            )));
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| {
                AccountDeletionError::Configuration(
                    "could not construct the OAuth lifecycle HTTP client".to_string(),
                )
            })?;
        Ok(Self {
            client,
            endpoint: format!(
                "{}/internal/v1/account-deletion/oauth-grants",
                base_url.trim_end_matches('/')
            ),
            internal_token,
        })
    }

    pub(super) async fn revoke_all(
        &self,
        player_id: &PlayerId,
    ) -> Result<(), AccountDeletionError> {
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.internal_token)
            .json(&RevokeOAuthGrantsRequest {
                player_id: player_id.as_str(),
            })
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() || error.is_connect() {
                    AccountDeletionError::Unavailable
                } else {
                    AccountDeletionError::Transport
                }
            })?;
        if response.status().is_success() {
            Ok(())
        } else if response.status().is_server_error() || response.status().as_u16() == 429 {
            Err(AccountDeletionError::Unavailable)
        } else {
            Err(AccountDeletionError::InvalidRecord)
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RevokeOAuthGrantsRequest<'a> {
    player_id: &'a str,
}
