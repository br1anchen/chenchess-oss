use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::sync::OwnedSemaphorePermit;

use crate::review_session_contract::ProviderUnavailableReason;

use super::{deployment_ceiling::DeploymentCeiling, AdmissionPool, ProcessorPrincipal};

const ENGINE_QUEUE_DEADLINE: Duration = Duration::from_secs(30);

/// Interactive work waits far less because it holds the slot for far less.
///
/// Measured on hosted staging 2026-08-31: an Alternative Move holds its lease
/// for 428 ms median and 466 ms at the observed maximum, so four waiters model
/// to about 1.9 seconds. Five seconds is well clear of that queue, and unlike
/// the batch class's thirty it fails a doomed wait while the Player is still
/// watching. Recorded in `docs/plans/006-…`, "Item 2 measured".
const INTERACTIVE_QUEUE_DEADLINE: Duration = Duration::from_secs(5);

/// Which engine class a workload is admitted into.
///
/// The two classes share no slot and no per-Player claim. A Game import holds
/// the batch slot for 6,543 ms median against an Alternative Move's 428 ms, and
/// before this split a Player exploring during their own Daily Coaching import
/// was refused outright rather than made to wait — `PlayerClaim` is per class,
/// so one claim used to cover both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::review_session_processor) enum EngineWorkload {
    /// One position on a Player's critical path.
    Interactive,
    /// A whole Game, or every pending Review Moment of one.
    Batch,
}

impl EngineWorkload {
    const fn label(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Batch => "batch",
        }
    }
}

struct ClassAdmission {
    pool: AdmissionPool,
    active_players: Arc<Mutex<BTreeSet<String>>>,
    queue_deadline: Duration,
    workload: EngineWorkload,
    /// Shared with the other class: the ceiling bounds the deployment, not a
    /// workload, so interactive and batch draw on one window.
    ceiling: Arc<DeploymentCeiling>,
}

pub(in crate::review_session_processor) struct EngineAdmission {
    interactive: ClassAdmission,
    batch: ClassAdmission,
}

#[derive(Debug)]
pub(in crate::review_session_processor) struct EngineLease {
    acquired_at: Instant,
    workload: EngineWorkload,
    _permit: OwnedSemaphorePermit,
    _player_claim: Option<PlayerClaim>,
}

#[derive(Debug)]
struct PlayerClaim {
    player_id: Option<String>,
    active_players: Arc<Mutex<BTreeSet<String>>>,
}

impl EngineAdmission {
    pub(in crate::review_session_processor) fn v1() -> Self {
        Self::new(
            1,
            4,
            ENGINE_QUEUE_DEADLINE,
            INTERACTIVE_QUEUE_DEADLINE,
            Arc::new(DeploymentCeiling::from_env()),
        )
    }

    fn new(
        slots: usize,
        max_waiting: usize,
        batch_deadline: Duration,
        interactive_deadline: Duration,
        ceiling: Arc<DeploymentCeiling>,
    ) -> Self {
        Self {
            interactive: ClassAdmission::new(
                slots,
                max_waiting,
                interactive_deadline,
                EngineWorkload::Interactive,
                ceiling.clone(),
            ),
            batch: ClassAdmission::new(
                slots,
                max_waiting,
                batch_deadline,
                EngineWorkload::Batch,
                ceiling,
            ),
        }
    }

    const fn class(&self, workload: EngineWorkload) -> &ClassAdmission {
        match workload {
            EngineWorkload::Interactive => &self.interactive,
            EngineWorkload::Batch => &self.batch,
        }
    }

    pub(in crate::review_session_processor) async fn acquire(
        &self,
        workload: EngineWorkload,
        principal: &ProcessorPrincipal,
    ) -> Result<EngineLease, ProviderUnavailableReason> {
        let class = self.class(workload);
        class
            .acquire_until(principal, Instant::now() + class.queue_deadline)
            .await
    }

    pub(in crate::review_session_processor) async fn acquire_until(
        &self,
        workload: EngineWorkload,
        principal: &ProcessorPrincipal,
        deadline: Instant,
    ) -> Result<EngineLease, ProviderUnavailableReason> {
        self.class(workload)
            .acquire_until(principal, deadline)
            .await
    }

