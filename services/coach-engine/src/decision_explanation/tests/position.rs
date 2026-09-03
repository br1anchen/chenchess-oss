use crate::review_session_contract::CurriculumLearningConcept as Concept;

use super::{collected_concepts, positive_highlight, SinglePvFixture};

const PAWN_MOVE: &[&str] = &["a2a3"];

const ROOK_ENDING_BEFORE_G5: &str = "8/6pp/1p6/p1p5/3p2k1/P3r3/1P1R2K1/8 b - - 0 38";
const PAWN_ENDING_BEFORE_G5: &str = "8/6pp/1p6/p1p5/3p2k1/P7/1P4K1/8 b - - 0 38";

#[test]
fn practical_rook_endings_accept_one_rook_per_side_with_only_pawns_remaining() {
    let concepts = concepts_after_pawn_move("4k2r/7p/8/8/8/8/P7/R3K3 w - - 0 1");

    assert!(concepts.contains(&Concept::PracticalRookEndings));
}

#[test]
fn practical_rook_endings_reject_queens_bishops_and_knights() {
    for fen in [
        "4k2r/7p/8/8/8/8/P7/R2QK3 w - - 0 1",
        "4k2r/7p/8/8/8/8/P7/R2BK3 w - - 0 1",
        "4k2r/7p/8/8/8/8/P7/R2NK3 w - - 0 1",
    ] {
        assert!(!concepts_after_pawn_move(fen).contains(&Concept::PracticalRookEndings));
    }
}

#[test]
fn practical_rook_endings_require_one_rook_for_each_side() {
    let concepts = concepts_after_pawn_move("4k3/7p/8/8/8/8/P7/R2RK3 w - - 0 1");

    assert!(!concepts.contains(&Concept::PracticalRookEndings));
}

#[test]
fn key_squares_rejects_rook_endgame_material() {
    let concepts = concepts_after_move(ROOK_ENDING_BEFORE_G5, "g7g5");

    assert!(!concepts.contains(&Concept::KeySquares));
}

#[test]
fn key_squares_accepts_pure_king_and_pawn_material() {
    let concepts = concepts_after_move(PAWN_ENDING_BEFORE_G5, "g7g5");

    assert!(concepts.contains(&Concept::KeySquares));
}

#[test]
fn opposition_rejects_rook_endgame_material() {
    let concepts = concepts_after_move(ROOK_ENDING_BEFORE_G5, "g7g5");

    assert!(!concepts.contains(&Concept::Opposition));
}

#[test]
fn opposition_accepts_pure_king_and_pawn_material() {
    let concepts = concepts_after_move(PAWN_ENDING_BEFORE_G5, "g7g5");

    assert!(concepts.contains(&Concept::Opposition));
}

#[test]
fn seventh_rank_rook_pawn_does_not_require_a_rook_piece() {
    let concepts = concepts_after_move("4k3/8/P7/8/8/8/8/4K3 w - - 0 1", "a6a7");

    assert!(concepts.contains(&Concept::SeventhRankRookPawn));
}

#[test]
fn seventh_rank_rook_pawn_rejects_non_pawn_material() {
    let concepts = concepts_after_move("4k2r/8/P7/8/8/8/8/4K2R w - - 0 1", "a6a7");

    assert!(!concepts.contains(&Concept::SeventhRankRookPawn));
}

#[test]
fn seventh_rank_rook_pawn_rejects_a_non_rook_file_pawn() {
    let concepts = concepts_after_move("4k3/8/1P6/8/8/8/8/4K3 w - - 0 1", "b6b7");

    assert!(!concepts.contains(&Concept::SeventhRankRookPawn));
}

fn concepts_after_pawn_move(fen: &str) -> Vec<Concept> {
    concepts_after_move(fen, PAWN_MOVE[0])
}

fn concepts_after_move(fen: &str, pawn_move: &str) -> Vec<Concept> {
    let fixture = SinglePvFixture {
        fen,
        best_root: pawn_move,
        best_line: &[pawn_move],
        best_score: 300,
        player_root: pawn_move,
        player_line: &[pawn_move],
        player_score: 300,
        classification: positive_highlight(),
    };
    collected_concepts(&fixture)[pawn_move].clone()
}
