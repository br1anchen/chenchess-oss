use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use chrono::{DateTime, NaiveDate, Utc};
use tokio::sync::Notify;

use super::*;
use crate::daily_coaching::{
    runs::{
        DailyCoachingRunAddress, DailyCoachingRunStatus, InMemoryDailyCoachingRunStore,
        RunStoreFuture,
    },
    selection::SelectedDailyCoachingGame,
    state::{InMemoryDailyCoachingStore, InitialBackfillMutation, StoredPlayingProfileConnection},
    DailyCoachingOwnerKey, DailyCoachingProvider, DailyCoachingSetupState,
    PlayingProfileConnectionStatus,
};
use crate::profile_game_feed::{
    ChessProfileProvider, DailyGameInputSource, DailyGameReviewRequest, ProfileGameFetchError,
    ProfileGameRequest, ProfileGameResponse, ProfileGameSourceIdentity, ProfileGameWindowEntry,
    RecentProfileGameCursor, RecentProfileGameScanPage,
};
use crate::review_session_contract::{RequestedEloProfile, RequestedReviewSide, ReviewSide};

struct TerminalReviewer;

impl crate::daily_coaching::reviewer::DailyGameReviewer for TerminalReviewer {
    fn review<'a>(
        &'a self,
        _player_id: &'a PlayerId,
        _request: &'a DailyGameReviewRequest,
    ) -> crate::daily_coaching::reviewer::DailyGameReviewFuture<'a> {
        Box::pin(async { crate::daily_coaching::reviewer::DailyGameReviewResult::Terminal })
    }
}

struct EmptyWindowClient;

impl ProfileGameClient for EmptyWindowClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move { Ok(empty_response(request)) })
    }
}

struct CountingEmptyWindowClient(AtomicUsize);

impl ProfileGameClient for CountingEmptyWindowClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(empty_response(request)) })
    }
}

struct ProfileHealthTestClient(AtomicUsize);

impl ProfileHealthTestClient {
    fn set_outcome(&self, outcome: usize) {
        self.0.store(outcome, Ordering::SeqCst);
    }
}

impl ProfileGameClient for ProfileHealthTestClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        let outcome = self.0.load(Ordering::SeqCst);
        Box::pin(async move {
            match outcome {
                0 => Err(ProfileGameFetchError::Status {
                    provider: request.provider(),
                    code: 404,
                    retry_after_seconds: None,
                }),
                1 => Err(ProfileGameFetchError::Timeout {
                    provider: request.provider(),
                }),
                _ => Ok(empty_response(request)),
            }
        })
    }
}

struct BlockingEmptyWindowClient {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

impl ProfileGameClient for BlockingEmptyWindowClient {
    fn fetch<'a>(
        &'a self,
        request: &'a ProfileGameRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProfileGameResponse, ProfileGameFetchError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.entered.notify_one();
            self.release.notified().await;
            Ok(empty_response(request))
        })
    }
}

struct RacingRunStore {
    inner: InMemoryDailyCoachingRunStore,
    calls: AtomicUsize,
    created: AtomicUsize,
    existing: AtomicUsize,
    claim_race: Option<ClaimInterleaving>,
    dashboard_read: Option<DashboardReadInterleaving>,
}

struct ClaimInterleaving {
    first_created: Arc<Notify>,
    release_first: Arc<Notify>,
    second_attempted: Arc<Notify>,
}

#[derive(Clone, Copy)]
enum LatestVisibleReadOrder {
    BeforePublication,
    AfterPublication,
}

struct DashboardReadInterleaving {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    order: LatestVisibleReadOrder,
}

impl RacingRunStore {
    fn new(
        state_store: Arc<InMemoryDailyCoachingStore>,
        first_created: Arc<Notify>,
        release_first: Arc<Notify>,
        second_attempted: Arc<Notify>,
    ) -> Self {
        Self {
            inner: InMemoryDailyCoachingRunStore::new(state_store),
            calls: AtomicUsize::new(0),
            created: AtomicUsize::new(0),
            existing: AtomicUsize::new(0),
            claim_race: Some(ClaimInterleaving {
                first_created,
                release_first,
                second_attempted,
            }),
            dashboard_read: None,
        }
    }

