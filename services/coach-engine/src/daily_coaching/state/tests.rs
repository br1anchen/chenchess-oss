use super::*;
use crate::{
    profile_game_feed::{
        DailyGameInputSource, DailyGameReviewRequest, ProfileGameSourceIdentity,
        ProfileGameTimeControlClass, ProfileGameWindowEntry, ProfileValidationError,
        RecentProfileGameCursor,
    },
    review_session_contract::{
        RequestedEloProfile, RequestedReviewSide, RetryDirective, ReviewSide,
    },
};

#[test]
fn first_connection_enables_coaching_and_captures_timezone_once() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "Europe/Oslo".to_string(),
            test_now(),
        )
        .unwrap();
    state.set_enabled(false, test_now()).unwrap();
    state
        .connect(
            &player(),
            chess_com_connection("ChessPlayer"),
            "America/New_York".to_string(),
            test_now(),
        )
        .unwrap();
    state
        .replace(lichess_connection("PlayerTwo"), "playerone")
        .unwrap();
    state
        .replace(lichess_connection("PlayerTwo"), "playerone")
        .unwrap();

    assert_eq!(
        state.project(),
        DailyCoachingSetupState::Connected {
            enabled: false,
            timezone: "Europe/Oslo".to_string(),
            connections: vec![
                lichess_connection("PlayerTwo").project(),
                chess_com_connection("ChessPlayer").project(),
            ],
        }
    );
}

#[test]
fn reconnect_after_removing_the_last_profile_does_not_redetect_timezone() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "Europe/Oslo".to_string(),
            test_now(),
        )
        .unwrap();
    state
        .remove(DailyCoachingProvider::Lichess, "playerone")
        .unwrap();
    state
        .connect(
            &player(),
            lichess_connection("PlayerTwo"),
            "America/New_York".to_string(),
            test_now(),
        )
        .unwrap();

    assert_eq!(
        state.project(),
        DailyCoachingSetupState::Connected {
            enabled: true,
            timezone: "Europe/Oslo".to_string(),
            connections: vec![lichess_connection("PlayerTwo").project()],
        }
    );
}

#[test]
fn stale_remove_conflicts_but_exact_retry_is_idempotent() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();
    state
        .replace(lichess_connection("PlayerTwo"), "playerone")
        .unwrap();

    let stale = state.remove(DailyCoachingProvider::Lichess, "playerone");
    assert!(matches!(
        stale,
        Err(DailyCoachingStoreError::Domain(
            DailyCoachingDomainError::StalePlayingProfile
        ))
    ));

    state
        .remove(DailyCoachingProvider::Lichess, "playertwo")
        .unwrap();
    state
        .remove(DailyCoachingProvider::Lichess, "playertwo")
        .unwrap();
    assert_eq!(state.project(), DailyCoachingSetupState::NotConnected);
}

#[test]
fn nudge_admission_is_rate_limited_per_player() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();
    let interval = Duration::from_secs(300);

    assert!(state.accept_nudge(test_now(), interval).unwrap().accepted);
    assert!(
        !state
            .accept_nudge(test_now() + chrono::TimeDelta::seconds(299), interval)
            .unwrap()
            .accepted
    );
    assert!(
        state
            .accept_nudge(test_now() + chrono::TimeDelta::seconds(300), interval)
            .unwrap()
            .accepted
    );
}

#[test]
fn replacement_and_removal_each_advance_the_run_fence() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();

    state
        .replace(lichess_connection("PlayerTwo"), "playerone")
        .unwrap();
    assert_eq!(state.run_fence(), 1);

    state
        .remove(DailyCoachingProvider::Lichess, "playertwo")
        .unwrap();
    assert_eq!(state.run_fence(), 2);
}

#[test]
fn fetched_backfill_cannot_mutate_a_replaced_connection() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();
    let stale_fence = state.run_fence();

    state
        .replace(lichess_connection("PlayerTwo"), "playerone")
        .unwrap();

    let error = state
        .resolve_initial_backfill(
            stale_fence,
            DailyCoachingProvider::Lichess,
            "playerone",
            vec![backfill_game("Synthet1", 100)],
        )
        .unwrap_err();

    assert!(matches!(error, DailyCoachingStoreError::Fenced));
    assert!(matches!(
        state
            .connection(DailyCoachingProvider::Lichess)
            .unwrap()
            .initial_backfill(),
        InitialBackfillSnapshot::Pending { .. }
    ));
}

#[test]
fn partial_backfill_checkpoint_round_trips_with_an_empty_candidate_set() {
    let owner_key = DailyCoachingOwnerKey::for_player(&player());
    let mut state = DailyCoachingDocument::empty(owner_key.clone());
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();
    let cursor = RecentProfileGameCursor::test(
        700,
        [ProfileGameSourceIdentity::lichess("Synthet1".to_string())],
    );

    state
        .checkpoint_initial_backfill(
            state.run_fence(),
            DailyCoachingProvider::Lichess,
            "playerone",
            Vec::new(),
            cursor.clone(),
        )
        .unwrap();
    let hydrated =
        serde_json::from_value::<DailyCoachingDocument>(serde_json::to_value(state).unwrap())
            .unwrap();
    hydrated.validate_for(&owner_key).unwrap();
    assert_eq!(
        hydrated
            .connection(DailyCoachingProvider::Lichess)
            .unwrap()
            .initial_backfill(),
        InitialBackfillSnapshot::Pending {
            games: Vec::new(),
            cursor: Some(cursor),
        }
    );
}

