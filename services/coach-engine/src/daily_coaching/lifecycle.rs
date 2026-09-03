use std::{collections::BTreeMap, sync::Arc, time::Duration};

use chrono::{DateTime, TimeDelta, Utc};
use chrono_tz::Tz;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::{
    profile_game_feed::{
        ProfileGameClient, ProfileGameFeed, ProfileGameFeedError, ProfileGameWindowEntry,
    },
    review_session_contract::PlayerId,
};

use super::{
    configuration::DailyCoachingConfiguration,
    delivery::DigestEmailRuntime,
    digest::{CoachingDigest, DigestedGameCard, FrozenDailyGameReview},
    operator::{DegradedProviderReason, OperatorDigestRuntime},
    reviewer::{DailyGameReviewResult, DailyGameReviewer},
    runs::{
        DailyCoachingGameResult, DailyCoachingRunAddress, DailyCoachingRunClaim,
        DailyCoachingRunConnection, DailyCoachingRunDocument, DailyCoachingRunLease,
        DailyCoachingRunOutcome, DailyCoachingRunStatus, DailyCoachingRunStore,
        DailyCoachingRunStoreError,
    },
    schedule::{next_date, DailyWindow, DailyWindowError},
    selection::select_daily_and_backfill_games,
    DailyCoachingDocument, DailyCoachingOwnerKey, DailyCoachingProvider, DailyCoachingStore,
    DailyCoachingStoreError,
};

mod initial_backfill;
mod profile_health;
mod work_boundary;

pub(super) use profile_health::ProfileCheckResult;
use work_boundary::{elapsed_time, WorkBoundary};

#[derive(Clone)]
pub(crate) struct DailyCoachingLifecycle {
    state_store: Arc<dyn DailyCoachingStore>,
    run_store: Arc<dyn DailyCoachingRunStore>,
    profile_feed: Arc<ProfileGameFeed<Arc<dyn ProfileGameClient>>>,
    reviewer: Arc<dyn DailyGameReviewer>,
    configuration: DailyCoachingConfiguration,
    holder_id: Arc<str>,
    email: DigestEmailRuntime,
    batch_capacity: Arc<Semaphore>,
    operator: OperatorDigestRuntime,
}

impl DailyCoachingLifecycle {
    pub(crate) fn new(
        state_store: Arc<dyn DailyCoachingStore>,
        run_store: Arc<dyn DailyCoachingRunStore>,
        profile_feed: Arc<ProfileGameFeed<Arc<dyn ProfileGameClient>>>,
        reviewer: Arc<dyn DailyGameReviewer>,
        configuration: DailyCoachingConfiguration,
        holder_id: impl Into<Arc<str>>,
    ) -> Self {
        let batch_capacity = Arc::new(Semaphore::new(configuration.operations.concurrent_runs));
        let operator = OperatorDigestRuntime::disabled(
            run_store.clone(),
            configuration.operations.operator_digest_utc_hour,
        );
        Self {
            state_store,
            run_store,
            profile_feed,
            reviewer,
            configuration,
            holder_id: holder_id.into(),
            email: DigestEmailRuntime::disabled(),
            batch_capacity,
            operator,
        }
    }

    pub(crate) fn with_email(mut self, email: DigestEmailRuntime) -> Self {
        self.email = email;
        self
    }

    pub(crate) fn with_operator(mut self, operator: OperatorDigestRuntime) -> Self {
        self.operator = operator;
        self
    }

    pub(crate) fn profile_feed(&self) -> &ProfileGameFeed<Arc<dyn ProfileGameClient>> {
        &self.profile_feed
    }

    pub(crate) fn run_store(&self) -> Arc<dyn DailyCoachingRunStore> {
        self.run_store.clone()
    }

