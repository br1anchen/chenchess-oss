use super::*;
use std::{collections::BTreeSet, sync::Arc};

use crate::daily_coaching::selection::SelectedDailyCoachingGame;
use crate::daily_coaching::state::{
    DailyCoachingStore, InMemoryDailyCoachingStore, InitialBackfillMutation,
    InitialBackfillSnapshot,
};
use crate::daily_coaching::DailyCoachingProvider;
use crate::profile_game_feed::{
    DailyGameInputSource, DailyGameReviewRequest, ProfileGameSourceIdentity,
    ProfileGameTimeControlClass, ProfileGameWindowEntry, RecentProfileGameCursor,
};
use crate::review_session_contract::{
    PlayerId, RequestedEloProfile, RequestedReviewSide, ReviewSide,
};

#[tokio::test]
async fn takeover_increments_the_token_and_rejects_the_previous_holder() {
    let (store, run) = claimed_run("holder-a");
    let address = run.address();
    let old_lease = run.lease().unwrap().clone();
    store.create(run).await.unwrap();
    let takeover_at = test_now() + TimeDelta::minutes(6);

    let taken = store
        .take_over(&address, "holder-b", takeover_at, Duration::from_secs(300))
        .await
        .unwrap()
        .unwrap();
    let rejected = store
        .complete(
            &address,
            &old_lease,
            DailyCoachingRunOutcome::NoDigest,
            takeover_at,
            90,
        )
        .await;

    assert_ne!(taken.lease().unwrap(), &old_lease);
    assert_eq!(taken.takeover_count(), 1);
    assert_eq!(rejected, Err(DailyCoachingRunStoreError::Fenced));
}

#[tokio::test]
async fn reclaimed_run_fences_the_stale_holders_backfill_completion() {
    let (store, run) = claimed_run("holder-a");
    let address = run.address();
    let connection = run.connections()[0].clone();
    let stale_lease = run.lease().unwrap().clone();
    store.create(run).await.unwrap();
    let takeover_at = test_now() + TimeDelta::minutes(6);
    let taken = store
        .take_over(&address, "holder-b", takeover_at, Duration::from_secs(300))
        .await
        .unwrap()
        .unwrap();
    let winner_lease = taken.lease().unwrap().clone();
    let winner_game = selected_game("Synthet1");
    let winner_cursor = RecentProfileGameCursor::test(
        winner_game.ended_at_unix_milliseconds,
        [winner_game.source_identity.clone()],
    );

    store
        .update_initial_backfill(
            &address,
            &winner_lease,
            &connection,
            InitialBackfillMutation::Checkpoint {
                games: vec![winner_game.clone()],
                cursor: winner_cursor.clone(),
            },
        )
        .await
        .unwrap();
    let rejected = store
        .update_initial_backfill(
            &address,
            &stale_lease,
            &connection,
            InitialBackfillMutation::Resolve(vec![selected_game("abcdefgh")]),
        )
        .await;

    assert_eq!(rejected, Err(DailyCoachingRunStoreError::Fenced));
    let state = store.state_store.read(&address.owner_key).await.unwrap();
    assert_eq!(
        state.connections()[0].initial_backfill(),
        InitialBackfillSnapshot::Pending {
            games: vec![winner_game],
            cursor: Some(winner_cursor),
        }
    );
}

#[tokio::test]
async fn replaced_connection_fences_stale_preselection_backfill_reconciliation() {
    let (store, run) = claimed_run("holder-a");
    let address = run.address();
    let connection = run.connections()[0].clone();
    let lease = run.lease().unwrap().clone();
    store.create(run).await.unwrap();
    let mut replacement = store.state_store.read(&address.owner_key).await.unwrap();
    replacement
        .replace(
            StoredPlayingProfileConnection::test(DailyCoachingProvider::Lichess, "PlayerTwo"),
            "playerone",
        )
        .unwrap();
    let mut replacement_game = selected_game("Synthet1");
    replacement_game.source_profile = "https://lichess.org/@/PlayerTwo".to_string();
    replacement
        .resolve_initial_backfill(
            replacement.run_fence(),
            DailyCoachingProvider::Lichess,
            "playertwo",
            vec![replacement_game.clone()],
        )
        .unwrap();
    store.state_store.insert_for_test(replacement);

    let rejected = store
        .update_initial_backfill(
            &address,
            &lease,
            &connection,
            InitialBackfillMutation::Reconcile(BTreeSet::from([replacement_game
                .source_identity
                .clone()])),
        )
        .await;

    assert_eq!(rejected, Err(DailyCoachingRunStoreError::Fenced));
    let current = store.state_store.read(&address.owner_key).await.unwrap();
    assert_eq!(
        current.connections()[0].initial_backfill(),
        InitialBackfillSnapshot::Owed(vec![replacement_game])
    );
}

