//! Shared, identity-free cache of prepared Review Moment analysis.
//!
//! Preparing a Review Moment — its position snapshot, its objective coach-turn
//! context, and the provider evidence behind both — is a pure function of the
//! Game, the side, and the Elo, exactly like the [`GameAnalysisRecord`] the
//! sibling `gameAnalysis` cache already shares. It used to be stored as a child
//! of one Player's one Review Session, so the second Player to review a Game,
//! and the same Player returning a week later on a new session, paid for it
//! again.
//!
//! It is now addressed by the **review key** instead:
//!
//! ```text
//! reviewAnalysis/{reviewKey}/moments/{momentDoc}
//! ```
//!
//! [`ReviewKey`] is the digest segment the Game Import ID already carries, and
//! it already hashes the durability schema version and the analysis generation,
//! so a generation bump lands on a different address and misses by
//! construction. There is no owner segment and no session segment, which is
//! what makes the entry shareable — and a property the stored document has to
//! keep. Nothing the Player authored may go in here; published Review Moment
//! Comments live in [`review_annotation_store`](crate::review_annotation_store)
//! precisely because they cannot.
//!
//! Authorization does not move with the data. A Player still reaches these
//! documents only through their own Review Session root, which still lives in
//! their own subtree, and the moment IDs that root lists are the only ones read.
//!
//! [`GameAnalysisRecord`]: crate::game_analysis_store::GameAnalysisRecord

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    firestore::codec::DurablePayload,
    game_import_store::ReviewSessionGame,
    review_durability::{game_import_review_key, path::hashed_path_segment},
    review_session_contract::{CriticalMomentId, GameImportId},
};

mod comment_publication;
pub(crate) mod durable_moment;
#[cfg(test)]
mod durable_storage_measurement;
mod entry;
mod eviction;
mod firestore;
#[cfg(test)]
pub(crate) mod test_fixtures;

pub(crate) use comment_publication::{
    PublishedReviewMomentComment, ReviewMomentCommentPublicationCheckpoint,
    ReviewMomentCommentPublicationOutcome,
};
pub(crate) use entry::{
    CheckpointReviewSessionMoment, InvalidEntryReason, LocalDecisionCheckpoint,
    PreparedReviewSessionMoment, RestoredReviewSessionMoment,
};
pub use entry::{
    InMemoryReviewAnalysisCache, ReviewAnalysisCacheError, ReviewAnalysisCacheFuture,
    ReviewAnalysisCacheStore, ReviewAnalysisEntries, ReviewAnalysisEntry, ReviewAnalysisMutation,
};
pub use eviction::{
    evict_review_analysis_cache_from_env, ReviewAnalysisEvictionMode, ReviewAnalysisEvictionReport,
};
pub(crate) use firestore::{review_analysis_cache_store, FirestoreFirstOpenPublication};

use durable_moment::DurableReviewMomentPayload;

pub(crate) const REVIEW_ANALYSIS_COLLECTION: &str = "reviewAnalysis";
pub(crate) const REVIEW_MOMENTS_COLLECTION: &str = "moments";

/// A cached Review Moment outlives every session that reads it, so it is bounded
/// by the same hard lifetime as the `gameAnalysis` record it belongs with rather
/// than by any session's expiry.
pub(crate) const REVIEW_ANALYSIS_CACHE_LIFETIME_HOURS: i64 = 2_160;

/// The identity-free digest that addresses one review's analysis.
///
/// Not constructed from a Game identity here on purpose: the only reviews with
/// analysis to cache are ones a Game Import already exists for, and taking the
/// key off that ID keeps a single encoder — `review_durability::review_key` —
/// authoritative for both addresses.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReviewKey(String);

