use std::{collections::BTreeSet, future::Future, pin::Pin, time::Duration};

use chrono::{DateTime, Utc};

use crate::profile_game_feed::ProfileGameSourceIdentity;

use super::super::digest::DigestedGameCard;
use super::super::selection::SelectedDailyCoachingGame;
use super::super::{
    digest::CoachingDigest,
    state::{DailyCoachingOwnerKey, InitialBackfillMutation},
    DailyCoachingDocument,
};
use super::{
    DailyCoachingGameResult, DailyCoachingRunAddress, DailyCoachingRunClaim,
    DailyCoachingRunConnection, DailyCoachingRunDocument, DailyCoachingRunLease,
    DailyCoachingRunOutcome, DailyCoachingRunStoreError,
};

pub(crate) type RunStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, DailyCoachingRunStoreError>> + Send + 'a>>;

pub(crate) trait DailyCoachingRunStore: Send + Sync {
    fn list_digested_game_cards<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Vec<DigestedGameCard>>;

    fn create<'a>(
        &'a self,
        run: DailyCoachingRunDocument,
    ) -> RunStoreFuture<'a, DailyCoachingRunClaim>;

    fn expired<'a>(
        &'a self,
        now: DateTime<Utc>,
    ) -> RunStoreFuture<'a, Vec<DailyCoachingRunDocument>>;

    fn finished_between<'a>(
        &'a self,
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> RunStoreFuture<'a, Vec<DailyCoachingRunDocument>>;

    fn check_fence<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument>;

    fn take_over<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        holder_id: &'a str,
        now: DateTime<Utc>,
        lease_ttl: Duration,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>>;

    fn heartbeat<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        now: DateTime<Utc>,
        lease_ttl: Duration,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument>;

    /// Which candidates a Coaching Digest already carries. `rebuilding` names the digest being
    /// regenerated, whose own Games stay selectable; every other digest's Games stay excluded.
    fn digested_sources<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        candidates: &'a [ProfileGameSourceIdentity],
        rebuilding: Option<&'a str>,
    ) -> RunStoreFuture<'a, BTreeSet<ProfileGameSourceIdentity>>;

    fn update_initial_backfill<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        connection: &'a DailyCoachingRunConnection,
        mutation: InitialBackfillMutation,
    ) -> RunStoreFuture<'a, DailyCoachingDocument>;

    fn freeze_selection<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        selection: Vec<SelectedDailyCoachingGame>,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument>;

    #[expect(
        clippy::too_many_arguments,
        reason = "the durable per-Game transition carries its complete fence, result, clock, and retention context"
    )]
    fn record_game<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        index: usize,
        result: DailyCoachingGameResult,
        now: DateTime<Utc>,
        retry_at: Option<DateTime<Utc>>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument>;

    fn publish<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        now: DateTime<Utc>,
        retention_days: u32,
        email_delivery_eligible: bool,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument>;

    #[cfg_attr(not(test), allow(dead_code))]
    fn reopen_for_regeneration<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        holder_id: &'a str,
        now: DateTime<Utc>,
        lease_ttl: Duration,
        deadline: DateTime<Utc>,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument>;

    /// The Player's published Coaching Digests, newest first.
    ///
    /// A digest nobody can read is omitted rather than failing the read. The
    /// archive backs both the dashboard and the digest-email redrive, so one
    /// unreadable document must not take either surface down with it.
    fn archive<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Vec<CoachingDigest>>;

    fn latest_visible<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>>;

    fn complete<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
        lease: &'a DailyCoachingRunLease,
        outcome: DailyCoachingRunOutcome,
        now: DateTime<Utc>,
        retention_days: u32,
    ) -> RunStoreFuture<'a, DailyCoachingRunDocument>;

    #[cfg(test)]
    fn read<'a>(
        &'a self,
        address: &'a DailyCoachingRunAddress,
    ) -> RunStoreFuture<'a, Option<DailyCoachingRunDocument>>;

    fn read_digest<'a>(
        &'a self,
        owner_key: &'a DailyCoachingOwnerKey,
        digest_id: &'a str,
    ) -> RunStoreFuture<'a, Option<(CoachingDigest, Vec<DigestedGameCard>)>>;
}
