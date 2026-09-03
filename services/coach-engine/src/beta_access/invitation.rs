use std::{fmt::Write, future::Future, pin::Pin, sync::Arc};

use chrono::{DateTime, Utc};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    hmac,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};

use crate::{deployment::DeploymentEnvironment, review_session_contract::PlayerId};

use super::{BetaAccessConfigurationError, NormalizedEmail};

mod resend;

use resend::ResendInvitationDelivery;

const AUTHENTICATOR_KEY_ENV: &str = "BETA_INVITATION_HMAC_KEY_V1";
const ENCRYPTION_KEY_ENV: &str = "BETA_INVITATION_ENCRYPTION_KEY_V1";
const RESEND_API_KEY_ENV: &str = "BETA_RESEND_API_KEY";
const AUTHENTICATOR_VERSION: u8 = 1;
const ENCRYPTION_VERSION: u8 = 1;
const RECORD_VERSION: u8 = 1;
const RANDOM_VALUE_BYTES: usize = 16;
const NONCE_BYTES: usize = 12;

type DeliveryFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<InvitationDeliveryReceipt, InvitationDeliveryError>> + Send + 'a,
    >,
>;

pub(crate) trait InvitationEmailDelivery: Send + Sync {
    fn deliver<'a>(&'a self, request: InvitationDeliveryRequest) -> DeliveryFuture<'a>;
}

pub(crate) struct InvitationDeliveryRequest {
    pub delivery_attempt: u32,
    pub invitation_id: String,
    pub email: NormalizedEmail,
    pub code: String,
}

#[derive(Debug)]
pub(crate) struct InvitationDeliveryReceipt {
    pub provider_message_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InvitationDeliveryError {
    Rejected,
    Retryable,
}

#[derive(Clone)]
pub(super) struct InvitationIssuer {
    cryptography: InvitationCryptography,
    delivery: Arc<dyn InvitationEmailDelivery>,
}

impl InvitationIssuer {
    fn new(
        authenticator_key: [u8; 32],
        encryption_key: [u8; 32],
        delivery: Arc<dyn InvitationEmailDelivery>,
    ) -> Result<Self, BetaAccessConfigurationError> {
        if authenticator_key == encryption_key {
            return Err(BetaAccessConfigurationError::InvitationCryptography);
        }
        Ok(Self {
            cryptography: InvitationCryptography::new(authenticator_key, encryption_key)?,
            delivery,
        })
    }

    pub(super) fn prepare(
        &self,
        request_id: String,
        email: NormalizedEmail,
        now: DateTime<Utc>,
    ) -> Result<PreparedInvitation, BetaAccessConfigurationError> {
        self.cryptography.prepare(request_id, email, now)
    }

    pub(super) async fn deliver(
        &self,
        invitation: &PreparedInvitation,
        delivery_attempt: u32,
    ) -> InvitationDeliveryAttempt {
        let result = self
            .delivery
            .deliver(InvitationDeliveryRequest {
                delivery_attempt,
                invitation_id: invitation.stored.id.clone(),
                email: invitation.stored.email.clone(),
                code: invitation.code.expose().to_string(),
            })
            .await;
        match result {
            Ok(receipt) => InvitationDeliveryAttempt::Sent {
                provider_message_id: receipt.provider_message_id,
            },
            Err(error) => InvitationDeliveryAttempt::Failed {
                retryable: error == InvitationDeliveryError::Retryable,
            },
        }
    }

    pub(super) fn recover(
        &self,
        stored: &StoredInvitation,
    ) -> Result<PreparedInvitation, BetaAccessConfigurationError> {
        let code = self.cryptography.decrypt(stored)?;
        if !self
            .cryptography
            .verify(stored, &stored.email, code.expose())
        {
            return Err(BetaAccessConfigurationError::InvitationCryptography);
        }
        Ok(PreparedInvitation {
            code,
            stored: stored.clone(),
        })
    }

    pub(super) fn lookup_id(&self, code: &InvitationCode) -> String {
        self.cryptography.lookup_id(code)
    }