    fn for_dashboard(
        state_store: Arc<InMemoryDailyCoachingStore>,
        entered: Arc<Notify>,
        release: Arc<Notify>,
        order: LatestVisibleReadOrder,
    ) -> Self {
        Self {
            inner: InMemoryDailyCoachingRunStore::new(state_store),
            calls: AtomicUsize::new(0),
            created: AtomicUsize::new(0),
            existing: AtomicUsize::new(0),
            claim_race: None,
            dashboard_read: Some(DashboardReadInterleaving {
                entered,
                release,
                order,
            }),
        }
    }
}

impl DailyCoachingRunStore for RacingRunStore {
    fn list_digested_game_cards<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Vec<DigestedGameCard>> {
        self.inner.list_digested_game_cards(owner_key)
    }

    fn create<'a>(
        &'a self,
        run: DailyCoachingRunDocument,
    ) -> RunStoreFuture<'a, DailyCoachingRunClaim> {
        let Some(interleaving) = &self.claim_race else {
            return self.inner.create(run);
        };
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let claim = self.inner.create(run).await?;
            match claim {
                DailyCoachingRunClaim::Created(_) => {
                    self.created.fetch_add(1, Ordering::SeqCst);
                }
                DailyCoachingRunClaim::Existing => {
                    self.existing.fetch_add(1, Ordering::SeqCst);
                }
            }
            if call == 0 {
                interleaving.first_created.notify_one();
                interleaving.release_first.notified().await;
            } else {
                interleaving.second_attempted.notify_one();
            }
            Ok(claim)
        })
    }

    fn expired<'a>(
        &'a self,
        now: DateTime<Utc>,
    ) -> RunStoreFuture<'a, Vec<DailyCoachingRunDocument>> {
        self.inner.expired(now)
    }

    fn finished_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> RunStoreFuture<'a, Vec<DailyCoachingRunDocument>> {
        self.inner.finished_between(starts_at, ends_at)
    }

    fn check_fence<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a super::super::runs::DailyCoachingRunLease,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        self.inner.check_fence(address, lease, now, retention_days)
    }

    fn take_over<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        holder_id: &'a str,
        now: DateTime<Utc>,
        lease_ttl: Duration,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>> {
        self.inner.take_over(address, holder_id, now, lease_ttl)
    }

    fn heartbeat<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a super::super::runs::DailyCoachingRunLease,
        now: DateTime<Utc>,
        lease_ttl: Duration,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        self.inner
            .heartbeat(address, lease, now, lease_ttl, retention_days)
    }

    fn digested_sources<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        candidates: &'a [ProfileGameSourceIdentity],
        rebuilding: Option<&'a str>,
    ) -> RunStoreFuture<'a, std::collections::BTreeSet<ProfileGameSourceIdentity>> {
        self.inner
            .digested_sources(owner_key, candidates, rebuilding)
    }

    fn update_initial_backfill<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a super::super::runs::DailyCoachingRunLease,
        connection: &'a super::super::runs::DailyCoachingRunConnection,
        mutation: InitialBackfillMutation,
    ) -> RunStoreFuture<'a, crate::daily_coaching::DailyCoachingDocument> {
        self.inner
            .update_initial_backfill(address, lease, connection, mutation)
    }

    fn freeze_selection<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a super::super::runs::DailyCoachingRunLease,
        selection: Vec<SelectedDailyCoachingGame>,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        self.inner
            .freeze_selection(address, lease, selection, now, retention_days)
    }

    fn record_game<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a super::super::runs::DailyCoachingRunLease,
        index: usize,
        result: super::super::runs::DailyCoachingGameResult,
        now: DateTime<Utc>,
        retry_at: Option<DateTime<Utc>>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        self.inner
            .record_game(address, lease, index, result, now, retry_at, retention_days)
    }

    fn publish<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a super::super::runs::DailyCoachingRunLease,
        now: DateTime<Utc>,
        retention_days: u32,
        email_delivery_eligible: bool,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        self.inner
            .publish(address, lease, now, retention_days, email_delivery_eligible)
    }

    fn reopen_for_regeneration<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        holder_id: &'a str,
        now: DateTime<Utc>,
        lease_ttl: std::time::Duration,
        deadline: DateTime<Utc>,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        self.inner
            .reopen_for_regeneration(address, holder_id, now, lease_ttl, deadline)
    }

    fn archive<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Vec<crate::daily_coaching::digest::CoachingDigest>> {
        self.inner.archive(owner_key)
    }

    fn latest_visible<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>> {
        let Some(interleaving) = &self.dashboard_read else {
            return self.inner.latest_visible(owner_key);
        };
        Box::pin(async move {
            match interleaving.order {
                LatestVisibleReadOrder::BeforePublication => {
                    let visible = self.inner.latest_visible(owner_key).await?;
                    interleaving.entered.notify_one();
                    interleaving.release.notified().await;
                    Ok(visible)
                }
                LatestVisibleReadOrder::AfterPublication => {
                    interleaving.entered.notify_one();
                    interleaving.release.notified().await;
                    self.inner.latest_visible(owner_key).await
                }
            }
        })
    }

    fn complete<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a super::super::runs::DailyCoachingRunLease,
        outcome: DailyCoachingRunOutcome,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument> {
        self.inner
            .complete(address, lease, outcome, now, retention_days)
    }

    fn read<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>> {
        self.inner.read(address)
    }

    fn read_digest<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
    ) -> RunStoreFuture<
        'a,
        Option<(
            crate::daily_coaching::digest::CoachingDigest,
            Vec<crate::daily_coaching::digest::DigestedGameCard>,
        )>,
    > {
        self.inner.read_digest(owner_key, digest_id)
    }
}

