//! Deliberate, expiring, revocable sharing of one Game Review address.
//!
//! Every web address below a Game Review is an identifier and never a bearer
//! capability: pasting one into a transcript hands over nothing, because the
//! route resolves behind sign-in and the Player subtree is the authorization
//! boundary. Showing a Review Moment to somebody else therefore has to be an
//! action the Player takes, not a side effect of the address existing.
//!
//! A **Review Share Grant** is that action. It reads:
//!
//! ```text
//! users/{ownerSegment}/reviewShares/{shareId}
//! ```
//!
//! and the Player is handed `review-share:{ownerSegment}:{secret}`, where
//! `shareId` is `sha256(secret)`. Three properties fall out of that layout:
//!
//! - **Durability holds no bearer material.** The document is named by the
//!   digest of the secret, so a reader of the store — a backup, an operator, a
//!   Firestore export — cannot mint the link. The `shareId` the owner revokes
//!   by is that same digest and is safe to show them. The secret itself rides
//!   in a URL path, so it does reach browser history and any origin or edge
//!   access log; that is what makes it short-lived and withdrawable rather than
//!   permanent.
//! - **Deletion revokes.** The grant lives inside the subtree account deletion
//!   removes recursively, so a deleted Player's outstanding links fail closed
//!   without anything remembering this store exists.
//! - **Resolution needs no identity.** The token names the subtree, which is
//!   what lets a recipient with no ChenChess account resolve it at all. The
//!   owner segment is an opaque digest and confers nothing on its own.
//!
//! A grant is read-only and scoped to one Game Import. The Review Moment card
//! renders from the review snapshot, so the snapshot is inside that scope: a
//! share discloses the review, opened at the shared address. There is
//! deliberately no second redacted projection, because a second projection is a
//! second thing to keep agreeing with the first.

use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

use chrono::{DateTime, TimeDelta, Utc};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    account_deletion::{application_data_document_path_for_owner, player_subtree_owner},
    firestore::{codec::DurablePayload, FirestoreDatabase, FirestoreError},
    review_durability::{
        game_import_belongs_to, is_lower_hex,
        path::{hashed_path_segment, lower_hex},
    },
    review_session_contract::{
        CriticalMomentId, GameImportId, MoveSequencePresentationKind, PlayerId,
    },
    review_session_processor::ProcessorPrincipal,
};

pub(crate) const REVIEW_SHARES_COLLECTION: &str = "reviewShares";

/// How long a minted link stays resolvable.
///
/// Short enough that a link which escapes its conversation stops working on its
/// own, long enough that the person it was sent to can open it after they get
/// home. Expiry is enforced when the token is resolved, so shortening it takes
/// effect on grants already outstanding.
pub const REVIEW_SHARE_LIFETIME_HOURS: i64 = 24;

const SECRET_BYTES: usize = 32;
const HEX_DIGEST_LENGTH: usize = 64;
const TOKEN_PREFIX: &str = "review-share";

/// Where a shared link opens.
///
/// Only the two addresses that are pure snapshot renders are shareable. The
/// canonical Game Review route is the interactive workspace — a chat panel,
/// board interaction, coaching commands — and has no read-only equivalent to
/// hand somebody, so a grant always names a Review Moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewShareAddress {
    pub game_import_id: GameImportId,
    pub review_moment_id: CriticalMomentId,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sequence_kind: Option<MoveSequencePresentationKind>,
}

/// One Player's outstanding grant, as both sides of the link see it.
///
/// The owner is recorded because resolution has to run the reads as them: the
/// token proves the grant, and the grant names whose review it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewShareGrant {
    pub share_id: String,
    pub owner: PlayerId,
    pub address: ReviewShareAddress,
    pub expires_at: DateTime<Utc>,
}

impl ReviewShareGrant {
    pub fn principal(&self) -> ProcessorPrincipal {
        ProcessorPrincipal::Player(self.owner.clone())
    }
}