#[test]
fn stalled_scan_games_remain_owed_and_preserve_the_terminal_reason_after_reconciliation() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();
    let game = backfill_game("Synthet1", 100);

    state
        .mutate_initial_backfill(
            state.run_fence(),
            DailyCoachingProvider::Lichess,
            "playerone",
            InitialBackfillMutation::ResolveStalled(vec![game.clone()]),
        )
        .unwrap();
    assert_eq!(
        state
            .connection(DailyCoachingProvider::Lichess)
            .unwrap()
            .initial_backfill(),
        InitialBackfillSnapshot::Owed(vec![game.clone()])
    );

    state
        .reconcile_initial_backfills(&BTreeSet::from([game.source_identity]))
        .unwrap();

    assert!(!state.has_unresolved_initial_backfill());
    assert!(state.has_unavailable_initial_backfill());
    let hydrated =
        serde_json::from_value::<DailyCoachingDocument>(serde_json::to_value(state).unwrap())
            .unwrap();
    hydrated
        .validate_for(&DailyCoachingOwnerKey::for_player(&player()))
        .unwrap();
    assert!(!hydrated.has_unresolved_initial_backfill());
    assert!(hydrated.has_unavailable_initial_backfill());
}

#[test]
fn new_replacement_and_readded_identities_each_mint_a_pending_backfill() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();
    assert!(matches!(
        state
            .connection(DailyCoachingProvider::Lichess)
            .unwrap()
            .initial_backfill(),
        InitialBackfillSnapshot::Pending { .. }
    ));

    let run_fence = state.run_fence();
    state
        .resolve_initial_backfill(
            run_fence,
            DailyCoachingProvider::Lichess,
            "playerone",
            vec![backfill_game("Synthet1", 100)],
        )
        .unwrap();
    state
        .replace(lichess_connection("PlayerTwo"), "playerone")
        .unwrap();
    assert!(matches!(
        state
            .connection(DailyCoachingProvider::Lichess)
            .unwrap()
            .initial_backfill(),
        InitialBackfillSnapshot::Pending { .. }
    ));

    state
        .remove(DailyCoachingProvider::Lichess, "playertwo")
        .unwrap();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "America/New_York".to_string(),
            test_now(),
        )
        .unwrap();
    assert!(matches!(
        state
            .connection(DailyCoachingProvider::Lichess)
            .unwrap()
            .initial_backfill(),
        InitialBackfillSnapshot::Pending { .. }
    ));
}

#[test]
fn unfinished_backfill_survives_disable_while_completed_backfill_does_not_repeat() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();
    let game = backfill_game("Synthet1", 100);
    let run_fence = state.run_fence();
    state
        .resolve_initial_backfill(
            run_fence,
            DailyCoachingProvider::Lichess,
            "playerone",
            vec![game.clone()],
        )
        .unwrap();

    state.set_enabled(false, test_now()).unwrap();
    state.set_enabled(true, test_now()).unwrap();
    assert_eq!(
        state
            .connection(DailyCoachingProvider::Lichess)
            .unwrap()
            .initial_backfill(),
        InitialBackfillSnapshot::Owed(vec![game.clone()])
    );

    state
        .reconcile_initial_backfills(&BTreeSet::from([game.source_identity]))
        .unwrap();
    state.set_enabled(false, test_now()).unwrap();
    state.set_enabled(true, test_now()).unwrap();
    assert!(matches!(
        state
            .connection(DailyCoachingProvider::Lichess)
            .unwrap()
            .initial_backfill(),
        InitialBackfillSnapshot::Completed
    ));
}

