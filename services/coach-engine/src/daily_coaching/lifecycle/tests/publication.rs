use crate::profile_game_feed::lichess_moves;
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use chrono::TimeDelta;
use tokio::sync::Notify;

use super::*;
use crate::{
    daily_coaching::{
        digest::{
            CoachingDigest, DailyGameTermination, FrozenDailyGameReview, PlayerRelativeOutcome,
        },
        reviewer::{DailyGameReviewFuture, DailyGameReviewResult, DailyGameReviewer},
        runs::{DailyCoachingGameResult, DailyCoachingRunDocument, DailyCoachingRunStore},
        selection::SelectedDailyCoachingGame,
    },
    profile_game_feed::{
        DailyGameReviewRequest, ProfileGameClient, ProfileGameFetchError, ProfileGameRequest,
        ProfileGameResponse,
    },
    review_session_contract::{
        CanonicalGameId, DecisiveGameTermination, GameImportId, GameReview, ImportProvenance,
        ImportedGame, LearningPlan, OperationCompletion, ReviewSessionEvent,
        ReviewSessionEventEnvelope, ReviewSide,
    },
};

mod initial_backfill;
mod priorities;

struct StaticWindowClient {
    body: Vec<u8>,
    calls: AtomicUsize,
}

impl StaticWindowClient {
    fn new(body: Vec<u8>) -> Self {
        Self {
            body,
            calls: AtomicUsize::new(0),
        }
    }
}

impl ProfileGameClient for StaticWindowClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(ProfileGameResponse {
                body: self.body.clone(),
                content_type: request.accept().to_string(),
            })
        })
    }
}

struct ScriptedReviewer {
    results: Mutex<VecDeque<DailyGameReviewResult>>,
    calls: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
}

impl ScriptedReviewer {
    fn new(results: impl IntoIterator<Item = DailyGameReviewResult>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().collect()),
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
        }
    }
}

impl DailyGameReviewer for ScriptedReviewer {
    fn review<'a>(
        &'a self,
        _player_id: &'a PlayerId,
        _request: &'a DailyGameReviewRequest,
    ) -> DailyGameReviewFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            tokio::task::yield_now().await;
            let result = self
                .results
                .lock()
                .expect("scripted reviewer is not poisoned")
                .pop_front()
                .expect("review result is scripted");
            self.active.fetch_sub(1, Ordering::SeqCst);
            result
        })
    }
}

struct BlockingSecondReviewer {
    first: DailyGameReviewResult,
    calls: AtomicUsize,
    second_entered: Arc<Notify>,
    release_second: Arc<Notify>,
}

impl DailyGameReviewer for BlockingSecondReviewer {
    fn review<'a>(
        &'a self,
        _player_id: &'a PlayerId,
        _request: &'a DailyGameReviewRequest,
    ) -> DailyGameReviewFuture<'a> {
        Box::pin(async move {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return self.first.clone();
            }
            self.second_entered.notify_one();
            self.release_second.notified().await;
            DailyGameReviewResult::Terminal
        })
    }
}

