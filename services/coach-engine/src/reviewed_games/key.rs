//! One Game on a Player's shelf, however it got there.
//!
//! A Game is the source Game and the side it was reviewed from, and nothing
//! else: the same Game reviewed as White and as Black is two reviews with two
//! sets of findings, while the same Game reviewed at three Elo Profiles is one
//! entry the Player sees once. Every surface that lists, merges, or deletes a
//! Game agrees on this and on nothing narrower, so they all say it this way
//! rather than each formatting their own string.

use std::fmt::{Display, Formatter};

use crate::imported_games::ImportedGameReviewSide;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReviewedGameKey {
    pub canonical_source_key: String,
    pub review_side: ImportedGameReviewSide,
}

impl Display for ReviewedGameKey {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let side = match self.review_side {
            ImportedGameReviewSide::White => "white",
            ImportedGameReviewSide::Black => "black",
            ImportedGameReviewSide::Both => "both",
        };
        write!(formatter, "{}:{side}", self.canonical_source_key)
    }
}