#[tokio::test]
async fn heartbeat_moves_the_expiry_without_changing_the_token() {
    let (store, run) = claimed_run("holder-a");
    let address = run.address();
    let lease = run.lease().unwrap().clone();
    store.create(run).await.unwrap();
    let heartbeat_at = test_now() + TimeDelta::minutes(1);

    let renewed = store
        .heartbeat(&address, &lease, heartbeat_at, Duration::from_secs(300), 90)
        .await
        .unwrap();

    assert!(renewed.lease().unwrap().expires_at > lease.expires_at);
    assert_eq!(renewed.lease().unwrap().fencing_token, lease.fencing_token);
}

#[tokio::test]
async fn overdue_takeover_holds_a_fenced_lease_long_enough_to_abandon() {
    let (store, run) = claimed_run("holder-a");
    let deadline = run.deadline;
    let address = run.address();
    store.create(run).await.unwrap();
    let takeover_at = deadline + TimeDelta::minutes(1);

    let taken = store
        .take_over(&address, "holder-b", takeover_at, Duration::from_secs(300))
        .await
        .unwrap()
        .unwrap();
    let lease = taken.lease().unwrap().clone();
    let abandoned = store
        .complete(
            &address,
            &lease,
            DailyCoachingRunOutcome::Abandoned,
            takeover_at,
            90,
        )
        .await
        .unwrap();

    assert!(lease.expires_at > deadline);
    assert_eq!(
        abandoned.outcome(),
        Some(DailyCoachingRunOutcome::Abandoned)
    );
}

#[tokio::test]
async fn terminal_transition_rechecks_the_state_fence_atomically() {
    let (store, run) = claimed_run("holder-a");
    let address = run.address();
    let lease = run.lease().unwrap().clone();
    store.create(run).await.unwrap();
    store
        .state_store
        .set_enabled(&address.owner_key, false, test_now())
        .await
        .unwrap();

    let completed = store
        .complete(
            &address,
            &lease,
            DailyCoachingRunOutcome::NoDigest,
            test_now(),
            90,
        )
        .await
        .unwrap();

    assert_eq!(completed.outcome(), Some(DailyCoachingRunOutcome::Fenced));
}

#[tokio::test]
async fn a_rejected_in_memory_transition_leaves_the_durable_run_unchanged() {
    let (store, run) = claimed_run("holder-a");
    let address = run.address();
    let lease = run.lease().unwrap().clone();
    store.create(run).await.unwrap();
    store
        .freeze_selection(
            &address,
            &lease,
            vec![SelectedDailyCoachingGame::daily(selected_game("Synthet1"))],
            test_now(),
            90,
        )
        .await
        .unwrap();

    let rejected = store
        .record_game(
            &address,
            &lease,
            0,
            DailyCoachingGameResult::Retryable,
            test_now(),
            None,
            90,
        )
        .await;

    assert_eq!(rejected, Err(DailyCoachingRunStoreError::InvalidRecord));
    let persisted = store.read(&address).await.unwrap().unwrap();
    assert_eq!(persisted.next_pending_game().unwrap().1.attempts(), 0);
}

#[test]
fn persisted_run_state_round_trips_and_rejects_illegal_field_combinations() {
    let (_, run) = claimed_run("holder-a");
    let stored = serde_json::to_value(&run).unwrap();
    assert_eq!(stored["schemaVersion"], 1);

    let decoded = serde_json::from_value::<DailyCoachingRunDocument>(stored.clone()).unwrap();
    assert_eq!(decoded, run);

    let mut non_current = stored.clone();
    non_current["schemaVersion"] = serde_json::json!(2);
    assert!(serde_json::from_value::<DailyCoachingRunDocument>(non_current).is_err());

    let mut mismatched = stored.clone();
    mismatched["status"] = serde_json::json!("completed");
    assert!(serde_json::from_value::<DailyCoachingRunDocument>(mismatched).is_err());

    let mut expired_before_completion = stored;
    expired_before_completion
        .as_object_mut()
        .unwrap()
        .remove("lease");
    expired_before_completion["status"] = serde_json::json!("completed");
    expired_before_completion["outcome"] = serde_json::json!("noDigest");
    expired_before_completion["finishedAt"] = expired_before_completion["purgeAt"].clone();
    expired_before_completion["nextAttemptAt"] = expired_before_completion["purgeAt"].clone();
    assert!(serde_json::from_value::<DailyCoachingRunDocument>(expired_before_completion).is_err());
}

