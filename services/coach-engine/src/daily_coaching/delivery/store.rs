use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Mutex};

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    beta_access::NormalizedEmail,
    firestore::{FirestoreDatabase, FirestoreError},
    review_durability::path::hashed_path_segment,
    review_session_contract::PlayerId,
};

use super::{
    valid_resend_provider_id, DigestEmailError, DigestEmailReadiness, EMAIL_RECORD_VERSION,
};
use crate::daily_coaching::DailyCoachingOwnerKey;

type EmailStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, DigestEmailError>> + Send + 'a>>;

pub(crate) trait DigestEmailStore: Send + Sync {
    fn observe_verified_email<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
        email: Option<&'a NormalizedEmail>,
        now: DateTime<Utc>,
    ) -> EmailStoreFuture<'a, ()>;

    fn set_enabled<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
        email: &'a NormalizedEmail,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> EmailStoreFuture<'a, ()>;

    fn preference<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> EmailStoreFuture<'a, Option<EmailPreference>>;

    fn claim<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
        claimed_at: DateTime<Utc>,
        lease_ttl: TimeDelta,
        retry_horizon: TimeDelta,
    ) -> EmailStoreFuture<'a, DeliveryClaim>;

    fn finish<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
        lease: DeliveryLease,
        completion: DeliveryCompletion,
    ) -> EmailStoreFuture<'a, ()>;

    fn suppress<'a>(
        &'a self,
        event: SuppressionEvent,
        now: DateTime<Utc>,
    ) -> EmailStoreFuture<'a, ()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EmailPreference {
    record_version: u8,
    owner_key: DailyCoachingOwnerKey,
    pub(super) player_id: PlayerId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) email: Option<NormalizedEmail>,
    pub(super) enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    suppressed_email: Option<NormalizedEmail>,
    updated_at: DateTime<Utc>,
}

