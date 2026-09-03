use std::{collections::BTreeMap, fmt::Write as _, future::Future, pin::Pin, sync::Arc};

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use ring::hmac;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    beta_access::NormalizedEmail,
    firestore::{FirestoreDatabase, FirestoreError},
    review_session_contract::PlayerId,
};

use super::{
    dashboard::{project_digest, DailyCoachingDigestDetail},
    digest::{CoachingDigest, DigestedGameCard},
    state::ProfileUnavailableNotice,
    valid_resend_provider_id, DailyCoachingOwnerKey, DailyCoachingProvider,
};

mod render;
mod resend;
mod store;

use render::{render_digest_email, render_profile_unavailable_email};
use resend::ResendDigestEmailDelivery;
use store::{
    DeliveryClaim, DeliveryCompletion, DeliveryLease, DeliverySuppressionReason, EmailPreference,
    EmailSuppressionReason, FirestoreDigestEmailStore, SuppressionEvent,
};
pub(crate) use store::{DigestEmailStore, InMemoryDigestEmailStore};

const EMAIL_RECORD_VERSION: u8 = 1;
const TOKEN_VERSION: &str = "v1";
const TOKEN_KEY_ENV: &str = "DAILY_COACHING_EMAIL_HMAC_KEY_V1";
const RESEND_API_KEY_ENV: &str = "DAILY_COACHING_RESEND_API_KEY";
const RESEND_WEBHOOK_SECRET_ENV: &str = "DAILY_COACHING_RESEND_WEBHOOK_SECRET";
const SVIX_TOLERANCE: TimeDelta = TimeDelta::minutes(5);
const DELIVERY_CLAIM_TTL: TimeDelta = TimeDelta::minutes(5);
// Resend retains idempotency keys for 24 hours. Keep the final recovery attempt
// inside that provider window so a lost success response cannot become a duplicate send.
const DELIVERY_RETRY_HORIZON: TimeDelta = TimeDelta::hours(23);

pub(crate) type EmailDeliveryFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DigestEmailReceipt, DigestEmailDeliveryError>> + Send + 'a>>;

pub(crate) trait DigestEmailDelivery: Send + Sync {
    fn deliver<'a>(&'a self, request: DigestEmailRequest) -> EmailDeliveryFuture<'a>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DigestEmailReadiness {
    Ready,
    NotConfigured,
    NoVerifiedEmail,
    Disabled,
    Suppressed,
}

#[derive(Clone)]
pub(crate) struct DigestEmailRuntime {
    store: Arc<dyn DigestEmailStore>,
    delivery: Arc<dyn DigestEmailDelivery>,
    tokens: Option<EmailTokenService>,
    webhook: Option<SvixWebhookVerifier>,
    public_origin: Arc<str>,
}

impl DigestEmailRuntime {
    pub(crate) fn disabled() -> Self {
        Self {
            store: Arc::new(InMemoryDigestEmailStore::default()),
            delivery: Arc::new(DisabledDigestEmailDelivery),
            tokens: None,
            webhook: None,
            public_origin: "http://127.0.0.1:4173".into(),
        }
    }

