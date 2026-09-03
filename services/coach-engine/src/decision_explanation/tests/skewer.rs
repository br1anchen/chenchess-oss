use crate::review_session_contract::CurriculumLearningConcept as Concept;

use super::{collected_concepts, positive_highlight, SinglePvFixture};

#[test]
fn skewer_accepts_a_king_as_the_sole_ray_blocker_before_the_payoff() {
    let fixture = SinglePvFixture {
        fen: "4k2q/8/8/8/8/8/8/RK6 w - - 0 1",
        best_root: "a1a8",
        best_line: &["a1a8", "e8e7", "a8h8"],
        best_score: 900,
        player_root: "a1a8",
        player_line: &["a1a8", "e8e7", "a8h8"],
        player_score: 900,
        classification: positive_highlight(),
    };

    assert!(collected_concepts(&fixture)["a1a8"].contains(&Concept::Skewer));
}

#[test]
fn skewer_rejects_a_piece_that_interposes_after_the_check() {
    let fixture = SinglePvFixture {
        fen: "r5k1/5pp1/P2p1b1p/2p5/2qpPP2/3R3P/1P2Q1P1/6K1 b - - 0 26",
        best_root: "c4c1",
        best_line: &["c4c1", "e2f1", "c1f1"],
        best_score: 575,
        player_root: "c4c1",
        player_line: &["c4c1", "e2f1", "c1f1"],
        player_score: 575,
        classification: positive_highlight(),
    };

    assert!(!collected_concepts(&fixture)["c4c1"].contains(&Concept::Skewer));
}

#[test]
fn skewer_rejects_an_unrelated_capture_after_the_king_moves() {
    let fixture = SinglePvFixture {
        fen: "4k3/7r/8/8/8/8/8/K2Q4 w - - 0 1",
        best_root: "d1h5",
        best_line: &["d1h5", "e8e7", "h5h7"],
        best_score: 500,
        player_root: "d1h5",
        player_line: &["d1h5", "e8e7", "h5h7"],
        player_score: 500,
        classification: positive_highlight(),
    };

    assert!(!collected_concepts(&fixture)["d1h5"].contains(&Concept::Skewer));
}

#[test]
fn skewer_rejects_a_discovered_check_by_another_piece() {
    let fixture = SinglePvFixture {
        fen: "4k3/5r2/8/8/8/8/4B3/4R2K w - - 0 1",
        best_root: "e2c4",
        best_line: &["e2c4", "e8d7", "c4f7"],
        best_score: 500,
        player_root: "e2c4",
        player_line: &["e2c4", "e8d7", "c4f7"],
        player_score: 500,
        classification: positive_highlight(),
    };

    assert!(!collected_concepts(&fixture)["e2c4"].contains(&Concept::Skewer));
}
