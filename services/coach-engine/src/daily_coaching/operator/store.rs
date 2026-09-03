use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Mutex};

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::firestore::{FirestoreDatabase, FirestoreError};

use super::{
    DegradedProviderEvent, OperatorDigestError, OperatorDigestWindow, ProfileUnavailableEvent,
    OPERATOR_RETENTION,
};

const COLLECTION: &str = "dailyCoachingOperatorDigests";
const EVENTS_COLLECTION: &str = "dailyCoachingOperatorEvents";
// A separate collection: the profile-unavailable query deserializes every document it reads.
const DEGRADED_EVENTS_COLLECTION: &str = "dailyCoachingDegradedProviderEvents";
const RECORD_VERSION: u8 = 1;
const MAX_TRANSACTION_ATTEMPTS: usize = 4;
const CLAIM_TTL: TimeDelta = TimeDelta::minutes(5);
const RETRY_HORIZON: TimeDelta = TimeDelta::hours(23);

type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, OperatorDigestError>> + Send + 'a>>;

pub(super) trait OperatorDigestStore: Send + Sync {
    fn claim<'a>(
        &'a self,
        window: &'a OperatorDigestWindow,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, Option<OperatorDigestLease>>;

    fn finish<'a>(
        &'a self,
        digest_id: &'a str,
        lease: OperatorDigestLease,
        provider_message_id: String,
    ) -> StoreFuture<'a, ()>;

    fn record_profile_unavailable<'a>(
        &'a self,
        event: ProfileUnavailableEvent,
    ) -> StoreFuture<'a, ()>;

    fn profile_unavailable_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> StoreFuture<'a, Vec<ProfileUnavailableEvent>>;

    fn record_degraded_provider<'a>(&'a self, event: DegradedProviderEvent) -> StoreFuture<'a, ()>;

    fn degraded_provider_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> StoreFuture<'a, Vec<DegradedProviderEvent>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OperatorDigestLease {
    claimed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperatorDigestClaim {
    Claimed(OperatorDigestLease),
    AlreadyClaimed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum OperatorDigestStatus {
    Pending,
    Sent,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperatorDigestRecord {
    record_version: u8,
    digest_id: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    first_claimed_at: DateTime<Utc>,
    claimed_at: DateTime<Utc>,
    attempt_count: u32,
    status: OperatorDigestStatus,
    provider_message_id: Option<String>,
    purge_at: DateTime<Utc>,
}

impl OperatorDigestRecord {
    fn new(window: &OperatorDigestWindow, now: DateTime<Utc>) -> Result<Self, OperatorDigestError> {
        let record = Self {
            record_version: RECORD_VERSION,
            digest_id: window.digest_id.clone(),
            starts_at: window.starts_at,
            ends_at: window.ends_at,
            first_claimed_at: now,
            claimed_at: now,
            attempt_count: 1,
            status: OperatorDigestStatus::Pending,
            provider_message_id: None,
            purge_at: window
                .ends_at
                .checked_add_signed(OPERATOR_RETENTION)
                .ok_or(OperatorDigestError::InvalidState)?,
        };
        record.validate()?;
        Ok(record)
    }

    fn claim(&mut self, now: DateTime<Utc>) -> Result<OperatorDigestClaim, OperatorDigestError> {
        self.validate()?;
        if self.status == OperatorDigestStatus::Sent
            || now.signed_duration_since(self.claimed_at) < CLAIM_TTL
            || now.signed_duration_since(self.first_claimed_at) >= RETRY_HORIZON
        {
            return Ok(OperatorDigestClaim::AlreadyClaimed);
        }
        self.claimed_at = now;
        self.attempt_count = self
            .attempt_count
            .checked_add(1)
            .ok_or(OperatorDigestError::InvalidState)?;
        Ok(OperatorDigestClaim::Claimed(OperatorDigestLease {
            claimed_at: now,
        }))
    }

    fn finish(
        &mut self,
        lease: OperatorDigestLease,
        provider_message_id: String,
    ) -> Result<(), OperatorDigestError> {
        if self.status != OperatorDigestStatus::Pending
            || self.claimed_at != lease.claimed_at
            || !super::super::valid_resend_provider_id(&provider_message_id)
        {
            return Err(OperatorDigestError::InvalidState);
        }
        self.status = OperatorDigestStatus::Sent;
        self.provider_message_id = Some(provider_message_id);
        self.validate()
    }

    fn validate(&self) -> Result<(), OperatorDigestError> {
        if self.record_version != RECORD_VERSION
            || self.digest_id.trim().is_empty()
            || self.starts_at >= self.ends_at
            || self.ends_at.signed_duration_since(self.starts_at) != TimeDelta::hours(24)
            || self.first_claimed_at < self.ends_at
            || self.claimed_at < self.first_claimed_at
            || self.attempt_count == 0
            || self.purge_at <= self.ends_at
            || (self.status == OperatorDigestStatus::Pending) != self.provider_message_id.is_none()
            || self
                .provider_message_id
                .as_deref()
                .is_some_and(|value| !super::super::valid_resend_provider_id(value))
        {
            Err(OperatorDigestError::InvalidState)
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
pub(super) struct InMemoryOperatorDigestStore {
    records: Mutex<BTreeMap<String, OperatorDigestRecord>>,
    events: Mutex<BTreeMap<String, ProfileUnavailableEvent>>,
    degraded_events: Mutex<BTreeMap<String, DegradedProviderEvent>>,
}

impl OperatorDigestStore for InMemoryOperatorDigestStore {
    fn claim<'a>(
        &'a self,
        window: &'a OperatorDigestWindow,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, Option<OperatorDigestLease>> {
        Box::pin(async move {
            let mut records = self
                .records
                .lock()
                .expect("in-memory Operator Digest store is not poisoned");
            let claim = match records.get_mut(&window.digest_id) {
                Some(record) => record.claim(now)?,
                None => {
                    records.insert(
                        window.digest_id.clone(),
                        OperatorDigestRecord::new(window, now)?,
                    );
                    OperatorDigestClaim::Claimed(OperatorDigestLease { claimed_at: now })
                }
            };
            Ok(match claim {
                OperatorDigestClaim::Claimed(lease) => Some(lease),
                OperatorDigestClaim::AlreadyClaimed => None,
            })
        })
    }

    fn finish<'a>(
        &'a self,
        digest_id: &'a str,
        lease: OperatorDigestLease,
        provider_message_id: String,
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            self.records
                .lock()
                .expect("in-memory Operator Digest store is not poisoned")
                .get_mut(digest_id)
                .ok_or(OperatorDigestError::InvalidState)?
                .finish(lease, provider_message_id)
        })
    }

    fn record_profile_unavailable<'a>(
        &'a self,
        event: ProfileUnavailableEvent,
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            event.validate()?;
            self.events
                .lock()
                .expect("in-memory Operator Digest event store is not poisoned")
                .entry(event.event_id.clone())
                .or_insert(event);
            Ok(())
        })
    }

    fn profile_unavailable_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> StoreFuture<'a, Vec<ProfileUnavailableEvent>> {
        Box::pin(async move {
            let mut events = self
                .events
                .lock()
                .expect("in-memory Operator Digest event store is not poisoned")
                .values()
                .filter(|event| event.entered_at >= starts_at && event.entered_at < ends_at)
                .cloned()
                .collect::<Vec<_>>();
            events.sort_by_key(|event| (event.entered_at, event.event_id.clone()));
            Ok(events)
        })
    }

    fn record_degraded_provider<'a>(&'a self, event: DegradedProviderEvent) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            event.validate()?;
            self.degraded_events
                .lock()
                .expect("in-memory Operator Digest event store is not poisoned")
                .entry(event.event_id.clone())
                .or_insert(event);
            Ok(())
        })
    }

    fn degraded_provider_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> StoreFuture<'a, Vec<DegradedProviderEvent>> {
        Box::pin(async move {
            let mut events = self
                .degraded_events
                .lock()
                .expect("in-memory Operator Digest event store is not poisoned")
                .values()
                .filter(|event| event.observed_at >= starts_at && event.observed_at < ends_at)
                .cloned()
                .collect::<Vec<_>>();
            events.sort_by_key(|event| (event.observed_at, event.event_id.clone()));
            Ok(events)
        })
    }
}

