use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use crate::{
    daily_coaching::{
        runs::{
            DailyCoachingRunAddress, DailyCoachingRunConnection, DailyCoachingRunDocument,
            DailyCoachingRunLease,
        },
        selection::{resolve_initial_backfill, MAX_INITIAL_BACKFILL_GAMES},
        state::{
            InitialBackfillMutation, InitialBackfillSnapshot, InitialBackfillUnavailableReason,
        },
        DailyCoachingProvider, DailyCoachingRunStoreError,
    },
    profile_game_feed::{
        ProfileGameSourceIdentity, ProfileGameWindowEntry, RecentProfileGameCount,
        RecentProfileGameScanPage,
    },
};

use super::{DailyCoachingLifecycle, DailyCoachingTickError, WorkBoundary};

impl DailyCoachingLifecycle {
    #[expect(
        clippy::too_many_arguments,
        reason = "the helper preserves the run's complete heartbeat and deadline boundary"
    )]
    pub(super) async fn initial_backfill_for_run(
        &self,
        address: &DailyCoachingRunAddress,
        lease: &mut DailyCoachingRunLease,
        run: &DailyCoachingRunDocument,
        connection: &DailyCoachingRunConnection,
        started_at: DateTime<Utc>,
        execution_start: tokio::time::Instant,
        deadline: tokio::time::Instant,
    ) -> Result<WorkBoundary<Vec<ProfileGameWindowEntry>>, DailyCoachingTickError> {
        let player_id = run.player_id()?.clone();
        let state = match self
            .await_work(
                address,
                lease,
                started_at,
                execution_start,
                deadline,
                async {
                    self.state_store
                        .bind_player(&address.owner_key, &player_id)
                        .await
                        .map_err(DailyCoachingTickError::State)
                },
            )
            .await?
        {
            WorkBoundary::Completed(state) => state,
            WorkBoundary::Deadline(now) => return Ok(WorkBoundary::Deadline(now)),
            WorkBoundary::Fenced => return Ok(WorkBoundary::Fenced),
        };
        let Some(current_connection) =
            state.connection_for_identity(connection.provider(), connection.identity_username())
        else {
            return Ok(WorkBoundary::Completed(Vec::new()));
        };
        match current_connection.initial_backfill() {
            InitialBackfillSnapshot::Owed(games) => Ok(WorkBoundary::Completed(games)),
            InitialBackfillSnapshot::Completed => Ok(WorkBoundary::Completed(Vec::new())),
            InitialBackfillSnapshot::Pending { games, cursor } => {
                let count = RecentProfileGameCount::try_from(
                    u8::try_from(
                        MAX_INITIAL_BACKFILL_GAMES
                            .saturating_sub(games.len())
                            .max(1),
                    )
                    .map_err(|_| DailyCoachingTickError::InvalidState)?,
                )
                .map_err(|_| DailyCoachingTickError::InvalidState)?;
                let page = match self
                    .await_work(
                        address,
                        lease,
                        started_at,
                        execution_start,
                        deadline,
                        async {
                            let result = self
                                .profile_feed
                                .scan_latest_eligible_games_at(
                                    connection.canonical_url(),
                                    count,
                                    cursor.as_ref(),
                                    started_at,
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
                    .await?
                {
                    WorkBoundary::Completed(Some(page)) => page,
                    WorkBoundary::Completed(None) => {
                        return Ok(WorkBoundary::Completed(Vec::new()));
                    }
                    WorkBoundary::Deadline(now) => return Ok(WorkBoundary::Deadline(now)),
                    WorkBoundary::Fenced => return Ok(WorkBoundary::Fenced),
                };
                let update = initial_backfill_update(games, page)?;
                let mutation_lease = lease.clone();
                let state_result = self
                    .await_work(
                        address,
                        lease,
                        started_at,
                        execution_start,
                        deadline,
                        async {
                            self.run_store
                                .update_initial_backfill(
                                    address,
                                    &mutation_lease,
                                    connection,
                                    update,
                                )
                                .await
                                .map_err(DailyCoachingTickError::Run)
                        },
                    )
                    .await;
                let state = match state_result {
                    Err(DailyCoachingTickError::Run(
                        crate::daily_coaching::DailyCoachingRunStoreError::Fenced,
                    )) => return Ok(WorkBoundary::Fenced),
                    Err(error) => return Err(error),
                    Ok(WorkBoundary::Completed(state)) => state,
                    Ok(WorkBoundary::Deadline(now)) => return Ok(WorkBoundary::Deadline(now)),
                    Ok(WorkBoundary::Fenced) => return Ok(WorkBoundary::Fenced),
                };
                let games = state
                    .connection_for_identity(connection.provider(), connection.identity_username())
                    .and_then(|connection| match connection.initial_backfill() {
                        InitialBackfillSnapshot::Owed(games) => Some(games),
                        InitialBackfillSnapshot::Pending { .. }
                        | InitialBackfillSnapshot::Completed => None,
                    })
                    .unwrap_or_default();
                Ok(WorkBoundary::Completed(games))
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the helper preserves the run's complete heartbeat and deadline boundary"
    )]
    pub(super) async fn reconcile_initial_backfill_for_run(
        &self,
        address: &DailyCoachingRunAddress,
        lease: &mut DailyCoachingRunLease,
        run: &DailyCoachingRunDocument,
        digested: &BTreeSet<ProfileGameSourceIdentity>,
        started_at: DateTime<Utc>,
        execution_start: tokio::time::Instant,
        deadline: tokio::time::Instant,
    ) -> Result<WorkBoundary<()>, DailyCoachingTickError> {
        for connection in run.connections() {
            let reconciled = digested
                .iter()
                .filter(|identity| {
                    DailyCoachingProvider::from(identity.provider) == connection.provider()
                })
                .cloned()
                .collect::<BTreeSet<_>>();
            if reconciled.is_empty() {
                continue;
            }
            let mutation_lease = lease.clone();
            match self
                .await_work(
                    address,
                    lease,
                    started_at,
                    execution_start,
                    deadline,
                    async {
                        self.run_store
                            .update_initial_backfill(
                                address,
                                &mutation_lease,
                                connection,
                                InitialBackfillMutation::Reconcile(reconciled),
                            )
                            .await
                            .map_err(DailyCoachingTickError::Run)
                    },
                )
                .await
            {
                Err(DailyCoachingTickError::Run(DailyCoachingRunStoreError::Fenced)) => {
                    return Ok(WorkBoundary::Fenced);
                }
                Err(error) => return Err(error),
                Ok(WorkBoundary::Completed(_)) => {}
                Ok(WorkBoundary::Deadline(now)) => return Ok(WorkBoundary::Deadline(now)),
                Ok(WorkBoundary::Fenced) => return Ok(WorkBoundary::Fenced),
            }
        }
        Ok(WorkBoundary::Completed(()))
    }
}

pub(super) fn initial_backfill_update(
    mut games: Vec<ProfileGameWindowEntry>,
    page: RecentProfileGameScanPage,
) -> Result<InitialBackfillMutation, DailyCoachingTickError> {
    let mutation = match page {
        RecentProfileGameScanPage::Complete(found) => {
            games.extend(found);
            InitialBackfillMutation::Resolve(
                resolve_initial_backfill(games)
                    .map_err(|_| DailyCoachingTickError::InvalidState)?,
            )
        }
        RecentProfileGameScanPage::Continue {
            games: found,
            cursor,
        } => {
            games.extend(found);
            let games = resolve_initial_backfill(games)
                .map_err(|_| DailyCoachingTickError::InvalidState)?;
            InitialBackfillMutation::Checkpoint { games, cursor }
        }
        RecentProfileGameScanPage::Stalled(found) => {
            games.extend(found);
            let games = resolve_initial_backfill(games)
                .map_err(|_| DailyCoachingTickError::InvalidState)?;
            if games.len() >= MAX_INITIAL_BACKFILL_GAMES {
                InitialBackfillMutation::Resolve(games)
            } else if !games.is_empty() {
                InitialBackfillMutation::ResolveStalled(games)
            } else {
                InitialBackfillMutation::Unavailable(InitialBackfillUnavailableReason::ScanStalled)
            }
        }
    };
    Ok(mutation)
}
