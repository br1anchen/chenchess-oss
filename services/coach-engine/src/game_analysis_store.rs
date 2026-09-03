//! Shared, identity-free Game Analysis cache.
//!
//! Reviewing a Game is expensive — the engine and human-move providers
//! dominate the import pipeline — and the result is a pure function of what
//! the Player named: the Game, the side, and the Elo. Two Players who name the
//! same three things get the same analysis, so the second one should not pay
//! for them again.
//!
//! An analysis entry therefore has **no owner**. It holds only what is derived
//! from a public Game plus engine analysis: nothing the Player authored,
//! nothing that identifies them, and no reference back to a Review Session.
//! That is what makes it safe to share, and it is a property the record shape
//! has to keep — see [`GameAnalysisRecord`].
//!
//! Per-Player state stays exactly where it was. Every Player still gets their
//! own [`GameImportRecord`](crate::game_import_store::GameImportRecord) with
//! their own owner, lifetime, and purge window; this cache only supplies the
//! analysis that record is built from. No authorization boundary moves.
//!
//! The cache is an optimization and never a dependency. Every failure — a
//! read error, a write error, an entry from an older shape — degrades to
//! recomputing the analysis, because a Player waiting longer is always better
//! than a Player getting an error.

use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
};

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    engine_analysis::EngineProvenance,
    operating_limits::IMPORTED_REVIEW_FACTS_CAPACITY,
    review_durability::{review_key, REVIEW_ANALYSIS_GENERATION, REVIEW_DURABILITY_SCHEMA_VERSION},
    review_session_contract::{
        CanonicalGameMove, CompletedGameOutcome, EloRating, GameRef, GameReview,
        GameReviewCriticalMoment, ImportedGame, OpeningMetadata, PositionRef, ReviewSide,
    },
    review_session_game_identity::ReviewSessionGameIdentity,
};

mod firestore;

pub(crate) use firestore::game_analysis_store;

const GAME_ANALYSIS_SLIDING_LIFETIME_HOURS: i64 = 720;
const GAME_ANALYSIS_HARD_LIFETIME_HOURS: i64 = 2_160;

pub type GameAnalysisStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, GameAnalysisStoreError>> + Send + 'a>>;

pub trait GameAnalysisStore: Send + Sync {
    /// `Ok(None)` covers every reason there is no usable analysis — absent,
    /// aged out, or written by an older shape. Callers recompute either way.
    fn find<'a>(
        &'a self,
        identity: &'a ReviewSessionGameIdentity,
        imported: &'a ImportedGame,
        now: DateTime<Utc>,
    ) -> GameAnalysisStoreFuture<'a, Option<Box<GameAnalysisRecord>>>;

    /// Last writer wins. Two Players racing on the same Game each computed
    /// analysis for the same inputs, so either result is correct to keep.
    fn put<'a>(
        &'a self,
        identity: &'a ReviewSessionGameIdentity,
        record: GameAnalysisRecord,
    ) -> GameAnalysisStoreFuture<'a, ()>;
}

/// Every field here must be derivable from the Game and the engine alone. An
/// owner, a Player ID, a Review Session ID, or anything the Player wrote does
/// not belong in a shared record — adding one would turn a cache into a
/// cross-Player disclosure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameAnalysisRecord {
    pub schema_version: u8,
    pub generation: u32,
    pub created_at: DateTime<Utc>,
    pub purge_at: DateTime<Utc>,
    pub hard_expires_at: DateTime<Utc>,
    pub identity_free_game: IdentityFreeGame,
    pub review_side: ReviewSide,
    pub resolved_elo: EloRating,
    pub review: GameReview,
    pub player_selected_moments: BTreeMap<u16, GameReviewCriticalMoment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_provenance: Option<EngineProvenance>,
}

