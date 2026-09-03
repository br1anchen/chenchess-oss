use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use crate::review_session_contract::{PlayerId, ProviderUnavailableReason};

use super::{DeploymentCeiling, EngineAdmission, EngineWorkload, PlayerClaim, ProcessorPrincipal};

fn player(id: &str) -> ProcessorPrincipal {
    ProcessorPrincipal::Player(PlayerId::try_from(id.to_owned()).unwrap())
}

fn admission(slots: usize, max_waiting: usize, deadline: Duration) -> EngineAdmission {
    // No ceiling: these tests are about per-class and per-Player admission, and
    // an unconfigured deployment ceiling never refuses.
    admission_with_ceiling(slots, max_waiting, deadline, None)
}

fn admission_with_ceiling(
    slots: usize,
    max_waiting: usize,
    deadline: Duration,
    limit: Option<usize>,
) -> EngineAdmission {
    EngineAdmission::new(
        slots,
        max_waiting,
        deadline,
        deadline,
        Arc::new(DeploymentCeiling::new(
            limit,
            60_000,
            Arc::new(super::super::player_traffic::SystemTrafficClock),
        )),
    )
}

#[tokio::test]
async fn the_deployment_ceiling_refuses_a_lease_no_per_player_bound_would() {
    let admission = admission_with_ceiling(1, 4, Duration::from_secs(1), Some(1));
    let first = admission
        .acquire(EngineWorkload::Batch, &player("player:first"))
        .await
        .expect("the first lease is under the ceiling");
    drop(first);

    // A different Player, a different class, and no slot contention: only the
    // deployment-wide window can refuse this.
    let refusal = admission
        .acquire(EngineWorkload::Interactive, &player("player:second"))
        .await
        .expect_err("the deployment window is full");

    assert!(matches!(
        refusal,
        ProviderUnavailableReason::RateLimited { .. }
    ));
}

#[tokio::test]
async fn optional_prefetch_does_not_run_past_the_deployment_ceiling() {
    let admission = admission_with_ceiling(1, 4, Duration::from_secs(1), Some(1));
    let lease = admission
        .acquire(EngineWorkload::Batch, &player("player:only"))
        .await
        .expect("the first lease is under the ceiling");
    drop(lease);

    assert!(
        admission.try_acquire_prefetch().is_none(),
        "optional work is exactly what a full deployment window should skip"
    );
}

#[tokio::test]
async fn rejects_a_second_operation_for_the_same_player() {
    let admission = admission(1, 4, Duration::from_secs(1));
    let principal = player("player:duplicate");
    let _lease = admission
        .acquire(EngineWorkload::Batch, &principal)
        .await
        .unwrap();

    assert_eq!(
        admission
            .acquire(EngineWorkload::Batch, &principal)
            .await
            .unwrap_err(),
        ProviderUnavailableReason::AdmissionLimit
    );
}

#[tokio::test]
async fn bounds_the_shared_waiting_queue() {
    let admission = Arc::new(admission(1, 1, Duration::from_secs(1)));
    let active = admission
        .acquire(EngineWorkload::Batch, &player("player:active"))
        .await
        .unwrap();
    let waiting_admission = admission.clone();
    let waiting = tokio::spawn(async move {
        waiting_admission
            .acquire(EngineWorkload::Batch, &player("player:waiting"))
            .await
    });
    wait_for_count(&admission.batch.pool.waiting, 1, "queued operation").await;

    assert_eq!(
        admission
            .acquire(EngineWorkload::Batch, &player("player:rejected"))
            .await
            .unwrap_err(),
        ProviderUnavailableReason::AdmissionLimit
    );

    drop(active);
    waiting.await.unwrap().unwrap();
}

#[tokio::test]
async fn cancelled_wait_releases_the_player_claim() {
    let admission = Arc::new(admission(1, 1, Duration::from_secs(1)));
    let _active = admission
        .acquire(EngineWorkload::Batch, &ProcessorPrincipal::LocalCoach)
        .await
        .unwrap();
    let waiting_admission = admission.clone();
    let principal = player("player:cancelled");
    let waiting_principal = principal.clone();
    let waiting = tokio::spawn(async move {
        waiting_admission
            .acquire(EngineWorkload::Batch, &waiting_principal)
            .await
    });
    wait_for_count(&admission.batch.pool.waiting, 1, "cancelled operation").await;
    waiting.abort();
    let _ = waiting.await;

    assert_eq!(admission.batch.pool.waiting.load(Ordering::Acquire), 0);
    PlayerClaim::acquire(&principal, admission.batch.active_players.clone())
        .expect("a cancelled wait must release its per-player claim");
}

