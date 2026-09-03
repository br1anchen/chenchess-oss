use std::future::Future;

use chrono::{DateTime, TimeDelta, Utc};

use crate::daily_coaching::runs::{
    DailyCoachingRunAddress, DailyCoachingRunLease, DailyCoachingRunOutcome,
};

use super::{DailyCoachingLifecycle, DailyCoachingTickError};

pub(super) enum WorkBoundary<T> {
    Completed(T),
    Deadline(DateTime<Utc>),
    Fenced,
}

impl DailyCoachingLifecycle {
    pub(super) async fn await_work<T>(
        &self,
        address: &DailyCoachingRunAddress,
        lease: &mut DailyCoachingRunLease,
        started_at: DateTime<Utc>,
        execution_start: tokio::time::Instant,
        deadline: tokio::time::Instant,
        work: impl Future<Output = Result<T, DailyCoachingTickError>>,
    ) -> Result<WorkBoundary<T>, DailyCoachingTickError> {
        let deadline_sleep = tokio::time::sleep_until(deadline);
        tokio::pin!(deadline_sleep);
        let mut heartbeat = tokio::time::interval_at(
            tokio::time::Instant::now() + self.configuration.heartbeat_interval,
            self.configuration.heartbeat_interval,
        );
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tokio::pin!(work);
        loop {
            tokio::select! {
                biased;
                _ = &mut deadline_sleep => {
                    return Ok(WorkBoundary::Deadline(elapsed_time(
                        started_at,
                        execution_start.elapsed(),
                    )?));
                }
                result = &mut work => return result.map(WorkBoundary::Completed),
                _ = heartbeat.tick() => {
                    let heartbeat_at = elapsed_time(started_at, execution_start.elapsed())?;
                    let renewed = self.run_store.heartbeat(
                        address,
                        lease,
                        heartbeat_at,
                        self.configuration.lease_ttl,
                        self.configuration.run_retention_days,
                    ).await?;
                    if renewed.outcome() == Some(DailyCoachingRunOutcome::Fenced) {
                        return Ok(WorkBoundary::Fenced);
                    }
                    *lease = renewed.lease()?.clone();
                }
            }
        }
    }
}

pub(super) fn elapsed_time(
    started_at: DateTime<Utc>,
    elapsed: std::time::Duration,
) -> Result<DateTime<Utc>, DailyCoachingTickError> {
    let elapsed = TimeDelta::from_std(elapsed).map_err(|_| DailyCoachingTickError::InvalidState)?;
    started_at
        .checked_add_signed(elapsed)
        .ok_or(DailyCoachingTickError::InvalidState)
}
