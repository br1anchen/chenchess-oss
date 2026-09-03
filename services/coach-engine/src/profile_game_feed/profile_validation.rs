use std::{future::Future, pin::Pin};

use serde::{Deserialize, Serialize};

use super::{
    ProfileGameClient, ProfileGameFeed, ProfileGameFetchError, ProfileGameRequest,
    ProfileGameResponse, JSON_MEDIA_TYPE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChessProfileProvider {
    Lichess,
    ChessCom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicChessProfile {
    provider: ChessProfileProvider,
    username: String,
}

impl PublicChessProfile {
    pub fn parse(url: &str) -> Result<Self, ProfileUrlError> {
        if url.contains(['?', '#']) {
            return Err(ProfileUrlError::UnparseableProfileUrl);
        }
        let Some(authority_and_path) = url.strip_prefix("https://") else {
            return Err(ProfileUrlError::UnparseableProfileUrl);
        };
        let Some((authority, _)) = authority_and_path.split_once('/') else {
            return Err(ProfileUrlError::UnparseableProfileUrl);
        };
        match authority {
            "lichess.org" | "www.chess.com" => {}
            "www.lichess.org" | "chess.com" => {
                return Err(ProfileUrlError::UnparseableProfileUrl);
            }
            _ => return Err(ProfileUrlError::UnsupportedProvider),
        }

        let url = url.strip_suffix('/').unwrap_or(url);
        let (provider, username) =
            if let Some(profile_path) = url.strip_prefix("https://lichess.org/@/") {
                (
                    ChessProfileProvider::Lichess,
                    profile_path.strip_suffix("/all").unwrap_or(profile_path),
                )
            } else if let Some(username) = url.strip_prefix("https://www.chess.com/member/") {
                (ChessProfileProvider::ChessCom, username)
            } else {
                return Err(ProfileUrlError::UnparseableProfileUrl);
            };
        if username.is_empty()
            || username.len() > 50
            || !username
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(ProfileUrlError::UnparseableProfileUrl);
        }
        Ok(Self {
            provider,
            username: username.to_string(),
        })
    }

    pub fn provider(&self) -> ChessProfileProvider {
        self.provider
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn identity_username(&self) -> String {
        self.username.to_ascii_lowercase()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProfileUrlError {
    #[error("public playing profile URL uses an unsupported provider")]
    UnsupportedProvider,
    #[error("public playing profile URL is not in an accepted form")]
    UnparseableProfileUrl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPublicChessProfile {
    provider: ChessProfileProvider,
    identity_username: String,
    username: String,
    canonical_url: String,
}

impl ValidatedPublicChessProfile {
    pub fn from_provider_username(
        provider: ChessProfileProvider,
        username: &str,
    ) -> Result<Self, ProfileUrlError> {
        let canonical_url = match provider {
            ChessProfileProvider::Lichess => format!("https://lichess.org/@/{username}"),
            ChessProfileProvider::ChessCom => {
                format!("https://www.chess.com/member/{username}")
            }
        };
        let parsed = PublicChessProfile::parse(&canonical_url)?;
        Ok(Self {
            provider,
            identity_username: parsed.identity_username(),
            username: username.to_string(),
            canonical_url,
        })
    }

    pub fn provider(&self) -> ChessProfileProvider {
        self.provider
    }

    pub fn identity_username(&self) -> &str {
        &self.identity_username
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn canonical_url(&self) -> &str {
        &self.canonical_url
    }
}

pub type ProfileValidationFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<ValidatedPublicChessProfile, ProfileValidationError>>
            + Send
            + 'a,
    >,
>;

pub trait PublicProfileValidator: Send + Sync {
    fn validate<'a>(&'a self, profile: &'a PublicChessProfile) -> ProfileValidationFuture<'a>;
}

impl<C> ProfileGameFeed<C>
where
    C: ProfileGameClient,
{
    pub async fn validate_profile(
        &self,
        profile: &PublicChessProfile,
    ) -> Result<ValidatedPublicChessProfile, ProfileValidationError> {
        let _request_guard = self.request_gate.lock().await;
        let request = match profile.provider() {
            ChessProfileProvider::Lichess => ProfileGameRequest::lichess_profile(profile),
            ChessProfileProvider::ChessCom => ProfileGameRequest::chess_com_profile(profile),
        };
        let response = match self.client.fetch(&request).await {
            Ok(response) => response,
            Err(ProfileGameFetchError::Status { code: 404, .. }) => {
                return Err(ProfileValidationError::ProfileNotFound);
            }
            Err(ProfileGameFetchError::Status {
                retry_after_seconds,
                ..
            }) => {
                return Err(ProfileValidationError::ProviderUnavailable {
                    retry_after_seconds,
                });
            }
            Err(error) => return Err(ProfileValidationError::Fetch(error)),
        };
        require_validation_content_type(&response)?;
        let (username, closed) = match profile.provider() {
            ChessProfileProvider::Lichess => {
                let validated: LichessPublicProfile = serde_json::from_slice(&response.body)
                    .map_err(|_| ProfileValidationError::MalformedProviderResponse)?;
                (validated.username, false)
            }
            ChessProfileProvider::ChessCom => {
                let validated: ChessComPublicProfile = serde_json::from_slice(&response.body)
                    .map_err(|_| ProfileValidationError::MalformedProviderResponse)?;
                (
                    validated.username,
                    matches!(
                        validated.status.as_deref(),
                        Some("closed" | "closed:fair_play_violations")
                    ),
                )
            }
        };
        if closed {
            return Err(ProfileValidationError::ProfileNotFound);
        }
        if !username.eq_ignore_ascii_case(profile.username()) {
            return Err(ProfileValidationError::MalformedProviderResponse);
        }
        let canonical_url = match profile.provider() {
            ChessProfileProvider::Lichess => format!("https://lichess.org/@/{username}"),
            ChessProfileProvider::ChessCom => {
                format!("https://www.chess.com/member/{username}")
            }
        };
        Ok(ValidatedPublicChessProfile {
            provider: profile.provider(),
            identity_username: username.to_ascii_lowercase(),
            username,
            canonical_url,
        })
    }
}

impl<C> PublicProfileValidator for ProfileGameFeed<C>
where
    C: ProfileGameClient,
{
    fn validate<'a>(&'a self, profile: &'a PublicChessProfile) -> ProfileValidationFuture<'a> {
        Box::pin(self.validate_profile(profile))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileValidationError {
    #[error("public playing profile was not found")]
    ProfileNotFound,
    #[error("public playing profile provider is unavailable")]
    ProviderUnavailable { retry_after_seconds: Option<u32> },
    #[error(transparent)]
    Fetch(#[from] ProfileGameFetchError),
    #[error("public playing profile provider returned malformed or contradictory data")]
    MalformedProviderResponse,
}

fn require_validation_content_type(
    response: &ProfileGameResponse,
) -> Result<(), ProfileValidationError> {
    if response
        .content_type
        .split(';')
        .next()
        .is_some_and(|actual| actual.trim().eq_ignore_ascii_case(JSON_MEDIA_TYPE))
    {
        Ok(())
    } else {
        Err(ProfileValidationError::MalformedProviderResponse)
    }
}

#[derive(Debug, Deserialize)]
struct LichessPublicProfile {
    username: String,
}

#[derive(Debug, Deserialize)]
struct ChessComPublicProfile {
    username: String,
    #[serde(default)]
    status: Option<String>,
}