    pub(super) fn verify(
        &self,
        stored: &StoredInvitation,
        email: &NormalizedEmail,
        code: &InvitationCode,
    ) -> bool {
        self.cryptography.verify(stored, email, code.expose())
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        delivery: Arc<dyn InvitationEmailDelivery>,
    ) -> Result<Self, BetaAccessConfigurationError> {
        Self::new([0x11; 32], [0x22; 32], delivery)
    }
}

pub(super) fn configured_invitation_issuer(
    environment: DeploymentEnvironment,
) -> anyhow::Result<Option<InvitationIssuer>> {
    let authenticator_key = optional_env(AUTHENTICATOR_KEY_ENV);
    let encryption_key = optional_env(ENCRYPTION_KEY_ENV);
    let resend_api_key = optional_env(RESEND_API_KEY_ENV);
    let present = [
        authenticator_key.is_some(),
        encryption_key.is_some(),
        resend_api_key.is_some(),
    ];
    if environment == DeploymentEnvironment::Production {
        if present.into_iter().any(|value| value) {
            anyhow::bail!(
                "{AUTHENTICATOR_KEY_ENV}, {ENCRYPTION_KEY_ENV}, and {RESEND_API_KEY_ENV} must be absent in production"
            );
        }
        return Ok(None);
    }
    if present.into_iter().all(|value| !value) {
        tracing::warn!(
            category = "configuration",
            "beta invitation grant and delivery are disabled until staging invitation secrets are provisioned"
        );
        return Ok(None);
    }
    let (Some(authenticator_key), Some(encryption_key), Some(resend_api_key)) =
        (authenticator_key, encryption_key, resend_api_key)
    else {
        anyhow::bail!(
            "{AUTHENTICATOR_KEY_ENV}, {ENCRYPTION_KEY_ENV}, and {RESEND_API_KEY_ENV} are required together"
        );
    };
    Ok(Some(InvitationIssuer::new(
        decode_key(AUTHENTICATOR_KEY_ENV, &authenticator_key)?,
        decode_key(ENCRYPTION_KEY_ENV, &encryption_key)?,
        Arc::new(ResendInvitationDelivery::new(resend_api_key)?),
    )?))
}

pub(super) struct PreparedInvitation {
    code: InvitationCode,
    pub(super) stored: StoredInvitation,
}

pub(super) struct InvitationCode(String);

impl InvitationCode {
    pub(super) fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.len() != RANDOM_VALUE_BYTES * 2
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        Some(Self(value.to_ascii_lowercase()))
    }

