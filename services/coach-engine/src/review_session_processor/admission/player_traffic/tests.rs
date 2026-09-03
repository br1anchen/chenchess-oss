use std::sync::Arc;

use crate::review_session_contract::{OperationId, PlayerId};

use super::{
    traffic_telemetry_field_names, ControllableTrafficClock, PlayerTrafficPolicy,
    PLAYER_COMMAND_LIMIT, PLAYER_COMMAND_WINDOW_MS, PLAYER_IMPORT_LIMIT, PLAYER_IMPORT_WINDOW_MS,
    PLAYER_TRAFFIC_POLICY_VERSION,
};

fn player(id: &str) -> PlayerId {
    PlayerId::try_from(id.to_owned()).unwrap()
}

fn operation(id: &str) -> OperationId {
    OperationId::try_from(id.to_owned()).unwrap()
}

fn policy(now_ms: u64) -> (PlayerTrafficPolicy, Arc<ControllableTrafficClock>) {
    let clock = Arc::new(ControllableTrafficClock::new(now_ms));
    (PlayerTrafficPolicy::v1_with_clock(clock.clone()), clock)
}

#[test]
fn command_window_admits_the_limit_and_rejects_the_next() {
    let (policy, _) = policy(0);
    let player = player("player:commands");
    for _ in 0..PLAYER_COMMAND_LIMIT {
        policy.admit_command(&player).unwrap();
    }
    assert_eq!(policy.admit_command(&player), Err(60));
}

#[test]
fn command_window_retry_after_is_the_earliest_admission_instant() {
    let (policy, clock) = policy(0);
    let player = player("player:retry");
    for _ in 0..PLAYER_COMMAND_LIMIT {
        policy.admit_command(&player).unwrap();
    }
    clock.advance_ms(30_000);
    assert_eq!(policy.admit_command(&player), Err(30));
    clock.advance_ms(PLAYER_COMMAND_WINDOW_MS - 30_000);
    policy.admit_command(&player).unwrap();
}

#[test]
fn two_players_do_not_share_a_command_window() {
    let (policy, _) = policy(0);
    let first = player("player:one");
    let second = player("player:two");
    for _ in 0..PLAYER_COMMAND_LIMIT {
        policy.admit_command(&first).unwrap();
    }
    assert_eq!(policy.admit_command(&first), Err(60));
    policy.admit_command(&second).unwrap();
}

#[test]
fn import_window_admits_the_limit_and_rejects_the_next() {
    let (policy, _) = policy(0);
    let player = player("player:imports");
    for index in 0..PLAYER_IMPORT_LIMIT {
        policy
            .admit_import(&player, &operation(&format!("operation:import:{index}")))
            .unwrap();
    }
    assert_eq!(
        policy.admit_import(&player, &operation("operation:import:overflow")),
        Err(600)
    );
}

#[test]
fn idempotent_import_redelivery_does_not_consume_another_allowance() {
    let (policy, _) = policy(0);
    let player = player("player:idempotent");
    let first = operation("operation:same-import");
    policy.admit_import(&player, &first).unwrap();
    for index in 1..PLAYER_IMPORT_LIMIT {
        policy
            .admit_import(&player, &operation(&format!("operation:other:{index}")))
            .unwrap();
    }
    policy.admit_import(&player, &first).unwrap();
    assert_eq!(
        policy.admit_import(&player, &operation("operation:fresh")),
        Err(600)
    );
}

#[test]
fn import_window_recovers_after_the_rolling_interval() {
    let (policy, clock) = policy(0);
    let player = player("player:import-recover");
    for index in 0..PLAYER_IMPORT_LIMIT {
        policy
            .admit_import(&player, &operation(&format!("operation:recover:{index}")))
            .unwrap();
    }
    clock.advance_ms(PLAYER_IMPORT_WINDOW_MS);
    policy
        .admit_import(&player, &operation("operation:recover:next"))
        .unwrap();
}

#[test]
fn concurrent_command_attempts_cannot_exceed_the_limit() {
    let (policy, _) = policy(0);
    let policy = Arc::new(policy);
    let player = player("player:concurrent");
    let workers = (0..PLAYER_COMMAND_LIMIT + 8)
        .map(|_| {
            let policy = policy.clone();
            let player = player.clone();
            std::thread::spawn(move || policy.admit_command(&player))
        })
        .collect::<Vec<_>>();
    let rejected = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .filter(Result::is_err)
        .count();
    assert_eq!(rejected, 8);
}

#[test]
fn telemetry_field_names_stay_privacy_safe() {
    let names = traffic_telemetry_field_names();
    assert_eq!(names[0], "event");
    assert!(names.contains(&"policy_version"));
    assert!(names.contains(&"class"));
    assert!(names.contains(&"decision"));
    assert!(names.contains(&"retry_after_seconds"));
    assert!(names.contains(&"occupancy"));
    assert!(names.contains(&"bounded_occupancy"));
    for forbidden in [
        "player_id",
        "playerId",
        "authToken",
        "request_body",
        "pgn",
        "fen",
    ] {
        assert!(!names.contains(&forbidden));
    }
    assert_eq!(PLAYER_TRAFFIC_POLICY_VERSION, "v1");
}

#[test]
fn generated_rate_limited_fixture_keeps_retry_after_shape() {
    let events: Vec<serde_json::Value> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/events.json"
    )))
    .unwrap();
    let limited = events
        .iter()
        .find(|event| event["event"]["reason"]["kind"] == "rateLimited")
        .expect("the generated contract includes a rateLimited outcome");
    assert_eq!(limited["event"]["kind"], "unavailable");
    assert_eq!(limited["event"]["reason"]["retryAfterSeconds"], 60);
    assert_eq!(limited["event"]["retry"]["kind"], "retryAfter");
    assert_eq!(limited["event"]["retry"]["seconds"], 60);
    let decoded: crate::review_session_contract::ReviewSessionEventEnvelope =
        serde_json::from_value(limited.clone()).unwrap();
    assert!(matches!(
        decoded.event,
        crate::review_session_contract::ReviewSessionEvent::Unavailable {
            reason: crate::review_session_contract::ProviderUnavailableReason::RateLimited {
                retry_after_seconds: 60,
            },
            retry: crate::review_session_contract::RetryDirective::RetryAfter { seconds: 60 },
            ..
        }
    ));
}
