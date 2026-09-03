use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, PoisonError},
};

use crate::review_session_contract::ProviderUnavailableReason;

use super::player_traffic::{
    expire_window, retry_after_seconds, PlayerTrafficClock, SystemTrafficClock,
};

/// Deployment-wide ceiling on admitted engine leases, in leases per window.
///
/// Absent means the meter records and never refuses, which is the default and
/// the only honest posture until production capacity has been measured. Every
/// other bound in this module is per-Player or per-instance-concurrency; this
/// is the one that answers "how much work is this deployment doing at all",
/// which is the exposure a public plugin listing creates.
pub(super) const ENGINE_CEILING_ENV: &str = "COACH_ENGINE_LEASE_CEILING_PER_MINUTE";

pub(super) const ENGINE_CEILING_WINDOW_MS: u64 = 60_000;

pub(super) struct DeploymentCeiling {
    limit: Option<usize>,
    window_ms: u64,
    clock: Arc<dyn PlayerTrafficClock>,
    admitted: Mutex<VecDeque<u64>>,
}

impl DeploymentCeiling {
    /// Reads the ceiling from the environment.
    ///
    /// A malformed value aborts startup rather than disabling the ceiling. A
    /// typo must not quietly remove a capacity control on the deployment a
    /// public listing points at; an absent value is the way to run without
    /// one, and it is spelled by omission.
    pub(super) fn from_env() -> Self {
        let limit = match std::env::var(ENGINE_CEILING_ENV) {
            Err(_) => None,
            Ok(raw) => Some(raw.trim().parse::<usize>().unwrap_or_else(|_| {
                panic!("{ENGINE_CEILING_ENV} must be a whole number of leases per minute")
            })),
        };
        Self::new(
            limit,
            ENGINE_CEILING_WINDOW_MS,
            Arc::new(SystemTrafficClock),
        )
    }

    pub(super) fn new(
        limit: Option<usize>,
        window_ms: u64,
        clock: Arc<dyn PlayerTrafficClock>,
    ) -> Self {
        Self {
            limit,
            window_ms,
            clock,
            admitted: Mutex::new(VecDeque::new()),
        }
    }

    /// Refuses when the deployment has already admitted its window's worth.
    ///
    /// The Player meets `RateLimited`, which the contract already carries and
    /// `terminal::retry_for` already turns into `RetryAfter`. A dedicated
    /// saturation reason would say the same sentence in new words and cost a
    /// reviewed-metadata change to do it.
    pub(super) fn check(&self) -> Result<(), ProviderUnavailableReason> {
        let Some(limit) = self.limit else {
            return Ok(());
        };
        let now_ms = self.clock.now_ms();
        let mut admitted = self.admitted.lock().unwrap_or_else(PoisonError::into_inner);
        expire_window(&mut admitted, now_ms, self.window_ms);
        if admitted.len() < limit {
            return Ok(());
        }
        let oldest_ms = admitted.front().copied().unwrap_or(now_ms);
        let retry_after_seconds = retry_after_seconds(oldest_ms, self.window_ms, now_ms);
        tracing::warn!(
            event = "coach_engine_deployment_ceiling",
            decision = "rejected",
            limit,
            window_milliseconds = self.window_ms,
            admitted_in_window = admitted.len(),
            retry_after_seconds,
            "deployment-wide engine lease ceiling reached"
        );
        Err(ProviderUnavailableReason::RateLimited {
            retry_after_seconds,
        })
    }

    /// Counts one admitted lease, whether or not a ceiling is configured.
    ///
    /// Recording while unconfigured is the point: the observed rate is how the
    /// production number gets chosen, and staging cannot measure a meter that
    /// only runs once someone has already guessed a limit.
    pub(super) fn record(&self) {
        let now_ms = self.clock.now_ms();
        let mut admitted = self.admitted.lock().unwrap_or_else(PoisonError::into_inner);
        expire_window(&mut admitted, now_ms, self.window_ms);
        admitted.push_back(now_ms);
        tracing::info!(
            event = "coach_engine_deployment_ceiling",
            decision = "admitted",
            limit = ?self.limit,
            window_milliseconds = self.window_ms,
            admitted_in_window = admitted.len(),
            "engine lease counted against the deployment window"
        );
    }
}

#[cfg(test)]
mod tests;
