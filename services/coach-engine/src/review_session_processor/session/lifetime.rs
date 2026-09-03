//! How long an in-memory Review Session stays resident.
//!
//! Nothing here is durable. A Review Session is a process-local actor holding
//! engine leases, prefetched analysis, and one Player's in-flight coaching, and
//! this bounds how long an idle one keeps that memory. Losing it costs nothing:
//! the review is addressable, its analysis is cached, and its comments are in
//! the annotation store, so the next command rebuilds the actor from durable
//! state the Player already owns.

use chrono::{DateTime, TimeDelta, Utc};

/// Idle sessions are evicted well before the process would notice them, but the
/// window is long enough that a Player who steps away mid-review comes back to
/// warm state rather than a rebuild.
const REVIEW_SESSION_IDLE_LIFETIME_HOURS: i64 = 72;
/// The hard ceiling, so a session that is touched forever still releases its
/// engine leases eventually.
const REVIEW_SESSION_ABSOLUTE_LIFETIME_HOURS: i64 = 336;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReviewSessionLifetime {
    created_at: DateTime<Utc>,
    last_activity_at: DateTime<Utc>,
    idle_expires_at: DateTime<Utc>,
    absolute_expires_at: DateTime<Utc>,
}

impl ReviewSessionLifetime {
    pub(crate) fn new(created_at: DateTime<Utc>) -> Self {
        Self {
            created_at,
            last_activity_at: created_at,
            idle_expires_at: created_at
                .checked_add_signed(TimeDelta::hours(REVIEW_SESSION_IDLE_LIFETIME_HOURS))
                .expect("a Review Session timestamp can advance by its idle lifetime"),
            absolute_expires_at: created_at
                .checked_add_signed(TimeDelta::hours(REVIEW_SESSION_ABSOLUTE_LIFETIME_HOURS))
                .expect("a Review Session timestamp can advance by its absolute lifetime"),
        }
    }

    pub(crate) fn is_expired(self, now: DateTime<Utc>) -> bool {
        now >= self.idle_expires_at || now >= self.absolute_expires_at
    }

    pub(crate) fn refreshed_at(self, now: DateTime<Utc>) -> Option<Self> {
        if self.is_expired(now) {
            return None;
        }
        let last_activity_at = self.last_activity_at.max(now);
        let idle_expires_at = last_activity_at
            .checked_add_signed(TimeDelta::hours(REVIEW_SESSION_IDLE_LIFETIME_HOURS))?;
        Some(Self {
            last_activity_at,
            idle_expires_at,
            ..self
        })
    }
}
