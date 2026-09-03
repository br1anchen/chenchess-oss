use std::{
    borrow::Cow,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    deployment::DeploymentEnvironment, review_session_contract::PlayerId, types::SharedState,
};
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, decode_header, Algorithm, Validation};
use serde::{Deserialize, Serialize};

use firebase::FirebaseKeySource;

mod authorization;
mod emulator;
mod environment;
mod firebase;
mod keys;
mod mcp_conformance;

pub(crate) use authorization::ensure_beta_access;
pub use authorization::{AuthorizedPlayer, AuthorizedWebPlayer};
use environment::{optional_coach_mcp_environment, optional_env, required_env, required_value};
use keys::{matching_keys, parse_jwks, verification_keys, VerificationKey};
pub(crate) use mcp_conformance::is_mcp_conformance_player_id;
use mcp_conformance::McpConformancePolicy;

const COACH_ACCESS_TOKEN_TTL_SECONDS: u64 = 10 * 60;
const COACH_CLOCK_SKEW_SECONDS: u64 = 5;
const COACH_MCP_VERSIONED_RESOURCE_SUFFIX: &str = "/v2";
#[derive(Clone)]
pub struct AuthConfig {
    firebase: Option<TokenVerifier>,
    coach_mcp: Option<TokenVerifier>,
    mcp_conformance: McpConformancePolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthenticationPurpose {
    Player,
    McpConformance,
}

#[derive(Clone)]
struct TokenVerifier {
    validation: Validation,
    profile: TokenProfile,
}

#[derive(Clone)]
enum TokenProfile {
    FirebaseWeb {
        project_id: String,
        identity: FirebaseIdentity,
    },
    CoachMcp {
        required_scope: String,
        keys: Vec<VerificationKey>,
    },
}

/// Where a Firebase ID token is minted, and therefore whether a signature is
/// demanded of it. Every claim rule is the same on both (ADR 0060).
#[derive(Clone)]
enum FirebaseIdentity {
    /// Google's live JWKS: a `kid` header and an RS256 signature are required.
    Google(FirebaseKeySource),
    /// A Firebase Auth emulator on this machine, which mints unsigned tokens.
    Emulator,
}

impl AuthConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let deployment_environment =
            DeploymentEnvironment::parse(&required_env("DEPLOYMENT_ENVIRONMENT")?)?;
        let mut config = Self::firebase(
            required_value("Firebase project ID", required_env("FIREBASE_PROJECT_ID")?)?,
            firebase_identity(optional_env("FIREBASE_AUTH_EMULATOR_HOST")?)?,
            McpConformancePolicy::for_environment(deployment_environment),
        )?;
        if let Some(coach_mcp) = optional_coach_mcp_environment(
            optional_env("JWT_JWKS")?,
            optional_env("OAUTH_ISSUER")?,
            optional_env("COACH_MCP_RESOURCE")?,
        )? {
            deployment_environment.validate_coach_oauth(&coach_mcp.issuer, &coach_mcp.resource)?;
            config.add_coach_mcp(
                coach_mcp.jwt_jwks,
                coach_mcp.issuer,
                coach_mcp.resource,
                optional_env("COACH_MCP_SCOPE")?.unwrap_or_else(|| "coach:review".to_string()),
            )?;
        }
        Ok(config)
    }

    pub fn new_firebase(
        project_id: impl Into<String>,
        firebase_jwks: impl AsRef<str>,
    ) -> anyhow::Result<Self> {
        let project_id = required_value("Firebase project ID", project_id.into())?;
        let jwks = parse_jwks(firebase_jwks.as_ref(), "Firebase JWKS")?;
        Self::firebase(
            project_id,
            FirebaseIdentity::Google(FirebaseKeySource::static_keys(verification_keys(&jwks)?)),
            McpConformancePolicy::Disabled,
        )
    }

    fn firebase(
        project_id: String,
        identity: FirebaseIdentity,
        mcp_conformance: McpConformancePolicy,
    ) -> anyhow::Result<Self> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_required_spec_claims(&["exp", "iat", "iss", "aud", "sub", "auth_time"]);
        validation.set_issuer(&[format!("https://securetoken.google.com/{project_id}")]);
        validation.set_audience(std::slice::from_ref(&project_id));
        if matches!(identity, FirebaseIdentity::Emulator) {
            // The one relaxation: an emulator token's signature segment is
            // empty. Issuer, audience, expiry, `iat`, `auth_time`, and `sub`
            // are read by this same `Validation`.
            validation.insecure_disable_signature_validation();
        }
        Ok(Self {
            firebase: Some(TokenVerifier {
                validation,
                profile: TokenProfile::FirebaseWeb {
                    project_id,
                    identity,
                },
            }),
            coach_mcp: None,
            mcp_conformance,
        })
    }

    pub fn new_coach_mcp(
        jwt_jwks: impl AsRef<str>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
        required_scope: impl Into<String>,
    ) -> anyhow::Result<Self> {
        let mut config = Self {
            firebase: None,
            coach_mcp: None,
            mcp_conformance: McpConformancePolicy::Disabled,
        };
        config.add_coach_mcp(jwt_jwks, issuer, audience, required_scope)?;
        Ok(config)
    }

    pub fn with_coach_mcp(
        mut self,
        jwt_jwks: impl AsRef<str>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
        required_scope: impl Into<String>,
    ) -> anyhow::Result<Self> {
        self.add_coach_mcp(jwt_jwks, issuer, audience, required_scope)?;
        Ok(self)
    }

    fn add_coach_mcp(
        &mut self,
        jwt_jwks: impl AsRef<str>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
        required_scope: impl Into<String>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.coach_mcp.is_none(),
            "Coach MCP authentication is already configured"
        );
        let required_scope = required_scope.into();
        anyhow::ensure!(
            !required_scope.trim().is_empty(),
            "Coach MCP required scope must not be empty"
        );
        let jwks = parse_jwks(jwt_jwks.as_ref(), "JWT_JWKS")?;
        let keys = verification_keys(&jwks)?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = COACH_CLOCK_SKEW_SECONDS;
        validation.set_required_spec_claims(&["exp", "iat", "iss", "aud", "sub", "scope", "jti"]);
        validation.set_issuer(&[issuer.into()]);
        let audience = audience.into();
        let versioned_audience = format!("{audience}{COACH_MCP_VERSIONED_RESOURCE_SUFFIX}");
        validation.set_audience(&[audience, versioned_audience]);
        self.coach_mcp = Some(TokenVerifier {
            validation,
            profile: TokenProfile::CoachMcp {
                required_scope,
                keys,
            },
        });
        Ok(())
    }

    async fn authenticate_with(
        &self,
        verifier: &TokenVerifier,
        token: &str,
    ) -> Result<AuthenticatedPlayer, AuthError> {
        let decodable = decodable_token(&verifier.profile, token).await?;
        if decodable.candidate_keys.is_empty() {
            return Err(AuthError::InvalidToken);
        }

        let mut errors = Vec::with_capacity(decodable.candidate_keys.len());

        for key in decodable.candidate_keys {
            match decode::<AuthTokenClaims>(
                &decodable.token,
                &key.decoding_key,
                &verifier.validation,
            ) {
                Ok(data) => {
                    return validate_claims(data.claims, &verifier.profile, self.mcp_conformance);
                }
                Err(error) => errors.push(error),
            }
        }

        tracing::debug!(
            ?errors,
            "Auth Token verification failed for every configured key"
        );
        Err(AuthError::InvalidToken)
    }

    pub async fn authenticate_token(&self, token: &str) -> Result<AuthenticatedPlayer, AuthError> {
        // An emulator ID token has no header `jsonwebtoken` can read, and the
        // Firebase profile is the only one that answers for it.
        let coach_mcp_bearer =
            decode_header(token).is_ok_and(|header| header.typ.as_deref() == Some("at+jwt"));
        let verifier = if coach_mcp_bearer {
            self.coach_mcp.as_ref()
        } else {
            self.firebase.as_ref()
        }
        .ok_or(AuthError::InvalidToken)?;
        self.authenticate_with(verifier, token).await
    }

    pub async fn authenticate_firebase_token(
        &self,
        token: &str,
    ) -> Result<AuthenticatedFirebasePlayer, AuthError> {
        let verifier = self.firebase.as_ref().ok_or(AuthError::InvalidToken)?;
        self.authenticate_with(verifier, token)
            .await?
            .into_firebase()
    }

    pub(crate) fn bypasses_beta_access(
        &self,
        player_id: &PlayerId,
        purpose: AuthenticationPurpose,
    ) -> bool {
        self.mcp_conformance == McpConformancePolicy::Staging
            && purpose == AuthenticationPurpose::McpConformance
            && is_mcp_conformance_player_id(player_id.as_str())
    }

    #[cfg(test)]
    pub(crate) fn with_mcp_conformance_for_test(
        mut self,
        environment: DeploymentEnvironment,
    ) -> Self {
        self.mcp_conformance = McpConformancePolicy::for_environment(environment);
        self
    }
}