#[test]
fn unavailable_notices_cover_every_connection_only_while_all_feeds_are_paused() {
    let mut state = empty_state();
    state
        .connect(
            &player(),
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();
    state
        .connect(
            &player(),
            chess_com_connection("ChessPlayer"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();

    state
        .observe_profile_health(
            DailyCoachingProvider::Lichess,
            "playerone",
            ProfileHealthObservation::ProfileUnavailable,
            test_now(),
        )
        .unwrap();
    assert!(state.profile_unavailable_notices().is_empty());

    state
        .observe_profile_health(
            DailyCoachingProvider::ChessCom,
            "chessplayer",
            ProfileHealthObservation::ProfileUnavailable,
            test_now() + chrono::TimeDelta::minutes(1),
        )
        .unwrap();
    let notices = state.profile_unavailable_notices();
    assert_eq!(notices.len(), 2);
    assert_eq!(notices[0].provider, DailyCoachingProvider::Lichess);
    assert_eq!(notices[0].epoch, 1);
    assert_eq!(notices[1].provider, DailyCoachingProvider::ChessCom);
    assert_eq!(notices[1].epoch, 1);

    state
        .observe_profile_health(
            DailyCoachingProvider::Lichess,
            "playerone",
            ProfileHealthObservation::Reachable,
            test_now() + chrono::TimeDelta::minutes(2),
        )
        .unwrap();
    assert!(state.profile_unavailable_notices().is_empty());
    state
        .observe_profile_health(
            DailyCoachingProvider::Lichess,
            "playerone",
            ProfileHealthObservation::ProfileUnavailable,
            test_now() + chrono::TimeDelta::minutes(3),
        )
        .unwrap();
    let notices = state.profile_unavailable_notices();
    assert_eq!(notices.len(), 2);
    assert_eq!(notices[0].provider, DailyCoachingProvider::Lichess);
    assert_eq!(notices[0].epoch, 2);
    assert_eq!(notices[1].provider, DailyCoachingProvider::ChessCom);
    assert_eq!(notices[1].epoch, 1);
}

#[test]
fn persisted_owner_keys_enforce_the_path_segment_contract() {
    let valid = "a".repeat(64);
    assert!(serde_json::from_str::<DailyCoachingOwnerKey>(&format!("\"{valid}\"")).is_ok());
    assert!(serde_json::from_str::<DailyCoachingOwnerKey>("\"ABC\"").is_err());
}

#[tokio::test]
async fn first_store_connection_binds_the_raw_player_identity() {
    let player_id = player();
    let owner_key = DailyCoachingOwnerKey::for_player(&player_id);
    let store = InMemoryDailyCoachingStore::default();

    store
        .connect_profile(
            &owner_key,
            &player_id,
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .await
        .unwrap();
    let state = store.read(&owner_key).await.unwrap();

    assert_eq!(state.player_id(), Some(&player_id));
}

#[test]
fn bound_player_identity_round_trips_in_version_one_state() {
    let player_id = player();
    let owner_key = DailyCoachingOwnerKey::for_player(&player_id);
    let mut state = DailyCoachingDocument::empty(owner_key.clone());
    state
        .connect(
            &player_id,
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap();

    let encoded = serde_json::to_value(&state).unwrap();
    assert_eq!(encoded["schemaVersion"], 1);
    let hydrated = serde_json::from_value::<DailyCoachingDocument>(encoded.clone()).unwrap();
    hydrated.validate_for(&owner_key).unwrap();
    assert_eq!(hydrated.player_id(), Some(&player_id));

    let mut non_current = encoded;
    non_current["schemaVersion"] = serde_json::json!(2);
    assert!(serde_json::from_value::<DailyCoachingDocument>(non_current).is_err());

    let reused_legacy_v1_shape = serde_json::json!({
        "schemaVersion": 1,
        "revision": 4,
        "enabled": false,
        "timezone": "UTC",
        "connections": []
    });
    assert!(serde_json::from_value::<DailyCoachingDocument>(reused_legacy_v1_shape).is_err());
}

#[test]
fn connection_rejects_a_player_identity_that_does_not_match_the_owner_key() {
    let mut state = empty_state();
    let other_player = PlayerId::try_from("player-b".to_string()).unwrap();

    let error = state
        .connect(
            &other_player,
            lichess_connection("PlayerOne"),
            "UTC".to_string(),
            test_now(),
        )
        .unwrap_err();

    assert!(matches!(error, DailyCoachingStoreError::InvalidRecord));
    assert_eq!(state.player_id(), None);
}

#[test]
fn zero_second_retry_after_falls_back_to_an_immediate_retry() {
    assert_eq!(
        super::super::retry_directive(&ProfileValidationError::ProviderUnavailable {
            retry_after_seconds: Some(0),
        }),
        RetryDirective::RetryAllowed
    );
}

fn lichess_connection(username: &str) -> StoredPlayingProfileConnection {
    StoredPlayingProfileConnection::test(DailyCoachingProvider::Lichess, username)
}

fn empty_state() -> DailyCoachingDocument {
    DailyCoachingDocument::empty(DailyCoachingOwnerKey::for_player(&player()))
}

fn player() -> PlayerId {
    PlayerId::try_from("player-a".to_string()).unwrap()
}

fn test_now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-10T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn chess_com_connection(username: &str) -> StoredPlayingProfileConnection {
    StoredPlayingProfileConnection::test(DailyCoachingProvider::ChessCom, username)
}

fn backfill_game(game_id: &str, ended_at_unix_milliseconds: u64) -> ProfileGameWindowEntry {
    ProfileGameWindowEntry {
        source_identity: ProfileGameSourceIdentity::lichess(game_id.to_string()),
        source_profile: "https://lichess.org/@/PlayerOne".to_string(),
        review_request: DailyGameReviewRequest {
            source: DailyGameInputSource::LichessUrl {
                url: format!("https://lichess.org/{game_id}"),
            },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::White,
            },
            elo_profile: RequestedEloProfile::FromImportedMetadata,
            ended_at_unix_milliseconds: Some(ended_at_unix_milliseconds),
        },
        ended_at_unix_milliseconds,
        time_control_raw: "600+0".to_string(),
        time_control_class: ProfileGameTimeControlClass::Rapid,
        expected_clock_seconds: Some(600),
        played_plies: 42,
    }
}
