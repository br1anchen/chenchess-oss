use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::{
    failure::{oauth_response_error, transport_error, unavailable_error},
    FirestoreError,
};

pub(crate) const FIRESTORE_SCOPE: &str = "https://www.googleapis.com/auth/datastore";
pub(crate) const IDENTITY_TOOLKIT_SCOPE: &str = "https://www.googleapis.com/auth/identitytoolkit";
const GOOGLE_OAUTH_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
pub(crate) struct ServiceAccountTokenSource {
    client: Client,
    identity: ServiceAccountIdentity,
    encoding_key: EncodingKey,
    scope: &'static str,
    cached: Mutex<Option<CachedAccessToken>>,
}

impl ServiceAccountTokenSource {
    pub(crate) fn new(
        expected_project_id: &str,
        service_account_json: &str,
        credential_name: &'static str,
        client: Client,
    ) -> Result<Self, FirestoreError> {
        Self::new_with_scope(
            expected_project_id,
            service_account_json,
            credential_name,
            FIRESTORE_SCOPE,
            client,
        )
    }

    pub(crate) fn new_with_scope(
        expected_project_id: &str,
        service_account_json: &str,
        credential_name: &'static str,
        scope: &'static str,
        client: Client,
    ) -> Result<Self, FirestoreError> {
        if scope.trim().is_empty() {
            return Err(FirestoreError::Configuration(
                "the service-account OAuth scope must not be empty".to_string(),
            ));
        }
        let credentials: ServiceAccountCredentials = serde_json::from_str(service_account_json)
            .map_err(|_| {
                FirestoreError::Configuration(format!(
                    "{credential_name} must contain service-account JSON"
                ))
            })?;
        let ServiceAccountCredentials {
            kind,
            project_id,
            private_key,
            client_email,
            token_uri,
        } = credentials;
        let expected_email_suffix = format!("@{expected_project_id}.iam.gserviceaccount.com");
        if kind != "service_account"
            || project_id != expected_project_id
            || client_email.trim().is_empty()
            || !client_email.ends_with(&expected_email_suffix)
            || token_uri != GOOGLE_OAUTH_TOKEN_ENDPOINT
        {
            return Err(FirestoreError::Configuration(format!(
                "{credential_name} must belong to FIREBASE_PROJECT_ID and use Google's OAuth token endpoint"
            )));
        }
        let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes()).map_err(|_| {
            FirestoreError::Configuration(
                "the Coach Engine service account private key is invalid".to_string(),
            )
        })?;
        Ok(Self {
            client,
            identity: ServiceAccountIdentity {
                client_email,
                token_uri,
            },
            encoding_key,
            scope,
            cached: Mutex::new(None),
        })
    }

    pub(crate) async fn access_token(&self) -> Result<String, FirestoreError> {
        let now = unix_timestamp()?;
        let mut cached = self.cached.lock().await;
        if let Some(token) = cached
            .as_ref()
            .filter(|token| token.expires_at > now.saturating_add(60))
        {
            return Ok(token.value.clone());
        }
        let assertion = encode(
            &Header::new(Algorithm::RS256),
            &ServiceAccountAssertion {
                iss: &self.identity.client_email,
                scope: self.scope,
                aud: &self.identity.token_uri,
                iat: now,
                exp: now.saturating_add(3600),
            },
            &self.encoding_key,
        )
        .map_err(|error| unavailable_error("service_account_assertion", &error))?;
        let response = self
            .client
            .post(&self.identity.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(|error| transport_error("service_account_token", &error))?;
        if !response.status().is_success() {
            return Err(oauth_response_error(response).await);
        }
        let response: AccessTokenResponse = response
            .json()
            .await
            .map_err(|error| unavailable_error("service_account_token_decode", &error))?;
        if !response.token_type.eq_ignore_ascii_case("bearer")
            || response.access_token.trim().is_empty()
        {
            tracing::error!(
                firestore_operation = "service_account_token_decode",
                token_type_is_bearer = response.token_type.eq_ignore_ascii_case("bearer"),
                access_token_present = !response.access_token.trim().is_empty(),
                "Firestore service-account token response was invalid"
            );
            return Err(FirestoreError::Unavailable);
        }
        let expires_at = now.saturating_add(response.expires_in);
        *cached = Some(CachedAccessToken {
            value: response.access_token.clone(),
            expires_at,
        });
        Ok(response.access_token)
    }
}

#[derive(Deserialize)]
struct ServiceAccountCredentials {
    #[serde(rename = "type")]
    kind: String,
    project_id: String,
    private_key: String,
    client_email: String,
    token_uri: String,
}

struct ServiceAccountIdentity {
    client_email: String,
    token_uri: String,
}

#[derive(Serialize)]
struct ServiceAccountAssertion<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    expires_in: u64,
    token_type: String,
}

struct CachedAccessToken {
    value: String,
    expires_at: u64,
}

fn unix_timestamp() -> Result<u64, FirestoreError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| FirestoreError::Unavailable)
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_account_is_bound_to_the_configured_project_and_google_endpoint() {
        let credentials = |project_id: &str, client_email: &str, token_uri: &str| {
            serde_json::json!({
                "type": "service_account",
                "project_id": project_id,
                "private_key": crate::certification_keys::private_key_pem(),
                "client_email": client_email,
                "token_uri": token_uri,
            })
            .to_string()
        };

        assert!(ServiceAccountTokenSource::new(
            "chenchess-test",
            &credentials(
                "chenchess-test",
                "coach-engine@chenchess-test.iam.gserviceaccount.com",
                GOOGLE_OAUTH_TOKEN_ENDPOINT,
            ),
            "TEST_SERVICE_ACCOUNT_JSON",
            Client::new(),
        )
        .is_ok());
        assert!(matches!(
            ServiceAccountTokenSource::new(
                "chenchess-test",
                &credentials(
                    "other-project",
                    "coach-engine@chenchess-test.iam.gserviceaccount.com",
                    GOOGLE_OAUTH_TOKEN_ENDPOINT,
                ),
                "TEST_SERVICE_ACCOUNT_JSON",
                Client::new(),
            ),
            Err(FirestoreError::Configuration(_))
        ));
        assert!(matches!(
            ServiceAccountTokenSource::new(
                "chenchess-test",
                &credentials(
                    "chenchess-test",
                    "coach-engine@chenchess-test.iam.gserviceaccount.com",
                    "https://example.test/token",
                ),
                "TEST_SERVICE_ACCOUNT_JSON",
                Client::new(),
            ),
            Err(FirestoreError::Configuration(_))
        ));
        assert!(matches!(
            ServiceAccountTokenSource::new(
                "chenchess-test",
                &credentials(
                    "chenchess-test",
                    "coach-engine@chenchess-test-other.iam.gserviceaccount.com",
                    GOOGLE_OAUTH_TOKEN_ENDPOINT,
                ),
                "TEST_SERVICE_ACCOUNT_JSON",
                Client::new(),
            ),
            Err(FirestoreError::Configuration(_))
        ));
    }
}