/// A token `jsonwebtoken` can read, and the keys that could carry its
/// signature.
struct DecodableToken<'a> {
    token: Cow<'a, str>,
    candidate_keys: Vec<VerificationKey>,
}

/// Only the emulator profile rewrites the token, and only its unreadable
/// `alg: none` header.
async fn decodable_token<'a>(
    profile: &TokenProfile,
    token: &'a str,
) -> Result<DecodableToken<'a>, AuthError> {
    match profile {
        TokenProfile::FirebaseWeb {
            identity: FirebaseIdentity::Emulator,
            ..
        } => Ok(DecodableToken {
            token: Cow::Owned(
                emulator::readable_emulator_token(token).ok_or(AuthError::InvalidToken)?,
            ),
            candidate_keys: vec![VerificationKey::unread()],
        }),
        TokenProfile::FirebaseWeb {
            identity: FirebaseIdentity::Google(keys),
            ..
        } => {
            let header = decode_header(token).map_err(|_| AuthError::InvalidToken)?;
            let kid = header.kid.as_deref().ok_or(AuthError::InvalidToken)?;
            Ok(DecodableToken {
                token: Cow::Borrowed(token),
                candidate_keys: keys.candidates(kid).await?,
            })
        }
        TokenProfile::CoachMcp { keys, .. } => {
            let header = decode_header(token).map_err(|_| AuthError::InvalidToken)?;
            if header.typ.as_deref() != Some("at+jwt") {
                return Err(AuthError::InvalidToken);
            }
            let kid = header.kid.as_deref().ok_or(AuthError::InvalidToken)?;
            Ok(DecodableToken {
                token: Cow::Borrowed(token),
                candidate_keys: matching_keys(keys, kid),
            })
        }
    }
}