/// A freshly minted grant, with the one copy of its secret that ever exists.
pub struct MintedReviewShare {
    pub grant: ReviewShareGrant,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReviewShareError {
    #[error("Review Share Grant persistence is misconfigured: {0}")]
    Configuration(String),
    #[error("Review Share Grant persistence is unavailable")]
    Unavailable,
    #[error("a stored Review Share Grant is unreadable")]
    InvalidRecord,
    #[error("that Game Review belongs to another Player")]
    NotOwned,
    #[error("that share link is not valid")]
    InvalidToken,
    #[error("that Review Moment is not part of that Game Review")]
    UnknownAddress,
    #[error("that share link is being read too quickly")]
    TooManyReads,
    #[error("that share link is no longer available")]
    NotFound,
    #[error("that share link has expired")]
    Expired,
}

impl From<FirestoreError> for ReviewShareError {
    fn from(error: FirestoreError) -> Self {
        match error {
            FirestoreError::Configuration(message) => Self::Configuration(message),
            FirestoreError::InvalidDocument => Self::InvalidRecord,
            FirestoreError::Transport | FirestoreError::Unavailable | FirestoreError::Conflict => {
                Self::Unavailable
            }
        }
    }
}

pub type ReviewShareStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ReviewShareError>> + Send + 'a>>;

/// The durable home of outstanding Review Share Grants.
///
/// Everything is addressed by owner segment rather than by Player ID, because
/// resolution starts from a token and never from an identity. Revocation is a
/// delete: a revoked link and a link that never existed answer the same way,
/// and there is no tombstone to expire separately from the grant.
pub trait ReviewShareStore: Send + Sync {
    fn put<'a>(
        &'a self,
        owner_segment: &'a str,
        grant: ReviewShareGrant,
    ) -> ReviewShareStoreFuture<'a, ()>;

    fn get<'a>(
        &'a self,
        owner_segment: &'a str,
        share_id: &'a str,
    ) -> ReviewShareStoreFuture<'a, Option<ReviewShareGrant>>;

    fn delete<'a>(
        &'a self,
        owner_segment: &'a str,
        share_id: &'a str,
    ) -> ReviewShareStoreFuture<'a, ()>;

    /// Every grant one Player currently has outstanding.
    ///
    /// Withdrawal has to survive the browser tab that minted the link, so the
    /// owner's own grants are readable back by identity even though nothing
    /// else about a grant is. Without this, "revocable" would mean "revocable
    /// until you navigate away".
    fn list<'a>(
        &'a self,
        owner_segment: &'a str,
    ) -> ReviewShareStoreFuture<'a, Vec<ReviewShareGrant>>;
}

/// Mints, resolves, and revokes Review Share Grants.
///
/// Expiry and revocation are decided here rather than by any caller, so the
/// unauthenticated resolve path cannot be the one that forgets to check.
pub struct ReviewShareRuntime {
    store: Arc<dyn ReviewShareStore>,
    random: SystemRandom,
    lifetime: TimeDelta,
    allowance: SharedReadAllowance,
    windows: Mutex<BTreeMap<String, SharedReadWindow>>,
}