#[tokio::test(start_paused = true)]
async fn caller_deadline_rejects_the_wait_and_releases_the_player_claim() {
    let admission = admission(1, 1, Duration::from_secs(1));
    let _active = admission
        .acquire(EngineWorkload::Batch, &ProcessorPrincipal::LocalCoach)
        .await
        .unwrap();
    let principal = player("player:deadline");

    let result = admission
        .acquire_until(
            EngineWorkload::Batch,
            &principal,
            std::time::Instant::now() + Duration::from_millis(10),
        )
        .await;

    assert_eq!(
        result.unwrap_err(),
        ProviderUnavailableReason::QueueDeadline
    );
    PlayerClaim::acquire(&principal, admission.batch.active_players.clone())
        .expect("a timed-out wait must release its per-player claim");
}

#[test]
fn optional_prefetch_uses_only_immediately_idle_capacity() {
    let admission = admission(1, 1, Duration::from_secs(1));

    let lease = admission
        .try_acquire_prefetch()
        .expect("idle capacity admits one optional prefetch");
    assert!(admission.try_acquire_prefetch().is_none());
    assert_eq!(admission.batch.pool.waiting.load(Ordering::Acquire), 0);

    drop(lease);
    assert!(admission.try_acquire_prefetch().is_some());
}

#[tokio::test]
async fn a_players_own_import_no_longer_blocks_their_board() {
    // The case that motivated the split. Daily Coaching submits a Game import
    // under the Player's own principal, so before the two classes had separate
    // claims this second acquire returned AdmissionLimit and the Player's drag
    // was refused outright rather than queued.
    let admission = admission(1, 4, Duration::from_secs(1));
    let principal = player("player:exploring");
    let _importing = admission
        .acquire(EngineWorkload::Batch, &principal)
        .await
        .expect("a Game import takes the batch slot");

    admission
        .acquire(EngineWorkload::Interactive, &principal)
        .await
        .expect("the same Player's Alternative Move takes the interactive slot");
}

#[tokio::test]
async fn a_saturated_batch_class_leaves_the_interactive_slot_free() {
    let admission = admission(1, 4, Duration::from_secs(1));
    let _holder = admission
        .acquire(EngineWorkload::Batch, &player("player:importing"))
        .await
        .unwrap();

    admission
        .acquire(EngineWorkload::Interactive, &player("player:exploring"))
        .await
        .expect("interactive capacity is not shared with the batch class");
}

#[tokio::test]
async fn each_class_still_admits_one_workload_per_player() {
    let admission = admission(1, 4, Duration::from_secs(1));
    let principal = player("player:duplicate");
    let _lease = admission
        .acquire(EngineWorkload::Interactive, &principal)
        .await
        .unwrap();

    assert_eq!(
        admission
            .acquire(EngineWorkload::Interactive, &principal)
            .await
            .unwrap_err(),
        ProviderUnavailableReason::AdmissionLimit
    );
}

#[test]
fn the_interactive_class_fails_a_doomed_wait_far_sooner_than_the_batch_class() {
    let admission = EngineAdmission::v1();

    assert_eq!(admission.interactive.queue_deadline, Duration::from_secs(5));
    assert_eq!(admission.batch.queue_deadline, Duration::from_secs(30));
}

#[test]
fn optional_prefetch_never_takes_the_interactive_slot() {
    let admission = admission(1, 1, Duration::from_secs(1));
    let _batch = admission
        .batch
        .pool
        .slots
        .clone()
        .try_acquire_owned()
        .unwrap();

    assert!(admission.try_acquire_prefetch().is_none());
    assert_eq!(admission.interactive.pool.slots.available_permits(), 1);
}

async fn wait_for_count(counter: &AtomicUsize, expected: usize, operation: &str) {
    tokio::time::timeout(Duration::from_millis(100), async {
        while counter.load(Ordering::Acquire) != expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the {operation} should enter admission"));
}