fn firebase_identity(emulator_host: Option<String>) -> anyhow::Result<FirebaseIdentity> {
    let Some(host) = emulator_host else {
        return Ok(FirebaseIdentity::Google(FirebaseKeySource::remote()?));
    };
    emulator::loopback_emulator_host(&host)?;
    Ok(FirebaseIdentity::Emulator)
}

fn validate_claims(
    claims: AuthTokenClaims,
    profile: &TokenProfile,
    mcp_conformance: McpConformancePolicy,
) -> Result<AuthenticatedPlayer, AuthError> {
    match profile {
        TokenProfile::CoachMcp { required_scope, .. } => {
            let now = unix_timestamp().map_err(|_| AuthError::InvalidToken)?;
            if claims.iat.is_none_or(|iat| {
                iat > now.saturating_add(COACH_CLOCK_SKEW_SECONDS)
                    || claims.exp.checked_sub(iat) != Some(COACH_ACCESS_TOKEN_TTL_SECONDS)
            }) || claims
                .jti
                .as_deref()
                .is_none_or(|jti| jti.trim().is_empty())
                || !claims.has_scope(required_scope)
            {
                return Err(AuthError::InvalidToken);
            }
        }
        TokenProfile::FirebaseWeb { project_id, .. } => {
            let now = unix_timestamp().map_err(|_| AuthError::InvalidToken)?;
            if project_id.trim().is_empty()
                || claims.iat.is_none_or(|iat| iat > now)
                || claims.auth_time.is_none_or(|auth_time| auth_time > now)
            {
                return Err(AuthError::InvalidToken);
            }
        }
    }
    let player_id = PlayerId::try_from(claims.sub).map_err(|_| AuthError::InvalidToken)?;
    let purpose = match profile {
        TokenProfile::FirebaseWeb { .. } => mcp_conformance.firebase_purpose(
            &player_id,
            claims
                .firebase
                .as_ref()
                .and_then(|firebase| firebase.sign_in_provider.as_deref()),
            claims.chenchess_mcp_conformance,
        )?,
        TokenProfile::CoachMcp { .. } => {
            mcp_conformance.coach_purpose(&player_id, claims.chenchess_mcp_conformance)?
        }
    };
    let authentication = match profile {
        TokenProfile::FirebaseWeb { .. } => AuthenticatedPlayerProfile::Firebase {
            authenticated_at: claims.auth_time.ok_or(AuthError::InvalidToken)?,
            chenchess_admin: claims.chenchess_admin.unwrap_or(false),
            email: claims.email,
            email_verified: claims.email_verified.unwrap_or(false),
            purpose,
            sign_in_provider: claims
                .firebase
                .and_then(|firebase| firebase.sign_in_provider),
        },
        TokenProfile::CoachMcp { .. } => AuthenticatedPlayerProfile::CoachMcp { purpose },
    };
    Ok(AuthenticatedPlayer {
        player_id,
        authentication,
    })
}