impl EmailPreference {
    fn new(
        owner_key: DailyCoachingOwnerKey,
        player_id: PlayerId,
        email: NormalizedEmail,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            record_version: EMAIL_RECORD_VERSION,
            owner_key,
            player_id,
            email: Some(email),
            enabled: true,
            suppressed_email: None,
            updated_at: now,
        }
    }

    pub(crate) fn can_receive(&self) -> bool {
        self.enabled
            && self
                .email
                .as_ref()
                .is_some_and(|email| self.suppressed_email.as_ref() != Some(email))
    }

    pub(super) fn readiness(&self) -> DigestEmailReadiness {
        let Some(email) = &self.email else {
            return DigestEmailReadiness::NoVerifiedEmail;
        };
        if !self.enabled {
            return DigestEmailReadiness::Disabled;
        }
        if self.suppressed_email.as_ref() == Some(email) {
            return DigestEmailReadiness::Suppressed;
        }
        DigestEmailReadiness::Ready
    }

    fn validate_for(&self, owner_key: &DailyCoachingOwnerKey) -> Result<(), DigestEmailError> {
        if self.record_version != EMAIL_RECORD_VERSION
            || &self.owner_key != owner_key
            || DailyCoachingOwnerKey::for_player(&self.player_id) != *owner_key
            || self.updated_at.timestamp_millis() <= 0
        {
            return Err(DigestEmailError::InvalidRecord);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeliveryLease {
    claimed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryClaim {
    Claimed(DeliveryLease),
    AlreadyClaimed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeliveryCompletion {
    Sent {
        provider_message_id: String,
        recipient: NormalizedEmail,
    },
    Suppressed(DeliverySuppressionReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DeliverySuppressionReason {
    NotSubscribed,
    ProviderRejected,
    ProviderHandoffFailed,
    RetryWindowExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum DeliveryStatus {
    Pending,
    Sent,
    Suppressed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeliveryRecord {
    record_version: u8,
    owner_key: DailyCoachingOwnerKey,
    digest_id: String,
    first_claimed_at: DateTime<Utc>,
    claimed_at: DateTime<Utc>,
    attempt_count: u32,
    status: DeliveryStatus,
    recipient: Option<NormalizedEmail>,
    provider_message_id: Option<String>,
    suppression_reason: Option<DeliverySuppressionReason>,
}

impl DeliveryRecord {
    fn pending(
        owner_key: DailyCoachingOwnerKey,
        digest_id: String,
        claimed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            record_version: EMAIL_RECORD_VERSION,
            owner_key,
            digest_id,
            first_claimed_at: claimed_at,
            claimed_at,
            attempt_count: 1,
            status: DeliveryStatus::Pending,
            recipient: None,
            provider_message_id: None,
            suppression_reason: None,
        }
    }

    fn reacquire(&mut self, claimed_at: DateTime<Utc>) {
        self.claimed_at = claimed_at;
        self.attempt_count = self.attempt_count.saturating_add(1);
    }

    fn is_recoverable(&self, now: DateTime<Utc>, lease_ttl: TimeDelta) -> bool {
        self.status == DeliveryStatus::Pending
            && lease_ttl > TimeDelta::zero()
            && now.signed_duration_since(self.claimed_at) >= lease_ttl
    }

    fn retry_window_expired(&self, now: DateTime<Utc>, retry_horizon: TimeDelta) -> bool {
        self.status == DeliveryStatus::Pending
            && retry_horizon > TimeDelta::zero()
            && now.signed_duration_since(self.first_claimed_at) >= retry_horizon
    }

    fn finish(&mut self, completion: DeliveryCompletion) {
        match completion {
            DeliveryCompletion::Sent {
                provider_message_id,
                recipient,
            } => {
                self.status = DeliveryStatus::Sent;
                self.recipient = Some(recipient);
                self.provider_message_id = Some(provider_message_id);
                self.suppression_reason = None;
            }
            DeliveryCompletion::Suppressed(reason) => {
                self.status = DeliveryStatus::Suppressed;
                self.recipient = None;
                self.provider_message_id = None;
                self.suppression_reason = Some(reason);
            }
        }
    }

    fn matches_sent(&self, event: &SuppressionEvent) -> bool {
        self.status == DeliveryStatus::Sent
            && self.owner_key == event.owner_key
            && self.digest_id == event.digest_id
            && self.recipient.as_ref() == Some(&event.email)
            && self.provider_message_id.as_deref() == Some(&event.provider_message_id)
    }

    fn validate_for(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        digest_id: &str,
    ) -> Result<(), DigestEmailError> {
        let completion_valid = match self.status {
            DeliveryStatus::Pending => {
                self.recipient.is_none()
                    && self.provider_message_id.is_none()
                    && self.suppression_reason.is_none()
            }
            DeliveryStatus::Sent => {
                self.recipient.is_some()
                    && self
                        .provider_message_id
                        .as_deref()
                        .is_some_and(valid_resend_provider_id)
                    && self.suppression_reason.is_none()
            }
            DeliveryStatus::Suppressed => {
                self.recipient.is_none()
                    && self.provider_message_id.is_none()
                    && self.suppression_reason.is_some()
            }
        };
        if self.record_version != EMAIL_RECORD_VERSION
            || &self.owner_key != owner_key
            || self.digest_id != digest_id
            || self.digest_id.trim().is_empty()
            || self.first_claimed_at.timestamp_millis() <= 0
            || self.claimed_at.timestamp_millis() <= 0
            || self.claimed_at < self.first_claimed_at
            || self.attempt_count == 0
            || !completion_valid
        {
            return Err(DigestEmailError::InvalidRecord);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SuppressionEvent {
    pub(super) event_id: String,
    pub(super) owner_key: DailyCoachingOwnerKey,
    pub(super) digest_id: String,
    pub(super) email: NormalizedEmail,
    pub(super) provider_message_id: String,
    pub(super) reason: EmailSuppressionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum EmailSuppressionReason {
    Bounce,
    Complaint,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EmailSuppressionRecord {
    record_version: u8,
    owner_key: DailyCoachingOwnerKey,
    email: NormalizedEmail,
    provider_message_id: String,
    event_id_hash: String,
    reason: EmailSuppressionReason,
    suppressed_at: DateTime<Utc>,
}

#[derive(Default)]
pub(crate) struct InMemoryDigestEmailStore {
    inner: Mutex<InMemoryEmailState>,
}

#[derive(Default)]
struct InMemoryEmailState {
    preferences: BTreeMap<DailyCoachingOwnerKey, EmailPreference>,
    deliveries: BTreeMap<(DailyCoachingOwnerKey, String), DeliveryRecord>,
    suppressions: BTreeMap<(DailyCoachingOwnerKey, String), EmailSuppressionRecord>,
}

impl DigestEmailStore for InMemoryDigestEmailStore {
    fn observe_verified_email<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
        email: Option<&'a NormalizedEmail>,
        now: DateTime<Utc>,
    ) -> EmailStoreFuture<'a, ()> {
        Box::pin(async move {
            let mut inner = self
                .inner
                .lock()
                .expect("digest email store is not poisoned");
            match inner.preferences.get_mut(owner_key) {
                Some(preference) => {
                    preference.validate_for(owner_key)?;
                    if preference.player_id != *player_id {
                        return Err(DigestEmailError::InvalidRecord);
                    }
                    if preference.email.as_ref() != email {
                        preference.email = email.cloned();
                        preference.updated_at = now;
                    }
                }
                None => {
                    let Some(email) = email else { return Ok(()) };
                    inner.preferences.insert(
                        owner_key.clone(),
                        EmailPreference::new(
                            owner_key.clone(),
                            player_id.clone(),
                            email.clone(),
                            now,
                        ),
                    );
                }
            }
            Ok(())
        })
    }

    fn set_enabled<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
        email: &'a NormalizedEmail,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> EmailStoreFuture<'a, ()> {
        Box::pin(async move {
            self.observe_verified_email(owner_key, player_id, Some(email), now)
                .await?;
            let mut inner = self
                .inner
                .lock()
                .expect("digest email store is not poisoned");
            let preference = inner
                .preferences
                .get_mut(owner_key)
                .ok_or(DigestEmailError::InvalidRecord)?;
            preference.enabled = enabled;
            preference.updated_at = now;
            Ok(())
        })
    }

    fn preference<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> EmailStoreFuture<'a, Option<EmailPreference>> {
        Box::pin(async move {
            let preference = self
                .inner
                .lock()
                .expect("digest email store is not poisoned")
                .preferences
                .get(owner_key)
                .cloned();
            if let Some(preference) = &preference {
                preference.validate_for(owner_key)?;
            }
            Ok(preference)
        })
    }

    fn claim<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
        claimed_at: DateTime<Utc>,
        lease_ttl: TimeDelta,
        retry_horizon: TimeDelta,
    ) -> EmailStoreFuture<'a, DeliveryClaim> {
        Box::pin(async move {
            if lease_ttl <= TimeDelta::zero() || retry_horizon <= lease_ttl {
                return Err(DigestEmailError::InvalidRecord);
            }
            let mut inner = self
                .inner
                .lock()
                .expect("digest email store is not poisoned");
            let key = (owner_key.clone(), digest_id.to_string());
            if let Some(record) = inner.deliveries.get_mut(&key) {
                record.validate_for(owner_key, digest_id)?;
                if record.retry_window_expired(claimed_at, retry_horizon) {
                    record.finish(DeliveryCompletion::Suppressed(
                        DeliverySuppressionReason::RetryWindowExpired,
                    ));
                    record.validate_for(owner_key, digest_id)?;
                    return Ok(DeliveryClaim::AlreadyClaimed);
                }
                if !record.is_recoverable(claimed_at, lease_ttl) {
                    return Ok(DeliveryClaim::AlreadyClaimed);
                }
                record.reacquire(claimed_at);
                record.validate_for(owner_key, digest_id)?;
                return Ok(DeliveryClaim::Claimed(DeliveryLease { claimed_at }));
            }
            let record =
                DeliveryRecord::pending(owner_key.clone(), digest_id.to_string(), claimed_at);
            record.validate_for(owner_key, digest_id)?;
            inner.deliveries.insert(key, record);
            Ok(DeliveryClaim::Claimed(DeliveryLease { claimed_at }))
        })
    }

    fn finish<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
        lease: DeliveryLease,
        completion: DeliveryCompletion,
    ) -> EmailStoreFuture<'a, ()> {
        Box::pin(async move {
            let mut inner = self
                .inner
                .lock()
                .expect("digest email store is not poisoned");
            let record = inner
                .deliveries
                .get_mut(&(owner_key.clone(), digest_id.to_string()))
                .ok_or(DigestEmailError::InvalidRecord)?;
            if record.status != DeliveryStatus::Pending || record.claimed_at != lease.claimed_at {
                return Ok(());
            }
            record.finish(completion);
            record.validate_for(owner_key, digest_id)
        })
    }

    fn suppress<'a>(
        &'a self,
        event: SuppressionEvent,
        now: DateTime<Utc>,
    ) -> EmailStoreFuture<'a, ()> {
        Box::pin(async move {
            let mut inner = self
                .inner
                .lock()
                .expect("digest email store is not poisoned");
            let event_id_hash = hashed_path_segment(&event.event_id);
            let suppression_key = (event.owner_key.clone(), event_id_hash.clone());
            if inner.suppressions.contains_key(&suppression_key) {
                return Ok(());
            }
            let Some(delivery) = inner
                .deliveries
                .get(&(event.owner_key.clone(), event.digest_id.clone()))
            else {
                return Ok(());
            };
            delivery.validate_for(&event.owner_key, &event.digest_id)?;
            if !delivery.matches_sent(&event) {
                return Ok(());
            }
            if let Some(preference) = inner.preferences.get_mut(&event.owner_key) {
                preference.validate_for(&event.owner_key)?;
                if preference.email.as_ref() == Some(&event.email) {
                    preference.suppressed_email = Some(event.email.clone());
                    preference.updated_at = now;
                }
            }
            inner.suppressions.insert(
                suppression_key,
                EmailSuppressionRecord {
                    record_version: EMAIL_RECORD_VERSION,
                    owner_key: event.owner_key,
                    email: event.email,
                    provider_message_id: event.provider_message_id,
                    event_id_hash,
                    reason: event.reason,
                    suppressed_at: now,
                },
            );
            Ok(())
        })
    }
}

pub(super) struct FirestoreDigestEmailStore {
    database: FirestoreDatabase,
}

impl FirestoreDigestEmailStore {
    pub(super) fn new(database: FirestoreDatabase) -> Self {
        Self { database }
    }

    fn preference_path(owner_key: &DailyCoachingOwnerKey) -> [String; 4] {
        [
            "users".to_string(),
            owner_key.as_str().to_string(),
            "dailyCoachingEmail".to_string(),
            "state".to_string(),
        ]
    }

    pub(super) fn delivery_path(owner_key: &DailyCoachingOwnerKey, digest_id: &str) -> [String; 4] {
        [
            "users".to_string(),
            owner_key.as_str().to_string(),
            "coachingDigestDeliveries".to_string(),
            digest_id.to_string(),
        ]
    }

    fn suppression_path(owner_key: &DailyCoachingOwnerKey, event_id: &str) -> [String; 4] {
        [
            "users".to_string(),
            owner_key.as_str().to_string(),
            "coachingEmailSuppressions".to_string(),
            hashed_path_segment(event_id),
        ]
    }

    async fn mutate_preference(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        player_id: &PlayerId,
        email: Option<&NormalizedEmail>,
        enabled: Option<bool>,
        now: DateTime<Utc>,
    ) -> Result<(), DigestEmailError> {
        let owned = Self::preference_path(owner_key);
        let path = owned.iter().map(String::as_str).collect::<Vec<_>>();
        for attempt in 0..4 {
            let transaction = self.database.begin_transaction().await?;
            let stored = self
                .database
                .get_document_in_transaction::<EmailPreference>(&path, &transaction)
                .await?;
            let existed = stored.is_some();
            let mut preference = match stored {
                Some(preference) => preference,
                None => {
                    let Some(email) = email else {
                        self.database.rollback_transaction(transaction).await?;
                        return Ok(());
                    };
                    EmailPreference::new(owner_key.clone(), player_id.clone(), email.clone(), now)
                }
            };
            preference.validate_for(owner_key)?;
            if preference.player_id != *player_id {
                self.database.rollback_transaction(transaction).await?;
                return Err(DigestEmailError::InvalidRecord);
            }
            preference.email = email.cloned();
            if let Some(enabled) = enabled {
                preference.enabled = enabled;
            }
            preference.updated_at = now;
            let write = if existed {
                self.database.update_write(&path, &preference, &[])?
            } else {
                self.database.create_write(&path, &preference, &[])?
            };
            match self
                .database
                .commit_transaction(transaction, vec![write])
                .await
            {
                Ok(()) => return Ok(()),
                Err(FirestoreError::Conflict) if attempt < 3 => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(DigestEmailError::Conflict)
    }
}

impl DigestEmailStore for FirestoreDigestEmailStore {
    fn observe_verified_email<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
        email: Option<&'a NormalizedEmail>,
        now: DateTime<Utc>,
    ) -> EmailStoreFuture<'a, ()> {
        Box::pin(async move {
            self.mutate_preference(owner_key, player_id, email, None, now)
                .await
        })
    }

    fn set_enabled<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        player_id: &'a PlayerId,
        email: &'a NormalizedEmail,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> EmailStoreFuture<'a, ()> {
        Box::pin(async move {
            self.mutate_preference(owner_key, player_id, Some(email), Some(enabled), now)
                .await
        })
    }

    fn preference<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> EmailStoreFuture<'a, Option<EmailPreference>> {
        Box::pin(async move {
            let owned = Self::preference_path(owner_key);
            let path = owned.iter().map(String::as_str).collect::<Vec<_>>();
            let preference = self.database.get_document::<EmailPreference>(&path).await?;
            if let Some(preference) = &preference {
                preference.validate_for(owner_key)?;
            }
            Ok(preference)
        })
    }

    fn claim<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
        claimed_at: DateTime<Utc>,
        lease_ttl: TimeDelta,
        retry_horizon: TimeDelta,
    ) -> EmailStoreFuture<'a, DeliveryClaim> {
        Box::pin(async move {
            if lease_ttl <= TimeDelta::zero() || retry_horizon <= lease_ttl {
                return Err(DigestEmailError::InvalidRecord);
            }
            let owned = Self::delivery_path(owner_key, digest_id);
            let path = owned.iter().map(String::as_str).collect::<Vec<_>>();
            for attempt in 0..4 {
                let transaction = self.database.begin_transaction().await?;
                let stored = self
                    .database
                    .get_document_in_transaction::<DeliveryRecord>(&path, &transaction)
                    .await?;
                let write = match stored {
                    Some(mut record) => {
                        record.validate_for(owner_key, digest_id)?;
                        if record.retry_window_expired(claimed_at, retry_horizon) {
                            record.finish(DeliveryCompletion::Suppressed(
                                DeliverySuppressionReason::RetryWindowExpired,
                            ));
                            record.validate_for(owner_key, digest_id)?;
                            let write = self.database.update_write(&path, &record, &[])?;
                            match self
                                .database
                                .commit_transaction(transaction, vec![write])
                                .await
                            {
                                Ok(()) => return Ok(DeliveryClaim::AlreadyClaimed),
                                Err(FirestoreError::Conflict) if attempt < 3 => continue,
                                Err(error) => return Err(error.into()),
                            }
                        }
                        if !record.is_recoverable(claimed_at, lease_ttl) {
                            self.database.rollback_transaction(transaction).await?;
                            return Ok(DeliveryClaim::AlreadyClaimed);
                        }
                        record.reacquire(claimed_at);
                        record.validate_for(owner_key, digest_id)?;
                        self.database.update_write(&path, &record, &[])?
                    }
                    None => {
                        let record = DeliveryRecord::pending(
                            owner_key.clone(),
                            digest_id.to_string(),
                            claimed_at,
                        );
                        record.validate_for(owner_key, digest_id)?;
                        self.database.create_write(&path, &record, &[])?
                    }
                };
                match self
                    .database
                    .commit_transaction(transaction, vec![write])
                    .await
                {
                    Ok(()) => {
                        return Ok(DeliveryClaim::Claimed(DeliveryLease { claimed_at }));
                    }
                    Err(FirestoreError::Conflict) if attempt < 3 => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(DigestEmailError::Conflict)
        })
    }

    fn finish<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
        lease: DeliveryLease,
        completion: DeliveryCompletion,
    ) -> EmailStoreFuture<'a, ()> {
        Box::pin(async move {
            let owned = Self::delivery_path(owner_key, digest_id);
            let path = owned.iter().map(String::as_str).collect::<Vec<_>>();
            for attempt in 0..4 {
                let transaction = self.database.begin_transaction().await?;
                let mut record = self
                    .database
                    .get_document_in_transaction::<DeliveryRecord>(&path, &transaction)
                    .await?
                    .ok_or(DigestEmailError::InvalidRecord)?;
                record.validate_for(owner_key, digest_id)?;
                if record.status != DeliveryStatus::Pending || record.claimed_at != lease.claimed_at
                {
                    self.database.rollback_transaction(transaction).await?;
                    return Ok(());
                }
                record.finish(completion.clone());
                record.validate_for(owner_key, digest_id)?;
                let write = self.database.update_write(&path, &record, &[])?;
                match self
                    .database
                    .commit_transaction(transaction, vec![write])
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(FirestoreError::Conflict) if attempt < 3 => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(DigestEmailError::Conflict)
        })
    }

    fn suppress<'a>(
        &'a self,
        event: SuppressionEvent,
        now: DateTime<Utc>,
    ) -> EmailStoreFuture<'a, ()> {
        Box::pin(async move {
            let preference_owned = Self::preference_path(&event.owner_key);
            let preference_path = preference_owned
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let suppression_owned = Self::suppression_path(&event.owner_key, &event.event_id);
            let suppression_path = suppression_owned
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let delivery_owned = Self::delivery_path(&event.owner_key, &event.digest_id);
            let delivery_path = delivery_owned
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            for attempt in 0..4 {
                let transaction = self.database.begin_transaction().await?;
                let Some(delivery) = self
                    .database
                    .get_document_in_transaction::<DeliveryRecord>(&delivery_path, &transaction)
                    .await?
                else {
                    self.database.rollback_transaction(transaction).await?;
                    return Ok(());
                };
                delivery.validate_for(&event.owner_key, &event.digest_id)?;
                if !delivery.matches_sent(&event) {
                    self.database.rollback_transaction(transaction).await?;
                    return Ok(());
                }
                let Some(mut preference) = self
                    .database
                    .get_document_in_transaction::<EmailPreference>(&preference_path, &transaction)
                    .await?
                else {
                    self.database.rollback_transaction(transaction).await?;
                    return Ok(());
                };
                preference.validate_for(&event.owner_key)?;
                if self
                    .database
                    .get_document_in_transaction::<EmailSuppressionRecord>(
                        &suppression_path,
                        &transaction,
                    )
                    .await?
                    .is_some()
                {
                    self.database.rollback_transaction(transaction).await?;
                    return Ok(());
                }
                let record = EmailSuppressionRecord {
                    record_version: EMAIL_RECORD_VERSION,
                    owner_key: event.owner_key.clone(),
                    email: event.email.clone(),
                    provider_message_id: event.provider_message_id.clone(),
                    event_id_hash: hashed_path_segment(&event.event_id),
                    reason: event.reason,
                    suppressed_at: now,
                };
                let mut writes =
                    vec![self
                        .database
                        .create_write(&suppression_path, &record, &[])?];
                if preference.email.as_ref() == Some(&event.email) {
                    preference.suppressed_email = Some(event.email.clone());
                    preference.updated_at = now;
                    writes.push(
                        self.database
                            .update_write(&preference_path, &preference, &[])?,
                    );
                }
                match self.database.commit_transaction(transaction, writes).await {
                    Ok(()) => return Ok(()),
                    Err(FirestoreError::Conflict) if attempt < 3 => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(DigestEmailError::Conflict)
        })
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