    pub(crate) fn configured(
        database: FirestoreDatabase,
        public_origin: impl Into<String>,
    ) -> anyhow::Result<Self> {
        let token_key = optional_env(TOKEN_KEY_ENV);
        let resend_api_key = optional_env(RESEND_API_KEY_ENV);
        let webhook_secret = optional_env(RESEND_WEBHOOK_SECRET_ENV);
        let present = [
            token_key.is_some(),
            resend_api_key.is_some(),
            webhook_secret.is_some(),
        ];
        if present.into_iter().all(|value| !value) {
            tracing::warn!(
                category = "configuration",
                "Daily Coaching digest email is disabled until its three delivery secrets are provisioned"
            );
            return Ok(Self::disabled());
        }
        let (Some(token_key), Some(resend_api_key), Some(webhook_secret)) =
            (token_key, resend_api_key, webhook_secret)
        else {
            anyhow::bail!(
                "{TOKEN_KEY_ENV}, {RESEND_API_KEY_ENV}, and {RESEND_WEBHOOK_SECRET_ENV} are required together"
            );
        };
        Ok(Self {
            store: Arc::new(FirestoreDigestEmailStore::new(database)),
            delivery: Arc::new(ResendDigestEmailDelivery::new(resend_api_key)?),
            tokens: Some(EmailTokenService::new(decode_hex_key(
                TOKEN_KEY_ENV,
                &token_key,
            )?)),
            webhook: Some(SvixWebhookVerifier::new(&webhook_secret)?),
            public_origin: Arc::from(public_origin.into().as_str()),
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        store: Arc<dyn DigestEmailStore>,
        delivery: Arc<dyn DigestEmailDelivery>,
    ) -> Self {
        Self {
            store,
            delivery,
            tokens: Some(EmailTokenService::new([0x31; 32])),
            webhook: Some(SvixWebhookVerifier::for_test([0x42; 32])),
            public_origin: "https://beta.chenchess.test".into(),
        }
    }

    pub(crate) async fn observe_verified_email(
        &self,
        player_id: &PlayerId,
        email: Option<&NormalizedEmail>,
        now: DateTime<Utc>,
    ) -> Result<(), DigestEmailError> {
        if !self.is_available() {
            return Ok(());
        }
        self.store
            .observe_verified_email(
                &DailyCoachingOwnerKey::for_player(player_id),
                player_id,
                email,
                now,
            )
            .await
    }

    pub(crate) async fn set_enabled(
        &self,
        player_id: &PlayerId,
        email: Option<&NormalizedEmail>,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> Result<(), DigestEmailError> {
        if !self.is_available() {
            return Err(DigestEmailError::NotConfigured);
        }
        let email = email.ok_or(DigestEmailError::NoVerifiedAccountEmail)?;
        self.store
            .set_enabled(
                &DailyCoachingOwnerKey::for_player(player_id),
                player_id,
                email,
                enabled,
                now,
            )
            .await
    }

    pub(crate) async fn preference_enabled(
        &self,
        player_id: &PlayerId,
    ) -> Result<Option<bool>, DigestEmailError> {
        if !self.is_available() {
            return Ok(None);
        }
        self.store
            .preference(&DailyCoachingOwnerKey::for_player(player_id))
            .await
            .map(|preference| {
                preference.and_then(|preference| preference.email.map(|_| preference.enabled))
            })
    }

    pub(crate) async fn can_receive(
        &self,
        owner_key: &DailyCoachingOwnerKey,
    ) -> Result<bool, DigestEmailError> {
        if !self.is_available() {
            return Ok(false);
        }
        self.store
            .preference(owner_key)
            .await
            .map(|preference| preference.is_some_and(|preference| preference.can_receive()))
    }

    pub(crate) async fn readiness(
        &self,
        owner_key: &DailyCoachingOwnerKey,
    ) -> Result<DigestEmailReadiness, DigestEmailError> {
        if !self.is_available() {
            return Ok(DigestEmailReadiness::NotConfigured);
        }
        self.store.preference(owner_key).await.map(|preference| {
            preference.map_or(DigestEmailReadiness::NoVerifiedEmail, |preference| {
                preference.readiness()
            })
        })
    }

    pub(crate) async fn begin_digest_delivery(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        digest_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<DeliveryLease>, DigestEmailError> {
        self.claim_delivery(owner_key, digest_id, now).await
    }

    pub(crate) async fn begin_profile_unavailable_delivery(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        notice: &ProfileUnavailableNotice,
        now: DateTime<Utc>,
    ) -> Result<Option<DeliveryLease>, DigestEmailError> {
        self.claim_delivery(owner_key, &profile_unavailable_delivery_id(notice), now)
            .await
    }

    async fn claim_delivery(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        digest_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<DeliveryLease>, DigestEmailError> {
        if !self.is_available() {
            return Ok(None);
        }
        self.store
            .claim(
                owner_key,
                digest_id,
                now,
                DELIVERY_CLAIM_TTL,
                DELIVERY_RETRY_HORIZON,
            )
            .await
            .map(|claim| match claim {
                DeliveryClaim::Claimed(lease) => Some(lease),
                DeliveryClaim::AlreadyClaimed => None,
            })
    }

    pub(crate) async fn deliver_claimed_digest(
        &self,
        digest: CoachingDigest,
        cards: Vec<DigestedGameCard>,
        lease: DeliveryLease,
    ) -> Result<(), DigestEmailError> {
        // Derived from the digest, not its id: a rebuilt digest must not reuse the original
        // send's identity, or the provider collapses it as a duplicate.
        let delivery_id = digest.delivery_id();
        self.deliver_claimed_digest_as(digest, cards, delivery_id, lease)
            .await
    }

    async fn deliver_claimed_digest_as(
        &self,
        digest: CoachingDigest,
        cards: Vec<DigestedGameCard>,
        delivery_id: String,
        lease: DeliveryLease,
    ) -> Result<(), DigestEmailError> {
        let owner_key = digest.owner_key.clone();
        let digest_id = digest.digest_id.clone();
        let Some(tokens) = &self.tokens else {
            return Err(DigestEmailError::NotConfigured);
        };
        let preference = self.store.preference(&owner_key).await?;
        let Some(preference) = preference.filter(EmailPreference::can_receive) else {
            return self
                .store
                .finish(
                    &owner_key,
                    &delivery_id,
                    lease,
                    DeliveryCompletion::Suppressed(DeliverySuppressionReason::NotSubscribed),
                )
                .await;
        };
        let email = preference.email.ok_or(DigestEmailError::InvalidRecord)?;
        let token = tokens.issue(&owner_key, &email);
        let digest_url = format!(
            "{}/dashboard/#digest={}",
            self.public_origin.trim_end_matches('/'),
            digest_id
        );
        let unsubscribe_url = format!(
            "{}/api/v1/daily-coaching/email/unsubscribe?token={token}",
            self.public_origin.trim_end_matches('/')
        );
        let projected = project_digest(digest, cards);
        let rendered = render_digest_email(&projected, &digest_url, &unsubscribe_url);
        self.deliver_claimed_email(
            DigestEmailRequest {
                delivery_id,
                owner_key,
                recipient: email,
                rendered,
                unsubscribe_url,
            },
            lease,
        )
        .await
    }

    pub(crate) async fn deliver_claimed_profile_unavailable(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        notice: &ProfileUnavailableNotice,
        lease: DeliveryLease,
    ) -> Result<(), DigestEmailError> {
        let delivery_id = profile_unavailable_delivery_id(notice);
        let preference = self.store.preference(owner_key).await?;
        let Some(preference) = preference.filter(EmailPreference::can_receive) else {
            return self
                .store
                .finish(
                    owner_key,
                    &delivery_id,
                    lease,
                    DeliveryCompletion::Suppressed(DeliverySuppressionReason::NotSubscribed),
                )
                .await;
        };
        let email = preference.email.ok_or(DigestEmailError::InvalidRecord)?;
        let Some(tokens) = &self.tokens else {
            return Err(DigestEmailError::NotConfigured);
        };
        let token = tokens.issue(owner_key, &email);
        let dashboard_url = format!("{}/dashboard/", self.public_origin.trim_end_matches('/'));
        let unsubscribe_url = format!(
            "{}/api/v1/daily-coaching/email/unsubscribe?token={token}",
            self.public_origin.trim_end_matches('/')
        );
        let rendered =
            render_profile_unavailable_email(notice.provider, &dashboard_url, &unsubscribe_url);
        self.deliver_claimed_email(
            DigestEmailRequest {
                delivery_id,
                owner_key: owner_key.clone(),
                recipient: email,
                rendered,
                unsubscribe_url,
            },
            lease,
        )
        .await
    }

    async fn deliver_claimed_email(
        &self,
        request: DigestEmailRequest,
        lease: DeliveryLease,
    ) -> Result<(), DigestEmailError> {
        let delivery_id = request.delivery_id.clone();
        let owner_key = request.owner_key.clone();
        let recipient = request.recipient.clone();
        let result = self.delivery.deliver(request).await;
        let completion = match result {
            Ok(receipt) => DeliveryCompletion::Sent {
                provider_message_id: receipt.provider_message_id,
                recipient,
            },
            Err(DigestEmailDeliveryError::Rejected) => {
                DeliveryCompletion::Suppressed(DeliverySuppressionReason::ProviderRejected)
            }
            Err(DigestEmailDeliveryError::Retryable) => {
                tracing::warn!(
                    category = "daily_coaching_email",
                    owner = owner_key.as_str(),
                    delivery_id,
                    "Daily Coaching email provider handoff will retry after the claim lease expires"
                );
                return Ok(());
            }
        };
        self.store
            .finish(&owner_key, &delivery_id, lease, completion)
            .await
    }

    pub(crate) async fn can_unsubscribe(&self, token: &str) -> bool {
        self.unsubscribe_target(token).await.is_some()
    }

    pub(crate) async fn unsubscribe(&self, token: &str, now: DateTime<Utc>) -> bool {
        let Some((owner_key, player_id, email)) = self.unsubscribe_target(token).await else {
            return false;
        };
        self.store
            .set_enabled(&owner_key, &player_id, &email, false, now)
            .await
            .is_ok()
    }

    async fn unsubscribe_target(
        &self,
        token: &str,
    ) -> Option<(DailyCoachingOwnerKey, PlayerId, NormalizedEmail)> {
        let verified = self.tokens.as_ref()?.verify(token)?;
        let preference = self.store.preference(&verified.owner_key).await.ok()??;
        let email = preference.email?;
        verified
            .matches_email(&email)
            .then_some((verified.owner_key, preference.player_id, email))
    }

    pub(crate) fn is_available(&self) -> bool {
        self.tokens.is_some() && self.webhook.is_some()
    }

    pub(crate) async fn ingest_webhook(
        &self,
        headers: WebhookHeaders<'_>,
        raw_body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<(), DigestWebhookError> {
        let verifier = self
            .webhook
            .as_ref()
            .ok_or(DigestWebhookError::Unavailable)?;
        verifier.verify(&headers, raw_body, now)?;
        let event: ResendWebhookEvent =
            serde_json::from_slice(raw_body).map_err(|_| DigestWebhookError::Invalid)?;
        let Some(reason) = suppression_reason(&event.kind) else {
            return Ok(());
        };
        let owner_key = event
            .data
            .tags
            .get("coaching_owner")
            .and_then(|value| DailyCoachingOwnerKey::parse(value.clone()).ok())
            .ok_or(DigestWebhookError::Invalid)?;
        let delivery_id = event
            .data
            .tags
            .get("delivery_id")
            .or_else(|| event.data.tags.get("digest_id"))
            .filter(|value| valid_delivery_id(value))
            .cloned()
            .ok_or(DigestWebhookError::Invalid)?;
        let [recipient] = event.data.to.as_slice() else {
            return Err(DigestWebhookError::Invalid);
        };
        let email = NormalizedEmail::parse(recipient).map_err(|_| DigestWebhookError::Invalid)?;
        if !valid_resend_provider_id(&event.data.email_id) {
            return Err(DigestWebhookError::Invalid);
        }
        self.store
            .suppress(
                SuppressionEvent {
                    event_id: headers.id.to_string(),
                    owner_key,
                    digest_id: delivery_id,
                    email,
                    provider_message_id: event.data.email_id,
                    reason,
                },
                now,
            )
            .await
            .map_err(DigestWebhookError::Store)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DigestEmailRequest {
    pub(crate) delivery_id: String,
    pub(crate) owner_key: DailyCoachingOwnerKey,
    pub(crate) recipient: NormalizedEmail,
    pub(crate) rendered: RenderedDigestEmail,
    pub(crate) unsubscribe_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedDigestEmail {
    pub(crate) subject: String,
    pub(crate) text: String,
    pub(crate) html: String,
}

#[derive(Debug)]
pub(crate) struct DigestEmailReceipt {
    pub(crate) provider_message_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DigestEmailDeliveryError {
    Rejected,
    Retryable,
}

struct DisabledDigestEmailDelivery;

impl DigestEmailDelivery for DisabledDigestEmailDelivery {
    fn deliver<'a>(&'a self, _request: DigestEmailRequest) -> EmailDeliveryFuture<'a> {
        Box::pin(async { Err(DigestEmailDeliveryError::Rejected) })
    }
}

#[derive(Clone)]
struct EmailTokenService {
    key: hmac::Key,
}

impl EmailTokenService {
    fn new(key: [u8; 32]) -> Self {
        Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, &key),
        }
    }

    fn issue(&self, owner_key: &DailyCoachingOwnerKey, email: &NormalizedEmail) -> String {
        let email_hash = hex(&Sha256::digest(email.as_str().as_bytes()));
        let payload = format!("{TOKEN_VERSION}.{}.{email_hash}", owner_key.as_str());
        let signature = hex(hmac::sign(&self.key, payload.as_bytes()).as_ref());
        format!("{payload}.{signature}")
    }

    fn verify(&self, token: &str) -> Option<VerifiedEmailToken> {
        let mut parts = token.split('.');
        let (Some(version), Some(owner), Some(email_hash), Some(signature), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return None;
        };
        if version != TOKEN_VERSION || email_hash.len() != 64 || signature.len() != 64 {
            return None;
        }
        let owner_key = DailyCoachingOwnerKey::parse(owner.to_string()).ok()?;
        let payload = format!("{version}.{owner}.{email_hash}");
        let signature = decode_hex::<32>(signature)?;
        hmac::verify(&self.key, payload.as_bytes(), &signature).ok()?;
        Some(VerifiedEmailToken {
            owner_key,
            email_hash: email_hash.to_string(),
        })
    }
}

struct VerifiedEmailToken {
    owner_key: DailyCoachingOwnerKey,
    email_hash: String,
}

impl VerifiedEmailToken {
    fn matches_email(&self, email: &NormalizedEmail) -> bool {
        self.email_hash == hex(&Sha256::digest(email.as_str().as_bytes()))
    }
}

pub(crate) struct WebhookHeaders<'a> {
    pub(crate) id: &'a str,
    pub(crate) timestamp: &'a str,
    pub(crate) signature: &'a str,
}

#[derive(Clone)]
struct SvixWebhookVerifier {
    key: hmac::Key,
}

impl SvixWebhookVerifier {
    fn new(secret: &str) -> anyhow::Result<Self> {
        let encoded = secret.strip_prefix("whsec_").unwrap_or(secret);
        let bytes = decode_base64(encoded).ok_or_else(|| {
            anyhow::anyhow!("{RESEND_WEBHOOK_SECRET_ENV} must be a valid Svix signing secret")
        })?;
        if bytes.len() < 16 || bytes.len() > 64 {
            anyhow::bail!("{RESEND_WEBHOOK_SECRET_ENV} has an invalid key length");
        }
        Ok(Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, &bytes),
        })
    }

    #[cfg(test)]
    fn for_test(key: [u8; 32]) -> Self {
        Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, &key),
        }
    }

    fn verify(
        &self,
        headers: &WebhookHeaders<'_>,
        raw_body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<(), DigestWebhookError> {
        if headers.id.is_empty() || headers.id.len() > 256 || headers.timestamp.len() > 20 {
            return Err(DigestWebhookError::Invalid);
        }
        let timestamp = headers
            .timestamp
            .parse::<i64>()
            .ok()
            .and_then(DateTime::from_timestamp_secs)
            .ok_or(DigestWebhookError::Invalid)?;
        if (now - timestamp).abs() > SVIX_TOLERANCE {
            return Err(DigestWebhookError::Invalid);
        }
        let mut signed =
            Vec::with_capacity(headers.id.len() + headers.timestamp.len() + raw_body.len() + 2);
        signed.extend_from_slice(headers.id.as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(headers.timestamp.as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(raw_body);
        for candidate in headers.signature.split_ascii_whitespace() {
            let Some(encoded) = candidate.strip_prefix("v1,") else {
                continue;
            };
            let Some(signature) = decode_base64(encoded) else {
                continue;
            };
            if hmac::verify(&self.key, &signed, &signature).is_ok() {
                return Ok(());
            }
        }
        Err(DigestWebhookError::Invalid)
    }
}

#[derive(Deserialize)]
struct ResendWebhookEvent {
    #[serde(rename = "type")]
    kind: String,
    data: ResendWebhookData,
}

#[derive(Deserialize)]
struct ResendWebhookData {
    email_id: String,
    to: Vec<String>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DigestEmailError {
    #[error("digest email is not configured in this deployment")]
    NotConfigured,
    #[error("digest email requires a verified account email")]
    NoVerifiedAccountEmail,
    #[error("digest email store is unavailable")]
    Unavailable,
    #[error("digest email store conflict")]
    Conflict,
    #[error("digest email record is invalid")]
    InvalidRecord,
}

impl From<FirestoreError> for DigestEmailError {
    fn from(error: FirestoreError) -> Self {
        match error {
            FirestoreError::Conflict => Self::Conflict,
            FirestoreError::InvalidDocument => Self::InvalidRecord,
            FirestoreError::Configuration(_)
            | FirestoreError::Transport
            | FirestoreError::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DigestWebhookError {
    #[error("digest webhook is unavailable")]
    Unavailable,
    #[error("digest webhook request is invalid")]
    Invalid,
    #[error(transparent)]
    Store(DigestEmailError),
}

fn valid_delivery_id(value: &str) -> bool {
    let digest = value.len() == "daily-YYYY-MM-DD".len()
        && value
            .strip_prefix("daily-")
            .is_some_and(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok());
    let replay = value
        .strip_prefix("daily-replay-v1-")
        .is_some_and(|identifier| {
            identifier.len() == 32 && identifier.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    digest
        || replay
        || (value.starts_with("profile-unavailable-v1-")
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
}

fn profile_unavailable_delivery_id(notice: &ProfileUnavailableNotice) -> String {
    let provider = match notice.provider {
        DailyCoachingProvider::Lichess => "lichess",
        DailyCoachingProvider::ChessCom => "chess-com",
    };
    let identity_hash = hex(&Sha256::digest(notice.identity_username.as_bytes()));
    format!(
        "profile-unavailable-v1-{provider}-{identity_hash}-{}",
        notice.epoch
    )
}

fn suppression_reason(kind: &str) -> Option<EmailSuppressionReason> {
    match kind {
        "email.bounced" => Some(EmailSuppressionReason::Bounce),
        "email.complained" => Some(EmailSuppressionReason::Complaint),
        _ => None,
    }
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn decode_hex_key(name: &str, value: &str) -> anyhow::Result<[u8; 32]> {
    decode_hex(value)
        .ok_or_else(|| anyhow::anyhow!("{name} must contain exactly 64 hexadecimal characters"))
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        bytes[index] = high << 4 | low;
    }
    Some(bytes)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn decode_base64(value: &str) -> Option<Vec<u8>> {
    if value.is_empty() || value.len() % 4 == 1 || !value.is_ascii() {
        return None;
    }
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    let mut padding = false;
    for byte in value.bytes() {
        if byte == b'=' {
            padding = true;
            continue;
        }
        if padding {
            return None;
        }
        let six = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        accumulator = accumulator << 6 | u32::from(six);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1_u32 << bits).saturating_sub(1);
        }
    }
    if bits > 0 && accumulator != 0 {
        return None;
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_player_and_email_scoped_and_can_only_encode_unsubscribe() {
        let tokens = EmailTokenService::new([0x11; 32]);
        let owner = DailyCoachingOwnerKey::for_player(
            &PlayerId::try_from("player-one".to_string()).unwrap(),
        );
        let email = NormalizedEmail::parse("player@example.com").unwrap();
        let token = tokens.issue(&owner, &email);

        let verified = tokens.verify(&token).unwrap();
        assert_eq!(verified.owner_key, owner);
        assert!(verified.matches_email(&email));
        assert!(!verified.matches_email(&NormalizedEmail::parse("other@example.com").unwrap()));
        assert!(!token.contains("player@example.com"));
    }

    #[test]
    fn base64_decoder_accepts_svix_standard_and_url_alphabets() {
        assert_eq!(decode_base64("aGVsbG8="), Some(b"hello".to_vec()));
        assert_eq!(decode_base64("__8="), Some(vec![0xff, 0xff]));
    }

    #[test]
    fn replay_delivery_ids_are_accepted_for_signed_webhooks() {
        assert!(valid_delivery_id(
            "daily-replay-v1-0123456789abcdef0123456789abcdef"
        ));
        assert!(!valid_delivery_id("daily-replay-v1-not-a-uuid"));
    }

    #[tokio::test]
    async fn delivery_document_creation_is_the_single_send_claim() {
        let store = Arc::new(InMemoryDigestEmailStore::default());
        let owner = DailyCoachingOwnerKey::for_player(
            &PlayerId::try_from("claim-player".to_string()).unwrap(),
        );
        let now = DateTime::parse_from_rfc3339("2026-08-11T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (left, right) = tokio::join!(
            store.claim(
                &owner,
                "daily-2026-08-10",
                now,
                DELIVERY_CLAIM_TTL,
                DELIVERY_RETRY_HORIZON,
            ),
            store.claim(
                &owner,
                "daily-2026-08-10",
                now,
                DELIVERY_CLAIM_TTL,
                DELIVERY_RETRY_HORIZON,
            ),
        );

        assert_eq!(
            [left.unwrap(), right.unwrap()]
                .into_iter()
                .filter(|claim| matches!(claim, DeliveryClaim::Claimed(_)))
                .count(),
            1
        );
    }

    #[test]
    fn only_bounces_and_complaints_suppress_recurring_digest_email() {
        assert_eq!(
            suppression_reason("email.bounced"),
            Some(EmailSuppressionReason::Bounce)
        );
        assert_eq!(
            suppression_reason("email.complained"),
            Some(EmailSuppressionReason::Complaint)
        );
        assert_eq!(suppression_reason("email.delivered"), None);
    }

    #[test]
    fn firestore_delivery_claim_uses_the_frozen_player_digest_path() {
        let owner = DailyCoachingOwnerKey::for_player(
            &PlayerId::try_from("path-player".to_string()).unwrap(),
        );
        assert_eq!(
            FirestoreDigestEmailStore::delivery_path(&owner, "daily-2026-08-10").join("/"),
            format!(
                "users/{}/coachingDigestDeliveries/daily-2026-08-10",
                owner.as_str()
            )
        );
    }
}
