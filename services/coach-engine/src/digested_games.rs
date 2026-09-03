//! Which Games Daily Coaching produced, for the surfaces that must not offer
//! to delete one.
//!
//! A Player may remove a Game they imported themselves. A Game Daily Coaching
//! digested is a different thing: a published Coaching Digest cites its
//! supporting Games and is immutable, and the connected playing profile would
//! import the Game again on the next tick, so removing one is not a delete at
//! all. The Review Session processor and the Imported Games listing hold the
//! Player's Game Imports and not their Daily Coaching runs, so they ask this
//! instead of reading that store.

use std::{collections::BTreeSet, future::Future, pin::Pin};

use crate::{review_session_contract::PlayerId, reviewed_games::ReviewedGameKey};

pub type DigestedGameFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, DigestedGameLookupError>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DigestedGameLookupError {
    #[error("Daily Coaching digested Games are unavailable")]
    Unavailable,
}

pub trait DigestedGameIndex: Send + Sync {
    /// Every Game Daily Coaching digested for this Player.
    ///
    /// Answered as a whole set rather than one question at a time, because both
    /// callers ask about a page of Games at once.
    fn digested_games<'a>(
        &'a self,
        owner: &'a PlayerId,
    ) -> DigestedGameFuture<'a, BTreeSet<ReviewedGameKey>>;
}

/// The index a runtime with no Daily Coaching behind it answers from.
///
/// Test-constructed and Local Coach runtimes have no Daily Coaching runs at
/// all, so nothing they hold was ever digested.
pub struct NoDigestedGames;

impl DigestedGameIndex for NoDigestedGames {
    fn digested_games<'a>(
        &'a self,
        _owner: &'a PlayerId,
    ) -> DigestedGameFuture<'a, BTreeSet<ReviewedGameKey>> {
        Box::pin(async { Ok(BTreeSet::new()) })
    }
}