#[derive(Debug, Clone)]
pub struct AuthenticatedPlayer {
    pub player_id: PlayerId,
    authentication: AuthenticatedPlayerProfile,
}

#[derive(Debug, Clone)]
pub(crate) enum VerifiedAccountEmailObservation {
    NotObserved,
    Observed(Option<crate::beta_access::NormalizedEmail>),
}

impl VerifiedAccountEmailObservation {
    pub(crate) fn email(&self) -> Option<&crate::beta_access::NormalizedEmail> {
        match self {
            Self::NotObserved | Self::Observed(None) => None,
            Self::Observed(Some(email)) => Some(email),
        }
    }
}

impl AuthenticatedPlayer {
    fn verified_email_observation(&self) -> VerifiedAccountEmailObservation {
        match &self.authentication {
            AuthenticatedPlayerProfile::Firebase {
                email: Some(email),
                email_verified: true,
                ..
            } => VerifiedAccountEmailObservation::Observed(
                crate::beta_access::NormalizedEmail::parse(email).ok(),
            ),
            AuthenticatedPlayerProfile::Firebase { .. } => {
                VerifiedAccountEmailObservation::Observed(None)
            }
            AuthenticatedPlayerProfile::CoachMcp { .. } => {
                VerifiedAccountEmailObservation::NotObserved
            }
        }
    }

    fn into_firebase(self) -> Result<AuthenticatedFirebasePlayer, AuthError> {
        match self.authentication {
            AuthenticatedPlayerProfile::Firebase {
                authenticated_at,
                chenchess_admin,
                email,
                email_verified,
                purpose,
                sign_in_provider,
            } => Ok(AuthenticatedFirebasePlayer {
                player_id: self.player_id,
                authenticated_at,
                chenchess_admin,
                email,
                email_verified,
                purpose,
                sign_in_provider,
            }),
            AuthenticatedPlayerProfile::CoachMcp { .. } => Err(AuthError::InvalidToken),
        }
    }

    fn purpose(&self) -> AuthenticationPurpose {
        match &self.authentication {
            AuthenticatedPlayerProfile::Firebase { purpose, .. }
            | AuthenticatedPlayerProfile::CoachMcp { purpose } => *purpose,
        }
    }

    fn is_firebase_conformance(&self) -> bool {
        matches!(
            &self.authentication,
            AuthenticatedPlayerProfile::Firebase {
                purpose: AuthenticationPurpose::McpConformance,
                ..
            }
        )
    }
}

#[derive(Debug, Clone)]
enum AuthenticatedPlayerProfile {
    Firebase {
        authenticated_at: u64,
        chenchess_admin: bool,
        email: Option<String>,
        email_verified: bool,
        purpose: AuthenticationPurpose,
        sign_in_provider: Option<String>,
    },
    CoachMcp {
        purpose: AuthenticationPurpose,
    },
}

pub struct AuthenticatedFirebasePlayer {
    pub player_id: PlayerId,
    pub authenticated_at: u64,
    pub chenchess_admin: bool,
    pub email: Option<String>,
    pub email_verified: bool,
    purpose: AuthenticationPurpose,
    pub sign_in_provider: Option<String>,
}

impl AuthenticatedFirebasePlayer {
    pub(crate) fn purpose(&self) -> AuthenticationPurpose {
        self.purpose
    }
}

#[async_trait]
impl FromRequestParts<SharedState> for AuthenticatedFirebasePlayer {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let player = authenticate_firebase_request(parts, state).await?;
        state
            .account_deletion
            .ensure_player_active(&player.player_id)
            .await
            .map_err(account_deletion_auth_error)?;
        Ok(player)
    }
}

#[derive(Debug, Clone)]
pub struct FirebaseAccountDeletionPrincipal {
    pub player_id: crate::review_session_contract::PlayerId,
    pub authenticated_at: u64,
}

#[derive(Debug, Clone)]
pub struct FirebaseAdministrator;

#[async_trait]
impl FromRequestParts<SharedState> for FirebaseAdministrator {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let player = authenticate_firebase_request(parts, state).await?;
        if !player.email_verified || !player.chenchess_admin {
            return Err(AuthError::InvalidToken);
        }
        state
            .account_deletion
            .ensure_player_active(&player.player_id)
            .await
            .map_err(account_deletion_auth_error)?;
        Ok(Self)
    }
}

