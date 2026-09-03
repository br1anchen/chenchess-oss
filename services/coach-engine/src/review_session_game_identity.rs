//! Stable identity for "the Game this conversation is about".
//!
//! A reopened Coach App card cannot carry its Review Session handle, and the
//! handle is never spoken in chat. What does survive in chat is what the
//! Player typed: the Chess.com or Lichess URL, or the pasted PGN. That is the
//! only durable way back to a specific review, so it is what a handle-less
//! resume keys on.
//!
//! Identity must be derivable two ways and agree: from a requested source
//! before anything is imported, and from a stored import's provenance. Both
//! are local — no provider call is needed to decide which review is meant.

use crate::{
    chess_com::parse_chess_com_game_identity,
    game_import::artifact_digest,
    lichess::LichessGameUrl,
    review_session_contract::{
        EloRating, GameInputSource, ImportProvenance, ImportedGame, ReviewSide,
    },
};

/// Opaque, stable, and safe to use as a storage key component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReviewSessionGameIdentity(String);

impl ReviewSessionGameIdentity {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Fingerprint of a review the Player is naming again, resolved without
    /// importing anything. `None` means the source cannot identify a Game,
    /// which is a rejection rather than a reason to guess at another review.
    ///
    /// Side and rating are part of the fingerprint because the same Game
    /// reviewed as White at 1300 and as Black at 1500 are different reviews
    /// with different findings.
    pub(crate) fn from_request(
        source: &GameInputSource,
        review_side: ReviewSide,
        elo_rating: EloRating,
    ) -> Option<Self> {
        Self::from_game(source).map(|game| game.with_profile(review_side, elo_rating))
    }

    /// Fingerprint of a review that already exists, taken from what the
    /// importer resolved and recorded. Resolved values on both sides, so a
    /// side or rating inferred from a qualified URL still matches.
    pub(crate) fn from_import(content: &ImportedGame) -> Self {
        Self::from_provenance(&content.provenance)
            .with_profile(content.review_side, content.elo_profile.rating)
    }

    fn with_profile(self, review_side: ReviewSide, elo_rating: EloRating) -> Self {
        let side = match review_side {
            ReviewSide::White => "white",
            ReviewSide::Black => "black",
            ReviewSide::Both => "both",
        };
        Self(format!("{}|{side}|{}", self.0, elo_rating.value()))
    }

    fn from_game(source: &GameInputSource) -> Option<Self> {
        match source {
            GameInputSource::LichessUrl { url } => LichessGameUrl::parse(url)
                .ok()
                .map(|parsed| Self(format!("lichess:{}", parsed.canonical_game_id()))),
            GameInputSource::ChessComUrl { url } => Self::from_chess_com_url(url),
            // The digest is taken over the exact submitted bytes, matching
            // import. A paraphrased or reconstructed PGN therefore fails to
            // match instead of resolving to some other review.
            GameInputSource::PastedPgn { pgn } => artifact_digest(pgn.as_bytes())
                .ok()
                .map(|digest| Self(format!("pgn:{}", digest.as_str()))),
            GameInputSource::LocalPgnFile { .. } => None,
        }
    }

    fn from_provenance(provenance: &ImportProvenance) -> Self {
        match provenance {
            ImportProvenance::Lichess {
                canonical_game_id, ..
            } => Self(format!("lichess:{}", canonical_game_id.as_str())),
            ImportProvenance::ChessCom {
                canonical_game_id,
                canonical_url,
                ..
            } => Self::from_chess_com_url(canonical_url).unwrap_or_else(|| {
                Self(format!(
                    "chessCom:invalidCanonicalUrl:{}",
                    canonical_game_id.as_str()
                ))
            }),
            ImportProvenance::PastedPgn { pgn_digest }
            | ImportProvenance::LocalPgn { pgn_digest } => {
                Self(format!("pgn:{}", pgn_digest.as_str()))
            }
        }
    }