#[tokio::test]
async fn tick_claims_one_due_window_completes_no_digest_and_never_reclaims_it() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let lifecycle = lifecycle(state_store, run_store.clone(), Arc::new(EmptyWindowClient));

    let first = lifecycle.tick(window.due_at).await.unwrap();
    let second = lifecycle.tick(window.due_at).await.unwrap();
    let run = run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .unwrap();
    let latest_visible = run_store.latest_visible(&owner()).await.unwrap().unwrap();

    assert_eq!(first.claimed, 1);
    assert_eq!(first.no_digest, 1);
    assert_eq!(second, DailyCoachingTickReport::default());
    assert_eq!(run.status(), DailyCoachingRunStatus::Completed);
    assert_eq!(run.outcome(), Some(DailyCoachingRunOutcome::NoDigest));
    assert_eq!(latest_visible.address(), run.address());
}

#[tokio::test]
async fn tick_records_a_missed_window_as_skipped_without_resolving_it() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let resolver = Arc::new(CountingEmptyWindowClient(AtomicUsize::new(0)));
    seed_player(&state_store, instant("2026-08-08T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let lifecycle = lifecycle(state_store, run_store.clone(), resolver.clone());

    let report = lifecycle.tick(window.deadline).await.unwrap();
    let run = run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(report.skipped, 1);
    assert_eq!(report.claimed, 0);
    assert_eq!(resolver.0.load(Ordering::SeqCst), 0);
    assert_eq!(run.status(), DailyCoachingRunStatus::Completed);
    assert_eq!(run.outcome(), Some(DailyCoachingRunOutcome::Skipped));
    assert!(run_store.latest_visible(&owner()).await.unwrap().is_none());
}

#[tokio::test]
async fn a_selection_404_sets_profile_unavailable_and_only_success_clears_it() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let resolver = Arc::new(ProfileHealthTestClient(AtomicUsize::new(0)));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let lifecycle = lifecycle(state_store.clone(), run_store, resolver.clone());

    let report = lifecycle.tick(window.due_at).await.unwrap();
    assert_eq!(report.no_digest, 1);
    assert_connection_status(
        &state_store,
        PlayingProfileConnectionStatus::ProfileUnavailable,
    )
    .await;

    resolver.set_outcome(1);
    assert!(matches!(
        lifecycle
            .check_profile(
                &player(),
                DailyCoachingProvider::Lichess,
                "playerone",
                window.due_at + TimeDelta::minutes(1),
            )
            .await
            .unwrap(),
        ProfileCheckResult::ProviderUnavailable(_)
    ));
    assert_connection_status(
        &state_store,
        PlayingProfileConnectionStatus::ProfileUnavailable,
    )
    .await;

    resolver.set_outcome(2);
    assert!(matches!(
        lifecycle
            .check_profile(
                &player(),
                DailyCoachingProvider::Lichess,
                "playerone",
                window.due_at + TimeDelta::minutes(2),
            )
            .await
            .unwrap(),
        ProfileCheckResult::Reachable
    ));
    assert_connection_status(&state_store, PlayingProfileConnectionStatus::Connected).await;
}