#[test]
fn retained_run_snapshot_excludes_mutable_initial_backfill_progress() {
    let owner = DailyCoachingOwnerKey::for_player(&player());
    let mut state = DailyCoachingDocument::empty(owner.clone());
    state
        .connect(
            &player(),
            StoredPlayingProfileConnection::test(DailyCoachingProvider::Lichess, "PlayerOne"),
            "UTC".to_string(),
            test_now() - TimeDelta::days(1),
        )
        .unwrap();
    state
        .resolve_initial_backfill(
            state.run_fence(),
            DailyCoachingProvider::Lichess,
            "playerone",
            vec![selected_game("Synthet1")],
        )
        .unwrap();
    let window = DailyWindow::resolve(
        &owner,
        chrono_tz::UTC,
        NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();

    let run = DailyCoachingRunDocument::claimed(
        &state,
        &window,
        "holder-a",
        test_now(),
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let serialized = serde_json::to_value(run).unwrap();

    assert_eq!(
        serialized["connections"][0],
        serde_json::json!({
            "provider": "lichess",
            "identityUsername": "playerone",
            "username": "PlayerOne",
            "canonicalUrl": "https://lichess.org/@/PlayerOne"
        })
    );
    assert!(!serialized.to_string().contains("Synthet1"));
}

/// Firestore keeps the queryable mirror timestamps at microsecond precision,
/// so a run written with sub-microsecond instants comes back changed. The
/// staged `daily-2026-08-14` run became unreadable exactly this way: its
/// mirrored `nextAttemptAt` lost 438 ns against `lease.expiresAt` and every
/// dashboard read failed as an invalid record.
#[test]
fn stored_instants_survive_firestore_microsecond_truncation() {
    let nano_now = test_now().with_nanosecond(123_456_789).unwrap();
    let owner = DailyCoachingOwnerKey::for_player(&player());
    let mut state = DailyCoachingDocument::empty(owner.clone());
    state
        .connect(
            &player(),
            StoredPlayingProfileConnection::test(DailyCoachingProvider::Lichess, "PlayerOne"),
            "UTC".to_string(),
            nano_now - TimeDelta::days(1),
        )
        .unwrap();
    let window = DailyWindow::resolve(
        &owner,
        chrono_tz::UTC,
        NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let mut run = DailyCoachingRunDocument::claimed(
        &state,
        &window,
        "holder-a",
        nano_now,
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    assert_eq!(
        firestore_truncated(&run),
        run,
        "claimed run must round-trip"
    );

    let lease = run.lease().unwrap().clone();
    run.heartbeat(
        &lease,
        nano_now + TimeDelta::minutes(1),
        Duration::from_secs(300),
    )
    .unwrap();
    assert_eq!(firestore_truncated(&run), run, "heartbeat must round-trip");

    run.complete(
        &lease,
        DailyCoachingRunOutcome::NoDigest,
        nano_now + TimeDelta::minutes(2),
        90,
    )
    .unwrap();
    assert_eq!(
        firestore_truncated(&run),
        run,
        "completed run must round-trip"
    );
}

/// What one write-then-read gives back: the exact mirror fields the store
/// queries by, truncated to whole microseconds the way Firestore stores them.
/// Deserialization revalidates, so a run the truncation changes fails here.
fn firestore_truncated(run: &DailyCoachingRunDocument) -> DailyCoachingRunDocument {
    let mut document = serde_json::to_value(run.clone()).unwrap();
    for (field, value) in firestore::run_query_timestamps(run) {
        let microseconds = value
            .with_nanosecond(value.nanosecond() / 1_000 * 1_000)
            .unwrap();
        document[field] = serde_json::json!(microseconds);
    }
    serde_json::from_value(document).unwrap()
}

fn claimed_run(holder: &str) -> (InMemoryDailyCoachingRunStore, DailyCoachingRunDocument) {
    let owner = DailyCoachingOwnerKey::for_player(&player());
    let mut state = DailyCoachingDocument::empty(owner.clone());
    state
        .connect(
            &player(),
            StoredPlayingProfileConnection::test(DailyCoachingProvider::Lichess, "PlayerOne"),
            "UTC".to_string(),
            test_now() - TimeDelta::days(1),
        )
        .unwrap();
    let window = DailyWindow::resolve(
        &owner,
        chrono_tz::UTC,
        NaiveDate::from_ymd_opt(2026, 8, 9).unwrap(),
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let run = DailyCoachingRunDocument::claimed(
        &state,
        &window,
        holder,
        test_now(),
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    state_store.insert_for_test(state);
    (InMemoryDailyCoachingRunStore::new(state_store), run)
}

fn player() -> PlayerId {
    PlayerId::try_from("player-a".to_string()).unwrap()
}

fn selected_game(game_id: &str) -> ProfileGameWindowEntry {
    let ended_at = u64::try_from(test_now().timestamp_millis()).unwrap();
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
            ended_at_unix_milliseconds: Some(ended_at),
        },
        ended_at_unix_milliseconds: ended_at,
        time_control_raw: "300+5".to_string(),
        time_control_class: ProfileGameTimeControlClass::Rapid,
        expected_clock_seconds: Some(500),
        played_plies: 42,
    }
}

fn test_now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-10T03:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