impl ReviewKey {
    pub(crate) fn from_game_import_id(game_import_id: &GameImportId) -> Option<Self> {
        game_import_review_key(game_import_id).map(|key| Self(key.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) fn cache_purge_at(written_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    written_at.checked_add_signed(TimeDelta::hours(REVIEW_ANALYSIS_CACHE_LIFETIME_HOURS))
}

pub(crate) fn moment_document_id(moment_id: &CriticalMomentId) -> String {
    hashed_path_segment(moment_id.as_str())
}

pub(crate) fn moments_collection_path(review_key: &ReviewKey) -> [&str; 3] {
    [
        REVIEW_ANALYSIS_COLLECTION,
        review_key.as_str(),
        REVIEW_MOMENTS_COLLECTION,
    ]
}

pub(crate) fn moment_document_path<'a>(
    review_key: &'a ReviewKey,
    moment_document_id: &'a str,
) -> [&'a str; 4] {
    [
        REVIEW_ANALYSIS_COLLECTION,
        review_key.as_str(),
        REVIEW_MOMENTS_COLLECTION,
        moment_document_id,
    ]
}

/// The stored shape of one cached Review Moment.
///
/// `purgeAt` is the cache lifetime, not any session's — the entry is shared, so
/// no single session may shorten or extend it.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReviewAnalysisMomentDocument {
    pub(crate) purge_at: DateTime<Utc>,
    payload: DurablePayload<DurableReviewMomentPayload>,
}

impl ReviewAnalysisMomentDocument {
    pub(crate) fn from_moment(
        moment: &ReviewAnalysisEntry,
        game: &ReviewSessionGame,
    ) -> Result<Self, ReviewAnalysisCacheError> {
        Ok(Self {
            purge_at: moment.purge_at,
            payload: DurablePayload::new(
                DurableReviewMomentPayload::from_moment(moment, game).map_err(|_| {
                    ReviewAnalysisCacheError::InvalidEntry(InvalidEntryReason::MomentEncode)
                })?,
            ),
        })
    }

    pub(crate) fn into_moment(
        self,
        game_import_id: &GameImportId,
        moment_id: CriticalMomentId,
        game: &ReviewSessionGame,
    ) -> Result<ReviewAnalysisEntry, ReviewAnalysisCacheError> {
        self.payload
            .into_inner()
            .into_moment(game_import_id, moment_id, self.purge_at, game)
            .map_err(|_| ReviewAnalysisCacheError::InvalidEntry(InvalidEntryReason::MomentDecode))
    }

    #[cfg(test)]
    pub(crate) fn into_payload_json(self) -> Result<String, ReviewAnalysisCacheError> {
        serde_json_canonicalizer::to_string(&self.payload.into_inner())
            .map_err(|_| ReviewAnalysisCacheError::InvalidEntry(InvalidEntryReason::Serialization))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        review_analysis_cache::test_fixtures::{fixture_identity, fixture_player},
        review_durability::game_import_id,
        review_session_contract::{EloRating, ReviewSide},
    };

    #[test]
    fn two_players_reviewing_one_game_address_the_same_cache_entry() {
        let first = ReviewKey::from_game_import_id(&game_import_id(
            &fixture_player("firebase-player-a"),
            &fixture_identity(),
        ))
        .unwrap();
        let second = ReviewKey::from_game_import_id(&game_import_id(
            &fixture_player("firebase-player-b"),
            &fixture_identity(),
        ))
        .unwrap();
        let other_side = ReviewKey::from_game_import_id(&game_import_id(
            &fixture_player("firebase-player-a"),
            &fixture_identity_for(ReviewSide::White, 1450),
        ))
        .unwrap();

        assert_eq!(first, second);
        assert!(!first.as_str().contains("firebase-player-a"));
        assert_ne!(first, other_side);
    }

    #[test]
    fn a_cache_address_is_rejected_when_the_game_import_id_is_not_digest_derived() {
        for malformed in [
            "game-import:fixture:1",
            "game-import:short:0123456789abcdef0123456789abcdef",
            "review-session:0123:0123",
        ] {
            assert_eq!(
                ReviewKey::from_game_import_id(
                    &GameImportId::try_from(malformed.to_string()).unwrap()
                ),
                None,
                "{malformed} must not resolve to a cache address"
            );
        }
    }

    fn fixture_identity_for(
        side: ReviewSide,
        elo: u16,
    ) -> crate::review_session_game_identity::ReviewSessionGameIdentity {
        crate::review_session_game_identity::ReviewSessionGameIdentity::from_request(
            &crate::review_session_contract::GameInputSource::LichessUrl {
                url: "https://lichess.org/Synthet1".to_string(),
            },
            side,
            EloRating::try_from(elo).unwrap(),
        )
        .unwrap()
    }
}