impl ReviewShareRuntime {
    pub fn new(store: Arc<dyn ReviewShareStore>) -> Self {
        Self {
            store,
            random: SystemRandom::new(),
            lifetime: TimeDelta::hours(REVIEW_SHARE_LIFETIME_HOURS),
            allowance: SharedReadAllowance::default(),
            windows: Mutex::new(BTreeMap::new()),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_allowance(mut self, allowance: SharedReadAllowance) -> Self {
        self.allowance = allowance;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_lifetime(mut self, lifetime: TimeDelta) -> Self {
        self.lifetime = lifetime;
        self
    }

    /// Mints one grant over an address the caller owns.
    ///
    /// Ownership is decided from the Game Import ID's own owner segment, so a
    /// Player cannot mint a link to a review that is not theirs even if they
    /// learn its address.
    pub async fn mint(
        &self,
        owner: &PlayerId,
        address: ReviewShareAddress,
        now: DateTime<Utc>,
    ) -> Result<MintedReviewShare, ReviewShareError> {
        if !game_import_belongs_to(
            &address.game_import_id,
            &ProcessorPrincipal::Player(owner.clone()),
        ) {
            return Err(ReviewShareError::NotOwned);
        }
        let mut secret_bytes = [0u8; SECRET_BYTES];
        self.random
            .fill(&mut secret_bytes)
            .map_err(|_| ReviewShareError::Unavailable)?;
        let secret = lower_hex(&secret_bytes);
        let owner_segment = player_subtree_owner(owner);
        let grant = ReviewShareGrant {
            share_id: share_id(&secret),
            owner: owner.clone(),
            address,
            expires_at: now + self.lifetime,
        };
        self.store.put(&owner_segment, grant.clone()).await?;
        Ok(MintedReviewShare {
            token: format!("{TOKEN_PREFIX}:{owner_segment}:{secret}"),
            grant,
        })
    }

    /// Answers one token, or says exactly why it will not.
    ///
    /// Expiry is distinguished from absence because the caller already holds
    /// the token: telling them it ran out is a better page than telling them it
    /// never existed, and it discloses nothing they did not have.
    pub async fn resolve(
        &self,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<ReviewShareGrant, ReviewShareError> {
        let (owner_segment, secret) = parse_token(token).ok_or(ReviewShareError::InvalidToken)?;
        let share_id = share_id(secret);
        let grant = self
            .store
            .get(owner_segment, &share_id)
            .await?
            .ok_or(ReviewShareError::NotFound)?;
        if grant.expires_at <= now {
            return Err(ReviewShareError::Expired);
        }
        self.admit_read(share_id, now).await?;
        Ok(grant)
    }

    /// Counts one unauthenticated request against its own grant's allowance.
    ///
    /// Metered after the grant is known and before any engine work is spent, so
    /// a caller without a valid token is refused by resolution rather than by
    /// this, and a caller with one cannot turn a single link into unbounded
    /// analysis load.
    async fn admit_read(
        &self,
        share_id: String,
        now: DateTime<Utc>,
    ) -> Result<(), ReviewShareError> {
        let mut windows = self.windows.lock().await;
        // Windows for links that have gone quiet are dropped rather than aged:
        // a grant lives a day at most, so the map cannot grow past the shares
        // one process has actually served.
        windows.retain(|_, window| {
            window
                .started_at
                .is_some_and(|started| now - started < self.allowance.window)
        });
        let window = windows.entry(share_id).or_default();
        if window.started_at.is_none() {
            window.started_at = Some(now);
        }
        window.reads += 1;
        if window.reads > self.allowance.reads {
            return Err(ReviewShareError::TooManyReads);
        }
        Ok(())
    }

    /// The grants a Player still has outstanding, newest first.
    ///
    /// Expired grants are dropped rather than listed: they are already refused
    /// at resolve, and offering the owner a link that no longer works would
    /// invite them to withdraw something that is already gone.
    pub async fn outstanding(
        &self,
        owner: &PlayerId,
        now: DateTime<Utc>,
    ) -> Result<Vec<ReviewShareGrant>, ReviewShareError> {
        let mut grants = self.store.list(&player_subtree_owner(owner)).await?;
        grants.retain(|grant| grant.expires_at > now);
        grants.sort_by(|left, right| {
            right
                .expires_at
                .cmp(&left.expires_at)
                .then_with(|| left.share_id.cmp(&right.share_id))
        });
        Ok(grants)
    }

    /// Withdraws one grant. Revoking a link that is already gone succeeds.
    pub async fn revoke(&self, owner: &PlayerId, share_id: &str) -> Result<(), ReviewShareError> {
        if !is_lower_hex_digest(share_id) {
            return Err(ReviewShareError::InvalidToken);
        }
        self.store
            .delete(&player_subtree_owner(owner), share_id)
            .await
    }
}

/// Withdraws every outstanding grant that opens into one of these reviews.
///
/// A deleted Game is re-importable at the same address, so a link left standing
/// would start resolving again against a review the Player deleted. Revoking is
/// a delete, so revoking a grant that is already gone succeeds.
pub(crate) async fn revoke_grants_for_reviews(
    store: &dyn ReviewShareStore,
    owner: &PlayerId,
    game_import_ids: &std::collections::BTreeSet<GameImportId>,
) -> Result<(), ReviewShareError> {
    let owner_segment = player_subtree_owner(owner);
    for grant in store.list(&owner_segment).await? {
        if game_import_ids.contains(&grant.address.game_import_id) {
            store.delete(&owner_segment, &grant.share_id).await?;
        }
    }
    Ok(())
}

/// What one share link is allowed to ask for, and how fast.
///
/// Resolving and reading are the only unauthenticated endpoints the Coach
/// Engine has, and a read spends real engine work. The token is the whole
/// authorization, so the grant it names is the only thing worth metering: an
/// attacker without a token cannot get past `resolve` to spend anything, and a
/// recipient with one needs a handful of reads to open a page, not hundreds.
/// The window is per process, like every other transient rule here.
#[derive(Debug, Clone, Copy)]
pub struct SharedReadAllowance {
    pub reads: u32,
    pub window: TimeDelta,
}

impl Default for SharedReadAllowance {
    fn default() -> Self {
        Self {
            reads: 60,
            window: TimeDelta::minutes(1),
        }
    }
}

#[derive(Default)]
struct SharedReadWindow {
    started_at: Option<DateTime<Utc>>,
    reads: u32,
}

/// The token, split into the subtree it names and the secret it proves.
fn parse_token(token: &str) -> Option<(&str, &str)> {
    let mut parts = token.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(TOKEN_PREFIX), Some(owner_segment), Some(secret), None)
            if is_lower_hex_digest(owner_segment) && is_lower_hex_digest(secret) =>
        {
            Some((owner_segment, secret))
        }
        _ => None,
    }
}

/// The public name of a grant: the digest of its secret.
fn share_id(secret: &str) -> String {
    hashed_path_segment(secret)
}

fn is_lower_hex_digest(value: &str) -> bool {
    is_lower_hex(value, HEX_DIGEST_LENGTH)
}

/// The store a runtime without configured durability answers from.
///
/// Local and self-hosted runtimes keep grants in process, which is the same
/// bound their Review Sessions already have.
#[derive(Default)]
pub struct InMemoryReviewShareStore {
    grants: Mutex<BTreeMap<(String, String), ReviewShareGrant>>,
}

impl ReviewShareStore for InMemoryReviewShareStore {
    fn put<'a>(
        &'a self,
        owner_segment: &'a str,
        grant: ReviewShareGrant,
    ) -> ReviewShareStoreFuture<'a, ()> {
        Box::pin(async move {
            self.grants
                .lock()
                .await
                .insert((owner_segment.to_string(), grant.share_id.clone()), grant);
            Ok(())
        })
    }