#[tokio::test]
async fn publishes_a_frozen_digest_and_card_from_one_successful_review() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let client = Arc::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo"],
    )));
    let reviewer = Arc::new(ScriptedReviewer::new([reviewed_result(
        "Synthet1",
        "game-import:daily:one",
    )]));
    let lifecycle = lifecycle_with_reviewer(
        state_store,
        run_store.clone(),
        client.clone(),
        reviewer.clone(),
    );

    let report = lifecycle.tick(window.due_at).await.unwrap();
    let run = run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .unwrap();
    let (digest, cards) = run_store
        .read_digest(&owner(), &run_address(window.coverage_date).run_id)
        .await
        .unwrap()
        .unwrap();
    let latest_visible = run_store.latest_visible(&owner()).await.unwrap().unwrap();
    let projected = crate::daily_coaching::dashboard::project_digest(digest.clone(), cards.clone());

    assert_eq!(report.published, 1);
    assert_eq!(run.outcome(), Some(DailyCoachingRunOutcome::Published));
    assert_eq!(latest_visible.address(), run.address());
    assert_eq!(digest.game_import_ids.len(), 1);
    assert!(!digest.email_delivery_eligible);
    assert_eq!(cards.len(), 1);
    assert_eq!(projected.digest_id, digest.digest_id);
    assert_eq!(projected.games[0].game_import_id, cards[0].game_import_id);
    assert_eq!(projected.games[0].learning_path_count, 0);
    let mut invalid_summary = digest.clone();
    invalid_summary.game_count = 2;
    assert!(invalid_summary.validate_summary().is_err());
    assert_eq!(cards[0].played_plies, 90);
    assert_eq!(cards[0].source_profile, "https://lichess.org/@/PlayerOne");
    let mut corrupted_digest = digest.clone();
    corrupted_digest.learning_path_count = corrupted_digest.learning_path_count.saturating_add(1);
    assert!(corrupted_digest.validate(&cards).is_err());
    let mut invalid_review_side = cards[0].clone();
    invalid_review_side.review_side = ReviewSide::Both;
    let mut invalid_outcome = cards[0].clone();
    invalid_outcome.player_outcome = PlayerRelativeOutcome::Draw;
    invalid_outcome.termination = DailyGameTermination::Decisive(DecisiveGameTermination::Other);
    let mut half_opening = cards[0].clone();
    half_opening.opening_eco = Some("C20".to_string());
    half_opening.opening_name = None;
    let mut invalid_rating = cards[0].clone();
    invalid_rating.player_rating = Some(99);
    let mut mismatched_provider = cards[0].clone();
    mismatched_provider.source_profile = "https://www.chess.com/member/PlayerOne".to_string();
    let mut invalid_time_control = cards[0].clone();
    invalid_time_control.time_control_raw = "300+5".to_string();
    assert!([
        invalid_review_side,
        invalid_outcome,
        half_opening,
        invalid_rating,
        mismatched_provider,
        invalid_time_control,
    ]
    .iter()
    .all(|card| card.validate().is_err()));
    let mut zero_path_cards = cards.clone();
    zero_path_cards[0].learning_path_count = 0;
    let mut zero_path_digest = digest.clone();
    zero_path_digest.learning_path_count = 0;
    assert!(zero_path_digest.validate(&zero_path_cards).is_ok());
    let mut archived = serde_json::to_value(&digest).unwrap();
    archived["priorityPolicyVersion"] =
        serde_json::json!("coaching-digest-priority/test-only-non-current");
    let archived = serde_json::from_value::<CoachingDigest>(archived).unwrap();
    assert!(archived.validate(&cards).is_ok());
    let mut legacy_digest = serde_json::to_value(&digest).unwrap();
    legacy_digest
        .as_object_mut()
        .unwrap()
        .remove("emailDeliveryEligible");
    let legacy_digest = serde_json::from_value::<CoachingDigest>(legacy_digest).unwrap();
    assert!(!legacy_digest.email_delivery_eligible);
    let mut corrupted_run = serde_json::to_value(&run).unwrap();
    corrupted_run["selection"][0]["progress"]["review"]["reviewSide"] = serde_json::json!("both");
    assert!(serde_json::from_value::<DailyCoachingRunDocument>(corrupted_run).is_err());
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    assert_eq!(reviewer.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dashboard_snapshot_includes_a_digest_published_after_its_run_marker_read() {
    assert_dashboard_publication_interleaving(LatestVisibleReadOrder::BeforePublication).await;
}

#[tokio::test]
async fn dashboard_snapshot_includes_a_digest_published_before_its_run_marker_read() {
    assert_dashboard_publication_interleaving(LatestVisibleReadOrder::AfterPublication).await;
}

async fn assert_dashboard_publication_interleaving(order: LatestVisibleReadOrder) {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let run_store = Arc::new(RacingRunStore::for_dashboard(
        state_store.clone(),
        entered.clone(),
        release.clone(),
        order,
    ));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let (address, lease) = prepare_publishable_run(
        &state_store,
        &run_store.inner,
        &window,
        "Synthet1Demo",
        "Synthet1",
        "game-import:daily:dashboard-snapshot",
    )
    .await;
    let runtime = crate::daily_coaching::DailyCoachingRuntime::new(
        state_store,
        Arc::new(crate::daily_coaching::UnavailableProfileValidator),
        "UTC",
        run_store.clone(),
        Arc::new(ProfileGameFeed::new(
            Arc::new(EmptyWindowClient) as Arc<dyn ProfileGameClient>
        )),
        Arc::new(TerminalReviewer),
        DailyCoachingConfiguration::standard(),
        "dashboard-snapshot-holder",
    )
    .unwrap();
    let reading = tokio::spawn(async move { runtime.dashboard(&player()).await.unwrap() });

    entered.notified().await;
    run_store
        .inner
        .publish(&address, &lease, window.due_at, 90, false)
        .await
        .unwrap();
    release.notify_one();

    let dashboard = reading.await.unwrap();
    let crate::daily_coaching::DailyCoachingDashboardState::Connected { lead, archive, .. } =
        dashboard
    else {
        panic!("the seeded Player remains connected");
    };
    assert_eq!(
        lead,
        crate::daily_coaching::DailyCoachingLeadState::Digest {
            digest_id: address.run_id.clone(),
        }
    );
    assert_eq!(archive.len(), 1);
    assert_eq!(archive[0].digest_id, address.run_id);
}

#[tokio::test]
async fn retry_takeover_reuses_the_frozen_selection_without_refetching_the_provider() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let client = Arc::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo"],
    )));
    let reviewer = Arc::new(ScriptedReviewer::new([
        DailyGameReviewResult::Retryable {
            retry_after_seconds: None,
        },
        reviewed_result("Synthet1", "game-import:daily:retry"),
    ]));
    let lifecycle = lifecycle_with_reviewer(
        state_store,
        run_store.clone(),
        client.clone(),
        reviewer.clone(),
    );

    let first = lifecycle.tick(window.due_at).await.unwrap();
    let retry_at = run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .unwrap()
        .next_attempt_at();
    let second = lifecycle.tick(retry_at).await.unwrap();

    assert_eq!(first.retry_deferred, 1);
    assert_eq!(second.taken_over, 1);
    assert_eq!(second.published, 1);
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    assert_eq!(reviewer.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn deadline_takeover_publishes_successes_already_frozen_in_the_run() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let client = Arc::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo", "Synthet2Demo"],
    )));
    let reviewer = Arc::new(ScriptedReviewer::new([
        reviewed_result("Synthet1", "game-import:daily:before-deadline"),
        DailyGameReviewResult::Retryable {
            retry_after_seconds: None,
        },
    ]));
    let lifecycle =
        lifecycle_with_reviewer(state_store, run_store.clone(), client.clone(), reviewer);
    let address = run_address(window.coverage_date);

    assert_eq!(
        lifecycle.tick(window.due_at).await.unwrap().retry_deferred,
        1
    );
    let taken = run_store
        .take_over(
            &address,
            "deadline-holder",
            window.deadline,
            DailyCoachingConfiguration::standard().lease_ttl,
        )
        .await
        .unwrap()
        .unwrap();
    let mut report = DailyCoachingTickReport::default();
    lifecycle
        .execute(taken, window.deadline, &mut report)
        .await
        .unwrap();
    let (_, cards) = run_store
        .read_digest(&owner(), &address.run_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(report.published, 1);
    assert_eq!(report.abandoned, 0);
    assert_eq!(cards.len(), 1);
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn transient_failures_stop_after_five_attempts_without_refetching_selection() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let client = Arc::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo"],
    )));
    let reviewer = Arc::new(ScriptedReviewer::new((0..5).map(|_| {
        DailyGameReviewResult::Retryable {
            retry_after_seconds: None,
        }
    })));
    let lifecycle = lifecycle_with_reviewer(
        state_store,
        run_store.clone(),
        client.clone(),
        reviewer.clone(),
    );
    let address = run_address(window.coverage_date);
    let mut attempt_at = window.due_at;

    for expected_attempt in 1..=5 {
        let report = lifecycle.tick(attempt_at).await.unwrap();
        if expected_attempt < 5 {
            assert_eq!(report.retry_deferred, 1);
            attempt_at = run_store
                .read(&address)
                .await
                .unwrap()
                .unwrap()
                .next_attempt_at();
        } else {
            assert_eq!(report.no_digest, 1);
            assert_eq!(report.permanent_game_failures, 0);
            assert_eq!(report.retry_exhausted, 1);
        }
    }

    assert_eq!(reviewer.calls.load(Ordering::SeqCst), 5);
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        run_store.read(&address).await.unwrap().unwrap().outcome(),
        Some(DailyCoachingRunOutcome::NoDigest)
    );
}

