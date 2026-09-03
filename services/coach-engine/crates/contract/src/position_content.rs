//! Content identity for a **Position Snapshot**, used to key provider compute.
//!
//! A [`PositionRef`](super::PositionRef) addresses a Position Snapshot exactly:
//! it covers the repetition history that reached the position and the move
//! number it was reached on, because a Review Moment has to be able to say
//! *this* position at *this* point in *this* game. That makes it the wrong key
//! for provider compute. The same board reached in two games, on two Alternative
//! Move branches, or by two Players carries a different history and a different
//! move number, so a `PositionRef` key would recompute it every time.
//!
//! A [`PositionContentId`] is the path-independent half of that identity: the
//! content a provider is actually given, and nothing about how the position was
//! reached. Stockfish and the Human Move Model both receive a FEN and nothing
//! else, so their answer is a pure function of exactly these fields:
//!
//! - the variant,
//! - the piece placement,
//! - the side to move,
//! - the castling rights,
//! - the *legal* en passant square — a recorded en passant square no move can
//!   capture is normalized away by the Position Snapshot builder, so the two
//!   spellings of one position share a key,
//! - the halfmove clock, which drives the fifty-move rule.
//!
//! Deliberately excluded: the fullmove number, the repetition history digest,
//! and the repetition state and status derived from that history. A provider
//! handed a bare FEN cannot see the history, so keying on it would only cost
//! hits. Excluding the fullmove number is what makes the same position reached
//! at different points in two games a single compute.
//!
//! There is no Player, owner, Game Import, Review, or Review Session material
//! here, and there must never be. That is what lets one cache entry serve every
//! Player who reaches the position.

use anyhow::Result;
use serde::Serialize;

use super::{
    model::canonical_sha256, position_builder::build_position_snapshot, CastlingRights, Color,
    EnPassantState, OccupiedSquare, PositionSnapshot, PositionVariant,
};

const CONTENT_SCHEMA: &str = "position-content/v1";

/// The path-independent content identity of a Position Snapshot. Opaque by
/// design: it is a cache key, never an address a caller takes apart.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct PositionContentId(String);

impl PositionContentId {
    /// Derives the content id from a Position Snapshot's typed fields. Nothing
    /// is reparsed, so there is no spelling of a position that could key
    /// differently from the snapshot that holds it.
    pub fn from_position_snapshot(snapshot: &PositionSnapshot) -> Self {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct PositionContent<'a> {
            schema: &'static str,
            variant: PositionVariant,
            occupied: &'a [OccupiedSquare],
            side_to_move: Color,
            castling_rights: &'a CastlingRights,
            en_passant: &'a EnPassantState,
            halfmove_clock: u16,
        }

