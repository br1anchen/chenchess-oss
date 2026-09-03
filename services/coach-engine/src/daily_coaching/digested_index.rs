//! Answering "did Daily Coaching produce this Game?" for readers outside it.
//!
//! The Review Session processor and the Imported Games listing both need the
//! answer and neither should take on Daily Coaching to get it, so this is the
//! one place the digested cards are read on their behalf.

use std::sync::Arc;

use crate::{
    digested_games::{DigestedGameFuture, DigestedGameIndex, DigestedGameLookupError},
    firestore::FirestoreDatabase,
    review_session_contract::PlayerId,
    reviewed_games::ReviewedGameKey,
};

use super::{runs, state::DailyCoachingOwnerKey};

/// The digested-Game answer, read from the cards a Digest publication wrote.
///
/// Holds only the run store, so a Review Session runtime can ask what Daily
/// Coaching produced without taking on Daily Coaching itself.
struct DigestedGameCards {
    runs: Arc<dyn runs::DailyCoachingRunStore>,
}

impl DigestedGameIndex for DigestedGameCards {
    fn digested_games<'a>(
        &'a self,
        owner: &'a PlayerId,
    ) -> DigestedGameFuture<'a, std::collections::BTreeSet<ReviewedGameKey>> {
        Box::pin(async move {
            let owner_key = DailyCoachingOwnerKey::for_player(owner);
            let cards = self
                .runs
                .list_digested_game_cards(&owner_key)
                .await
                .map_err(|error| {
                    tracing::error!(
                        category = "daily_coaching",
                        %error,
                        "failed to list digested Games"
                    );
                    DigestedGameLookupError::Unavailable
                })?;
            /* Fails closed on a card that will not validate. Every other reader
            of these cards skips a bad one because a Game it cannot describe is
            a Game it cannot list; this reader is deciding whether a Player may
            delete, and a card it cannot read is exactly the case where
            answering "not digested" would delete a digested Game. */
            cards
                .into_iter()
                .map(|card| {
                    card.validate()
                        .map(|()| ReviewedGameKey {
                            canonical_source_key: card.canonical_source_key(),
                            review_side: card.review_side(),
                        })
                        .map_err(|error| {
                            tracing::error!(
                                category = "daily_coaching",
                                %error,
                                "a digested Game card is unreadable"
                            );
                            DigestedGameLookupError::Unavailable
                        })
                })
                .collect()
        })
    }
}

pub(crate) fn digested_game_index(database: FirestoreDatabase) -> Arc<dyn DigestedGameIndex> {
    Arc::new(DigestedGameCards {
        runs: Arc::new(runs::firestore::FirestoreDailyCoachingRunStore::new(
            database,
        )),
    })
}