#[tokio::test]
async fn a_successful_check_at_the_claim_horizon_does_not_resurrect_the_window() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let resolver = Arc::new(CountingEmptyWindowClient(AtomicUsize::new(0)));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let lifecycle = lifecycle(state_store, run_store.clone(), resolver.clone());

    assert!(matches!(
        lifecycle
            .check_profile(
                &player(),
                DailyCoachingProvider::Lichess,
                "playerone",
                window.deadline,
            )
            .await
            .unwrap(),
        ProfileCheckResult::Reachable
    ));
    assert_eq!(resolver.0.load(Ordering::SeqCst), 1);
    assert!(run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn disablement_fences_a_run_at_its_next_safe_boundary() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let lifecycle = lifecycle(
        state_store.clone(),
        run_store.clone(),
        Arc::new(BlockingEmptyWindowClient {
            entered: entered.clone(),
            release: release.clone(),
        }),
    );
    let ticking = tokio::spawn({
        let lifecycle = lifecycle.clone();
        async move { lifecycle.tick(window.due_at).await }
    });
    entered.notified().await;

    state_store
        .set_enabled(&owner(), false, window.due_at)
        .await
        .unwrap();
    release.notify_one();
    let report = ticking.await.unwrap().unwrap();
    let run = run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(report.fenced, 1);
    assert_eq!(run.outcome(), Some(DailyCoachingRunOutcome::Fenced));
}

