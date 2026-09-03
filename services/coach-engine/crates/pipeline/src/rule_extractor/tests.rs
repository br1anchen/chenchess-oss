use super::{
    extract, extract_selected_moment, AfterMoveEvidence, CriticalMomentCategory, MoveEvidence,
    OpeningPrinciple, RuleExtractorError, TeachingFactVocabularyVersion, TeachingTheme,
};
use crate::{
    domain::{EloProfile, Game, HumanMoveCandidate, ReviewSide},
    engine_analysis::{EngineAnalysis, PositionEvaluation},
    human_move_model::HumanMovePrediction,
    pgn::parse_pgn,
    review_session_contract::{
        GameReviewMomentClassification, NeutralReviewReason, PositiveHighlightGrade,
    },
};

#[test]
fn extracts_a_forcing_tactical_loss() {
    let game = game("[FEN \"6k1/7p/8/8/8/3Q4/8/K5N1 w - - 0 1\"]\n\n1. Qxh7+ *");
    let before = engine("g1f3", 80);
    let after = engine("e8e7", 250);
    let human = human(&[("d3h7", 0.24), ("g1f3", 0.18)]);

    let facts = extract(
        &game,
        elo(1200),
        ReviewSide::Both,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        }],
    )
    .expect("complete evidence should extract facts");

    let moment = &facts.critical_moments[0];
    assert_eq!(moment.category, CriticalMomentCategory::Tactical);
    assert_eq!(moment.objective.best_move, "g1f3");
    assert_eq!(moment.objective.played_move, "d3h7");
    assert_eq!(moment.objective.centipawn_loss, Some(330));
    assert_eq!(
        moment.objective.played_evaluation,
        PositionEvaluation::Centipawns(-250)
    );
    assert!(matches!(
        moment.classification,
        GameReviewMomentClassification::ImprovementOpportunity {
            ref correction
        } if correction.better_move_uci == "g1f3"
            && correction.better_move_san == "Nf3"
    ));
}

#[test]
fn carries_better_move_san_for_bishop_and_knight_corrections() {
    let cases = [
        (
            "1. Nf3 g6 2. g3 Bg7 3. Bg2 Nf6 4. Nc3 O-O 5. O-O Nc6 6. d4 d6 7. a3 Bg4 8. h3 Bxf3 9. Bxf3 Nd7 10. Bxc6 bxc6 11. e4 e5 12. Ne2 *",
            23,
            "c1e3",
            "c3e2",
            "Be3",
        ),
        (
            "1. Nf3 g6 2. g3 Bg7 3. Bg2 Nf6 4. Nc3 O-O 5. O-O Nc6 6. d4 d6 7. a3 Bg4 8. h3 Bxf3 9. Bxf3 Nd7 10. Bxc6 bxc6 11. e4 e5 12. Ne2 Nb6 13. b3 Qd7 14. g4 d5 15. f3 Rfe8 16. Bb2 Qd8 17. Ng3 Bf8 18. Qd3 c5 19. dxe5 Nd7 20. f4 c6 21. exd5 Qh4 22. Kg2 Rac8 23. Rad1 Nb6 24. c4 Bh6 25. g5 Bxg5 26. fxg5 Qxg5 27. Rde1 h5 28. Kh2 h4 29. Qf3 *",
            57,
            "g3e4",
            "d3f3",
            "Ne4",
        ),
    ];

    for (pgn, ply, best_move, played_move, expected_san) in cases {
        let game = game(pgn);
        let before = engine(best_move, 40);
        let human = human(&[(best_move, 0.40), (played_move, 0.20)]);
        let facts = extract_selected_moment(
            &game,
            elo(1200),
            &MoveEvidence {
                ply,
                engine_before: &before,
                after_move: AfterMoveEvidence::Analyzed(PositionEvaluation::Centipawns(300)),
                human_before: &human,
            },
        )
        .expect("legal correction evidence should extract facts");

        assert!(matches!(
            facts.selected_moment.classification,
            GameReviewMomentClassification::ImprovementOpportunity {
                ref correction
            } if correction.better_move_uci == best_move
                && correction.better_move_san == expected_san
        ));
    }
}