impl GameAnalysisRecord {
    pub fn new(
        imported: &ImportedGame,
        review: GameReview,
        player_selected_moments: Vec<GameReviewCriticalMoment>,
        engine_provenance: Option<EngineProvenance>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let purge_at = created_at
            .checked_add_signed(TimeDelta::hours(GAME_ANALYSIS_SLIDING_LIFETIME_HOURS))
            .expect("a Game Analysis timestamp can advance by its sliding lifetime");
        let hard_expires_at = created_at
            .checked_add_signed(TimeDelta::hours(GAME_ANALYSIS_HARD_LIFETIME_HOURS))
            .expect("a Game Analysis timestamp can advance by its hard lifetime");
        Self {
            schema_version: REVIEW_DURABILITY_SCHEMA_VERSION,
            generation: REVIEW_ANALYSIS_GENERATION,
            created_at,
            purge_at,
            hard_expires_at,
            identity_free_game: IdentityFreeGame::from(imported),
            review_side: imported.review_side,
            resolved_elo: imported.elo_profile.rating,
            review,
            player_selected_moments: player_selected_moments
                .into_iter()
                .map(|moment| (moment.ply, moment))
                .collect(),
            engine_provenance,
        }
    }

    /// Analysis comes back out in the shape the import pipeline hands around,
    /// so a cache hit and a fresh review are indistinguishable downstream.
    pub fn into_review(
        self,
    ) -> (
        GameReview,
        Vec<GameReviewCriticalMoment>,
        Option<EngineProvenance>,
    ) {
        (
            self.review,
            self.player_selected_moments.into_values().collect(),
            self.engine_provenance,
        )
    }

    fn is_usable(&self, imported: &ImportedGame, now: DateTime<Utc>) -> bool {
        let expected_initial_expiry = self
            .created_at
            .checked_add_signed(TimeDelta::hours(GAME_ANALYSIS_SLIDING_LIFETIME_HOURS));
        let expected_hard_expiry = self
            .created_at
            .checked_add_signed(TimeDelta::hours(GAME_ANALYSIS_HARD_LIFETIME_HOURS));
        self.schema_version == REVIEW_DURABILITY_SCHEMA_VERSION
            && self.generation == REVIEW_ANALYSIS_GENERATION
            && self.identity_free_game == IdentityFreeGame::from(imported)
            && self.review_side == imported.review_side
            && self.resolved_elo == imported.elo_profile.rating
            && Some(self.hard_expires_at) == expected_hard_expiry
            && expected_initial_expiry.is_some_and(|expiry| self.purge_at >= expiry)
            && now < self.purge_at
            && self.purge_at <= self.hard_expires_at
    }

    fn refresh(&mut self, now: DateTime<Utc>) {
        let Some(sliding_expiry) =
            now.checked_add_signed(TimeDelta::hours(GAME_ANALYSIS_SLIDING_LIFETIME_HOURS))
        else {
            return;
        };
        self.purge_at = self.purge_at.max(sliding_expiry.min(self.hard_expires_at));
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityFreeGame {
    pub game_ref: GameRef,
    pub opening: OpeningMetadata,
    pub outcome: CompletedGameOutcome,
    pub moves: Vec<CanonicalGameMove>,
    pub final_position_ref: PositionRef,
}

impl From<&ImportedGame> for IdentityFreeGame {
    fn from(imported: &ImportedGame) -> Self {
        Self {
            game_ref: imported.game.game_ref.clone(),
            opening: imported.game.opening.clone(),
            outcome: imported.game.outcome,
            moves: imported.game.moves.clone(),
            final_position_ref: imported.game.final_position_ref.clone(),
        }
    }
}

pub(crate) fn game_analysis_document_id(identity: &ReviewSessionGameIdentity) -> String {
    review_key(identity)
}

#[derive(Default)]
struct InMemoryGameAnalysis {
    records: BTreeMap<String, GameAnalysisRecord>,
    insertion_order: VecDeque<String>,
}

#[derive(Default)]
pub struct InMemoryGameAnalysisStore {
    state: Mutex<InMemoryGameAnalysis>,
}

impl GameAnalysisStore for InMemoryGameAnalysisStore {
    fn find<'a>(
        &'a self,
        identity: &'a ReviewSessionGameIdentity,
        imported: &'a ImportedGame,
        now: DateTime<Utc>,
    ) -> GameAnalysisStoreFuture<'a, Option<Box<GameAnalysisRecord>>> {
        Box::pin(async move {
            let key = game_analysis_document_id(identity);
            let mut state = self.state.lock().await;
            let Some(record) = state.records.get_mut(&key) else {
                return Ok(None);
            };
            if !record.is_usable(imported, now) {
                return Ok(None);
            }
            record.refresh(now);
            Ok(Some(Box::new(record.clone())))
        })
    }