    pub(crate) async fn tick(
        &self,
        now: DateTime<Utc>,
    ) -> Result<DailyCoachingTickReport, DailyCoachingTickError> {
        let mut report = DailyCoachingTickReport::default();
        let tick_started = tokio::time::Instant::now();
        for expired in self.run_store.expired(now).await? {
            let observed_at = elapsed_time(now, tick_started.elapsed())?;
            let address = expired.address();
            let taken = match self
                .run_store
                .take_over(
                    &address,
                    &self.holder_id,
                    observed_at,
                    self.configuration.lease_ttl,
                )
                .await
            {
                Ok(Some(taken)) => taken,
                Ok(None) => continue,
                Err(error) => {
                    report.record_failure(&DailyCoachingTickError::Run(error), "takeover");
                    continue;
                }
            };
            report.taken_over += 1;
            if let Err(error) = self.execute(taken, observed_at, &mut report).await {
                report.record_failure(&error, "resume");
            }
        }

        let states = self.state_store.list().await?;
        let mut operator_projection_ready = true;
        for state in states.iter().cloned() {
            let observed_at = elapsed_time(now, tick_started.elapsed())?;
            if let Err(error) = self
                .redrive_profile_unavailable_email(&state, observed_at)
                .await
            {
                if matches!(&error, DailyCoachingTickError::Operator) {
                    operator_projection_ready = false;
                }
                report.record_failure(&error, "profile_unavailable_email_redrive");
            }
            if let Err(error) = self
                .redrive_digest_email(state.owner_key(), observed_at)
                .await
            {
                report.record_failure(&error, "email_redrive");
            }
            if let Err(error) = self.process_player(state, observed_at, &mut report).await {
                report.record_failure(&error, "claim");
            }
        }
        if !operator_projection_ready {
            return Err(DailyCoachingTickError::Operator);
        }
        self.operator
            .deliver_due(&states, now)
            .await
            .map_err(|_| DailyCoachingTickError::Operator)?;
        Ok(report)
    }

    pub(crate) async fn promote(
        &self,
        player_id: &PlayerId,
        now: DateTime<Utc>,
    ) -> Result<bool, DailyCoachingTickError> {
        if !self.configuration.operations.run_claims_enabled {
            return Ok(false);
        }
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        let state = self.state_store.bind_player(&owner_key, player_id).await?;
        if !self.has_due_window(&state, now)? {
            return Ok(false);
        }
        let admission = self
            .state_store
            .accept_nudge(&owner_key, now, self.configuration.nudge_interval)
            .await?;
        if !admission.accepted {
            return Ok(false);
        }
        let state = admission.state;
        let lifecycle = self.clone();
        tokio::spawn(async move {
            let mut report = DailyCoachingTickReport::default();
            if let Err(error) = lifecycle.process_player(state, now, &mut report).await {
                tracing::error!(
                    category = error.diagnostic_category(),
                    "Daily Coaching arrival promotion failed"
                );
            }
        });
        Ok(true)
    }

    /// Rebuilds the Player's latest terminal Daily Window end to end — selection, Game Review,
    /// publication, digest email — without advancing the ordinary schedule (ADR 0048).
    ///
    /// Returns `false` when nothing is available to rebuild: no terminal window, Daily Coaching
    /// disabled, the Run-claims kill switch off, or a rebuild of that window already in flight.
    /// `true` means the rebuild was admitted, not that it has published.
    pub(crate) async fn force_regenerate_last_digest(
        &self,
        player_id: &PlayerId,
        now: DateTime<Utc>,
    ) -> Result<bool, DailyCoachingTickError> {
        if !self.configuration.operations.run_claims_enabled {
            return Ok(false);
        }
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        let state = self.state_store.bind_player(&owner_key, player_id).await?;
        if !state.is_enabled() {
            return Ok(false);
        }
        let Some(terminal) = self.run_store.latest_visible(&owner_key).await? else {
            return Ok(false);
        };
        let deadline = now
            .checked_add_signed(
                TimeDelta::from_std(self.configuration.claim_horizon)
                    .map_err(|_| DailyCoachingTickError::InvalidState)?,
            )
            .ok_or(DailyCoachingTickError::InvalidState)?;
        // The window's own terminal state is the admission control: reopening moves it to Active,
        // so a second request finds it no longer terminal and is refused.
        let reopened = match self
            .run_store
            .reopen_for_regeneration(
                &terminal.address(),
                &self.holder_id,
                now,
                self.configuration.lease_ttl,
                deadline,
            )
            .await
        {
            Ok(reopened) => reopened,
            Err(DailyCoachingRunStoreError::Fenced) => return Ok(false),
            Err(error) => return Err(DailyCoachingTickError::Run(error)),
        };
        let lifecycle = self.clone();
        tokio::spawn(async move {
            let mut report = DailyCoachingTickReport::default();
            if let Err(error) = lifecycle.execute(reopened, now, &mut report).await {
                tracing::error!(
                    category = error.diagnostic_category(),
                    "Forced Digest Regeneration failed"
                );
            }
        });
        Ok(true)
    }