    /// Optional prefetch rides only idle batch capacity, never the interactive
    /// slot: it exists to use a quiet engine, not to compete with a drag.
    pub(in crate::review_session_processor) fn try_acquire_prefetch(&self) -> Option<EngineLease> {
        self.batch.ceiling.check().ok()?;
        let permit = self.batch.pool.slots.clone().try_acquire_owned().ok()?;
        tracing::info!(
            event = "coach_engine_admission_completion",
            queue_depth = 0,
            saturated = false,
            queue_wait_milliseconds = 0,
            workload = "review_moment_prefetch",
            workload_class = EngineWorkload::Batch.label(),
            "idle engine capacity admitted for optional prefetch"
        );
        self.batch.ceiling.record();
        Some(EngineLease {
            acquired_at: Instant::now(),
            workload: EngineWorkload::Batch,
            _permit: permit,
            _player_claim: None,
        })
    }
}

impl ClassAdmission {
    fn new(
        slots: usize,
        max_waiting: usize,
        queue_deadline: Duration,
        workload: EngineWorkload,
        ceiling: Arc<DeploymentCeiling>,
    ) -> Self {
        Self {
            pool: AdmissionPool::new(slots, max_waiting, queue_deadline),
            active_players: Arc::new(Mutex::new(BTreeSet::new())),
            queue_deadline,
            workload,
            ceiling,
        }
    }

    async fn acquire_until(
        &self,
        principal: &ProcessorPrincipal,
        deadline: Instant,
    ) -> Result<EngineLease, ProviderUnavailableReason> {
        let queued_at = Instant::now();
        if let Err(reason) = self.ceiling.check() {
            log_rejection(
                queued_at,
                self.pool.waiting.load(std::sync::atomic::Ordering::Acquire),
                &reason,
                self.workload,
            );
            return Err(reason);
        }
        let player_claim = match PlayerClaim::acquire(principal, self.active_players.clone()) {
            Ok(claim) => claim,
            Err(reason) => {
                log_rejection(
                    queued_at,
                    self.pool.waiting.load(std::sync::atomic::Ordering::Acquire),
                    &reason,
                    self.workload,
                );
                return Err(reason);
            }
        };
        let (permit, queue_depth) = self.pool.acquire_until_observed(deadline).await;
        let permit = match permit {
            Ok(permit) => permit,
            Err(reason) => {
                log_rejection(queued_at, queue_depth, &reason, self.workload);
                return Err(reason);
            }
        };
        tracing::info!(
            event = "coach_engine_admission_completion",
            queue_depth,
            saturated = queue_depth > 0,
            queue_wait_milliseconds = queued_at.elapsed().as_millis(),
            workload_class = self.workload.label(),
            "engine workload admitted"
        );
        self.ceiling.record();
        Ok(EngineLease {
            acquired_at: Instant::now(),
            workload: self.workload,
            _permit: permit,
            _player_claim: Some(player_claim),
        })
    }
}

impl PlayerClaim {
    fn acquire(
        principal: &ProcessorPrincipal,
        active_players: Arc<Mutex<BTreeSet<String>>>,
    ) -> Result<Self, ProviderUnavailableReason> {
        let player_id = match principal {
            ProcessorPrincipal::LocalCoach => None,
            ProcessorPrincipal::Player(player_id) => Some(player_id.as_str().to_owned()),
        };
        if let Some(player_id) = &player_id {
            let mut active = active_players
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !active.insert(player_id.clone()) {
                return Err(ProviderUnavailableReason::AdmissionLimit);
            }
        }
        Ok(Self {
            player_id,
            active_players,
        })
    }
}

impl Drop for PlayerClaim {
    fn drop(&mut self) {
        if let Some(player_id) = &self.player_id {
            self.active_players
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(player_id);
        }
    }
}

impl Drop for EngineLease {
    fn drop(&mut self) {
        tracing::info!(
            event = "coach_engine_lease_completion",
            lease_occupancy_milliseconds = self.acquired_at.elapsed().as_millis(),
            workload_class = self.workload.label(),
            "engine workload lease released"
        );
    }
}

fn log_rejection(
    queued_at: Instant,
    queue_depth: usize,
    reason: &ProviderUnavailableReason,
    workload: EngineWorkload,
) {
    tracing::warn!(
        event = "coach_engine_admission_completion",
        queue_depth,
        saturated = true,
        queue_wait_milliseconds = queued_at.elapsed().as_millis(),
        reason = ?reason,
        workload_class = workload.label(),
        "engine workload admission rejected"
    );
}

#[cfg(test)]
mod tests;
