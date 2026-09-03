use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::review_session_contract::ProviderUnavailableReason;

use super::ProcessorPrincipal;

mod deployment_ceiling;
mod engine;
mod player_traffic;

pub(super) use engine::{EngineAdmission, EngineLease, EngineWorkload};
pub(crate) use player_traffic::PlayerTrafficPolicy;
#[cfg(test)]
pub(crate) use player_traffic::{
    ControllableTrafficClock, PLAYER_COMMAND_LIMIT, PLAYER_COMMAND_WINDOW_MS, PLAYER_IMPORT_LIMIT,
};

const COACH_QUEUE_DEADLINE: Duration = Duration::from_secs(2);

pub(super) struct CoachAdmission {
    web: AdmissionPool,
    local: AdmissionPool,
}

struct AdmissionPool {
    slots: Arc<Semaphore>,
    waiting: AtomicUsize,
    max_waiting: usize,
    queue_deadline: Duration,
}

struct Waiting<'a>(&'a AtomicUsize);

impl CoachAdmission {
    pub(super) fn v1() -> Self {
        Self {
            web: AdmissionPool::new(4, 8, COACH_QUEUE_DEADLINE),
            local: AdmissionPool::new(1, 1, COACH_QUEUE_DEADLINE),
        }
    }

    pub(super) async fn acquire(
        &self,
        principal: &ProcessorPrincipal,
    ) -> Result<OwnedSemaphorePermit, ProviderUnavailableReason> {
        match principal {
            ProcessorPrincipal::LocalCoach => self.local.acquire().await,
            ProcessorPrincipal::Player(_) => self.web.acquire().await,
        }
    }
}

impl AdmissionPool {
    fn new(slots: usize, max_waiting: usize, queue_deadline: Duration) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(slots)),
            waiting: AtomicUsize::new(0),
            max_waiting,
            queue_deadline,
        }
    }

    async fn acquire(&self) -> Result<OwnedSemaphorePermit, ProviderUnavailableReason> {
        self.acquire_until(Instant::now() + self.queue_deadline)
            .await
    }

    async fn acquire_until(
        &self,
        deadline: Instant,
    ) -> Result<OwnedSemaphorePermit, ProviderUnavailableReason> {
        self.acquire_until_observed(deadline).await.0
    }

    async fn acquire_until_observed(
        &self,
        deadline: Instant,
    ) -> (
        Result<OwnedSemaphorePermit, ProviderUnavailableReason>,
        usize,
    ) {
        if let Ok(permit) = self.slots.clone().try_acquire_owned() {
            return (Ok(permit), 0);
        }
        let waiting = self.waiting.fetch_add(1, Ordering::AcqRel);
        let queue_depth = waiting + 1;
        if waiting >= self.max_waiting {
            self.waiting.fetch_sub(1, Ordering::AcqRel);
            return (Err(ProviderUnavailableReason::AdmissionLimit), queue_depth);
        }
        let _waiting = Waiting(&self.waiting);
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .min(self.queue_deadline);
        let result = match tokio::time::timeout(remaining, self.slots.clone().acquire_owned()).await
        {
            Ok(result) => result.map_err(|_| ProviderUnavailableReason::AdmissionLimit),
            Err(_) => Err(ProviderUnavailableReason::QueueDeadline),
        };
        (result, queue_depth)
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