    pub(super) fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoredInvitation {
    pub(super) authenticator: String,
    pub(super) authenticator_version: u8,
    pub(super) ciphertext: String,
    pub(super) created_at: DateTime<Utc>,
    pub(super) delivery_attempt: u32,
    pub(super) delivery_retryable: Option<bool>,
    pub(super) delivery_status: InvitationDeliveryStatus,
    pub(super) email: NormalizedEmail,
    pub(super) encryption_nonce: String,
    pub(super) encryption_version: u8,
    pub(super) id: String,
    pub(super) lookup_id: String,
    pub(super) provider_message_id: Option<String>,
    pub(super) record_version: u8,
    pub(super) redeemed_at: Option<DateTime<Utc>>,
    pub(super) redeemed_by: Option<PlayerId>,
    pub(super) request_id: String,
    pub(super) status: InvitationStatus,
}

impl StoredInvitation {
    pub(super) fn valid_shape(&self) -> bool {
        self.authenticator_version == AUTHENTICATOR_VERSION
            && self.encryption_version == ENCRYPTION_VERSION
            && self.record_version == RECORD_VERSION
            && self.delivery_attempt > 0
            && opaque_hex(&self.id, RANDOM_VALUE_BYTES)
            && opaque_hex(&self.lookup_id, 32)
            && opaque_hex(&self.request_id, 32)
            && opaque_hex(&self.authenticator, 32)
            && opaque_hex(&self.encryption_nonce, NONCE_BYTES)
            && opaque_hex(&self.ciphertext, RANDOM_VALUE_BYTES * 2 + aead::MAX_TAG_LEN)
            && match self.status {
                InvitationStatus::Issued | InvitationStatus::Revoked => {
                    self.redeemed_at.is_none() && self.redeemed_by.is_none()
                }
                InvitationStatus::Redeemed => {
                    self.redeemed_at
                        .is_some_and(|redeemed_at| redeemed_at >= self.created_at)
                        && self.redeemed_by.is_some()
                }
            }
            && match self.delivery_status {
                InvitationDeliveryStatus::Pending => {
                    self.delivery_retryable.is_none() && self.provider_message_id.is_none()
                }
                InvitationDeliveryStatus::Sent => {
                    self.delivery_retryable.is_none()
                        && self
                            .provider_message_id
                            .as_deref()
                            .is_some_and(valid_provider_id)
                }
                InvitationDeliveryStatus::Failed => {
                    self.delivery_retryable.is_some() && self.provider_message_id.is_none()
                }
            }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum InvitationDeliveryStatus {
    Pending,
    Sent,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum InvitationStatus {
    Issued,
    Revoked,
    Redeemed,
}

pub(super) enum InvitationDeliveryAttempt {
    Sent { provider_message_id: String },
    Failed { retryable: bool },
}

impl InvitationDeliveryAttempt {
    pub(super) fn status(&self) -> InvitationDeliveryStatus {
        match self {
            Self::Sent { .. } => InvitationDeliveryStatus::Sent,
            Self::Failed { .. } => InvitationDeliveryStatus::Failed,
        }
    }

    pub(super) fn metadata(&self) -> (Option<bool>, Option<&str>) {
        match self {
            Self::Sent {
                provider_message_id,
            } => (None, Some(provider_message_id)),
            Self::Failed { retryable } => (Some(*retryable), None),
        }
    }
}

#[derive(Clone)]
struct InvitationCryptography {
    authenticator_key: Arc<hmac::Key>,
    encryption_key: Arc<[u8; 32]>,
    random: SystemRandom,
}

impl InvitationCryptography {
    fn new(
        authenticator_key: [u8; 32],
        encryption_key: [u8; 32],
    ) -> Result<Self, BetaAccessConfigurationError> {
        UnboundKey::new(&aead::AES_256_GCM, &encryption_key)
            .map_err(|_| BetaAccessConfigurationError::InvitationCryptography)?;
        Ok(Self {
            authenticator_key: Arc::new(hmac::Key::new(hmac::HMAC_SHA256, &authenticator_key)),
            encryption_key: Arc::new(encryption_key),
            random: SystemRandom::new(),
        })
    }

    fn prepare(
        &self,
        request_id: String,
        email: NormalizedEmail,
        now: DateTime<Utc>,
    ) -> Result<PreparedInvitation, BetaAccessConfigurationError> {
        let id = random_hex(&self.random, RANDOM_VALUE_BYTES)?;
        let code = InvitationCode(random_hex(&self.random, RANDOM_VALUE_BYTES)?);
        let lookup_id = self.lookup_id(&code);
        let context = canonical_context(&id, &email);
        let mut authenticated = context.clone();
        append_field(&mut authenticated, code.expose().as_bytes())?;
        let authenticator = hex(hmac::sign(&self.authenticator_key, &authenticated).as_ref());

        let mut nonce_bytes = [0u8; NONCE_BYTES];
        self.random
            .fill(&mut nonce_bytes)
            .map_err(|_| BetaAccessConfigurationError::Randomness)?;
        let mut ciphertext = code.expose().as_bytes().to_vec();
        encryption_key(&self.encryption_key)?
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(context),
                &mut ciphertext,
            )
            .map_err(|_| BetaAccessConfigurationError::InvitationCryptography)?;

        Ok(PreparedInvitation {
            code,
            stored: StoredInvitation {
                authenticator,
                authenticator_version: AUTHENTICATOR_VERSION,
                ciphertext: hex(&ciphertext),
                created_at: now,
                delivery_attempt: 1,
                delivery_retryable: None,
                delivery_status: InvitationDeliveryStatus::Pending,
                email,
                encryption_nonce: hex(&nonce_bytes),
                encryption_version: ENCRYPTION_VERSION,
                id,
                lookup_id,
                provider_message_id: None,
                record_version: RECORD_VERSION,
                redeemed_at: None,
                redeemed_by: None,
                request_id,
                status: InvitationStatus::Issued,
            },
        })
    }

    fn verify(&self, stored: &StoredInvitation, email: &NormalizedEmail, code: &str) -> bool {
        if !stored.valid_shape() || &stored.email != email || !opaque_hex(code, RANDOM_VALUE_BYTES)
        {
            return false;
        }
        let code = InvitationCode(code.to_string());
        if stored.lookup_id != self.lookup_id(&code) {
            return false;
        }
        let mut authenticated = canonical_context(&stored.id, email);
        if append_field(&mut authenticated, code.expose().as_bytes()).is_err() {
            return false;
        }
        let Ok(expected) = decode_hex(&stored.authenticator) else {
            return false;
        };
        hmac::verify(&self.authenticator_key, &authenticated, &expected).is_ok()
    }

    fn lookup_id(&self, code: &InvitationCode) -> String {
        let mut context = b"chenchess-beta-invitation-lookup-v1\0".to_vec();
        append_field(&mut context, code.expose().as_bytes()).expect("bounded invitation code");
        hex(hmac::sign(&self.authenticator_key, &context).as_ref())
    }

    fn decrypt(
        &self,
        stored: &StoredInvitation,
    ) -> Result<InvitationCode, BetaAccessConfigurationError> {
        if !stored.valid_shape() {
            return Err(BetaAccessConfigurationError::InvitationCryptography);
        }
        let nonce = decode_fixed::<NONCE_BYTES>(&stored.encryption_nonce)?;
        let mut ciphertext = decode_hex(&stored.ciphertext)?;
        let plaintext = encryption_key(&self.encryption_key)?
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(canonical_context(&stored.id, &stored.email)),
                &mut ciphertext,
            )
            .map_err(|_| BetaAccessConfigurationError::InvitationCryptography)?;
        let code = std::str::from_utf8(plaintext)
            .map_err(|_| BetaAccessConfigurationError::InvitationCryptography)?
            .to_string();
        if !opaque_hex(&code, RANDOM_VALUE_BYTES) {
            return Err(BetaAccessConfigurationError::InvitationCryptography);
        }
        Ok(InvitationCode(code))
    }
}

fn encryption_key(key: &[u8; 32]) -> Result<LessSafeKey, BetaAccessConfigurationError> {
    UnboundKey::new(&aead::AES_256_GCM, key)
        .map(LessSafeKey::new)
        .map_err(|_| BetaAccessConfigurationError::InvitationCryptography)
}

fn canonical_context(id: &str, email: &NormalizedEmail) -> Vec<u8> {
    let mut context = b"chenchess-beta-invitation-v1\0".to_vec();
    append_field(&mut context, id.as_bytes()).expect("bounded invitation ID");
    append_field(&mut context, email.as_str().as_bytes()).expect("bounded normalized email");
    context
}

fn append_field(target: &mut Vec<u8>, value: &[u8]) -> Result<(), BetaAccessConfigurationError> {
    let length = u16::try_from(value.len())
        .map_err(|_| BetaAccessConfigurationError::InvitationCryptography)?;
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
    Ok(())
}

fn random_hex(
    random: &SystemRandom,
    byte_count: usize,
) -> Result<String, BetaAccessConfigurationError> {
    let mut bytes = vec![0u8; byte_count];
    random
        .fill(&mut bytes)
        .map_err(|_| BetaAccessConfigurationError::Randomness)?;
    Ok(hex(&bytes))
}

fn decode_key(name: &'static str, value: &str) -> anyhow::Result<[u8; 32]> {
    decode_fixed(value)
        .map_err(|_| anyhow::anyhow!("{name} must contain exactly 64 lowercase hex characters"))
}

fn decode_fixed<const N: usize>(value: &str) -> Result<[u8; N], BetaAccessConfigurationError> {
    let bytes = decode_hex(value)?;
    bytes
        .try_into()
        .map_err(|_| BetaAccessConfigurationError::InvitationCryptography)
}

fn decode_hex(value: &str) -> Result<Vec<u8>, BetaAccessConfigurationError> {
    if !value.len().is_multiple_of(2) || !value.is_ascii() {
        return Err(BetaAccessConfigurationError::InvitationCryptography);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_digit(value: u8) -> Result<u8, BetaAccessConfigurationError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(BetaAccessConfigurationError::InvitationCryptography),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn opaque_hex(value: &str, byte_count: usize) -> bool {
    value.len() == byte_count * 2
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn valid_provider_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingDelivery {
        requests: Mutex<Vec<InvitationDeliveryRequest>>,
    }

    impl InvitationEmailDelivery for RecordingDelivery {
        fn deliver<'a>(&'a self, request: InvitationDeliveryRequest) -> DeliveryFuture<'a> {
            Box::pin(async move {
                self.requests.lock().unwrap().push(request);
                Ok(InvitationDeliveryReceipt {
                    provider_message_id: "provider-message-1".to_string(),
                })
            })
        }
    }

    #[tokio::test]
    async fn invitation_secrets_are_independent_bound_and_recoverable_only_with_the_key() {
        let delivery = Arc::new(RecordingDelivery::default());
        let issuer = InvitationIssuer::for_test(delivery.clone()).unwrap();
        let email = NormalizedEmail::parse("player@example.com").unwrap();
        let first = issuer
            .prepare("a".repeat(64), email.clone(), Utc::now())
            .unwrap();
        let second = issuer
            .prepare("a".repeat(64), email.clone(), Utc::now())
            .unwrap();

        assert_eq!(first.stored.id.len(), 32);
        assert_eq!(first.code.expose().len(), 32);
        assert_ne!(first.stored.id, second.stored.id);
        assert_ne!(first.code.expose(), second.code.expose());
        assert!(!first.stored.ciphertext.contains(first.code.expose()));
        let recovered = issuer.recover(&first.stored).unwrap();
        assert_eq!(recovered.code.expose(), first.code.expose());

        let wrong_email = NormalizedEmail::parse("other@example.com").unwrap();
        let mut tampered = first.stored.clone();
        tampered.email = wrong_email;
        assert!(issuer.recover(&tampered).is_err());

        let attempt = issuer.deliver(&first, first.stored.delivery_attempt).await;
        assert_eq!(attempt.status(), InvitationDeliveryStatus::Sent);
        let captured = delivery.requests.lock().unwrap();
        assert_eq!(captured[0].code, first.code.expose());
        assert_eq!(captured[0].invitation_id, first.stored.id);
    }
}