    pub(crate) fn spawn_scheduler(&self) {
        let lifecycle = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(lifecycle.configuration.tick_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if let Err(error) = lifecycle.tick(Utc::now()).await {
                    tracing::error!(
                        category = error.diagnostic_category(),
                        "Daily Coaching Tick failed"
                    );
                }
            }
        });
    }

    pub(crate) async fn archive(
        &self,
        owner_key: &DailyCoachingOwnerKey,
    ) -> Result<Vec<CoachingDigest>, DailyCoachingRunStoreError> {
        self.run_store.archive(owner_key).await
    }

    pub(crate) async fn dashboard_snapshot(
        &self,
        owner_key: &DailyCoachingOwnerKey,
    ) -> Result<(Option<DailyCoachingRunDocument>, Vec<CoachingDigest>), DailyCoachingRunStoreError>
    {
        // Publication commits the visible run marker and digest atomically. Reading the marker
        // first guarantees the following archive read cannot omit a digest already advertised
        // by that marker, while a publication between the reads is included by the archive.
        let latest_visible = self.run_store.latest_visible(owner_key).await?;
        let archive = self.run_store.archive(owner_key).await?;
        Ok((latest_visible, archive))
    }

    pub(crate) async fn read_digest(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        digest_id: &str,
    ) -> Result<Option<(CoachingDigest, Vec<DigestedGameCard>)>, DailyCoachingRunStoreError> {
        self.run_store.read_digest(owner_key, digest_id).await
    }

    pub(crate) async fn start_manual_digest_run(
        &self,
        player_id: &PlayerId,
        now: DateTime<Utc>,
    ) -> Result<bool, DailyCoachingTickError> {
        let owner_key = DailyCoachingOwnerKey::for_player(player_id);
        if !self
            .email
            .can_receive(&owner_key)
            .await
            .map_err(|_| DailyCoachingTickError::Email)?
            || !self.run_store.archive(&owner_key).await?.is_empty()
        {
            return Ok(false);
        }
        self.promote(player_id, now).await
    }

    async fn redrive_digest_email(
        &self,
        owner_key: &DailyCoachingOwnerKey,
        now: DateTime<Utc>,
    ) -> Result<(), DailyCoachingTickError> {
        if !self.email.is_available() {
            return Ok(());
        }
        for archived in self.run_store.archive(owner_key).await? {
            if !archived.email_delivery_eligible {
                continue;
            }
            let Some(delivery_lease) = self
                .email
                .begin_digest_delivery(owner_key, &archived.delivery_id(), now)
                .await
                .map_err(|_| DailyCoachingTickError::Email)?
            else {
                continue;
            };
            let (digest, cards) = self
                .run_store
                .read_digest(owner_key, &archived.digest_id)
                .await?
                .ok_or(DailyCoachingTickError::InvalidState)?;
            self.email
                .deliver_claimed_digest(digest, cards, delivery_lease)
                .await
                .map_err(|_| DailyCoachingTickError::Email)?;
        }
        Ok(())
    }

    async fn process_player(
        &self,
        mut state: DailyCoachingDocument,
        started_at: DateTime<Utc>,
        report: &mut DailyCoachingTickReport,
    ) -> Result<(), DailyCoachingTickError> {
        let processing_started = tokio::time::Instant::now();
        while state.is_enabled() {
            let now = elapsed_time(started_at, processing_started.elapsed())?;
            let Some(coverage_date) = state.next_daily_window() else {
                return Err(DailyCoachingTickError::InvalidState);
            };
            let timezone = state
                .timezone()
                .ok_or(DailyCoachingTickError::InvalidState)?
                .parse::<Tz>()
                .map_err(|_| DailyCoachingTickError::InvalidState)?;
            let window = DailyWindow::resolve(
                state.owner_key(),
                timezone,
                coverage_date,
                &self.configuration,
            )?;
            if now < window.due_at {
                break;
            }
            if !self.configuration.operations.run_claims_enabled {
                break;
            }

            let missed = now >= window.deadline;
            let claimed = if missed {
                self.run_store
                    .create(DailyCoachingRunDocument::skipped(
                        &state,
                        &window,
                        now,
                        self.configuration.run_retention_days,
                    )?)
                    .await?
            } else {
                self.run_store
                    .create(DailyCoachingRunDocument::claimed(
                        &state,
                        &window,
                        &self.holder_id,
                        now,
                        &self.configuration,
                    )?)
                    .await?
            };

            let next = next_date(coverage_date)?;
            let advanced = self
                .state_store
                .advance_daily_window(state.owner_key(), coverage_date, next)
                .await?;
            state = advanced;

            if let DailyCoachingRunClaim::Created(run) = claimed {
                if missed {
                    report.skipped += 1;
                } else {
                    report.claimed += 1;
                    self.execute(*run, now, report).await?;
                }
            }
        }
        Ok(())
    }

    fn has_due_window(
        &self,
        state: &DailyCoachingDocument,
        now: DateTime<Utc>,
    ) -> Result<bool, DailyCoachingTickError> {
        if !state.is_enabled() {
            return Ok(false);
        }
        let timezone = state
            .timezone()
            .ok_or(DailyCoachingTickError::InvalidState)?
            .parse::<Tz>()
            .map_err(|_| DailyCoachingTickError::InvalidState)?;
        let date = state
            .next_daily_window()
            .ok_or(DailyCoachingTickError::InvalidState)?;
        let window = DailyWindow::resolve(state.owner_key(), timezone, date, &self.configuration)?;
        Ok(now >= window.due_at && now < window.deadline)
    }

    async fn execute(
        &self,
        mut run: DailyCoachingRunDocument,
        started_at: DateTime<Utc>,
        report: &mut DailyCoachingTickReport,
    ) -> Result<(), DailyCoachingTickError> {
        let address = run.address();
        let mut lease = run.lease()?.clone();
        if self
            .stop_if_fenced(&address, &mut run, &mut lease, started_at, report)
            .await?
        {
            return Ok(());
        }
        if started_at >= run.deadline() {
            if run.selection().is_some() {
                let boundary = self
                    .terminalize_unfinished(&address, &lease, run, started_at, report)
                    .await?;
                if matches!(boundary, TerminalizationBoundary::Fenced) {
                    return Ok(());
                }
                return self
                    .finish_reviewed_run_at(&address, &lease, started_at, report)
                    .await;
            }
            self.complete_and_record(
                &address,
                &lease,
                DailyCoachingRunOutcome::Abandoned,
                started_at,
                report,
            )
            .await?;
            return Ok(());
        }

        let execution_start = tokio::time::Instant::now();
        let deadline_after = run
            .deadline()
            .signed_duration_since(started_at)
            .to_std()
            .map_err(|_| DailyCoachingTickError::InvalidState)?;
        let deadline = execution_start + deadline_after;

        if run.selection().is_none() {
            // A reopened window rebuilds its own digest, so its Games stay selectable; every
            // other window's Games remain excluded by their cards.
            let rebuilding_digest =
                (run.regeneration_count() > 0).then_some(address.run_id.as_str());
            let SelectionInputs {
                backfill,
                candidates,
                degraded_providers,
                transient_feed_error,
            } = match self
                .resolve_selection_inputs(
                    &address,
                    &mut lease,
                    &run,
                    started_at,
                    execution_start,
                    deadline,
                )
                .await?
            {
                WorkBoundary::Completed(inputs) => inputs,
                WorkBoundary::Deadline(now) => {
                    self.complete_and_record(
                        &address,
                        &lease,
                        DailyCoachingRunOutcome::Abandoned,
                        now,
                        report,
                    )
                    .await?;
                    return Ok(());
                }
                WorkBoundary::Fenced => {
                    report.fenced += 1;
                    return Ok(());
                }
            };
            if let Some(error) = transient_feed_error {
                if candidates.is_empty() && backfill.is_empty() {
                    return Err(DailyCoachingTickError::Feed(error));
                }
                tracing::warn!(
                    category = "daily_coaching_profile_feed",
                    %error,
                    "Daily Coaching published a partial provider window"
                );
                // A warn alone let a provider vanish from the digest unnoticed; tell the operator.
                let observed_at = elapsed_time(started_at, execution_start.elapsed())?;
                let player_id = run.player_id()?.clone();
                for (provider, reason) in &degraded_providers {
                    self.operator
                        .record_degraded_provider(
                            &player_id,
                            &address.owner_key,
                            *provider,
                            &address.run_id,
                            *reason,
                            observed_at,
                        )
                        .await
                        .map_err(|_| DailyCoachingTickError::Operator)?;
                }
            }
            let identities = candidates
                .iter()
                .chain(&backfill)
                .map(|candidate| candidate.source_identity.clone())
                .collect::<Vec<_>>();
            let digested = match self
                .await_work(
                    &address,
                    &mut lease,
                    started_at,
                    execution_start,
                    deadline,
                    async {
                        self.run_store
                            .digested_sources(&address.owner_key, &identities, rebuilding_digest)
                            .await
                            .map_err(DailyCoachingTickError::Run)
                    },
                )
                .await?
            {
                WorkBoundary::Completed(digested) => digested,
                WorkBoundary::Deadline(now) => {
                    self.complete_and_record(
                        &address,
                        &lease,
                        DailyCoachingRunOutcome::Abandoned,
                        now,
                        report,
                    )
                    .await?;
                    return Ok(());
                }
                WorkBoundary::Fenced => {
                    report.fenced += 1;
                    return Ok(());
                }
            };
            if !backfill.is_empty() && !digested.is_empty() {
                match self
                    .reconcile_initial_backfill_for_run(
                        &address,
                        &mut lease,
                        &run,
                        &digested,
                        started_at,
                        execution_start,
                        deadline,
                    )
                    .await?
                {
                    WorkBoundary::Completed(()) => {}
                    WorkBoundary::Deadline(now) => {
                        self.complete_and_record(
                            &address,
                            &lease,
                            DailyCoachingRunOutcome::Abandoned,
                            now,
                            report,
                        )
                        .await?;
                        return Ok(());
                    }
                    WorkBoundary::Fenced => {
                        report.fenced += 1;
                        return Ok(());
                    }
                }
            }
            let selected = select_daily_and_backfill_games(candidates, backfill, &digested)
                .map_err(|_| DailyCoachingTickError::InvalidState)?;
            let selected_at = elapsed_time(started_at, execution_start.elapsed())?;
            if selected.is_empty() {
                self.complete_and_record(
                    &address,
                    &lease,
                    DailyCoachingRunOutcome::NoDigest,
                    selected_at,
                    report,
                )
                .await?;
                return Ok(());
            }
            run = self
                .run_store
                .freeze_selection(
                    &address,
                    &lease,
                    selected,
                    selected_at,
                    self.configuration.run_retention_days,
                )
                .await?;
            if run.outcome() == Some(DailyCoachingRunOutcome::Fenced) {
                report.fenced += 1;
                return Ok(());
            }
            lease = run.lease()?.clone();
        }

        loop {
            let Some((index, game)) = run.next_pending_game() else {
                return self
                    .finish_reviewed_run(&address, &lease, started_at, execution_start, report)
                    .await;
            };
            let selected = game.selected.clone();
            let attempts = game.attempts();
            if attempts >= self.configuration.game_max_attempts {
                let now = elapsed_time(started_at, execution_start.elapsed())?;
                run = self
                    .run_store
                    .record_game(
                        &address,
                        &lease,
                        index,
                        DailyCoachingGameResult::RetryExhausted { attempted: false },
                        now,
                        None,
                        self.configuration.run_retention_days,
                    )
                    .await?;
                report.retry_exhausted += 1;
                continue;
            }
            let player_id = run.player_id()?.clone();
            let review_result = match self
                .await_work(
                    &address,
                    &mut lease,
                    started_at,
                    execution_start,
                    deadline,
                    async {
                        let _capacity = self
                            .batch_capacity
                            .acquire()
                            .await
                            .map_err(|_| DailyCoachingTickError::InvalidState)?;
                        Ok(self
                            .reviewer
                            .review(&player_id, &selected.review_request)
                            .await)
                    },
                )
                .await?
            {
                WorkBoundary::Completed(result) => result,
                WorkBoundary::Deadline(now) => {
                    let boundary = self
                        .terminalize_unfinished(&address, &lease, run, now, report)
                        .await?;
                    if matches!(boundary, TerminalizationBoundary::Fenced) {
                        return Ok(());
                    }
                    return self
                        .finish_reviewed_run_at(&address, &lease, now, report)
                        .await;
                }
                WorkBoundary::Fenced => {
                    report.fenced += 1;
                    return Ok(());
                }
            };
            let now = elapsed_time(started_at, execution_start.elapsed())?;
            let (result, retry_at) = match review_result {
                DailyGameReviewResult::Reviewed {
                    game_import_id,
                    imported_game,
                    review,
                } => match FrozenDailyGameReview::capture(
                    &selected,
                    game_import_id,
                    &imported_game,
                    &review,
                ) {
                    Ok(review) => (DailyCoachingGameResult::Reviewed(review), None),
                    Err(_) => {
                        report.permanent_game_failures += 1;
                        (DailyCoachingGameResult::Terminal, None)
                    }
                },
                DailyGameReviewResult::Terminal => {
                    report.permanent_game_failures += 1;
                    (DailyCoachingGameResult::Terminal, None)
                }
                DailyGameReviewResult::Retryable {
                    retry_after_seconds: _,
                } if attempts + 1 >= self.configuration.game_max_attempts => {
                    report.retry_exhausted += 1;
                    (
                        DailyCoachingGameResult::RetryExhausted { attempted: true },
                        None,
                    )
                }
                DailyGameReviewResult::Retryable {
                    retry_after_seconds,
                } => {
                    let retry_at = now
                        .checked_add_signed(
                            TimeDelta::from_std(self.retry_delay(
                                &selected,
                                attempts + 1,
                                retry_after_seconds,
                            ))
                            .map_err(|_| DailyCoachingTickError::InvalidState)?,
                        )
                        .ok_or(DailyCoachingTickError::InvalidState)?;
                    (
                        DailyCoachingGameResult::Retryable,
                        Some(retry_at.min(run.deadline())),
                    )
                }
            };
            let retrying = matches!(result, DailyCoachingGameResult::Retryable);
            run = self
                .run_store
                .record_game(
                    &address,
                    &lease,
                    index,
                    result,
                    now,
                    retry_at,
                    self.configuration.run_retention_days,
                )
                .await?;
            if run.outcome() == Some(DailyCoachingRunOutcome::Fenced) {
                report.fenced += 1;
                return Ok(());
            }
            if retrying {
                report.retry_deferred += 1;
                return Ok(());
            }
            lease = run.lease()?.clone();
        }
    }

    async fn terminalize_unfinished(
        &self,
        address: &DailyCoachingRunAddress,
        lease: &DailyCoachingRunLease,
        mut run: DailyCoachingRunDocument,
        now: DateTime<Utc>,
        report: &mut DailyCoachingTickReport,
    ) -> Result<TerminalizationBoundary, DailyCoachingTickError> {
        while let Some((index, _)) = run.next_pending_game() {
            run = self
                .run_store
                .record_game(
                    address,
                    lease,
                    index,
                    DailyCoachingGameResult::UnfinishedAtDeadline,
                    now,
                    None,
                    self.configuration.run_retention_days,
                )
                .await?;
            if run.outcome() == Some(DailyCoachingRunOutcome::Fenced) {
                report.fenced += 1;
                return Ok(TerminalizationBoundary::Fenced);
            }
        }
        Ok(TerminalizationBoundary::Ready)
    }

    async fn finish_reviewed_run(
        &self,
        address: &DailyCoachingRunAddress,
        lease: &DailyCoachingRunLease,
        started_at: DateTime<Utc>,
        execution_start: tokio::time::Instant,
        report: &mut DailyCoachingTickReport,
    ) -> Result<(), DailyCoachingTickError> {
        self.finish_reviewed_run_at(
            address,
            lease,
            elapsed_time(started_at, execution_start.elapsed())?,
            report,
        )
        .await
    }

    async fn finish_reviewed_run_at(
        &self,
        address: &DailyCoachingRunAddress,
        lease: &DailyCoachingRunLease,
        now: DateTime<Utc>,
        report: &mut DailyCoachingTickReport,
    ) -> Result<(), DailyCoachingTickError> {
        let published = self
            .run_store
            .publish(
                address,
                lease,
                now,
                self.configuration.run_retention_days,
                self.email.is_available(),
            )
            .await?;
        match published.outcome() {
            Some(DailyCoachingRunOutcome::Published) => {
                report.published += 1;
                let (digest, cards) = self
                    .run_store
                    .read_digest(&address.owner_key, &address.run_id)
                    .await?
                    .ok_or(DailyCoachingTickError::InvalidState)?;
                if digest.email_delivery_eligible {
                    let delivery_lease = self
                        .email
                        .begin_digest_delivery(&digest.owner_key, &digest.delivery_id(), now)
                        .await
                        .map_err(|_| DailyCoachingTickError::Email)?;
                    if let Some(delivery_lease) = delivery_lease {
                        self.email
                            .deliver_claimed_digest(digest, cards, delivery_lease)
                            .await
                            .map_err(|_| DailyCoachingTickError::Email)?;
                    }
                }
            }
            Some(DailyCoachingRunOutcome::NoDigest) => {
                report.no_digest += 1;
            }
            Some(DailyCoachingRunOutcome::Fenced) => {
                report.fenced += 1;
            }
            _ => return Err(DailyCoachingTickError::InvalidState),
        }
        Ok(())
    }

    fn retry_delay(
        &self,
        selected: &ProfileGameWindowEntry,
        attempt: u8,
        retry_after_seconds: Option<u32>,
    ) -> Duration {
        let exponent = u32::from(attempt.saturating_sub(1)).min(31);
        let base = self
            .configuration
            .game_retry_initial
            .saturating_mul(1_u32 << exponent)
            .min(self.configuration.game_retry_max);
        let digest = Sha256::digest(
            format!("{}:{attempt}", selected.source_identity.canonical_key()).as_bytes(),
        );
        let jitter_percent = 75_u32 + u32::from(digest[0]) % 51;
        let jittered = base.saturating_mul(jitter_percent) / 100;
        retry_after_seconds
            .map(|seconds| Duration::from_secs(u64::from(seconds)))
            .map_or(jittered, |provider| provider.max(jittered))
    }

    async fn stop_if_fenced(
        &self,
        address: &super::runs::DailyCoachingRunAddress,
        run: &mut DailyCoachingRunDocument,
        lease: &mut super::runs::DailyCoachingRunLease,
        now: DateTime<Utc>,
        report: &mut DailyCoachingTickReport,
    ) -> Result<bool, DailyCoachingTickError> {
        let checked = self
            .run_store
            .check_fence(address, lease, now, self.configuration.run_retention_days)
            .await?;
        match (checked.status(), checked.outcome()) {
            (DailyCoachingRunStatus::Active, None) => {
                *run = checked;
                *lease = run.lease()?.clone();
                Ok(false)
            }
            (DailyCoachingRunStatus::Completed, Some(DailyCoachingRunOutcome::Fenced)) => {
                report.fenced += 1;
                Ok(true)
            }
            _ => Err(DailyCoachingTickError::InvalidState),
        }
    }

    async fn complete_and_record(
        &self,
        address: &super::runs::DailyCoachingRunAddress,
        lease: &super::runs::DailyCoachingRunLease,
        outcome: DailyCoachingRunOutcome,
        now: DateTime<Utc>,
        report: &mut DailyCoachingTickReport,
    ) -> Result<(), DailyCoachingTickError> {
        let completed = self
            .run_store
            .complete(
                address,
                lease,
                outcome,
                now,
                self.configuration.run_retention_days,
            )
            .await?;
        match completed.outcome() {
            Some(DailyCoachingRunOutcome::NoDigest) => report.no_digest += 1,
            Some(DailyCoachingRunOutcome::Fenced) => report.fenced += 1,
            Some(DailyCoachingRunOutcome::Abandoned) => report.abandoned += 1,
            Some(DailyCoachingRunOutcome::Published | DailyCoachingRunOutcome::Skipped) | None => {
                return Err(DailyCoachingTickError::InvalidState);
            }
        }
        Ok(())
    }
}