#[tokio::test]
async fn transient_backoff_past_the_deadline_waits_and_is_never_permanent() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let reviewer = Arc::new(ScriptedReviewer::new([DailyGameReviewResult::Retryable {
        retry_after_seconds: Some(u32::MAX),
    }]));
    let lifecycle = lifecycle_with_reviewer(
        state_store,
        run_store.clone(),
        Arc::new(StaticWindowClient::new(window_body(
            &window,
            &["Synthet1Demo"],
        ))),
        reviewer,
    );
    let address = run_address(window.coverage_date);

    let first = lifecycle.tick(window.due_at).await.unwrap();
    let waiting = run_store.read(&address).await.unwrap().unwrap();
    assert_eq!(first.retry_deferred, 1);
    assert_eq!(first.permanent_game_failures, 0);
    assert_eq!(waiting.next_attempt_at(), window.deadline);
    assert!(run_store
        .read_digest(&owner(), &address.run_id)
        .await
        .unwrap()
        .is_none());

    let taken = run_store
        .take_over(
            &address,
            "deadline-holder",
            window.deadline,
            DailyCoachingConfiguration::standard().lease_ttl,
        )
        .await
        .unwrap()
        .unwrap();
    let mut report = DailyCoachingTickReport::default();
    lifecycle
        .execute(taken, window.deadline, &mut report)
        .await
        .unwrap();

    assert_eq!(report.no_digest, 1);
    assert_eq!(report.permanent_game_failures, 0);
    assert_eq!(report.retry_exhausted, 0);
}