pub(super) struct FirestoreOperatorDigestStore {
    database: FirestoreDatabase,
}

impl FirestoreOperatorDigestStore {
    pub(super) fn new(database: FirestoreDatabase) -> Self {
        Self { database }
    }

    fn path(digest_id: &str) -> [&str; 2] {
        [COLLECTION, digest_id]
    }
}

impl OperatorDigestStore for FirestoreOperatorDigestStore {
    fn claim<'a>(
        &'a self,
        window: &'a OperatorDigestWindow,
        now: DateTime<Utc>,
    ) -> StoreFuture<'a, Option<OperatorDigestLease>> {
        Box::pin(async move {
            let path = Self::path(&window.digest_id);
            for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                let stored = self
                    .database
                    .get_document_in_transaction::<OperatorDigestRecord>(&path, &transaction)
                    .await?;
                let (record, claim, existed) = match stored {
                    Some(mut record) => {
                        if record.digest_id != window.digest_id
                            || record.starts_at != window.starts_at
                            || record.ends_at != window.ends_at
                        {
                            self.database.rollback_transaction(transaction).await?;
                            return Err(OperatorDigestError::InvalidState);
                        }
                        let claim = record.claim(now)?;
                        if claim == OperatorDigestClaim::AlreadyClaimed {
                            self.database.rollback_transaction(transaction).await?;
                            return Ok(None);
                        }
                        (record, claim, true)
                    }
                    None => (
                        OperatorDigestRecord::new(window, now)?,
                        OperatorDigestClaim::Claimed(OperatorDigestLease { claimed_at: now }),
                        false,
                    ),
                };
                let timestamps = [("purgeAt", record.purge_at)];
                let write = if existed {
                    self.database.update_write(&path, &record, &timestamps)?
                } else {
                    self.database.create_write(&path, &record, &timestamps)?
                };
                match self
                    .database
                    .commit_transaction(transaction, vec![write])
                    .await
                {
                    Ok(()) => {
                        let OperatorDigestClaim::Claimed(lease) = claim else {
                            return Err(OperatorDigestError::InvalidState);
                        };
                        return Ok(Some(lease));
                    }
                    Err(FirestoreError::Conflict) if attempt + 1 < MAX_TRANSACTION_ATTEMPTS => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(OperatorDigestError::Store)
        })
    }

    fn finish<'a>(
        &'a self,
        digest_id: &'a str,
        lease: OperatorDigestLease,
        provider_message_id: String,
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let path = Self::path(digest_id);
            for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
                let transaction = self.database.begin_transaction().await?;
                let Some(mut record) = self
                    .database
                    .get_document_in_transaction::<OperatorDigestRecord>(&path, &transaction)
                    .await?
                else {
                    self.database.rollback_transaction(transaction).await?;
                    return Err(OperatorDigestError::InvalidState);
                };
                record.finish(lease, provider_message_id.clone())?;
                let write =
                    self.database
                        .update_write(&path, &record, &[("purgeAt", record.purge_at)])?;
                match self
                    .database
                    .commit_transaction(transaction, vec![write])
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(FirestoreError::Conflict) if attempt + 1 < MAX_TRANSACTION_ATTEMPTS => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(OperatorDigestError::Store)
        })
    }

    fn record_profile_unavailable<'a>(
        &'a self,
        event: ProfileUnavailableEvent,
    ) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            event.validate()?;
            match self
                .database
                .create_document(
                    &[EVENTS_COLLECTION],
                    &event.event_id,
                    &event,
                    &[("enteredAt", event.entered_at), ("purgeAt", event.purge_at)],
                )
                .await
            {
                Ok(()) | Err(FirestoreError::Conflict) => Ok(()),
                Err(error) => Err(error.into()),
            }
        })
    }

    fn profile_unavailable_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> StoreFuture<'a, Vec<ProfileUnavailableEvent>> {
        Box::pin(async move {
            let mut events = self
                .database
                .query_collection_group_timestamp_range_without_status::<ProfileUnavailableEvent>(
                    EVENTS_COLLECTION,
                    "enteredAt",
                    starts_at,
                    ends_at,
                )
                .await?
                .into_iter()
                .map(|stored| {
                    stored.value.validate()?;
                    if stored.path.as_slice() != [EVENTS_COLLECTION, &stored.value.event_id] {
                        return Err(OperatorDigestError::InvalidState);
                    }
                    Ok(stored.value)
                })
                .collect::<Result<Vec<_>, _>>()?;
            events.sort_by_key(|event| (event.entered_at, event.event_id.clone()));
            Ok(events)
        })
    }

    fn record_degraded_provider<'a>(&'a self, event: DegradedProviderEvent) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            event.validate()?;
            match self
                .database
                .create_document(
                    &[DEGRADED_EVENTS_COLLECTION],
                    &event.event_id,
                    &event,
                    &[
                        ("observedAt", event.observed_at),
                        ("purgeAt", event.purge_at),
                    ],
                )
                .await
            {
                Ok(()) | Err(FirestoreError::Conflict) => Ok(()),
                Err(error) => Err(error.into()),
            }
        })
    }

    fn degraded_provider_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> StoreFuture<'a, Vec<DegradedProviderEvent>> {
        Box::pin(async move {
            let mut events = self
                .database
                .query_collection_group_timestamp_range_without_status::<DegradedProviderEvent>(
                    DEGRADED_EVENTS_COLLECTION,
                    "observedAt",
                    starts_at,
                    ends_at,
                )
                .await?
                .into_iter()
                .map(|stored| {
                    stored.value.validate()?;
                    if stored.path.as_slice()
                        != [DEGRADED_EVENTS_COLLECTION, &stored.value.event_id]
                    {
                        return Err(OperatorDigestError::InvalidState);
                    }
                    Ok(stored.value)
                })
                .collect::<Result<Vec<_>, _>>()?;
            events.sort_by_key(|event| (event.observed_at, event.event_id.clone()));
            Ok(events)
        })
    }
}