#[test]
fn extracts_a_quiet_positional_concession_for_an_advanced_player() {
    let game = game("1. h3 *");
    let before = engine("g1f3", 40);
    let after = engine("g8f6", 50);
    let human = human(&[("g1f3", 0.31), ("h2h3", 0.08)]);

    let facts = extract(
        &game,
        elo(2000),
        ReviewSide::Both,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        }],
    )
    .expect("complete evidence should extract facts");

    let moment = &facts.critical_moments[0];
    assert_eq!(moment.category, CriticalMomentCategory::Positional);
    assert_eq!(moment.objective.centipawn_loss, Some(90));
    assert!(!moment.human.played_move_is_human_likely);
}

#[test]
fn distinguishes_a_human_likely_mistake_from_objective_best_play() {
    let game = game("1. a3 *");
    let before = engine("e2e4", 50);
    let after = engine("e7e5", 70);
    let human = human(&[("a2a3", 0.42), ("e2e4", 0.28)]);

    let facts = extract(
        &game,
        elo(1500),
        ReviewSide::Both,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        }],
    )
    .expect("complete evidence should extract facts");

    let moment = &facts.critical_moments[0];
    assert_eq!(moment.objective.best_move, "e2e4");
    assert_eq!(moment.human.most_likely_move, "a2a3");
    assert_eq!(moment.human.played_move_probability, Some(0.42));
    assert!(moment.human.played_move_is_human_likely);
    assert_eq!(
        moment.teaching.vocabulary_version,
        TeachingFactVocabularyVersion::V1
    );
    assert_eq!(
        moment.teaching.opening_principles,
        vec![OpeningPrinciple::OccupyTheCenter]
    );
}

#[test]
fn requires_provider_evidence_for_every_move_in_the_game() {
    let game = game("1. e4 e5 *");
    let before = engine("e2e4", 30);
    let after = engine("e7e5", -30);
    let human = human(&[("e2e4", 0.45)]);

    let error = extract(
        &game,
        elo(1200),
        ReviewSide::Both,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        }],
    )
    .expect_err("partial evidence must not produce whole-Game facts");

    assert!(matches!(
        error,
        RuleExtractorError::MissingEvidence { ply: 2 }
    ));
}

#[test]
fn selects_only_teachable_moments_after_evaluating_the_whole_game() {
    let game = game("1. e4 a6 *");
    let first_before = engine("e2e4", 30);
    let first_after = engine("e7e5", -20);
    let first_human = human(&[("e2e4", 0.45)]);
    let second_before = engine("g8f6", 20);
    let second_after = engine("g1f3", 120);
    let second_human = human(&[("a7a6", 0.25), ("g8f6", 0.22)]);

    let facts = extract(
        &game,
        elo(1500),
        ReviewSide::Both,
        &[
            MoveEvidence {
                ply: 1,
                engine_before: &first_before,
                after_move: analyzed(&first_after),
                human_before: &first_human,
            },
            MoveEvidence {
                ply: 2,
                engine_before: &second_before,
                after_move: analyzed(&second_after),
                human_before: &second_human,
            },
        ],
    )
    .expect("whole-Game evidence should extract facts");

    assert_eq!(facts.critical_moments.len(), 1);
    assert_eq!(facts.critical_moments[0].ply, 2);
    assert!(facts.summary.contains("Analyzed 2 plies"));
}

#[test]
fn extracts_a_player_selected_move_even_when_it_is_not_a_critical_moment() {
    let game = game("1. e4 *");
    let before = engine("e2e4", 30);
    let after = engine("e7e5", -30);
    let human = human(&[("e2e4", 0.45)]);

    let facts = extract_selected_moment(
        &game,
        elo(1200),
        &MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        },
    )
    .expect("Player selection should force one fact packet");

    assert_eq!(facts.selected_moment.ply, 1);
    assert!(matches!(
        facts.selected_moment.classification,
        GameReviewMomentClassification::Neutral {
            ref reasons
        } if reasons == &[NeutralReviewReason::SoundWithoutConcreteAchievement]
    ));
}