#[tokio::test]
async fn a_permanent_failure_advances_to_the_next_game_and_reviews_sequentially() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let client = Arc::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo", "Synthet2Demo"],
    )));
    let reviewer = Arc::new(ScriptedReviewer::new([
        DailyGameReviewResult::Terminal,
        reviewed_result("Synthet2", "game-import:daily:second"),
    ]));
    let lifecycle =
        lifecycle_with_reviewer(state_store, run_store.clone(), client, reviewer.clone());

    let report = lifecycle.tick(window.due_at).await.unwrap();
    let (_, cards) = run_store
        .read_digest(&owner(), &run_address(window.coverage_date).run_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(report.permanent_game_failures, 1);
    assert_eq!(report.published, 1);
    assert_eq!(reviewer.calls.load(Ordering::SeqCst), 2);
    assert_eq!(reviewer.max_active.load(Ordering::SeqCst), 1);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].source_identity.game_id, "Synthet2");
}

#[tokio::test]
async fn a_published_source_is_filtered_before_the_next_windows_review() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let first_window = current_window(&state_store).await;
    let first_reviewer = Arc::new(ScriptedReviewer::new([reviewed_result(
        "Synthet1",
        "game-import:daily:first-elo",
    )]));
    let first = lifecycle_with_reviewer(
        state_store.clone(),
        run_store.clone(),
        Arc::new(StaticWindowClient::new(window_body(
            &first_window,
            &["Synthet1Demo"],
        ))),
        first_reviewer.clone(),
    );
    assert_eq!(first.tick(first_window.due_at).await.unwrap().published, 1);

    let second_window = current_window(&state_store).await;
    let second_reviewer = Arc::new(ScriptedReviewer::new([reviewed_result(
        "Synthet1",
        "game-import:daily:different-elo",
    )]));
    let second = lifecycle_with_reviewer(
        state_store,
        run_store.clone(),
        Arc::new(StaticWindowClient::new(window_body(
            &second_window,
            &["Synthet1Demo"],
        ))),
        second_reviewer.clone(),
    );
    let report = second.tick(second_window.due_at).await.unwrap();

    assert_eq!(report.no_digest, 1);
    assert_eq!(second_reviewer.calls.load(Ordering::SeqCst), 0);
    assert!(run_store
        .read_digest(&owner(), &run_address(second_window.coverage_date).run_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn archive_orders_late_publications_by_published_time_descending() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let older_window = current_window(&state_store).await;
    let newer_window = DailyWindow::resolve(
        &owner(),
        chrono_tz::UTC,
        next_date(older_window.coverage_date).unwrap(),
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let (older_address, _) = prepare_publishable_run(
        &state_store,
        &run_store,
        &older_window,
        "Synthet1Demo",
        "Synthet1",
        "game-import:daily:archive-older",
    )
    .await;
    let (newer_address, newer_lease) = prepare_publishable_run(
        &state_store,
        &run_store,
        &newer_window,
        "Synthet2Demo",
        "Synthet2",
        "game-import:daily:archive-newer",
    )
    .await;
    run_store
        .publish(&newer_address, &newer_lease, newer_window.due_at, 90, true)
        .await
        .unwrap();

    let recovered_at = newer_window.due_at + TimeDelta::minutes(1);
    let recovered = run_store
        .take_over(
            &older_address,
            "late-recovery-holder",
            recovered_at,
            DailyCoachingConfiguration::standard().lease_ttl,
        )
        .await
        .unwrap()
        .unwrap();
    run_store
        .publish(
            &older_address,
            recovered.lease().unwrap(),
            recovered_at,
            90,
            false,
        )
        .await
        .unwrap();

    let archive = run_store.archive(&owner()).await.unwrap();
    assert_eq!(
        archive
            .iter()
            .map(|digest| digest.digest_id.clone())
            .collect::<Vec<_>>(),
        vec![older_address.run_id, newer_address.run_id,]
    );
    assert!(!archive[0].email_delivery_eligible);
    assert!(archive[1].email_delivery_eligible);
}

#[tokio::test]
async fn regeneration_republishes_one_window_in_place() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let window = current_window(&state_store).await;
    let (address, lease) = prepare_publishable_run(
        &state_store,
        &run_store,
        &window,
        "Synthet1Demo",
        "Synthet1",
        "game-import:daily:original",
    )
    .await;

    let published = run_store
        .publish(&address, &lease, window.due_at, 90, false)
        .await
        .unwrap();
    assert_eq!(published.regeneration_count(), 0);
    let first = run_store.archive(&owner()).await.unwrap();
    assert_eq!(first.len(), 1);

    // An Administrator reopens the published window and rebuilds it.
    let reopened = run_store
        .reopen_for_regeneration(
            &address,
            "regeneration-holder",
            window.due_at + TimeDelta::hours(2),
            Duration::from_secs(300),
            window.due_at + TimeDelta::hours(6),
        )
        .await
        .unwrap();
    assert!(
        reopened.selection().is_none(),
        "a rebuild re-selects rather than republishing frozen reviews"
    );
    let regeneration_lease = reopened.lease().unwrap().clone();
    let selected = ProfileGameFeed::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo"],
    )))
    .eligible_games_in_window(
        "https://lichess.org/@/PlayerOne",
        window.starts_at,
        window.ends_at,
    )
    .await
    .unwrap()
    .pop()
    .unwrap();
    let frozen = frozen_review(&selected, "Synthet1", "game-import:daily:rebuilt");
    run_store
        .freeze_selection(
            &address,
            &regeneration_lease,
            vec![SelectedDailyCoachingGame::daily(selected)],
            window.due_at + TimeDelta::hours(2),
            90,
        )
        .await
        .unwrap();
    run_store
        .record_game(
            &address,
            &regeneration_lease,
            0,
            DailyCoachingGameResult::Reviewed(frozen),
            window.due_at + TimeDelta::hours(2),
            None,
            90,
        )
        .await
        .unwrap();

    let regenerated = run_store
        .publish(
            &address,
            &regeneration_lease,
            window.due_at + TimeDelta::hours(3),
            90,
            false,
        )
        .await
        .unwrap();

    assert_eq!(
        regenerated.outcome(),
        Some(DailyCoachingRunOutcome::Published)
    );
    assert_eq!(regenerated.regeneration_count(), 1);
    let archive = run_store.archive(&owner()).await.unwrap();
    assert_eq!(
        archive.len(),
        1,
        "one coverage date keeps exactly one Coaching Digest"
    );
    assert_eq!(archive[0].digest_id, address.run_id);
    let (digest, cards) = run_store
        .read_digest(&owner(), &address.run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(digest.game_import_ids.len(), 1);
    assert_eq!(
        digest.game_import_ids[0].as_str(),
        "game-import:daily:rebuilt",
        "the rebuilt digest carries the new Game Review, not the superseded one"
    );
    assert_eq!(cards.len(), 1);
    assert_eq!(digest.ordered_card_keys.len(), 1);
    // The digest identity is stable, so the delivery identity must not be: the provider
    // idempotency key is derived from it and would collapse the rebuilt send into the original.
    assert_eq!(digest.regeneration_count, 1);
    assert_eq!(digest.delivery_id(), format!("{}-r1", address.run_id));
    assert_ne!(digest.delivery_id(), digest.digest_id);
}

