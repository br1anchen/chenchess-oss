use std::{fmt::Write, future::Future, net::IpAddr, pin::Pin, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use ring::hmac;
use serde::{Deserialize, Serialize};

use crate::{
    deployment::DeploymentEnvironment, firestore::FirestoreDatabase,
    review_session_contract::PlayerId,
};

mod firestore;
mod invitation;
#[cfg(test)]
mod memory;
mod redemption;

use invitation::{
    configured_invitation_issuer, InvitationDeliveryAttempt, InvitationDeliveryStatus,
    InvitationIssuer, InvitationStatus, StoredInvitation,
};
use redemption::{
    BetaAccessRedemptionAttempt, BetaAccessRedemptionCandidate, BetaAccessRedemptionCommit,
    BetaAccessRedemptionTarget,
};
pub(crate) use redemption::{BetaAccessRedemptionIdentity, BetaAccessRedemptionResult};

#[cfg(test)]
pub(crate) use invitation::{
    InvitationDeliveryError, InvitationDeliveryReceipt, InvitationDeliveryRequest,
    InvitationEmailDelivery,
};
#[cfg(test)]
pub(crate) use memory::InMemoryBetaAccessStore;
#[cfg(test)]
use memory::UnavailableBetaAccessStore;

const RATE_LIMIT_KEY_ENV: &str = "BETA_ACCESS_RATE_LIMIT_HMAC_KEY";
const RATE_LIMIT_LIFETIME_HOURS: i64 = 24;
pub(crate) const EMAIL_ATTEMPT_LIMIT: u16 = 5;
pub(crate) const IP_ATTEMPT_LIMIT: u16 = 25;
pub(crate) const REDEMPTION_PLAYER_ATTEMPT_LIMIT: u16 = 10;
pub(crate) const REDEMPTION_IP_ATTEMPT_LIMIT: u16 = 25;

type SubmitFuture<'a> =
    Pin<Box<dyn Future<Output = Result<BetaAccessStoreOutcome, BetaAccessStoreError>> + Send + 'a>>;
type ListFuture<'a> = Pin<
    Box<dyn Future<Output = Result<Vec<BetaAccessAdminRequest>, BetaAccessStoreError>> + Send + 'a>,
>;
type GrantTargetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<BetaAccessGrantTarget, BetaAccessStoreError>> + Send + 'a>>;
type CommitGrantFuture<'a> =
    Pin<Box<dyn Future<Output = Result<BetaAccessGrantCommit, BetaAccessStoreError>> + Send + 'a>>;
type RecordDeliveryFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), BetaAccessStoreError>> + Send + 'a>>;
type InvitationTargetFuture<'a> = Pin<
    Box<dyn Future<Output = Result<BetaAccessInvitationTarget, BetaAccessStoreError>> + Send + 'a>,
>;
type BeginRetryFuture<'a> =
    Pin<Box<dyn Future<Output = Result<BetaAccessRetryCommit, BetaAccessStoreError>> + Send + 'a>>;
type RevokeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<BetaAccessRevokeResult, BetaAccessStoreError>> + Send + 'a>>;
type RevokeAccessFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<BetaAccessAuthorizationRevokeResult, BetaAccessStoreError>>
            + Send
            + 'a,
    >,
>;
type RedemptionTargetFuture<'a> = Pin<
    Box<dyn Future<Output = Result<BetaAccessRedemptionTarget, BetaAccessStoreError>> + Send + 'a>,
>;
type CommitRedemptionFuture<'a> = Pin<
    Box<dyn Future<Output = Result<BetaAccessRedemptionCommit, BetaAccessStoreError>> + Send + 'a>,
>;
type HasAccessFuture<'a> =
    Pin<Box<dyn Future<Output = Result<bool, BetaAccessStoreError>> + Send + 'a>>;