impl From<FirestoreError> for OperatorDigestError {
    fn from(error: FirestoreError) -> Self {
        match error {
            FirestoreError::Configuration(_)
            | FirestoreError::Transport
            | FirestoreError::Unavailable
            | FirestoreError::Conflict => Self::Store,
            FirestoreError::InvalidDocument => Self::InvalidState,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_claim_is_lease_fenced_idempotent_and_bounded() {
        let window = OperatorDigestWindow {
            digest_id: "operator-2026-08-12-08".to_string(),
            starts_at: instant("2026-08-11T08:00:00Z"),
            ends_at: instant("2026-08-12T08:00:00Z"),
        };
        let first_claimed_at = instant("2026-08-12T08:01:00Z");
        let mut record = OperatorDigestRecord::new(&window, first_claimed_at).unwrap();
        let first_lease = OperatorDigestLease {
            claimed_at: first_claimed_at,
        };

        assert_eq!(
            record
                .claim(first_claimed_at + TimeDelta::minutes(4))
                .unwrap(),
            OperatorDigestClaim::AlreadyClaimed
        );
        let OperatorDigestClaim::Claimed(second_lease) = record
            .claim(first_claimed_at + TimeDelta::minutes(5))
            .unwrap()
        else {
            panic!("the expired claim must be recoverable")
        };
        assert!(matches!(
            record.finish(first_lease, "stale-provider-id".to_string()),
            Err(OperatorDigestError::InvalidState)
        ));
        record
            .finish(second_lease, "provider-message-2".to_string())
            .unwrap();
        assert_eq!(
            record
                .claim(first_claimed_at + TimeDelta::minutes(10))
                .unwrap(),
            OperatorDigestClaim::AlreadyClaimed
        );

        let mut expired = OperatorDigestRecord::new(&window, first_claimed_at).unwrap();
        assert_eq!(
            expired.claim(first_claimed_at + RETRY_HORIZON).unwrap(),
            OperatorDigestClaim::AlreadyClaimed
        );
    }

    fn instant(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }
}