    fn from_chess_com_url(url: &str) -> Option<Self> {
        parse_chess_com_game_identity(url)
            .ok()
            .map(|(kind, game_id)| Self(format!("chessCom:{}:{game_id}", kind.as_path())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_session_contract::{ArtifactDigest, CanonicalGameId};

    fn lichess_source() -> GameInputSource {
        GameInputSource::LichessUrl {
            url: "https://lichess.org/Synthet1Demo/black".to_string(),
        }
    }

    fn elo(value: u16) -> EloRating {
        EloRating::try_from(value).unwrap()
    }

    #[test]
    fn a_request_and_its_import_agree_on_one_fingerprint() {
        let requested = ReviewSessionGameIdentity::from_request(
            &lichess_source(),
            ReviewSide::Black,
            elo(1450),
        )
        .unwrap();
        let imported = ReviewSessionGameIdentity::from_provenance(&ImportProvenance::Lichess {
            canonical_game_id: CanonicalGameId::try_from("Synthet1".to_string()).unwrap(),
            side_qualified_url: "https://lichess.org/Synthet1Demo/black".to_string(),
            canonical_url: "https://lichess.org/Synthet1".to_string(),
            export_contract_version: "v1".to_string(),
            captured_at: "2026-08-01T00:00:00Z".to_string(),
            response_digest: ArtifactDigest::try_from(format!("sha256:{}", "a".repeat(64)))
                .unwrap(),
            pgn_digest: ArtifactDigest::try_from(format!("sha256:{}", "b".repeat(64))).unwrap(),
        })
        .with_profile(ReviewSide::Black, elo(1450));

        assert_eq!(requested, imported);
    }

    #[test]
    fn side_and_rating_separate_reviews_of_the_same_game() {
        let white = ReviewSessionGameIdentity::from_request(
            &lichess_source(),
            ReviewSide::White,
            elo(1300),
        );
        let black = ReviewSessionGameIdentity::from_request(
            &lichess_source(),
            ReviewSide::Black,
            elo(1300),
        );
        let stronger = ReviewSessionGameIdentity::from_request(
            &lichess_source(),
            ReviewSide::White,
            elo(1500),
        );

        assert_ne!(white, black);
        assert_ne!(white, stronger);
    }

    #[test]
    fn chess_com_urls_include_the_game_kind_in_the_fingerprint() {
        let live = ReviewSessionGameIdentity::from_request(
            &GameInputSource::ChessComUrl {
                url: "https://www.chess.com/game/live/100000000002".to_string(),
            },
            ReviewSide::White,
            elo(1300),
        )
        .unwrap();
        let daily = ReviewSessionGameIdentity::from_request(
            &GameInputSource::ChessComUrl {
                url: "https://www.chess.com/game/daily/100000000002".to_string(),
            },
            ReviewSide::White,
            elo(1300),
        )
        .unwrap();
        let computer = ReviewSessionGameIdentity::from_request(
            &GameInputSource::ChessComUrl {
                url: "https://www.chess.com/game/computer/100000000002".to_string(),
            },
            ReviewSide::White,
            elo(1300),
        )
        .unwrap();

        assert_eq!(
            [live.as_str(), daily.as_str(), computer.as_str()],
            [
                "chessCom:live:100000000002|white|1300",
                "chessCom:daily:100000000002|white|1300",
                "chessCom:computer:100000000002|white|1300",
            ]
        );
    }

    #[test]
    fn chess_com_provenance_derives_the_game_kind_from_its_canonical_url() {
        let live = ReviewSessionGameIdentity::from_provenance(&chess_com_provenance(
            "https://www.chess.com/game/live/100000000002",
        ))
        .with_profile(ReviewSide::Black, elo(1450));
        let daily = ReviewSessionGameIdentity::from_provenance(&chess_com_provenance(
            "https://www.chess.com/game/daily/100000000002",
        ))
        .with_profile(ReviewSide::Black, elo(1450));
        let computer = ReviewSessionGameIdentity::from_provenance(&chess_com_provenance(
            "https://www.chess.com/game/computer/100000000002",
        ))
        .with_profile(ReviewSide::Black, elo(1450));

        assert_eq!(
            [live.as_str(), daily.as_str(), computer.as_str()],
            [
                "chessCom:live:100000000002|black|1450",
                "chessCom:daily:100000000002|black|1450",
                "chessCom:computer:100000000002|black|1450",
            ]
        );
    }

    #[test]
    fn an_unidentifiable_source_yields_no_fingerprint() {
        assert!(ReviewSessionGameIdentity::from_request(
            &GameInputSource::LichessUrl {
                url: "https://example.com/not-a-game".to_string(),
            },
            ReviewSide::White,
            elo(1300),
        )
        .is_none());
    }

    fn chess_com_provenance(canonical_url: &str) -> ImportProvenance {
        ImportProvenance::ChessCom {
            canonical_game_id: CanonicalGameId::try_from("100000000002".to_string()).unwrap(),
            canonical_url: canonical_url.to_string(),
            fetch_contract_version: "test-contract/v1".to_string(),
            captured_at: "2026-08-01T00:00:00Z".to_string(),
            response_digest: ArtifactDigest::try_from(format!("sha256:{}", "a".repeat(64)))
                .unwrap(),
            pgn_digest: ArtifactDigest::try_from(format!("sha256:{}", "b".repeat(64))).unwrap(),
        }
    }
}