    fn get<'a>(
        &'a self,
        owner_segment: &'a str,
        share_id: &'a str,
    ) -> ReviewShareStoreFuture<'a, Option<ReviewShareGrant>> {
        Box::pin(async move {
            Ok(self
                .grants
                .lock()
                .await
                .get(&(owner_segment.to_string(), share_id.to_string()))
                .cloned())
        })
    }

    fn delete<'a>(
        &'a self,
        owner_segment: &'a str,
        share_id: &'a str,
    ) -> ReviewShareStoreFuture<'a, ()> {
        Box::pin(async move {
            self.grants
                .lock()
                .await
                .remove(&(owner_segment.to_string(), share_id.to_string()));
            Ok(())
        })
    }

    fn list<'a>(
        &'a self,
        owner_segment: &'a str,
    ) -> ReviewShareStoreFuture<'a, Vec<ReviewShareGrant>> {
        Box::pin(async move {
            Ok(self
                .grants
                .lock()
                .await
                .iter()
                .filter(|((owner, _), _)| owner == owner_segment)
                .map(|(_, grant)| grant.clone())
                .collect())
        })
    }
}

/// The durable store a deployed Coach Engine hands its web binding.
///
/// Grants are Player-owned durable records like published comments, so they use
/// the same application database and the same subtree that account deletion
/// removes; a runtime without one keeps its grants in process.
pub fn configured_review_share_store() -> anyhow::Result<Arc<dyn ReviewShareStore>> {
    Ok(review_share_store(FirestoreDatabase::from_env()?))
}

pub(crate) fn review_share_store(database: FirestoreDatabase) -> Arc<dyn ReviewShareStore> {
    Arc::new(FirestoreReviewShareStore { database })
}

struct FirestoreReviewShareStore {
    database: FirestoreDatabase,
}