trait BetaAccessStore: Send + Sync {
    fn begin_retry<'a>(
        &'a self,
        invitation_id: &'a str,
        request_id: &'a str,
        expected_attempt: u32,
    ) -> BeginRetryFuture<'a>;
    fn commit_grant<'a>(&'a self, invitation: StoredInvitation) -> CommitGrantFuture<'a>;
    fn commit_redemption<'a>(
        &'a self,
        candidate: BetaAccessRedemptionCandidate,
        player_id: PlayerId,
        now: DateTime<Utc>,
    ) -> CommitRedemptionFuture<'a>;
    fn grant_target<'a>(&'a self, request_id: &'a str) -> GrantTargetFuture<'a>;
    fn has_access<'a>(&'a self, player_id: &'a PlayerId) -> HasAccessFuture<'a>;
    fn invitation_target<'a>(&'a self, request_id: &'a str) -> InvitationTargetFuture<'a>;
    fn list(&self) -> ListFuture<'_>;
    fn record_delivery<'a>(
        &'a self,
        invitation_id: &'a str,
        request_id: &'a str,
        delivery_attempt: u32,
        attempt: InvitationDeliveryAttempt,
    ) -> RecordDeliveryFuture<'a>;
    fn redemption_target<'a>(
        &'a self,
        attempt: BetaAccessRedemptionAttempt,
    ) -> RedemptionTargetFuture<'a>;
    fn revoke<'a>(&'a self, request_id: &'a str) -> RevokeFuture<'a>;
    fn revoke_access<'a>(&'a self, request_id: &'a str) -> RevokeAccessFuture<'a>;
    fn submit<'a>(&'a self, submission: BetaAccessSubmission) -> SubmitFuture<'a>;
}

/// Owns beta request and authorization behavior for the current deployment.
#[derive(Clone)]
pub struct BetaAccessRuntime {
    mode: BetaAccessMode,
}

#[derive(Clone)]
enum BetaAccessMode {
    BypassAuthorization,
    Unavailable,
    Enabled(EnabledBetaAccess),
}

#[derive(Clone)]
struct EnabledBetaAccess {
    hasher: RateLimitHasher,
    invitation: Option<InvitationIssuer>,
    store: Arc<dyn BetaAccessStore>,
}

impl BetaAccessRuntime {
    /// Builds a runtime that bypasses beta authorization without reading beta
    /// persistence. Production and explicitly non-beta test fixtures use this
    /// mode.
    pub fn disabled() -> Self {
        Self {
            mode: BetaAccessMode::BypassAuthorization,
        }
    }

    fn unavailable() -> Self {
        Self {
            mode: BetaAccessMode::Unavailable,
        }
    }

    fn new(
        store: Arc<dyn BetaAccessStore>,
        rate_limit_key: &[u8],
        invitation: Option<InvitationIssuer>,
    ) -> Result<Self, BetaAccessConfigurationError> {
        Ok(Self {
            mode: BetaAccessMode::Enabled(EnabledBetaAccess {
                hasher: RateLimitHasher::new(rate_limit_key)?,
                invitation,
                store,
            }),
        })
    }