#[test]
fn classifies_an_objectively_sound_concrete_achievement_as_a_good_positive_highlight() {
    let game = game("[FEN \"6k1/7p/8/8/8/3Q4/8/K5N1 w - - 0 1\"]\n\n1. Qxh7+ *");
    let before = engine("d3h7", 80);
    let after = engine("e8e7", -80);
    let human = human(&[("d3h7", 0.24), ("g1f3", 0.18)]);

    let facts = extract_selected_moment(
        &game,
        elo(1200),
        &MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        },
    )
    .expect("complete legal evidence should classify the selected move");

    assert!(matches!(
        facts.selected_moment.classification,
        GameReviewMomentClassification::PositiveHighlight {
            grade: PositiveHighlightGrade::Good,
            ..
        }
    ));
}

#[test]
fn automatically_selects_a_qualifying_positive_highlight() {
    let game = game("[FEN \"6k1/7p/8/8/8/3Q4/8/K5N1 w - - 0 1\"]\n\n1. Qxh7+ *");
    let before = engine("d3h7", 80);
    let after = engine("e8e7", -80);
    let human = human(&[("d3h7", 0.24), ("g1f3", 0.18)]);

    let facts = extract(
        &game,
        elo(1200),
        ReviewSide::White,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        }],
    )
    .expect("a qualifying Positive Highlight should enter automatic selection");

    assert!(matches!(
        facts.critical_moments[0].classification,
        GameReviewMomentClassification::PositiveHighlight {
            grade: PositiveHighlightGrade::Good,
            ..
        }
    ));
}

#[test]
fn does_not_select_a_non_best_move_for_the_best_lines_tactical_payoff() {
    let game = game("[FEN \"7k/5p2/8/3Q4/8/5KR1/1r6/8 b - - 0 1\"]\n\n1... Ra2 *");
    let before = EngineAnalysis {
        best_move: "f7f5".to_string(),
        evaluation: PositionEvaluation::Centipawns(20),
        principal_variation: vec![
            "f7f5".to_string(),
            "d5f5".to_string(),
            "b2b3".to_string(),
            "f3e2".to_string(),
            "b3g3".to_string(),
        ],
        depth: 16,
    };
    let after = engine("d5f5", 0);
    let human = human(&[("f7f5", 0.60), ("b2b3", 0.20), ("b2a2", 0.10)]);

    let facts = extract(
        &game,
        elo(1200),
        ReviewSide::Both,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        }],
    )
    .expect("complete legal evidence should extract facts");

    assert!(
        facts.critical_moments.is_empty(),
        "a non-best move must not inherit the best line's tactical payoff"
    );
}

#[test]
fn derives_great_only_when_objective_and_strong_elo_evidence_agree() {
    let game = game("[FEN \"6k1/7p/8/8/8/3Q4/8/K5N1 w - - 0 1\"]\n\n1. Qxh7+ *");
    let before = engine("d3h7", 80);
    let after = engine("e8e7", -80);
    let human = human(&[
        ("g1f3", 0.50),
        ("g1e2", 0.25),
        ("a1b1", 0.12),
        ("d3d1", 0.09),
        ("d3h7", 0.04),
    ]);

    let facts = extract_selected_moment(
        &game,
        elo(1200),
        &MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        },
    )
    .expect("complete legal evidence should classify the selected move");

    assert!(matches!(
        facts.selected_moment.classification,
        GameReviewMomentClassification::PositiveHighlight {
            grade: PositiveHighlightGrade::Great,
            ..
        }
    ));
}

#[test]
fn selected_terminal_move_reports_missing_post_move_evaluation_without_inventing_one() {
    let game = game("[FEN \"7k/8/5KQ1/8/8/8/8/8 w - - 0 1\"]\n\n1. Qg7# *");
    let before = EngineAnalysis {
        best_move: "g6g7".to_string(),
        evaluation: PositionEvaluation::MateIn(1),
        principal_variation: vec!["g6g7".to_string()],
        depth: 16,
    };
    let human = human(&[("g6g7", 0.61)]);

    let facts = extract_selected_moment(
        &game,
        elo(1200),
        &MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: AfterMoveEvidence::Terminal,
            human_before: &human,
        },
    )
    .expect("terminal selection should remain reviewable");

    assert_eq!(facts.selected_moment.objective.played_evaluation, None);
    assert!(matches!(
        facts.selected_moment.classification,
        GameReviewMomentClassification::PositiveHighlight { .. }
    ));
}