/// The stored form of one grant.
///
/// `expiresAt` is promoted to a Firestore timestamp because it is the field a
/// sweep would query on; the rest rides in the queryless payload. Nothing here
/// is the secret — the document is named by its digest, and the digest is not
/// invertible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewShareDocument {
    expires_at: DateTime<Utc>,
    payload: DurablePayload<ReviewSharePayload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewSharePayload {
    owner: PlayerId,
    address: ReviewShareAddress,
}

impl ReviewShareDocument {
    fn from_grant(grant: &ReviewShareGrant) -> Self {
        Self {
            expires_at: grant.expires_at,
            payload: DurablePayload::new(ReviewSharePayload {
                owner: grant.owner.clone(),
                address: grant.address.clone(),
            }),
        }
    }

    fn into_grant(self, share_id: String) -> ReviewShareGrant {
        let payload = self.payload.into_inner();
        ReviewShareGrant {
            share_id,
            owner: payload.owner,
            address: payload.address,
            expires_at: self.expires_at,
        }
    }
}

impl FirestoreReviewShareStore {
    async fn write(
        &self,
        owner_segment: &str,
        grant: ReviewShareGrant,
    ) -> Result<(), ReviewShareError> {
        let path = grant_path(owner_segment, &grant.share_id);
        let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
        let document = ReviewShareDocument::from_grant(&grant);
        let write = self.database.create_write(
            &path_refs,
            &document,
            &[("expiresAt", document.expires_at)],
        )?;
        Ok(self.database.commit(vec![write]).await?)
    }

    async fn read(
        &self,
        owner_segment: &str,
        share_id: &str,
    ) -> Result<Option<ReviewShareGrant>, ReviewShareError> {
        let path = grant_path(owner_segment, share_id);
        let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
        Ok(self
            .database
            .get_document::<ReviewShareDocument>(&path_refs)
            .await?
            .map(|document| document.into_grant(share_id.to_string())))
    }

    async fn read_all(
        &self,
        owner_segment: &str,
    ) -> Result<Vec<ReviewShareGrant>, ReviewShareError> {
        let path = grants_path(owner_segment);
        let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
        Ok(self
            .database
            .list_documents::<ReviewShareDocument>(&path_refs)
            .await?
            .into_iter()
            .map(|(share_id, document)| document.into_grant(share_id))
            .collect())
    }

    async fn remove(&self, owner_segment: &str, share_id: &str) -> Result<(), ReviewShareError> {
        let path = grant_path(owner_segment, share_id);
        let path_refs = path.iter().map(String::as_str).collect::<Vec<_>>();
        let write = self.database.delete_write(&path_refs)?;
        Ok(self.database.commit(vec![write]).await?)
    }
}

impl ReviewShareStore for FirestoreReviewShareStore {
    fn put<'a>(
        &'a self,
        owner_segment: &'a str,
        grant: ReviewShareGrant,
    ) -> ReviewShareStoreFuture<'a, ()> {
        Box::pin(self.write(owner_segment, grant))
    }

    fn get<'a>(
        &'a self,
        owner_segment: &'a str,
        share_id: &'a str,
    ) -> ReviewShareStoreFuture<'a, Option<ReviewShareGrant>> {
        Box::pin(self.read(owner_segment, share_id))
    }

    fn delete<'a>(
        &'a self,
        owner_segment: &'a str,
        share_id: &'a str,
    ) -> ReviewShareStoreFuture<'a, ()> {
        Box::pin(self.remove(owner_segment, share_id))
    }

    fn list<'a>(
        &'a self,
        owner_segment: &'a str,
    ) -> ReviewShareStoreFuture<'a, Vec<ReviewShareGrant>> {
        Box::pin(self.read_all(owner_segment))
    }
}

/// Roots every grant in the Player subtree account deletion removes, so a
/// deleted Player's outstanding links stop resolving without this store taking
/// part in deletion at all.
fn grants_path(owner_segment: &str) -> Vec<String> {
    let mut path = application_data_document_path_for_owner(owner_segment).to_vec();
    path.push(REVIEW_SHARES_COLLECTION.to_string());
    path
}

fn grant_path(owner_segment: &str, share_id: &str) -> Vec<String> {
    let mut path = grants_path(owner_segment);
    path.push(share_id.to_string());
    path
}

#[cfg(test)]
#[path = "review_share/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "review_share/firestore_tests.rs"]
mod firestore_tests;