    fn enabled(&self) -> Option<&EnabledBetaAccess> {
        match &self.mode {
            BetaAccessMode::Enabled(enabled) => Some(enabled),
            BetaAccessMode::BypassAuthorization | BetaAccessMode::Unavailable => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn in_memory(
        store: Arc<InMemoryBetaAccessStore>,
        rate_limit_key: &[u8],
    ) -> Result<Self, BetaAccessConfigurationError> {
        Self::new(store, rate_limit_key, None)
    }

    #[cfg(test)]
    pub(crate) fn in_memory_with_delivery(
        store: Arc<InMemoryBetaAccessStore>,
        rate_limit_key: &[u8],
        delivery: Arc<dyn InvitationEmailDelivery>,
    ) -> Result<Self, BetaAccessConfigurationError> {
        Self::new(
            store,
            rate_limit_key,
            Some(InvitationIssuer::for_test(delivery)?),
        )
    }

    #[cfg(test)]
    pub(crate) fn unavailable_store(rate_limit_key: &[u8]) -> Self {
        Self::new(Arc::new(UnavailableBetaAccessStore), rate_limit_key, None)
            .expect("the test rate-limit key must be valid")
    }

    pub(crate) async fn submit(
        &self,
        email: NormalizedEmail,
        source_ip: IpAddr,
        now: DateTime<Utc>,
    ) -> Result<(), BetaAccessStoreError> {
        let enabled = self.enabled().ok_or(BetaAccessStoreError::Unavailable)?;
        let email_rate_key = enabled
            .hasher
            .identifier(b"email", email.as_str().as_bytes());
        let source_ip = source_ip.to_string();
        let ip_rate_key = enabled
            .hasher
            .identifier(b"source-ip", source_ip.as_bytes());
        enabled
            .store
            .submit(BetaAccessSubmission {
                email,
                email_rate_key,
                ip_rate_key,
                now,
            })
            .await
            .map(|_| ())
    }

    pub(crate) async fn require_access(
        &self,
        player_id: &PlayerId,
    ) -> Result<(), BetaAccessAuthorizationError> {
        let enabled = match &self.mode {
            BetaAccessMode::BypassAuthorization => return Ok(()),
            BetaAccessMode::Unavailable => return Err(BetaAccessAuthorizationError::Unavailable),
            BetaAccessMode::Enabled(enabled) => enabled,
        };
        match enabled.store.has_access(player_id).await {
            Ok(true) => Ok(()),
            Ok(false) => Err(BetaAccessAuthorizationError::Required),
            Err(error) => Err(BetaAccessAuthorizationError::Store(error)),
        }
    }

    pub(crate) async fn list(
        &self,
        filter: BetaAccessRequestFilter,
    ) -> Result<Vec<BetaAccessAdminRequest>, BetaAccessStoreError> {
        const MAX_RESULTS: usize = 100;

        let enabled = self.enabled().ok_or(BetaAccessStoreError::Unavailable)?;
        let mut requests = enabled.store.list().await?;
        requests.retain(|request| filter.matches(&request.request));
        requests.sort_by(|left, right| {
            right
                .request
                .created_at
                .cmp(&left.request.created_at)
                .then_with(|| left.request.id.cmp(&right.request.id))
        });
        requests.truncate(MAX_RESULTS);
        Ok(requests)
    }

    pub(crate) async fn redeemed_player_id(
        &self,
        request_id: &str,
    ) -> Result<Option<PlayerId>, BetaAccessInvitationError> {
        if !opaque_request_id(request_id) {
            return Err(BetaAccessInvitationError::InvalidRequest);
        }
        let enabled = self
            .enabled()
            .ok_or(BetaAccessInvitationError::Unavailable)?;
        match enabled.store.invitation_target(request_id).await? {
            BetaAccessInvitationTarget::NotIssued => Ok(None),
            BetaAccessInvitationTarget::Invitation(invitation) => {
                if invitation.status != InvitationStatus::Redeemed {
                    return Ok(None);
                }
                invitation
                    .redeemed_by
                    .clone()
                    .map(Some)
                    .ok_or(BetaAccessInvitationError::Store(
                        BetaAccessStoreError::InvalidRecord,
                    ))
            }
        }
    }

    pub(crate) async fn grant(
        &self,
        request_id: &str,
        now: DateTime<Utc>,
    ) -> Result<BetaAccessGrantResult, BetaAccessInvitationError> {
        if !opaque_request_id(request_id) {
            return Err(BetaAccessInvitationError::InvalidRequest);
        }
        let enabled = self
            .enabled()
            .ok_or(BetaAccessInvitationError::Unavailable)?;
        let issuer = enabled
            .invitation
            .as_ref()
            .ok_or(BetaAccessInvitationError::Unavailable)?;
        let email = match enabled.store.grant_target(request_id).await? {
            BetaAccessGrantTarget::Pending(email) => email,
            BetaAccessGrantTarget::AlreadyGranted => {
                return Ok(BetaAccessGrantResult::AlreadyGranted)
            }
        };
        let invitation = issuer.prepare(request_id.to_string(), email, now)?;
        match enabled
            .store
            .commit_grant(invitation.stored.clone())
            .await?
        {
            BetaAccessGrantCommit::AlreadyGranted => {
                return Ok(BetaAccessGrantResult::AlreadyGranted)
            }
            BetaAccessGrantCommit::Issued => {}
        }
        let delivery_attempt = invitation.stored.delivery_attempt;
        let attempt = issuer.deliver(&invitation, delivery_attempt).await;
        let result = match &attempt {
            InvitationDeliveryAttempt::Sent { .. } => BetaAccessGrantResult::Delivered,
            InvitationDeliveryAttempt::Failed { .. } => BetaAccessGrantResult::DeliveryFailed,
        };
        enabled
            .store
            .record_delivery(&invitation.stored.id, request_id, delivery_attempt, attempt)
            .await?;
        Ok(result)
    }

    pub(crate) async fn retry_delivery(
        &self,
        request_id: &str,
    ) -> Result<BetaAccessRetryResult, BetaAccessInvitationError> {
        if !opaque_request_id(request_id) {
            return Err(BetaAccessInvitationError::InvalidRequest);
        }
        let enabled = self
            .enabled()
            .ok_or(BetaAccessInvitationError::Unavailable)?;
        let issuer = enabled
            .invitation
            .as_ref()
            .ok_or(BetaAccessInvitationError::Unavailable)?;
        let stored = match enabled.store.invitation_target(request_id).await? {
            BetaAccessInvitationTarget::NotIssued => return Ok(BetaAccessRetryResult::NotIssued),
            BetaAccessInvitationTarget::Invitation(stored) => stored,
        };
        match stored.status {
            InvitationStatus::Revoked => return Ok(BetaAccessRetryResult::Revoked),
            InvitationStatus::Redeemed => return Ok(BetaAccessRetryResult::Redeemed),
            InvitationStatus::Issued => {}
        }
        if stored.delivery_status != InvitationDeliveryStatus::Failed
            || stored.delivery_retryable != Some(true)
        {
            return Ok(BetaAccessRetryResult::NotRetryable);
        }
        let invitation = issuer.recover(&stored)?;
        let delivery_attempt = match enabled
            .store
            .begin_retry(&stored.id, request_id, stored.delivery_attempt)
            .await?
        {
            BetaAccessRetryCommit::Started { delivery_attempt } => delivery_attempt,
            BetaAccessRetryCommit::NotRetryable => return Ok(BetaAccessRetryResult::NotRetryable),
            BetaAccessRetryCommit::Revoked => return Ok(BetaAccessRetryResult::Revoked),
            BetaAccessRetryCommit::Redeemed => return Ok(BetaAccessRetryResult::Redeemed),
        };
        let attempt = issuer.deliver(&invitation, delivery_attempt).await;
        let result = match &attempt {
            InvitationDeliveryAttempt::Sent { .. } => BetaAccessRetryResult::Delivered,
            InvitationDeliveryAttempt::Failed { .. } => BetaAccessRetryResult::DeliveryFailed,
        };
        enabled
            .store
            .record_delivery(&stored.id, request_id, delivery_attempt, attempt)
            .await?;
        Ok(result)
    }

    pub(crate) async fn revoke(
        &self,
        request_id: &str,
    ) -> Result<BetaAccessRevokeResult, BetaAccessInvitationError> {
        if !opaque_request_id(request_id) {
            return Err(BetaAccessInvitationError::InvalidRequest);
        }
        self.enabled()
            .ok_or(BetaAccessInvitationError::Unavailable)?
            .store
            .revoke(request_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn revoke_access(
        &self,
        request_id: &str,
    ) -> Result<BetaAccessAuthorizationRevokeResult, BetaAccessInvitationError> {
        if !opaque_request_id(request_id) {
            return Err(BetaAccessInvitationError::InvalidRequest);
        }
        self.enabled()
            .ok_or(BetaAccessInvitationError::Unavailable)?
            .store
            .revoke_access(request_id)
            .await
            .map_err(Into::into)
    }
}

/// Builds the beta request runtime from staging configuration.
///
/// Production rejects the beta-only HMAC key. Staging remains available for
/// deployment before final provisioning, but the request endpoint returns a
/// temporary-unavailable result until the key is present.
pub async fn configured_beta_access_runtime() -> anyhow::Result<BetaAccessRuntime> {
    let environment = std::env::var("DEPLOYMENT_ENVIRONMENT")?;
    let environment = DeploymentEnvironment::parse(&environment)?;
    let rate_limit_key = optional_env(RATE_LIMIT_KEY_ENV);
    let invitation = configured_invitation_issuer(environment)?;
    if environment == DeploymentEnvironment::Production {
        if rate_limit_key.is_some() {
            anyhow::bail!("{RATE_LIMIT_KEY_ENV} must be absent in production");
        }
        return Ok(BetaAccessRuntime::disabled());
    }
    let Some(rate_limit_key) = rate_limit_key else {
        if invitation.is_some() {
            anyhow::bail!(
                "{RATE_LIMIT_KEY_ENV} is required when beta invitation delivery is configured"
            );
        }
        tracing::warn!(
            category = "configuration",
            "beta access requests are disabled until the staging rate-limit key is provisioned"
        );
        return Ok(BetaAccessRuntime::unavailable());
    };
    if std::env::var_os("FIREBASE_PROJECT_ID").is_none() {
        anyhow::bail!("FIREBASE_PROJECT_ID is required when {RATE_LIMIT_KEY_ENV} is configured");
    }
    BetaAccessRuntime::new(
        firestore::beta_access_store(FirestoreDatabase::from_env()?),
        rate_limit_key.as_bytes(),
        invitation,
    )
    .map_err(Into::into)
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct NormalizedEmail(String);

impl<'de> Deserialize<'de> for NormalizedEmail {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let email = Self::parse(&value).map_err(serde::de::Error::custom)?;
        if email.as_str() != value {
            return Err(serde::de::Error::custom(
                "email address must already be normalized",
            ));
        }
        Ok(email)
    }
}

impl NormalizedEmail {
    pub(crate) fn parse(input: &str) -> Result<Self, InvalidEmail> {
        let value = input.trim();
        if value.len() > 254 || !value.is_ascii() {
            return Err(InvalidEmail);
        }
        let mut parts = value.split('@');
        let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(InvalidEmail);
        };
        if !valid_local_part(local) || !valid_domain(domain) {
            return Err(InvalidEmail);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_local_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'.' | b'!'
                        | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'/'
                        | b'='
                        | b'?'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'{'
                        | b'|'
                        | b'}'
                        | b'~'
                )
        })
}

fn valid_domain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[derive(Debug, thiserror::Error)]
#[error("email address is invalid")]
pub(crate) struct InvalidEmail;

#[derive(Clone)]
struct RateLimitHasher {
    key: Arc<hmac::Key>,
}

impl RateLimitHasher {
    fn new(key: &[u8]) -> Result<Self, BetaAccessConfigurationError> {
        if !(32..=256).contains(&key.len()) {
            return Err(BetaAccessConfigurationError::RateLimitKey);
        }
        Ok(Self {
            key: Arc::new(hmac::Key::new(hmac::HMAC_SHA256, key)),
        })
    }

    fn identifier(&self, purpose: &[u8], value: &[u8]) -> String {
        let mut context = hmac::Context::with_key(&self.key);
        context.update(b"chenchess-beta-access-rate-limit-v1\0");
        context.update(purpose);
        context.update(b"\0");
        context.update(value);
        let mut encoded = String::with_capacity(64);
        for byte in context.sign().as_ref() {
            write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
        }
        encoded
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BetaAccessConfigurationError {
    #[error("beta invitation cryptography is invalid")]
    InvitationCryptography,
    #[error("beta invitation randomness is unavailable")]
    Randomness,
    #[error("beta access rate-limit key must contain 32 to 256 bytes")]
    RateLimitKey,
}

#[derive(Clone)]
struct BetaAccessSubmission {
    email: NormalizedEmail,
    email_rate_key: String,
    ip_rate_key: String,
    now: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BetaAccessStoreOutcome {
    Recorded,
    Duplicate,
    RateLimited,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BetaAccessStoreError {
    #[error("beta access persistence is misconfigured: {0}")]
    Configuration(String),
    #[error("beta access persistence transport failed")]
    Transport,
    #[error("beta access persistence is unavailable")]
    Unavailable,
    #[error("beta access persistence conflicted")]
    Conflict,
    #[error("beta access persistence returned an invalid record")]
    InvalidRecord,
    #[error("beta access request was not found")]
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BetaAccessAuthorizationError {
    #[error("Beta Access is required")]
    Required,
    #[error("beta access authorization is unavailable")]
    Unavailable,
    #[error(transparent)]
    Store(#[from] BetaAccessStoreError),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BetaAccessInvitationError {
    #[error("beta access invitation configuration is unavailable")]
    Unavailable,
    #[error("beta access invitation request is invalid")]
    InvalidRequest,
    #[error(transparent)]
    Configuration(#[from] BetaAccessConfigurationError),
    #[error(transparent)]
    Store(#[from] BetaAccessStoreError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BetaAccessGrantResult {
    Delivered,
    DeliveryFailed,
    AlreadyGranted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BetaAccessRetryResult {
    Delivered,
    DeliveryFailed,
    NotIssued,
    NotRetryable,
    Revoked,
    Redeemed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BetaAccessRevokeResult {
    Revoked,
    NotIssued,
    AlreadyRevoked,
    AlreadyRedeemed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BetaAccessAuthorizationRevokeResult {
    Revoked,
    NotGranted,
    AlreadyRevoked,
}

enum BetaAccessGrantTarget {
    Pending(NormalizedEmail),
    AlreadyGranted,
}

enum BetaAccessInvitationTarget {
    NotIssued,
    Invitation(Box<StoredInvitation>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BetaAccessGrantCommit {
    Issued,
    AlreadyGranted,
}

enum BetaAccessRetryCommit {
    Started { delivery_attempt: u32 },
    NotRetryable,
    Revoked,
    Redeemed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BetaAccessRequestStatus {
    Pending,
    Granted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BetaAccessAuthorizationStatus {
    Active,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BetaAccessRequest {
    id: String,
    email: NormalizedEmail,
    status: BetaAccessRequestStatus,
    created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery_status: Option<InvitationDeliveryStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery_retryable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    invitation_status: Option<InvitationStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_status: Option<BetaAccessAuthorizationStatus>,
}

impl BetaAccessRequest {
    pub(crate) fn access_is_active(&self) -> bool {
        self.access_status == Some(BetaAccessAuthorizationStatus::Active)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BetaAccessAdminRequest {
    pub(crate) request: BetaAccessRequest,
    pub(crate) redeemed_player_id: Option<PlayerId>,
}

impl BetaAccessAdminRequest {
    fn new(
        request: BetaAccessRequest,
        invitation: Option<&StoredInvitation>,
    ) -> Result<Self, BetaAccessStoreError> {
        let redeemed_player_id = if request.invitation_status == Some(InvitationStatus::Redeemed) {
            let invitation = invitation.ok_or(BetaAccessStoreError::InvalidRecord)?;
            if !invitation.valid_shape()
                || invitation.request_id != request.id
                || invitation.email != request.email
                || invitation.status != InvitationStatus::Redeemed
                || Some(invitation.delivery_status) != request.delivery_status
                || invitation.delivery_retryable != request.delivery_retryable
            {
                return Err(BetaAccessStoreError::InvalidRecord);
            }
            Some(
                invitation
                    .redeemed_by
                    .clone()
                    .ok_or(BetaAccessStoreError::InvalidRecord)?,
            )
        } else {
            None
        };
        Ok(Self {
            request,
            redeemed_player_id,
        })
    }
}

fn opaque_request_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct BetaAccessRequestFilter {
    pub email_contains: Option<String>,
    pub status: Option<BetaAccessRequestStatus>,
}

impl BetaAccessRequestFilter {
    fn matches(&self, request: &BetaAccessRequest) -> bool {
        self.status.is_none_or(|status| request.status == status)
            && self
                .email_contains
                .as_deref()
                .is_none_or(|query| request.email.as_str().contains(query))
    }
}

#[derive(Clone, Copy, Debug)]
struct RateLimitState {
    attempts: u16,
    window_started_at: DateTime<Utc>,
    purge_at: DateTime<Utc>,
}

impl RateLimitState {
    fn consume(current: Option<Self>, limit: u16, now: DateTime<Utc>) -> (Self, bool) {
        let mut state = current
            .filter(|state| state.purge_at > now)
            .unwrap_or_else(|| Self {
                attempts: 0,
                window_started_at: now,
                purge_at: now + Duration::hours(RATE_LIMIT_LIFETIME_HOURS),
            });
        if state.attempts >= limit {
            return (state, false);
        }
        state.attempts += 1;
        (state, true)
    }

    fn has_valid_shape(self) -> bool {
        self.attempts > 0
            && self.purge_at > self.window_started_at
            && self.purge_at - self.window_started_at <= Duration::hours(RATE_LIMIT_LIFETIME_HOURS)
    }
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn normalized_email_trims_and_folds_ascii_case() {
        let email = NormalizedEmail::parse("  Player+Beta@Example.COM ").unwrap();

        assert_eq!(email.as_str(), "player+beta@example.com");
    }

    #[test]
    fn normalized_email_rejects_malformed_addresses() {
        for invalid in [
            "missing-at.example",
            "two@@example.com",
            ".leading@example.com",
            "double..dot@example.com",
            "player@-example.com",
            "player@example..com",
            "pläyer@example.com",
        ] {
            assert!(
                NormalizedEmail::parse(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn keyed_identifiers_are_scoped_fixed_length_and_do_not_expose_input() {
        let hasher = RateLimitHasher::new(TEST_KEY).unwrap();
        let email = hasher.identifier(b"email", b"player@example.com");
        let ip = hasher.identifier(b"source-ip", b"player@example.com");

        assert_eq!(email.len(), 64);
        assert_ne!(email, ip);
        assert!(!email.contains("player"));
    }

    #[tokio::test]
    async fn in_memory_store_keeps_one_request_and_counts_duplicate_attempts() {
        let store = InMemoryBetaAccessStore::default();
        let now = "2026-08-02T10:00:00Z".parse().unwrap();
        let submission = BetaAccessSubmission {
            email: NormalizedEmail::parse("player@example.com").unwrap(),
            email_rate_key: "email-key".to_string(),
            ip_rate_key: "ip-key".to_string(),
            now,
        };

        assert_eq!(
            store.submit(submission.clone()).await.unwrap(),
            BetaAccessStoreOutcome::Recorded
        );
        assert_eq!(
            store.submit(submission).await.unwrap(),
            BetaAccessStoreOutcome::Duplicate
        );
        assert_eq!(store.request_count(), 1);
    }

    #[test]
    fn expired_rate_limit_starts_a_new_bounded_window() {
        let started_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let expired = RateLimitState {
            attempts: IP_ATTEMPT_LIMIT,
            window_started_at: started_at,
            purge_at: started_at + Duration::hours(RATE_LIMIT_LIFETIME_HOURS),
        };

        let (renewed, allowed) =
            RateLimitState::consume(Some(expired), IP_ATTEMPT_LIMIT, expired.purge_at);

        assert!(allowed);
        assert_eq!(renewed.attempts, 1);
        assert_eq!(
            renewed.purge_at - renewed.window_started_at,
            Duration::hours(RATE_LIMIT_LIFETIME_HOURS)
        );
    }
}