enum TerminalizationBoundary {
    Ready,
    Fenced,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
/// Counts the outcomes observed during one Daily Coaching Tick.
pub struct DailyCoachingTickReport {
    /// Newly claimed Runs.
    pub claimed: u64,
    /// Missed windows recorded as `Skipped`.
    pub skipped: u64,
    /// Expired leases taken over by this cell.
    pub taken_over: u64,
    /// Runs completed because the window contained no eligible Games.
    pub no_digest: u64,
    /// Runs handed to the later selection stage.
    pub pending_selection: u64,
    /// Runs stopped after their state fence changed.
    pub fenced: u64,
    /// Runs stopped at their deadline.
    pub abandoned: u64,
    /// Runs that atomically published a Coaching Digest.
    pub published: u64,
    /// Games durably deferred for a bounded retry.
    pub retry_deferred: u64,
    /// Games that reached a permanent terminal-unreviewed state.
    pub permanent_game_failures: u64,
    /// Games that used their complete transient retry budget.
    pub retry_exhausted: u64,
    /// Isolated items that failed while the Tick continued.
    pub failed: u64,
}

impl DailyCoachingTickReport {
    fn record_failure(&mut self, error: &DailyCoachingTickError, phase: &'static str) {
        self.failed += 1;
        tracing::error!(
            category = error.diagnostic_category(),
            phase,
            %error,
            "Daily Coaching Tick item failed"
        );
    }
}

/// Everything one Run needs from its connected providers before selection can run.
struct SelectionInputs {
    backfill: Vec<ProfileGameWindowEntry>,
    candidates: Vec<ProfileGameWindowEntry>,
    /// The first reason each provider dropped out, if any did.
    degraded_providers: BTreeMap<DailyCoachingProvider, DegradedProviderReason>,
    /// Kept so a Run that lost every provider fails instead of publishing nothing.
    transient_feed_error: Option<ProfileGameFeedError>,
}

impl DailyCoachingLifecycle {
    /// Reads the owed initial backfill and the daily window from every connected provider.
    ///
    /// A provider that fails transiently is recorded and skipped rather than failing the Run, so
    /// this is the one place that decides what a lost provider costs. Both passes share it; when
    /// they did not, a change to that policy had to be written twice.
    async fn resolve_selection_inputs(
        &self,
        address: &DailyCoachingRunAddress,
        lease: &mut DailyCoachingRunLease,
        run: &DailyCoachingRunDocument,
        started_at: DateTime<Utc>,
        execution_start: tokio::time::Instant,
        deadline: tokio::time::Instant,
    ) -> Result<WorkBoundary<SelectionInputs>, DailyCoachingTickError> {
        let mut inputs = SelectionInputs {
            backfill: Vec::new(),
            candidates: Vec::new(),
            degraded_providers: BTreeMap::new(),
            transient_feed_error: None,
        };
        for connection in run.connections() {
            let boundary = match self
                .initial_backfill_for_run(
                    address,
                    lease,
                    run,
                    connection,
                    started_at,
                    execution_start,
                    deadline,
                )
                .await
            {
                Ok(boundary) => boundary,
                Err(DailyCoachingTickError::Feed(error)) => {
                    inputs.record_degraded(connection, error);
                    continue;
                }
                Err(error) => return Err(error),
            };
            match boundary {
                WorkBoundary::Completed(games) => inputs.backfill.extend(games),
                WorkBoundary::Deadline(now) => return Ok(WorkBoundary::Deadline(now)),
                WorkBoundary::Fenced => return Ok(WorkBoundary::Fenced),
            }
        }
        for connection in run.connections() {
            let boundary = match self
                .await_work(
                    address,
                    lease,
                    started_at,
                    execution_start,
                    deadline,
                    async {
                        let result = self
                            .profile_feed
                            .eligible_games_in_window(
                                connection.canonical_url(),
                                run.starts_at(),
                                run.ends_at(),
                            )
                            .await;
                        self.observe_profile_feed(
                            &address.owner_key,
                            connection,
                            result,
                            started_at,
                        )
                        .await
                    },
                )
                .await
            {
                Ok(boundary) => boundary,
                Err(DailyCoachingTickError::Feed(error)) => {
                    inputs.record_degraded(connection, error);
                    continue;
                }
                Err(error) => return Err(error),
            };
            match boundary {
                WorkBoundary::Completed(found) => {
                    inputs.candidates.extend(found.unwrap_or_default());
                }
                WorkBoundary::Deadline(now) => return Ok(WorkBoundary::Deadline(now)),
                WorkBoundary::Fenced => return Ok(WorkBoundary::Fenced),
            }
        }
        Ok(WorkBoundary::Completed(inputs))
    }
}

impl SelectionInputs {
    fn record_degraded(
        &mut self,
        connection: &DailyCoachingRunConnection,
        error: ProfileGameFeedError,
    ) {
        self.degraded_providers
            .entry(connection.provider())
            .or_insert_with(|| degraded_provider_reason(&error));
        self.transient_feed_error.get_or_insert(error);
    }
}

/// Collapses a provider feed failure to a label the Operator Digest can carry. The error's own
/// text is not used: `reqwest` renders the requested URL, which contains the Player's provider
/// handle, and an unexpected content type echoes a provider-controlled header.
fn degraded_provider_reason(error: &ProfileGameFeedError) -> DegradedProviderReason {
    use crate::profile_game_feed::ProfileGameFetchError;
    match error {
        ProfileGameFeedError::InvalidProfileUrl(_) => DegradedProviderReason::InvalidProfileUrl,
        ProfileGameFeedError::Fetch(fetch) => match fetch {
            ProfileGameFetchError::Client(_) => DegradedProviderReason::ClientMisconfigured,
            ProfileGameFetchError::Connection { .. } => DegradedProviderReason::ProviderUnreachable,
            ProfileGameFetchError::Timeout { .. } => DegradedProviderReason::ProviderTimeout,
            ProfileGameFetchError::Transport { .. } => DegradedProviderReason::ProviderTransport,
            ProfileGameFetchError::Status { .. } => DegradedProviderReason::ProviderStatus,
            ProfileGameFetchError::ResponseTooLarge { .. } => {
                DegradedProviderReason::ResponseTooLarge
            }
        },
        ProfileGameFeedError::UnexpectedContentType { .. } => {
            DegradedProviderReason::UnexpectedContentType
        }
        ProfileGameFeedError::MalformedProviderResponse => {
            DegradedProviderReason::MalformedResponse
        }
        ProfileGameFeedError::InvalidWindow => DegradedProviderReason::InvalidWindow,
    }
}

#[derive(Debug, thiserror::Error)]
/// Failure that prevents a Daily Coaching Tick or promotion from continuing.
pub enum DailyCoachingTickError {
    /// Daily Coaching state could not be read or advanced.
    #[error(transparent)]
    State(#[from] DailyCoachingStoreError),
    /// A Run claim or lease operation failed.
    #[error(transparent)]
    Run(#[from] DailyCoachingRunStoreError),
    /// A published digest's email claim or terminal delivery record failed.
    #[error("Daily Coaching digest email persistence failed")]
    Email,
    /// A Daily Window could not be resolved.
    #[error(transparent)]
    Window(#[from] DailyWindowError),
    /// A provider window could not be read safely.
    #[error(transparent)]
    Feed(#[from] ProfileGameFeedError),
    /// The daily operational heartbeat could not be assembled or handed off.
    #[error("Daily Coaching Operator Digest failed")]
    Operator,
    /// Persisted lifecycle fields violate the Run contract.
    #[error("Daily Coaching lifecycle state is invalid")]
    InvalidState,
}

impl DailyCoachingTickError {
    fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::State(_) => "daily_coaching_state",
            Self::Run(_) => "daily_coaching_run",
            Self::Email => "daily_coaching_email",
            Self::Window(_) | Self::InvalidState => "daily_coaching_window",
            Self::Feed(_) => "daily_coaching_profile_feed",
            Self::Operator => "daily_coaching_operator_digest",
        }
    }
}

#[cfg(test)]
mod tests;