#[async_trait]
impl FromRequestParts<SharedState> for FirebaseAccountDeletionPrincipal {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let player = authenticate_firebase_request(parts, state).await?;
        Ok(Self {
            player_id: player.player_id,
            authenticated_at: player.authenticated_at,
        })
    }
}

async fn authenticate_firebase_request(
    parts: &Parts,
    state: &SharedState,
) -> Result<AuthenticatedFirebasePlayer, AuthError> {
    let player = state
        .auth
        .authenticate_firebase_token(bearer_token(parts)?)
        .await?;
    if player.purpose() != AuthenticationPurpose::Player {
        return Err(AuthError::InvalidToken);
    }
    Ok(player)
}

fn bearer_token(parts: &Parts) -> Result<&str, AuthError> {
    let value = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .ok_or(AuthError::MissingToken)?
        .to_str()
        .map_err(|_| AuthError::InvalidToken)?;

    let mut parts = value.split_ascii_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some(scheme), Some(token), None) if scheme.eq_ignore_ascii_case("bearer") => Ok(token),
        _ => Err(AuthError::InvalidToken),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Missing bearer Auth Token")]
    MissingToken,
    #[error("Invalid Auth Token")]
    InvalidToken,
    #[error("The Player account is being deleted")]
    AccountDeleting,
    #[error("Beta Access is required")]
    BetaAccessRequired,
    #[error("Authentication state is temporarily unavailable")]
    AuthenticationUnavailable,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let body = Json(AuthErrorResponse {
            error: self.to_string(),
        });

        let status = match self {
            Self::MissingToken | Self::InvalidToken => StatusCode::UNAUTHORIZED,
            Self::AccountDeleting | Self::BetaAccessRequired => StatusCode::FORBIDDEN,
            Self::AuthenticationUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, body).into_response()
    }
}

#[derive(Deserialize)]
struct AuthTokenClaims {
    sub: String,
    exp: u64,
    #[serde(default)]
    iat: Option<u64>,
    #[serde(default)]
    auth_time: Option<u64>,
    #[serde(default)]
    jti: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default, rename = "chenchessAdmin")]
    chenchess_admin: Option<bool>,
    #[serde(default, rename = "chenchessMcpConformance")]
    chenchess_mcp_conformance: Option<bool>,
    #[serde(default)]
    firebase: Option<FirebaseTokenClaims>,
}

#[derive(Deserialize)]
struct FirebaseTokenClaims {
    #[serde(default)]
    sign_in_provider: Option<String>,
}

impl AuthTokenClaims {
    fn has_scope(&self, required_scope: &str) -> bool {
        self.scope
            .as_deref()
            .unwrap_or_default()
            .split_ascii_whitespace()
            .any(|scope| scope == required_scope)
    }
}

#[derive(Serialize)]
struct AuthErrorResponse {
    error: String,
}

fn account_deletion_auth_error(error: crate::account_deletion::AccountDeletionError) -> AuthError {
    match error {
        crate::account_deletion::AccountDeletionError::AccountDeleting => {
            AuthError::AccountDeleting
        }
        _ => AuthError::AuthenticationUnavailable,
    }
}