#[tokio::test]
async fn disabled_overdue_run_is_fenced_before_deadline_or_provider_work() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let client = Arc::new(CountingEmptyWindowClient(AtomicUsize::new(0)));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let state = state_store.read(&owner()).await.unwrap();
    run_store
        .create(
            DailyCoachingRunDocument::claimed(
                &state,
                &window,
                "expired-holder",
                window.due_at,
                &DailyCoachingConfiguration::standard(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    state_store
        .set_enabled(&owner(), false, window.due_at)
        .await
        .unwrap();
    let lifecycle = lifecycle(state_store, run_store.clone(), client.clone());

    let report = lifecycle
        .tick(window.deadline + TimeDelta::seconds(1))
        .await
        .unwrap();
    let run = run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(report.taken_over, 1);
    assert_eq!(report.fenced, 1);
    assert_eq!(report.abandoned, 0);
    assert_eq!(client.0.load(Ordering::SeqCst), 0);
    assert_eq!(run.outcome(), Some(DailyCoachingRunOutcome::Fenced));
}

#[tokio::test]
async fn fencing_during_deadline_terminalization_stops_before_finishing_the_run() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let state = state_store.read(&owner()).await.unwrap();
    let run = DailyCoachingRunDocument::claimed(
        &state,
        &window,
        "deadline-holder",
        window.due_at,
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let address = run.address();
    let lease = run.lease().unwrap().clone();
    run_store.create(run).await.unwrap();
    let run = run_store
        .freeze_selection(
            &address,
            &lease,
            vec![SelectedDailyCoachingGame::daily(selected_game(&window))],
            window.due_at,
            90,
        )
        .await
        .unwrap();
    state_store
        .set_enabled(&owner(), false, window.deadline)
        .await
        .unwrap();
    let lifecycle = lifecycle(state_store, run_store.clone(), Arc::new(EmptyWindowClient));
    let mut report = DailyCoachingTickReport::default();

    let boundary = lifecycle
        .terminalize_unfinished(&address, &lease, run, window.deadline, &mut report)
        .await
        .unwrap();

    assert!(matches!(boundary, TerminalizationBoundary::Fenced));
    assert_eq!(report.fenced, 1);
    assert_eq!(report.failed, 0);
    assert_eq!(
        run_store.read(&address).await.unwrap().unwrap().outcome(),
        Some(DailyCoachingRunOutcome::Fenced)
    );
}

#[tokio::test(start_paused = true)]
async fn resolution_that_reaches_the_deadline_is_abandoned_not_no_digest() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let lifecycle = lifecycle(
        state_store,
        run_store.clone(),
        Arc::new(BlockingEmptyWindowClient {
            entered: entered.clone(),
            release,
        }),
    );
    let ticking = tokio::spawn({
        let lifecycle = lifecycle.clone();
        async move { lifecycle.tick(window.due_at).await }
    });
    entered.notified().await;

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
    let run = run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(report.abandoned, 1);
    assert_eq!(report.no_digest, 0);
    assert_eq!(run.outcome(), Some(DailyCoachingRunOutcome::Abandoned));
}

#[tokio::test(start_paused = true)]
async fn lifecycle_heartbeats_its_lease_while_resolution_is_in_flight() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let lifecycle = lifecycle(
        state_store,
        run_store.clone(),
        Arc::new(BlockingEmptyWindowClient {
            entered: entered.clone(),
            release: release.clone(),
        }),
    );
    let ticking = tokio::spawn({
        let lifecycle = lifecycle.clone();
        async move { lifecycle.tick(window.due_at).await }
    });
    entered.notified().await;
    let address = run_address(window.coverage_date);
    let original = run_store
        .read(&address)
        .await
        .unwrap()
        .unwrap()
        .lease()
        .unwrap()
        .clone();

    tokio::time::advance(Duration::from_secs(60)).await;
    tokio::task::yield_now().await;
    let renewed = run_store
        .read(&address)
        .await
        .unwrap()
        .unwrap()
        .lease()
        .unwrap()
        .clone();

    assert_ne!(renewed, original);
    release.notify_one();
    assert_eq!(ticking.await.unwrap().unwrap().no_digest, 1);
}

#[tokio::test]
async fn tick_takes_over_an_expired_run_before_finishing_it() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let state = state_store.read(&owner()).await.unwrap();
    let run = DailyCoachingRunDocument::claimed(
        &state,
        &window,
        "expired-holder",
        window.due_at,
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap();
    let address = run.address();
    let expired_lease = run.lease().unwrap().clone();
    run_store.create(run).await.unwrap();
    let lifecycle = lifecycle(state_store, run_store.clone(), Arc::new(EmptyWindowClient));
    let takeover_at = window.due_at + TimeDelta::minutes(6);

    let report = lifecycle.tick(takeover_at).await.unwrap();
    let stale_write = run_store
        .complete(
            &address,
            &expired_lease,
            DailyCoachingRunOutcome::NoDigest,
            takeover_at,
            90,
        )
        .await;

    assert_eq!(report.taken_over, 1);
    assert_eq!(report.no_digest, 1);
    assert_eq!(stale_write, Err(DailyCoachingRunStoreError::Fenced));
}

