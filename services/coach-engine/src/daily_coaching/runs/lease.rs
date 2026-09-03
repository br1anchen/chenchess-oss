use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};

use super::{DailyCoachingRunLease, DailyCoachingRunStoreError};

impl DailyCoachingRunLease {
    pub(super) fn first(
        holder_id: &str,
        now: DateTime<Utc>,
        lease_ttl: Duration,
    ) -> Result<Self, DailyCoachingRunStoreError> {
        Self::with_token(holder_id, 1, now, lease_ttl)
    }

    pub(super) fn takeover(
        holder_id: &str,
        previous_token: u64,
        now: DateTime<Utc>,
        lease_ttl: Duration,
    ) -> Result<Self, DailyCoachingRunStoreError> {
        let token = previous_token
            .checked_add(1)
            .ok_or(DailyCoachingRunStoreError::InvalidRecord)?;
        Self::with_token(holder_id, token, now, lease_ttl)
    }

    fn with_token(
        holder_id: &str,
        fencing_token: u64,
        now: DateTime<Utc>,
        lease_ttl: Duration,
    ) -> Result<Self, DailyCoachingRunStoreError> {
        if holder_id.trim().is_empty() {
            return Err(DailyCoachingRunStoreError::InvalidRecord);
        }
        Ok(Self {
            holder_id: holder_id.to_string(),
            fencing_token,
            heartbeat_at: now,
            expires_at: expires_at(now, lease_ttl)?,
        })
    }
}

pub(super) fn expires_at(
    now: DateTime<Utc>,
    lease_ttl: Duration,
) -> Result<DateTime<Utc>, DailyCoachingRunStoreError> {
    now.checked_add_signed(
        TimeDelta::from_std(lease_ttl).map_err(|_| DailyCoachingRunStoreError::InvalidRecord)?,
    )
    .map(super::stored_instant)
    .ok_or(DailyCoachingRunStoreError::InvalidRecord)
}