fn unix_timestamp() -> Result<u64, std::time::SystemTimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::Serialize;
    use serde_json::Value;

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    use super::{
        environment::required_coach_mcp_environment, firebase_identity,
        optional_coach_mcp_environment, AuthConfig, AuthError, FirebaseIdentity,
        McpConformancePolicy,
    };

    const ISSUER: &str = "https://coach-auth.example.test";
    const RESOURCE: &str = "https://coach-auth.example.test/mcp";
    const SCOPE: &str = "coach:review";
    const KID: &str = "coach-test-key";

    #[test]
    fn coach_mcp_environment_rejects_wholly_missing_configuration() {
        let error = required_coach_mcp_environment(None, None, None).unwrap_err();

        assert_eq!(
            error.to_string(),
            "JWT_JWKS, OAUTH_ISSUER, and COACH_MCP_RESOURCE are required together"
        );
    }

    #[test]
    fn optional_coach_mcp_environment_is_absent_when_wholly_unconfigured() {
        let coach_mcp = optional_coach_mcp_environment(None, None, None)
            .expect("wholly missing Coach MCP configuration should not be an error");

        assert!(coach_mcp.is_none());
    }

    #[test]
    fn optional_coach_mcp_environment_rejects_partial_configuration() {
        let error =
            optional_coach_mcp_environment(Some("jwks".to_string()), None, None).unwrap_err();

        assert_eq!(
            error.to_string(),
            "JWT_JWKS, OAUTH_ISSUER, and COACH_MCP_RESOURCE are required together"
        );
    }

    #[test]
    fn coach_mcp_environment_rejects_partial_configuration() {
        let error = required_coach_mcp_environment(
            Some("jwks".to_string()),
            Some("issuer".to_string()),
            None,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "JWT_JWKS, OAUTH_ISSUER, and COACH_MCP_RESOURCE are required together"
        );
    }

    #[tokio::test]
    async fn coach_mcp_profile_accepts_only_the_exact_token_contract() {
        let auth = AuthConfig::new_coach_mcp(jwks_with_kid(), ISSUER, RESOURCE, SCOPE)
            .expect("Coach MCP test configuration should be valid");
        let now = unix_timestamp();
        let valid = TestClaims {
            sub: "firebase-player-a",
            exp: now + 600,
            iat: Some(now),
            auth_time: None,
            iss: ISSUER,
            aud: RESOURCE,
            jti: Some("access-token-1"),
            scope: Some("openid coach:review"),
        };
        let long_sub = "x".repeat(129);

        let player = auth
            .authenticate_token(&signed_token(&valid, "at+jwt", KID))
            .await
            .expect("the exact Coach MCP token contract should pass");
        assert_eq!(player.player_id.as_str(), "firebase-player-a");
        auth.authenticate_token(&signed_token(
            &TestClaims {
                exp: now + 604,
                iat: Some(now + 4),
                jti: Some("access-token-with-clock-skew"),
                ..valid.clone()
            },
            "at+jwt",
            KID,
        ))
        .await
        .expect("the shared five-second clock-skew policy should pass");

        let invalid_cases = [
            signed_token(
                &TestClaims {
                    iss: "https://wrong-issuer.example.test",
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(
                &TestClaims {
                    aud: "https://coach-auth.example.test/not-mcp",
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(
                &TestClaims {
                    scope: Some("openid profile"),
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(
                &TestClaims {
                    exp: now + 601,
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(
                &TestClaims {
                    exp: now - 120,
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(
                &TestClaims {
                    sub: " ",
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(
                &TestClaims {
                    sub: &long_sub,
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(
                &TestClaims {
                    iat: None,
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(
                &TestClaims {
                    iat: Some(now + 120),
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(
                &TestClaims {
                    jti: None,
                    ..valid.clone()
                },
                "at+jwt",
                KID,
            ),
            signed_token(&valid, "JWT", KID),
            signed_token(&valid, "at+jwt", "unknown-key"),
        ];

        for (case, token) in invalid_cases.into_iter().enumerate() {
            assert!(
                matches!(
                    auth.authenticate_token(&token).await,
                    Err(AuthError::InvalidToken)
                ),
                "invalid Coach MCP token case {case} was accepted"
            );
        }
    }

    #[tokio::test]
    async fn coach_mcp_profile_accepts_the_versioned_endpoint_audience() {
        let auth = AuthConfig::new_coach_mcp(jwks_with_kid(), ISSUER, RESOURCE, SCOPE)
            .expect("Coach MCP test configuration should be valid");
        let now = unix_timestamp();
        let versioned = TestClaims {
            sub: "firebase-player-a",
            exp: now + 600,
            iat: Some(now),
            auth_time: None,
            iss: ISSUER,
            aud: "https://coach-auth.example.test/mcp/v2",
            jti: Some("access-token-v2"),
            scope: Some("openid coach:review"),
        };

        auth.authenticate_token(&signed_token(&versioned, "at+jwt", KID))
            .await
            .expect("the versioned Coach MCP audience should pass");
    }

    #[tokio::test]
    async fn firebase_profile_derives_the_exact_player_id_from_verified_subject() {
        const PROJECT_ID: &str = "chenchess-test";
        let auth = AuthConfig::new_firebase(PROJECT_ID, jwks_with_kid())
            .expect("Firebase test configuration should be valid");
        let now = unix_timestamp();
        let valid = TestClaims {
            sub: "firebase-player-a",
            exp: now + 600,
            iat: Some(now),
            auth_time: Some(now),
            iss: "https://securetoken.google.com/chenchess-test",
            aud: PROJECT_ID,
            jti: None,
            scope: None,
        };

        let player = auth
            .authenticate_token(&signed_token(&valid, "JWT", KID))
            .await
            .expect("the exact Firebase ID-token profile should pass");
        assert_eq!(player.player_id.as_str(), "firebase-player-a");

        let invalid_cases = [
            signed_token(
                &TestClaims {
                    iss: "https://securetoken.google.com/wrong-project",
                    ..valid.clone()
                },
                "JWT",
                KID,
            ),
            signed_token(
                &TestClaims {
                    aud: "wrong-project",
                    ..valid.clone()
                },
                "JWT",
                KID,
            ),
            signed_token(
                &TestClaims {
                    iat: Some(now + 60),
                    ..valid.clone()
                },
                "JWT",
                KID,
            ),
            signed_token(
                &TestClaims {
                    auth_time: Some(now + 60),
                    ..valid.clone()
                },
                "JWT",
                KID,
            ),
            signed_token(&valid, "JWT", "unknown-key"),
        ];
        for token in invalid_cases {
            assert!(matches!(
                auth.authenticate_token(&token).await,
                Err(AuthError::InvalidToken)
            ));
        }
    }

    #[tokio::test]
    async fn combined_profile_accepts_both_bearers_but_keeps_the_identity_bridge_firebase_only() {
        const PROJECT_ID: &str = "chenchess-test";
        let auth = AuthConfig::new_firebase(PROJECT_ID, jwks_with_kid())
            .expect("Firebase test configuration should be valid")
            .with_coach_mcp(jwks_with_kid(), ISSUER, RESOURCE, SCOPE)
            .expect("Coach MCP test configuration should be valid");
        let now = unix_timestamp();
        let firebase = signed_token(
            &TestClaims {
                sub: "firebase-player-a",
                exp: now + 600,
                iat: Some(now),
                auth_time: Some(now),
                iss: "https://securetoken.google.com/chenchess-test",
                aud: PROJECT_ID,
                jti: None,
                scope: None,
            },
            "JWT",
            KID,
        );
        let coach = signed_token(
            &TestClaims {
                sub: "firebase-player-a",
                exp: now + 600,
                iat: Some(now),
                auth_time: None,
                iss: ISSUER,
                aud: RESOURCE,
                jti: Some("access-token-combined"),
                scope: Some(SCOPE),
            },
            "at+jwt",
            KID,
        );

        assert_eq!(
            auth.authenticate_token(&firebase)
                .await
                .expect("Firebase bearer should authenticate")
                .player_id
                .as_str(),
            "firebase-player-a"
        );
        assert_eq!(
            auth.authenticate_token(&coach)
                .await
                .expect("Coach bearer should authenticate")
                .player_id
                .as_str(),
            "firebase-player-a"
        );
        assert!(matches!(
            auth.authenticate_firebase_token(&coach).await,
            Err(AuthError::InvalidToken)
        ));
    }

    #[tokio::test]
    async fn coach_mcp_profile_accepts_every_published_rotation_key() {
        let auth = AuthConfig::new_coach_mcp(jwks_with_rotation_kids(), ISSUER, RESOURCE, SCOPE)
            .expect("overlapping Coach MCP keys should be valid");
        let now = unix_timestamp();
        let claims = TestClaims {
            sub: "firebase-player-a",
            exp: now + 600,
            iat: Some(now),
            auth_time: None,
            iss: ISSUER,
            aud: RESOURCE,
            jti: Some("rotation-access-token"),
            scope: Some(SCOPE),
        };

        for kid in ["retiring-key", "active-key"] {
            assert_eq!(
                auth.authenticate_token(&signed_token(&claims, "at+jwt", kid))
                    .await
                    .expect("every overlapping key should verify")
                    .player_id
                    .as_str(),
                "firebase-player-a"
            );
        }
    }

    #[test]
    fn coach_engine_refuses_to_start_against_an_emulator_host_off_this_machine() {
        let Err(error) = firebase_identity(Some("192.0.2.10:9099".to_string())) else {
            panic!("an emulator host off this machine must refuse at startup");
        };

        assert_eq!(
            error.to_string(),
            "FIREBASE_AUTH_EMULATOR_HOST must resolve to loopback, and 192.0.2.10:9099 does not"
        );
        assert!(matches!(
            firebase_identity(Some("127.0.0.1:9099".to_string()))
                .expect("a loopback emulator host arms the local identity path"),
            FirebaseIdentity::Emulator
        ));
        assert!(matches!(
            firebase_identity(None).expect("an unset emulator host stays on Google's key set"),
            FirebaseIdentity::Google(_)
        ));
    }

    #[tokio::test]
    async fn the_loopback_emulator_profile_keeps_every_claim_rule_it_relaxes_no_signature_for() {
        const PROJECT_ID: &str = "chenchess-emulator";
        let auth = AuthConfig::firebase(
            PROJECT_ID.to_string(),
            FirebaseIdentity::Emulator,
            McpConformancePolicy::Disabled,
        )
        .expect("the emulator profile should configure");
        let now = unix_timestamp();
        let valid = TestClaims {
            sub: "firebase-player-a",
            exp: now + 600,
            iat: Some(now),
            auth_time: Some(now),
            iss: "https://securetoken.google.com/chenchess-emulator",
            aud: PROJECT_ID,
            jti: None,
            scope: None,
        };

        let player = auth
            .authenticate_token(&emulator_token(&valid))
            .await
            .expect("an unsigned emulator ID token should pass");
        assert_eq!(player.player_id.as_str(), "firebase-player-a");

        let invalid_cases = [
            emulator_token(&TestClaims {
                iss: "https://securetoken.google.com/wrong-project",
                ..valid.clone()
            }),
            emulator_token(&TestClaims {
                aud: "wrong-project",
                ..valid.clone()
            }),
            emulator_token(&TestClaims {
                exp: now - 120,
                ..valid.clone()
            }),
            emulator_token(&TestClaims {
                iat: Some(now + 600),
                ..valid.clone()
            }),
            emulator_token(&TestClaims {
                iat: None,
                ..valid.clone()
            }),
            emulator_token(&TestClaims {
                auth_time: Some(now + 600),
                ..valid.clone()
            }),
            emulator_token(&TestClaims {
                auth_time: None,
                ..valid.clone()
            }),
            emulator_token(&TestClaims {
                sub: " ",
                ..valid.clone()
            }),
            // Signed tokens stay the Google path's business, even here.
            signed_token(&valid, "JWT", KID),
        ];
        for (case, token) in invalid_cases.into_iter().enumerate() {
            assert!(
                matches!(
                    auth.authenticate_token(&token).await,
                    Err(AuthError::InvalidToken)
                ),
                "invalid emulator token case {case} was accepted"
            );
        }
    }

    #[tokio::test]
    async fn an_unarmed_firebase_profile_refuses_an_unsigned_emulator_token() {
        const PROJECT_ID: &str = "chenchess-test";
        let auth = AuthConfig::new_firebase(PROJECT_ID, jwks_with_kid())
            .expect("Firebase test configuration should be valid");
        let now = unix_timestamp();

        assert!(matches!(
            auth.authenticate_token(&emulator_token(&TestClaims {
                sub: "firebase-player-a",
                exp: now + 600,
                iat: Some(now),
                auth_time: Some(now),
                iss: "https://securetoken.google.com/chenchess-test",
                aud: PROJECT_ID,
                jti: None,
                scope: None,
            }))
            .await,
            Err(AuthError::InvalidToken)
        ));
    }

    /// The shape the Firebase Auth emulator mints: `alg: none`, no `kid`, and
    /// an empty signature segment.
    fn emulator_token(claims: &TestClaims<'_>) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload =
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).expect("test claims serialize"));
        format!("{header}.{payload}.")
    }

    fn jwks_with_kid() -> String {
        let mut jwks: Value = serde_json::from_str(crate::certification_keys::jwks())
            .expect("test JWKS is valid JSON");
        jwks["keys"][0]["kid"] = Value::String(KID.to_owned());
        jwks["keys"][0]["alg"] = Value::String("RS256".to_owned());
        jwks["keys"][0]["use"] = Value::String("sig".to_owned());
        serde_json::to_string(&jwks).expect("test JWKS serializes")
    }

    fn jwks_with_rotation_kids() -> String {
        let mut jwks: Value =
            serde_json::from_str(&jwks_with_kid()).expect("test JWKS is valid JSON");
        let mut active = jwks["keys"][0].clone();
        jwks["keys"][0]["kid"] = Value::String("retiring-key".to_owned());
        active["kid"] = Value::String("active-key".to_owned());
        jwks["keys"]
            .as_array_mut()
            .expect("JWKS keys is an array")
            .push(active);
        serde_json::to_string(&jwks).expect("test JWKS serializes")
    }

    fn signed_token(claims: &TestClaims<'_>, typ: &str, kid: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.typ = Some(typ.to_owned());
        header.kid = Some(kid.to_owned());
        encode(
            &header,
            claims,
            &EncodingKey::from_rsa_pem(crate::certification_keys::private_key_pem().as_bytes())
                .expect("valid test private key"),
        )
        .expect("test token signs")
    }

    fn unix_timestamp() -> usize {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time is after Unix epoch")
            .as_secs() as usize
    }

    #[derive(Clone, Serialize)]
    struct TestClaims<'a> {
        sub: &'a str,
        exp: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        iat: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth_time: Option<usize>,
        iss: &'a str,
        aud: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        jti: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<&'a str>,
    }
}