    fn put<'a>(
        &'a self,
        identity: &'a ReviewSessionGameIdentity,
        record: GameAnalysisRecord,
    ) -> GameAnalysisStoreFuture<'a, ()> {
        Box::pin(async move {
            let mut state = self.state.lock().await;
            let InMemoryGameAnalysis {
                records,
                insertion_order,
            } = &mut *state;
            records.retain(|_, existing| existing.purge_at > record.created_at);
            insertion_order.retain(|identity| records.contains_key(identity));
            let key = game_analysis_document_id(identity);
            if records.insert(key.clone(), record).is_none() {
                insertion_order.push_back(key);
            }
            while records.len() > IMPORTED_REVIEW_FACTS_CAPACITY {
                let Some(evicted) = insertion_order.pop_front() else {
                    break;
                };
                records.remove(&evicted);
            }
            Ok(())
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GameAnalysisStoreError {
    #[error("Game Analysis persistence is misconfigured: {0}")]
    Configuration(String),
    #[error("Game Analysis persistence transport failed")]
    Transport,
    #[error("Game Analysis persistence is unavailable")]
    Unavailable,
    #[error("Game Analysis retention changed concurrently")]
    Conflict,
    #[error("Game Analysis persistence returned an invalid record")]
    InvalidRecord,
}

impl GameAnalysisStoreError {
    pub(crate) fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::Configuration(_) => "configuration",
            Self::Transport => "transport",
            Self::Unavailable => "unavailable",
            Self::Conflict => "conflict",
            Self::InvalidRecord => "invalid-record",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_session_contract::{
        ImportedGame, OperationCompletion, ReviewSessionEvent, ReviewSessionEventEnvelope,
    };

    fn fixture_import() -> ImportedGame {
        serde_json::from_str::<ImportedGame>(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
        )))
        .unwrap()
    }

    fn fixture_review() -> GameReview {
        let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/coach-engine-sdk/fixtures/events.json"
        )))
        .unwrap();
        events
            .into_iter()
            .find_map(|event| match event.event {
                ReviewSessionEvent::Completed { result } => match *result {
                    OperationCompletion::GameImported { review, .. } => Some(*review),
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    }

    #[tokio::test]
    async fn analysis_is_shared_by_fingerprint_and_ages_out() {
        let store = InMemoryGameAnalysisStore::default();
        let created_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = fixture_import();
        let identity = ReviewSessionGameIdentity::from_import(&imported);
        store
            .put(
                &identity,
                GameAnalysisRecord::new(&imported, fixture_review(), Vec::new(), None, created_at),
            )
            .await
            .unwrap();

        // No principal is involved anywhere: whoever asks with the same
        // fingerprint gets the same analysis, which is the entire point.
        assert!(store
            .find(&identity, &imported, created_at)
            .await
            .unwrap()
            .is_some());
        let mut other_import = imported.clone();
        other_import.review_side = crate::review_session_contract::ReviewSide::White;
        let other_identity = ReviewSessionGameIdentity::from_import(&other_import);
        assert!(store
            .find(&other_identity, &other_import, created_at)
            .await
            .unwrap()
            .is_none());
        assert!(store
            .find(
                &identity,
                &imported,
                created_at + TimeDelta::hours(GAME_ANALYSIS_SLIDING_LIFETIME_HOURS),
            )
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_superseded_generation_reads_as_a_miss() {
        let store = InMemoryGameAnalysisStore::default();
        let created_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = fixture_import();
        let identity = ReviewSessionGameIdentity::from_import(&imported);
        let mut stale =
            GameAnalysisRecord::new(&imported, fixture_review(), Vec::new(), None, created_at);
        stale.generation = REVIEW_ANALYSIS_GENERATION.wrapping_sub(1);
        store.put(&identity, stale).await.unwrap();

        // Bumping the generation is how a Stockfish or Maia change invalidates
        // everything at once, so a stale entry must never be served.
        assert!(store
            .find(&identity, &imported, created_at)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_payload_for_another_review_profile_reads_as_a_miss() {
        let store = InMemoryGameAnalysisStore::default();
        let created_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = fixture_import();
        let identity = ReviewSessionGameIdentity::from_import(&imported);
        let mut mismatched =
            GameAnalysisRecord::new(&imported, fixture_review(), Vec::new(), None, created_at);
        mismatched.review_side = crate::review_session_contract::ReviewSide::White;
        store.put(&identity, mismatched).await.unwrap();

        assert!(store
            .find(&identity, &imported, created_at)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn malformed_retention_bounds_read_as_a_miss() {
        let store = InMemoryGameAnalysisStore::default();
        let created_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = fixture_import();
        let identity = ReviewSessionGameIdentity::from_import(&imported);
        let mut malformed =
            GameAnalysisRecord::new(&imported, fixture_review(), Vec::new(), None, created_at);
        malformed.hard_expires_at += TimeDelta::hours(1);
        store.put(&identity, malformed).await.unwrap();

        assert!(store
            .find(&identity, &imported, created_at)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn hits_slide_retention_without_crossing_the_hard_expiry() {
        let store = InMemoryGameAnalysisStore::default();
        let created_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = fixture_import();
        let identity = ReviewSessionGameIdentity::from_import(&imported);
        let record =
            GameAnalysisRecord::new(&imported, fixture_review(), Vec::new(), None, created_at);
        let hard_expires_at = record.hard_expires_at;
        store.put(&identity, record).await.unwrap();

        for elapsed_hours in [719, 1_438, 2_157] {
            let found = store
                .find(
                    &identity,
                    &imported,
                    created_at + TimeDelta::hours(elapsed_hours),
                )
                .await
                .unwrap()
                .unwrap();
            assert!(found.purge_at <= hard_expires_at);
        }
        assert!(store
            .find(&identity, &imported, hard_expires_at)
            .await
            .unwrap()
            .is_none());
    }

    #[test]
    fn the_document_id_is_opaque_and_generation_scoped() {
        let imported = fixture_import();
        let identity = ReviewSessionGameIdentity::from_import(&imported);
        let id = game_analysis_document_id(&identity);

        assert_eq!(id.len(), 64);
        assert!(!id.contains(|character: char| !character.is_ascii_hexdigit()));
        let mut other_import = imported;
        other_import.review_side = crate::review_session_contract::ReviewSide::White;
        assert_ne!(
            id,
            game_analysis_document_id(&ReviewSessionGameIdentity::from_import(&other_import))
        );
    }

    #[tokio::test]
    async fn an_old_chess_com_document_id_is_a_normal_cache_miss() {
        let store = InMemoryGameAnalysisStore::default();
        let created_at: DateTime<Utc> = "2026-08-01T10:00:00Z".parse().unwrap();
        let imported = fixture_import();
        let identity = ReviewSessionGameIdentity::from_request(
            &crate::review_session_contract::GameInputSource::ChessComUrl {
                url: "https://www.chess.com/game/live/100000000002".to_string(),
            },
            crate::review_session_contract::ReviewSide::Black,
            crate::review_session_contract::EloRating::try_from(1246).unwrap(),
        )
        .unwrap();
        let record =
            GameAnalysisRecord::new(&imported, fixture_review(), Vec::new(), None, created_at);
        let mut legacy_material = vec![REVIEW_DURABILITY_SCHEMA_VERSION];
        legacy_material.extend_from_slice(&REVIEW_ANALYSIS_GENERATION.to_be_bytes());
        legacy_material.extend_from_slice(b"chessCom:100000000002|black|1246");
        let legacy_document_id =
            crate::review_durability::path::hashed_path_segment(legacy_material);
        store
            .state
            .lock()
            .await
            .records
            .insert(legacy_document_id, record);

        assert!(store
            .find(&identity, &imported, created_at)
            .await
            .unwrap()
            .is_none());
    }
}