#[test]
fn extracts_a_forced_mate_conversion_theme_from_engine_evidence() {
    let game = game("[FEN \"8/8/8/8/8/5k2/q7/7K b - - 0 1\"]\n\n1... Qb2 *");
    let before = EngineAnalysis {
        best_move: "a2g2".to_string(),
        evaluation: PositionEvaluation::MateIn(1),
        principal_variation: vec!["a2g2".to_string()],
        depth: 16,
    };
    let after = engine("h1g1", -710);
    let human = human(&[("a2b2", 0.49), ("a2g2", 0.33)]);

    let facts = extract(
        &game,
        elo(1246),
        ReviewSide::Both,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        }],
    )
    .expect("forced-mate evidence should extract facts");

    assert_eq!(
        facts.critical_moments[0].teaching.themes,
        vec![TeachingTheme::ForcedMateConversion]
    );
    assert!(facts.critical_moments[0]
        .teaching
        .opening_principles
        .is_empty());
}

#[test]
fn serializes_objective_and_human_facts_as_separate_llm_inputs() {
    let game = game("1. a3 *");
    let before = engine("e2e4", 50);
    let after = engine("e7e5", 70);
    let human = human(&[("a2a3", 0.42), ("e2e4", 0.28)]);
    let facts = extract(
        &game,
        elo(1500),
        ReviewSide::Both,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: analyzed(&after),
            human_before: &human,
        }],
    )
    .expect("complete evidence should extract facts");

    let json = serde_json::to_value(&facts).expect("facts should serialize");
    assert_eq!(json["criticalMoments"][0]["objective"]["bestMove"], "e2e4");
    assert_eq!(
        json["criticalMoments"][0]["human"]["mostLikelyMove"],
        "a2a3"
    );
    assert_eq!(
        json["criticalMoments"][0]["objective"]["centipawnLoss"],
        120
    );
}

#[test]
fn accepts_explicit_terminal_evidence_for_the_final_move() {
    let game = game("[FEN \"7k/8/5KQ1/8/8/8/8/8 w - - 0 1\"]\n\n1. Qg7# *");
    let before = EngineAnalysis {
        best_move: "g6g7".to_string(),
        evaluation: PositionEvaluation::MateIn(1),
        principal_variation: vec!["g6g7".to_string()],
        depth: 16,
    };
    let human = human(&[("g6g7", 0.61)]);

    let facts = extract(
        &game,
        elo(1200),
        ReviewSide::Both,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: AfterMoveEvidence::Terminal,
            human_before: &human,
        }],
    )
    .expect("explicit terminal evidence completes the whole Game");

    assert!(facts.critical_moments.is_empty());
}

#[test]
fn rejects_an_evaluation_that_cannot_change_perspective() {
    let game = game("1. a3 *");
    let before = engine("e2e4", 0);
    let human = human(&[("a2a3", 0.3)]);

    let error = extract(
        &game,
        elo(1500),
        ReviewSide::Both,
        &[MoveEvidence {
            ply: 1,
            engine_before: &before,
            after_move: AfterMoveEvidence::Analyzed(PositionEvaluation::Centipawns(i32::MIN)),
            human_before: &human,
        }],
    )
    .expect_err("unrepresentable perspective changes must be rejected");

    assert!(matches!(
        error,
        RuleExtractorError::InvalidEvaluation { ply: 1 }
    ));
}

fn game(pgn: &str) -> Game {
    parse_pgn(pgn).expect("test PGN should be legal")
}

fn engine(best_move: &str, centipawns: i32) -> EngineAnalysis {
    EngineAnalysis {
        best_move: best_move.to_string(),
        evaluation: PositionEvaluation::Centipawns(centipawns),
        principal_variation: vec![best_move.to_string()],
        depth: 16,
    }
}

fn analyzed(engine: &EngineAnalysis) -> AfterMoveEvidence {
    AfterMoveEvidence::Analyzed(engine.evaluation)
}

fn human(candidates: &[(&str, f64)]) -> HumanMovePrediction {
    HumanMovePrediction {
        candidates: candidates
            .iter()
            .enumerate()
            .map(|(index, (uci, probability))| HumanMoveCandidate {
                uci: (*uci).to_string(),
                probability: *probability,
                rank: index + 1,
            })
            .collect(),
        win_probability: Some(0.5),
    }
}

fn elo(rating: u16) -> EloProfile {
    EloProfile::try_from(rating).expect("test Elo should be valid")
}