#[tokio::test]
async fn a_scheduled_publication_still_refuses_to_replace_a_digest() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let window = current_window(&state_store).await;
    let (address, lease) = prepare_publishable_run(
        &state_store,
        &run_store,
        &window,
        "Synthet1Demo",
        "Synthet1",
        "game-import:daily:original",
    )
    .await;
    run_store
        .publish(&address, &lease, window.due_at, 90, false)
        .await
        .unwrap();

    let republished = run_store
        .publish(&address, &lease, window.due_at, 90, false)
        .await
        .unwrap();

    assert_eq!(republished.regeneration_count(), 0);
    assert_eq!(run_store.archive(&owner()).await.unwrap().len(), 1);
}

#[tokio::test]
async fn selection_excludes_other_windows_but_not_the_one_being_rebuilt() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let window = current_window(&state_store).await;
    let (address, lease) = prepare_publishable_run(
        &state_store,
        &run_store,
        &window,
        "Synthet1Demo",
        "Synthet1",
        "game-import:daily:original",
    )
    .await;
    run_store
        .publish(&address, &lease, window.due_at, 90, false)
        .await
        .unwrap();
    let identities = vec![ProfileGameSourceIdentity::lichess("Synthet1".to_string())];

    // An ordinary Run still sees the Game as digested and will not reselect it.
    assert_eq!(
        run_store
            .digested_sources(&owner(), &identities, None)
            .await
            .unwrap()
            .len(),
        1
    );
    // A different window rebuilding its own digest must not reclaim this one's Game.
    assert_eq!(
        run_store
            .digested_sources(&owner(), &identities, Some("daily-2000-01-01"))
            .await
            .unwrap()
            .len(),
        1,
        "another window's rebuild never frees this window's Games"
    );
    // The window that owns the card may reselect it while rebuilding.
    assert!(
        run_store
            .digested_sources(&owner(), &identities, Some(&address.run_id))
            .await
            .unwrap()
            .is_empty(),
        "a rebuild reselects the Games its own digest carries"
    );
}

