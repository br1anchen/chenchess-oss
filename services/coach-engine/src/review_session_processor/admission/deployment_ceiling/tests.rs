use std::sync::Arc;

use super::super::player_traffic::ControllableTrafficClock;
use super::*;

const WINDOW_MS: u64 = 60_000;

fn ceiling(limit: Option<usize>) -> (DeploymentCeiling, Arc<ControllableTrafficClock>) {
    let clock = Arc::new(ControllableTrafficClock::new(1_000));
    let ceiling = DeploymentCeiling::new(limit, WINDOW_MS, clock.clone());
    (ceiling, clock)
}

fn retry_after(reason: &ProviderUnavailableReason) -> u32 {
    match reason {
        ProviderUnavailableReason::RateLimited {
            retry_after_seconds,
        } => *retry_after_seconds,
        other => panic!("expected a rate-limited refusal, got {other:?}"),
    }
}

#[test]
fn admits_work_below_the_ceiling_without_refusing() {
    let (ceiling, _clock) = ceiling(Some(3));

    for _ in 0..3 {
        ceiling.check().expect("under the ceiling");
        ceiling.record();
    }
}

#[test]
fn refuses_at_the_ceiling_with_a_retry_the_player_can_act_on() {
    let (ceiling, _clock) = ceiling(Some(2));
    ceiling.record();
    ceiling.record();

    let reason = ceiling.check().expect_err("the window is full");

    // The Player meets the reason the contract already carries, so
    // `terminal::retry_for` turns it into `RetryAfter` with no new vocabulary.
    let seconds = retry_after(&reason);
    assert!(
        (1..=60).contains(&seconds),
        "retry must land inside the window, got {seconds}"
    );
}

#[test]
fn admits_again_once_the_window_rolls_past_the_oldest_lease() {
    let (ceiling, clock) = ceiling(Some(1));
    ceiling.record();
    ceiling.check().expect_err("the window is full");

    clock.advance_ms(WINDOW_MS);

    ceiling.check().expect("the oldest lease has expired");
}

#[test]
fn an_unconfigured_ceiling_records_without_ever_refusing() {
    let (ceiling, _clock) = ceiling(None);

    // Recording while unconfigured is how the production number gets measured.
    for _ in 0..1_000 {
        ceiling.check().expect("no ceiling is configured");
        ceiling.record();
    }
}

#[test]
fn the_ceiling_bounds_the_deployment_rather_than_one_caller() {
    // Nothing about the meter is keyed by Player: work admitted for anyone
    // fills the same window, which is the whole point of a deployment ceiling.
    let (ceiling, _clock) = ceiling(Some(2));
    ceiling.record();
    ceiling.record();

    ceiling
        .check()
        .expect_err("a second Player meets the deployment's ceiling, not their own");
}

#[test]
fn a_partly_expired_window_refuses_only_until_the_oldest_lease_ages_out() {
    let (ceiling, clock) = ceiling(Some(2));
    ceiling.record();
    clock.advance_ms(WINDOW_MS / 2);
    ceiling.record();

    let reason = ceiling.check().expect_err("both leases are still inside");
    // The oldest is half a window old, so roughly half a window remains.
    assert_eq!(retry_after(&reason), 30);

    clock.advance_ms(WINDOW_MS / 2);
    ceiling.check().expect("the oldest lease has now expired");
}