#[tokio::test]
async fn tick_and_arrival_race_resolves_to_one_created_run_and_one_existing_no_op() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let first_created = Arc::new(Notify::new());
    let release_first = Arc::new(Notify::new());
    let second_attempted = Arc::new(Notify::new());
    let run_store = Arc::new(RacingRunStore::new(
        state_store.clone(),
        first_created.clone(),
        release_first.clone(),
        second_attempted.clone(),
    ));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let lifecycle = lifecycle(state_store, run_store.clone(), Arc::new(EmptyWindowClient));
    let ticking = tokio::spawn({
        let lifecycle = lifecycle.clone();
        async move { lifecycle.tick(window.due_at).await }
    });
    first_created.notified().await;

    assert!(lifecycle.promote(&player(), window.due_at).await.unwrap());
    second_attempted.notified().await;
    release_first.notify_one();
    let report = ticking.await.unwrap().unwrap();
    let run = run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(run_store.calls.load(Ordering::SeqCst), 2);
    assert_eq!(run_store.created.load(Ordering::SeqCst), 1);
    assert_eq!(run_store.existing.load(Ordering::SeqCst), 1);
    assert_eq!(report.claimed, 1);
    assert_eq!(run.outcome(), Some(DailyCoachingRunOutcome::NoDigest));
}

#[tokio::test]
async fn arrival_returns_before_work_and_rate_limits_a_second_nudge() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let lifecycle = lifecycle(
        state_store,
        run_store,
        Arc::new(BlockingEmptyWindowClient {
            entered: entered.clone(),
            release: release.clone(),
        }),
    );

    assert!(lifecycle.promote(&player(), window.due_at).await.unwrap());
    assert!(!lifecycle.promote(&player(), window.due_at).await.unwrap());
    entered.notified().await;
    release.notify_one();
}