#[tokio::test]
async fn forced_regeneration_rebuilds_the_last_window_without_moving_the_schedule() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let window = current_window(&state_store).await;
    let (address, lease) = prepare_publishable_run(
        &state_store,
        &run_store,
        &window,
        "Synthet1Demo",
        "Synthet1",
        "game-import:daily:original",
    )
    .await;
    run_store
        .publish(&address, &lease, window.due_at, 90, false)
        .await
        .unwrap();
    let before = state_store.read(&owner()).await.unwrap();

    let lifecycle = regeneration_lifecycle(state_store.clone(), run_store.clone());
    let admitted = lifecycle
        .force_regenerate_last_digest(&player(), window.due_at + TimeDelta::hours(2))
        .await
        .unwrap();

    assert!(admitted, "a published window is available to rebuild");
    let reopened = run_store.read(&address).await.unwrap().unwrap();
    assert_eq!(reopened.regeneration_count(), 1);

    // A second request finds the window no longer terminal and is refused.
    assert!(!lifecycle
        .force_regenerate_last_digest(&player(), window.due_at + TimeDelta::hours(2))
        .await
        .unwrap());

    let after = state_store.read(&owner()).await.unwrap();
    assert_eq!(
        after.next_daily_window(),
        before.next_daily_window(),
        "a rebuild never advances the ordinary schedule"
    );
    assert_eq!(after.run_fence(), before.run_fence());
}

#[tokio::test]
async fn forced_regeneration_is_unavailable_without_a_terminal_window() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let lifecycle = regeneration_lifecycle(state_store, run_store);

    assert!(!lifecycle
        .force_regenerate_last_digest(&player(), instant("2026-08-10T02:00:00Z"))
        .await
        .unwrap());
}

async fn prepare_publishable_run(
    state_store: &InMemoryDailyCoachingStore,
    run_store: &InMemoryDailyCoachingRunStore,
    window: &DailyWindow,
    source_game_id: &str,
    canonical_game_id: &str,
    game_import_id: &str,
) -> (DailyCoachingRunAddress, DailyCoachingRunLease) {
    let state = state_store.read(&owner()).await.unwrap();
    let run = DailyCoachingRunDocument::claimed(
        &state,
        window,
        "archive-order-holder",
        window.due_at,
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let address = run.address();
    let lease = run.lease().unwrap().clone();
    run_store.create(run).await.unwrap();
    let selected = ProfileGameFeed::new(StaticWindowClient::new(window_body(
        window,
        &[source_game_id],
    )))
    .eligible_games_in_window(
        "https://lichess.org/@/PlayerOne",
        window.starts_at,
        window.ends_at,
    )
    .await
    .unwrap()
    .pop()
    .unwrap();
    let frozen = frozen_review(&selected, canonical_game_id, game_import_id);
    run_store
        .freeze_selection(
            &address,
            &lease,
            vec![SelectedDailyCoachingGame::daily(selected)],
            window.due_at,
            90,
        )
        .await
        .unwrap();
    run_store
        .record_game(
            &address,
            &lease,
            0,
            DailyCoachingGameResult::Reviewed(frozen),
            window.due_at,
            None,
            90,
        )
        .await
        .unwrap();
    (address, lease)
}

#[tokio::test(start_paused = true)]
async fn deadline_terminalizes_unfinished_games_and_publishes_partial_success() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let client = Arc::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo", "Synthet2Demo"],
    )));
    let second_entered = Arc::new(Notify::new());
    let reviewer = Arc::new(BlockingSecondReviewer {
        first: reviewed_result("Synthet1", "game-import:daily:partial"),
        calls: AtomicUsize::new(0),
        second_entered: second_entered.clone(),
        release_second: Arc::new(Notify::new()),
    });
    let lifecycle = lifecycle_with_reviewer(state_store, run_store.clone(), client, reviewer);
    let ticking = tokio::spawn({
        let lifecycle = lifecycle.clone();
        async move { lifecycle.tick(window.due_at).await }
    });
    second_entered.notified().await;

    tokio::time::advance(
        window
            .deadline
            .signed_duration_since(window.due_at)
            .to_std()
            .unwrap(),
    )
    .await;
    tokio::task::yield_now().await;
    let report = ticking.await.unwrap().unwrap();
    let (_, cards) = run_store
        .read_digest(&owner(), &run_address(window.coverage_date).run_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(report.published, 1);
    assert_eq!(report.abandoned, 0);
    assert_eq!(cards.len(), 1);
}