        Self(canonical_sha256(&PositionContent {
            schema: CONTENT_SCHEMA,
            variant: snapshot.variant,
            occupied: &snapshot.occupied,
            side_to_move: snapshot.side_to_move,
            castling_rights: &snapshot.castling_rights,
            en_passant: &snapshot.en_passant,
            halfmove_clock: snapshot.halfmove_clock,
        }))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A requested position, canonicalized once: the FEN a provider should be given,
/// and the content id that identifies it in a cache.
///
/// These two travel together on purpose. A cache that keys on canonical content
/// but forwards the caller's raw spelling would store a result computed from one
/// spelling of a position under a key naming another, and then serve it to every
/// caller who spells it differently — an en passant square the provider encodes
/// but the key ignores is exactly that bug. Sharing is only sound if the input
/// the provider saw is the input the key names, and deriving both from one
/// canonicalization is what guarantees it.
///
/// That includes the fullmove number, which the content id ignores: the FEN pins
/// it to the earliest value consistent with the halfmove clock rather than
/// keeping the caller's. Otherwise the position sent on a miss would carry one
/// caller's move counter while the entry served every caller, and soundness would
/// rest on the claim that no provider reads that field. Pinning it makes the FEN a
/// function of the key instead, so the claim is not needed.
pub struct CanonicalPosition {
    /// The canonical FEN: the Position Snapshot builder's own canonicalization —
    /// which is where an en passant square nothing can capture is normalized
    /// away — with the fullmove number pinned.
    pub fen: String,
    pub content_id: PositionContentId,
}

impl CanonicalPosition {
    /// Canonicalizes a FEN by building the Position Snapshot the contract would
    /// build for it. No history is supplied because none of it is keyed, and the
    /// snapshot is discarded — only its canonical position survives.
    pub fn from_fen(fen: &str) -> Result<Self> {
        let snapshot = build_position_snapshot(fen, &[])?;
        Ok(Self {
            content_id: PositionContentId::from_position_snapshot(&snapshot),
            fen: pin_fullmove_number(&snapshot),
        })
    }
}

/// Rewrites a canonical FEN's fullmove number to the earliest value the halfmove
/// clock allows, so two spellings of one position produce one FEN.
///
/// The clock counts halfmoves since the last capture or pawn move, so at least
/// that many plies have been played, and the pinned number is the smallest that
/// keeps the two fields consistent. Inventing a number is safe where an
/// arithmetically impossible pair would not be: a provider may validate the FEN
/// it is given.
fn pin_fullmove_number(snapshot: &PositionSnapshot) -> String {
    let elapsed_plies_before_this_move = u32::from(snapshot.halfmove_clock)
        .saturating_sub(u32::from(snapshot.side_to_move == Color::Black));
    let fullmove_number = elapsed_plies_before_this_move.div_ceil(2) + 1;
    let mut fields = snapshot.fen.split_whitespace();
    let position = fields.by_ref().take(5).collect::<Vec<_>>().join(" ");
    format!("{position} {fullmove_number}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SICILIAN_AFTER_TWO_MOVES: &str =
        "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b Kkq - 1 2";

    #[test]
    fn the_path_that_reached_a_position_does_not_change_its_content() {
        // Same board, same rights, same clock, reached after a different
        // history. The Position Snapshot address differs — that is its job —
        // but the compute is identical.
        let direct = build_position_snapshot(SICILIAN_AFTER_TWO_MOVES, &[]).unwrap();
        let transposed = build_position_snapshot(
            SICILIAN_AFTER_TWO_MOVES,
            &["rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"],
        )
        .unwrap();

        assert_ne!(direct.position_ref, transposed.position_ref);
        assert_eq!(
            PositionContentId::from_position_snapshot(&direct),
            PositionContentId::from_position_snapshot(&transposed)
        );
    }

    #[test]
    fn a_later_fullmove_number_is_the_same_content() {
        let early = build_position_snapshot(SICILIAN_AFTER_TWO_MOVES, &[]).unwrap();
        let late = build_position_snapshot(
            "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b Kkq - 1 40",
            &[],
        )
        .unwrap();

        assert_ne!(early.fullmove_number, late.fullmove_number);
        assert_eq!(
            PositionContentId::from_position_snapshot(&early),
            PositionContentId::from_position_snapshot(&late)
        );
    }

    #[test]
    fn an_uncapturable_en_passant_square_is_the_same_content() {
        // Black has no pawn on d4 or f4, so nothing can take on e3. The builder
        // normalizes the square away and both spellings are one position.
        let recorded = content("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
        let normalized = content("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");

        assert_eq!(recorded, normalized);
    }

    #[test]
    fn a_capturable_en_passant_square_changes_the_content() {
        // One board, one side to move, one clock, one set of rights: the en
        // passant square is the only difference, and the black c4 pawn can
        // actually take on d3, so the builder keeps it. Isolating the field this
        // way is what proves the content id reads it at all.
        const WITH_TARGET: &str = "rnbqkbnr/pp1ppppp/8/8/2pP4/5N2/PP2PPPP/RNBQKB1R b Kkq d3 0 3";
        const WITHOUT_TARGET: &str = "rnbqkbnr/pp1ppppp/8/8/2pP4/5N2/PP2PPPP/RNBQKB1R b Kkq - 0 3";

        let with_target = CanonicalPosition::from_fen(WITH_TARGET).unwrap();
        assert_eq!(
            with_target.fen.split_whitespace().nth(3),
            Some("d3"),
            "a capturable en passant square must survive canonicalization"
        );
        assert_ne!(with_target.content_id, content(WITHOUT_TARGET));
    }

    #[test]
    fn the_other_provider_relevant_fields_all_change_the_content() {
        let baseline = content(SICILIAN_AFTER_TWO_MOVES);

        for variation in [
            // A different board.
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            // The other side to move.
            "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R w Kkq - 1 2",
            // Fewer castling rights.
            "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b kq - 1 2",
            // A different halfmove clock, which moves the fifty-move horizon.
            "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b Kkq - 7 2",
        ] {
            assert_ne!(
                baseline,
                content(variation),
                "{variation} must not share content with the baseline"
            );
        }
    }

    #[test]
    fn an_unparseable_fen_has_no_canonical_position() {
        assert!(CanonicalPosition::from_fen("not-a-fen").is_err());
    }

    #[test]
    fn two_spellings_of_one_position_canonicalize_to_one_fen() {
        let early = CanonicalPosition::from_fen(SICILIAN_AFTER_TWO_MOVES).unwrap();
        let late = CanonicalPosition::from_fen(
            "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b Kkq e3 1 40",
        )
        .unwrap();

        assert_eq!(early.fen, late.fen);
        assert_eq!(early.content_id, late.content_id);
    }

    #[test]
    fn the_pinned_fullmove_number_stays_consistent_with_the_halfmove_clock() {
        for (fen, expected) in [
            (
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                1,
            ),
            // Black to move with a clock of 1: one ply has been played, which the
            // first full move already accounts for.
            (SICILIAN_AFTER_TWO_MOVES, 1),
            // Seven plies without a capture or pawn move cannot fit in fewer than
            // four full moves when Black is to move.
            (
                "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b Kkq - 7 9",
                4,
            ),
        ] {
            let canonical = CanonicalPosition::from_fen(fen).unwrap();
            let snapshot = build_position_snapshot(&canonical.fen, &[]).unwrap();

            assert_eq!(
                snapshot.fullmove_number, expected,
                "{fen} should pin to fullmove {expected}"
            );
            // The pinned FEN must still describe the same position, and a real
            // parser must accept it.
            let plies_elapsed = 2 * (snapshot.fullmove_number - 1)
                + u32::from(snapshot.side_to_move == Color::Black);
            assert!(plies_elapsed >= u32::from(snapshot.halfmove_clock));
            assert_eq!(
                PositionContentId::from_position_snapshot(&snapshot),
                canonical.content_id
            );
        }
    }

    fn content(fen: &str) -> PositionContentId {
        CanonicalPosition::from_fen(fen)
            .expect("the test position is a valid FEN")
            .content_id
    }
}