#[tokio::test]
async fn a_continue_page_that_fills_the_backfill_cap_checkpoints_until_the_cutoff_is_exhausted() {
    let state_store = InMemoryDailyCoachingStore::default();
    seed_player_with_pending_backfill(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let mut games = (0..5)
        .map(|index| selected_game_for(&window, &format!("85SQH9d{index}")))
        .collect::<Vec<_>>();
    let found = games.pop().unwrap();
    let cursor = RecentProfileGameCursor::test_exhausting_cutoff(
        found.ended_at_unix_milliseconds,
        [found.source_identity.clone()],
    );

    let mutation = super::initial_backfill::initial_backfill_update(
        games,
        RecentProfileGameScanPage::Continue {
            games: vec![found],
            cursor,
        },
    )
    .unwrap();

    assert!(matches!(
        mutation,
        InitialBackfillMutation::Checkpoint { games, .. } if games.len() == 5
    ));
}

#[tokio::test]
async fn a_stalled_scan_preserves_discovered_games_for_selection() {
    let state_store = InMemoryDailyCoachingStore::default();
    seed_player_with_pending_backfill(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let found = selected_game(&window);

    let mutation = super::initial_backfill::initial_backfill_update(
        Vec::new(),
        RecentProfileGameScanPage::Stalled(vec![found.clone()]),
    )
    .unwrap();

    assert_eq!(
        mutation,
        InitialBackfillMutation::ResolveStalled(vec![found])
    );
}

fn lifecycle(
    state_store: Arc<InMemoryDailyCoachingStore>,
    run_store: Arc<dyn DailyCoachingRunStore>,
    client: Arc<dyn ProfileGameClient>,
) -> Arc<DailyCoachingLifecycle> {
    lifecycle_with_configuration(
        state_store,
        run_store,
        client,
        DailyCoachingConfiguration::standard(),
    )
}

fn lifecycle_with_configuration(
    state_store: Arc<InMemoryDailyCoachingStore>,
    run_store: Arc<dyn DailyCoachingRunStore>,
    client: Arc<dyn ProfileGameClient>,
    configuration: DailyCoachingConfiguration,
) -> Arc<DailyCoachingLifecycle> {
    Arc::new(DailyCoachingLifecycle::new(
        state_store,
        run_store,
        Arc::new(ProfileGameFeed::new(client)),
        Arc::new(TerminalReviewer),
        configuration,
        "test-holder",
    ))
}

#[tokio::test]
async fn kill_switch_leaves_due_windows_unclaimed_and_invisible() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let mut configuration = DailyCoachingConfiguration::standard();
    configuration.operations.run_claims_enabled = false;
    let lifecycle = lifecycle_with_configuration(
        state_store.clone(),
        run_store.clone(),
        Arc::new(EmptyWindowClient),
        configuration,
    );

    let report = lifecycle.tick(window.deadline).await.unwrap();

    assert_eq!(report, DailyCoachingTickReport::default());
    assert!(run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        state_store
            .read(&owner())
            .await
            .unwrap()
            .next_daily_window(),
        Some(window.coverage_date)
    );
    assert!(!lifecycle.promote(&player(), window.due_at).await.unwrap());
}

#[test]
fn batch_capacity_is_two_and_reserves_interactive_engine_workers() {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    let lifecycle = lifecycle(state_store, run_store, Arc::new(EmptyWindowClient));

    let first = lifecycle.batch_capacity.try_acquire().unwrap();
    let second = lifecycle.batch_capacity.try_acquire().unwrap();
    assert!(lifecycle.batch_capacity.try_acquire().is_err());
    drop(first);
    assert!(lifecycle.batch_capacity.try_acquire().is_ok());
    drop(second);
}

async fn seed_player(store: &InMemoryDailyCoachingStore, connected_at: DateTime<Utc>) {
    seed_player_with_pending_backfill(store, connected_at).await;
    store
        .resolve_initial_backfill(
            &owner(),
            0,
            DailyCoachingProvider::Lichess,
            "playerone".to_string(),
            Vec::new(),
        )
        .await
        .unwrap();
}

async fn seed_player_with_pending_backfill(
    store: &InMemoryDailyCoachingStore,
    connected_at: DateTime<Utc>,
) {
    store
        .connect_profile(
            &owner(),
            &player(),
            StoredPlayingProfileConnection::test(DailyCoachingProvider::Lichess, "PlayerOne"),
            "UTC".to_string(),
            connected_at,
        )
        .await
        .unwrap();
}

async fn current_window(store: &InMemoryDailyCoachingStore) -> DailyWindow {
    let state = store.read(&owner()).await.unwrap();
    DailyWindow::resolve(
        state.owner_key(),
        chrono_tz::UTC,
        state.next_daily_window().unwrap(),
        &DailyCoachingConfiguration::standard(),
    )
    .unwrap()
}

async fn assert_connection_status(
    store: &InMemoryDailyCoachingStore,
    expected: PlayingProfileConnectionStatus,
) {
    let state = store.read(&owner()).await.unwrap().project();
    let DailyCoachingSetupState::Connected { connections, .. } = state else {
        panic!("the seeded Player stays connected")
    };
    assert_eq!(connections[0].status, expected);
}

fn run_address(coverage_date: NaiveDate) -> DailyCoachingRunAddress {
    DailyCoachingRunAddress {
        owner_key: owner(),
        run_id: format!("daily-{coverage_date}"),
    }
}

fn owner() -> DailyCoachingOwnerKey {
    DailyCoachingOwnerKey::for_player(&player())
}

fn player() -> PlayerId {
    PlayerId::try_from("player-a".to_string()).unwrap()
}

fn instant(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn empty_response(request: &ProfileGameRequest) -> ProfileGameResponse {
    ProfileGameResponse {
        body: match request.provider() {
            ChessProfileProvider::Lichess => Vec::new(),
            ChessProfileProvider::ChessCom => br#"{"games":[]}"#.to_vec(),
        },
        content_type: request.accept().to_string(),
    }
}

fn selected_game(window: &DailyWindow) -> ProfileGameWindowEntry {
    selected_game_for(window, "Synthet1")
}

fn selected_game_for(window: &DailyWindow, game_id: &str) -> ProfileGameWindowEntry {
    let ended_at_unix_milliseconds =
        u64::try_from((window.ends_at - TimeDelta::hours(1)).timestamp_millis()).unwrap();
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
        time_control_class: crate::profile_game_feed::ProfileGameTimeControlClass::Rapid,
        expected_clock_seconds: Some(600),
        played_plies: 42,
    }
}

mod publication;