#[tokio::test]
async fn publication_fails_closed_while_a_frozen_game_is_still_pending() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let state = state_store.read(&owner()).await.unwrap();
    let run = DailyCoachingRunDocument::claimed(
        &state,
        &window,
        "pending-publication-holder",
        window.due_at,
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let address = run.address();
    let lease = run.lease().unwrap().clone();
    run_store.create(run).await.unwrap();
    let selected = ProfileGameFeed::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo", "Synthet2Demo"],
    )))
    .eligible_games_in_window(
        "https://lichess.org/@/PlayerOne",
        window.starts_at,
        window.ends_at,
    )
    .await
    .unwrap();
    let frozen = frozen_review(&selected[0], "Synthet1", "game-import:daily:pending");
    let run = run_store
        .freeze_selection(
            &address,
            &lease,
            selected
                .into_iter()
                .map(SelectedDailyCoachingGame::daily)
                .collect(),
            window.due_at,
            90,
        )
        .await
        .unwrap();
    let lease = run.lease().unwrap().clone();
    run_store
        .record_game(
            &address,
            &lease,
            0,
            DailyCoachingGameResult::Reviewed(frozen),
            window.due_at,
            None,
            90,
        )
        .await
        .unwrap();

    assert_eq!(
        run_store
            .publish(&address, &lease, window.due_at, 90, false,)
            .await,
        Err(DailyCoachingRunStoreError::InvalidRecord)
    );
    assert!(run_store
        .read_digest(&owner(), &address.run_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn stale_holder_is_fenced_and_concurrent_winner_replays_converge() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let state = state_store.read(&owner()).await.unwrap();
    let run = DailyCoachingRunDocument::claimed(
        &state,
        &window,
        "publication-holder",
        window.due_at,
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let address = run.address();
    let lease = run.lease().unwrap().clone();
    run_store.create(run).await.unwrap();
    let client = StaticWindowClient::new(window_body(&window, &["Synthet1Demo"]));
    let selected = ProfileGameFeed::new(client)
        .eligible_games_in_window(
            "https://lichess.org/@/PlayerOne",
            window.starts_at,
            window.ends_at,
        )
        .await
        .unwrap()
        .pop()
        .unwrap();
    let frozen = frozen_review(&selected, "Synthet1", "game-import:daily:race");
    let run = run_store
        .freeze_selection(
            &address,
            &lease,
            vec![SelectedDailyCoachingGame::daily(selected)],
            window.due_at,
            90,
        )
        .await
        .unwrap();
    let lease = run.lease().unwrap().clone();
    let recorded = run_store
        .record_game(
            &address,
            &lease,
            0,
            DailyCoachingGameResult::Reviewed(frozen),
            window.due_at,
            None,
            90,
        )
        .await
        .unwrap();

    let taken = run_store
        .take_over(
            &address,
            "publication-winner",
            recorded.next_attempt_at(),
            DailyCoachingConfiguration::standard().lease_ttl,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        run_store
            .publish(&address, &lease, recorded.next_attempt_at(), 90, false,)
            .await,
        Err(DailyCoachingRunStoreError::Fenced)
    );
    assert!(run_store
        .read_digest(&owner(), &address.run_id)
        .await
        .unwrap()
        .is_none());
    let lease = taken.lease().unwrap().clone();

    let (left, right) = tokio::join!(
        run_store.publish(&address, &lease, recorded.next_attempt_at(), 90, false,),
        run_store.publish(&address, &lease, recorded.next_attempt_at(), 90, false,),
    );
    let left = left.unwrap();
    let right = right.unwrap();
    let (_, cards) = run_store
        .read_digest(&owner(), &address.run_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(left.outcome(), Some(DailyCoachingRunOutcome::Published));
    assert_eq!(right.outcome(), Some(DailyCoachingRunOutcome::Published));
    assert_eq!(cards.len(), 1);
}

/// A Lifecycle whose provider feed and reviewer never resolve new work, so a forced rebuild is
/// observable through its Run and state transitions alone.
fn regeneration_lifecycle(
    state_store: Arc<InMemoryDailyCoachingStore>,
    run_store: Arc<InMemoryDailyCoachingRunStore>,
) -> Arc<DailyCoachingLifecycle> {
    lifecycle_with_reviewer(
        state_store,
        run_store as Arc<dyn DailyCoachingRunStore>,
        Arc::new(EmptyWindowClient) as Arc<dyn ProfileGameClient>,
        Arc::new(TerminalReviewer),
    )
}

fn lifecycle_with_reviewer(
    state_store: Arc<InMemoryDailyCoachingStore>,
    run_store: Arc<dyn DailyCoachingRunStore>,
    client: Arc<dyn ProfileGameClient>,
    reviewer: Arc<dyn DailyGameReviewer>,
) -> Arc<DailyCoachingLifecycle> {
    Arc::new(DailyCoachingLifecycle::new(
        state_store,
        run_store,
        Arc::new(ProfileGameFeed::new(client)),
        reviewer,
        DailyCoachingConfiguration::standard(),
        "test-holder",
    ))
}

fn window_body(window: &DailyWindow, ids: &[&str]) -> Vec<u8> {
    let ended_at = (window.ends_at - TimeDelta::hours(1)).timestamp_millis();
    ids.iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "variant": "standard",
                "status": "mate",
                "speed": "rapid",
                "clock": { "initial": 600, "increment": 0 },
                "moves": lichess_moves(90),
                "lastMoveAt": ended_at,
                "players": {
                    "white": { "userId": "Opponent" },
                    "black": { "userId": "PlayerOne" }
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

fn reviewed_result(game_id: &str, game_import_id: &str) -> DailyGameReviewResult {
    reviewed_result_with_plan(game_id, game_import_id, LearningPlan::empty())
}

fn reviewed_result_with_plan(
    game_id: &str,
    game_import_id: &str,
    learning_plan: LearningPlan,
) -> DailyGameReviewResult {
    let mut imported: ImportedGame = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
    )))
    .expect("the generated imported Game fixture is valid");
    let ImportProvenance::Lichess {
        canonical_game_id,
        side_qualified_url,
        canonical_url,
        ..
    } = &mut imported.provenance
    else {
        panic!("the generated fixture is a Lichess import");
    };
    *canonical_game_id = CanonicalGameId::try_from(game_id.to_string()).unwrap();
    *side_qualified_url = format!("https://lichess.org/{game_id}0000/black");
    *canonical_url = format!("https://lichess.org/{game_id}");
    let mut review = fixture_review();
    review.learning_plan = learning_plan;
    DailyGameReviewResult::Reviewed {
        game_import_id: GameImportId::try_from(game_import_id.to_string()).unwrap(),
        imported_game: Box::new(imported),
        review: Box::new(review),
    }
}

fn frozen_review(
    selected: &crate::profile_game_feed::ProfileGameWindowEntry,
    game_id: &str,
    game_import_id: &str,
) -> FrozenDailyGameReview {
    let DailyGameReviewResult::Reviewed {
        game_import_id,
        imported_game,
        review,
    } = reviewed_result(game_id, game_import_id)
    else {
        unreachable!("the fixture review succeeds")
    };
    FrozenDailyGameReview::capture(selected, game_import_id, &imported_game, &review).unwrap()
}

fn fixture_review() -> GameReview {
    let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/events.json"
    )))
    .expect("the generated event fixtures are valid");
    events
        .into_iter()
        .find_map(|envelope| match envelope.event {
            ReviewSessionEvent::Completed { result } => match *result {
                OperationCompletion::GameImported { review, .. } => Some(*review),
                _ => None,
            },
            _ => None,
        })
        .expect("the generated fixtures contain an imported Game Review")
}
