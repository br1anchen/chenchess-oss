use super::*;

#[test]
fn back_rank_mate_selects_the_specific_terminal_path_and_suppresses_broad_parents() {
    let (explanation, tracks) = single_pv_build(SinglePvFixture {
        fen: "6k1/5ppp/8/8/8/8/8/R6K w - - 0 1",
        best_root: "a1a8",
        best_line: &["a1a8"],
        best_score: 10_000,
        player_root: "h1g1",
        player_line: &["h1g1"],
        player_score: 0,
        classification: improvement_for("a1a8"),
    });

    assert_eq!(tracks.len(), 1);
    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::BackRankMate,
        }
    );
    assert_eq!(explanation.selected_paths.len(), 1);
    let owner = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == explanation.selected_paths[0].candidate_ref)
        .unwrap();
    assert!(owner.outcomes.iter().any(|outcome| matches!(
        outcome.data,
        SemanticOutcomeData::TerminalStateReached {
            result: crate::review_session_contract::DecisionTerminalState::Checkmate,
            ..
        }
    )));
}

#[test]
fn smothered_mate_is_proved_from_checker_occupancy_mobility_and_terminal_facts() {
    let (explanation, tracks) = single_pv_build(SinglePvFixture {
        fen: "6rk/6pp/7N/8/8/8/8/K7 w - - 0 1",
        best_root: "h6f7",
        best_line: &["h6f7"],
        best_score: 10_000,
        player_root: "a1b1",
        player_line: &["a1b1"],
        player_score: 0,
        classification: improvement_for("h6f7"),
    });

    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::SmotheredMate,
        }
    );
    let path = &explanation.selected_paths[0];
    let supporting = &path.concept_validation_proof.supporting_fact_refs;
    assert!(explanation.facts.iter().any(|fact| {
        supporting.contains(&fact.fact_ref)
            && matches!(fact.data, AtomicChessFactData::Checkers { .. })
    }));
    assert!(explanation.facts.iter().any(|fact| {
        supporting.contains(&fact.fact_ref)
            && matches!(fact.data, AtomicChessFactData::LegalDestinations { .. })
    }));
    assert!(explanation.facts.iter().any(|fact| {
        supporting.contains(&fact.fact_ref)
            && matches!(
                fact.data,
                AtomicChessFactData::TerminalPosition {
                    state: crate::review_session_contract::DecisionTerminalState::Checkmate,
                    ..
                }
            )
    }));
}

#[test]
fn mating_family_contributes_matches_while_selected_paths_preserve_attribution() {
    let (reinforced, reinforced_tracks) = single_pv_build(SinglePvFixture {
        fen: "6k1/5ppp/8/8/8/8/8/R6K w - - 0 1",
        best_root: "a1a8",
        best_line: &["a1a8"],
        best_score: 10_000,
        player_root: "a1a8",
        player_line: &["a1a8"],
        player_score: 10_000,
        classification: positive_highlight(),
    });
    assert_eq!(
        reinforced.selected_paths[0].attribution,
        ExplanationPathAttribution::Reinforcement
    );
    assert_eq!(
        reinforced_tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::BackRankMate,
        }
    );

    let conceded_fixture = SinglePvFixture {
        fen: "r6k/8/8/8/8/8/1P3PPP/6K1 w - - 0 1",
        best_root: "g1f1",
        best_line: &["g1f1"],
        best_score: 0,
        player_root: "b2b3",
        player_line: &["b2b3", "a8a1"],
        player_score: -10_000,
        classification: improvement_for("g1f1"),
    };
    assert!(collected_concepts(&conceded_fixture)["b2b3"]
        .contains(&crate::review_session_contract::CurriculumLearningConcept::BackRankMate));
    let (conceded, conceded_tracks) = single_pv_build(conceded_fixture);
    assert_eq!(
        conceded.selected_paths[0].attribution,
        ExplanationPathAttribution::ConcededRefutation
    );
    assert_eq!(
        conceded_tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::RookEndgame,
        }
    );
    let owner = conceded
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == conceded.selected_paths[0].candidate_ref)
        .unwrap();
    assert!(owner
        .origins
        .contains(&crate::review_session_contract::DecisionCandidateOrigin::PlayerPlayed));
}

#[test]
fn canonical_mating_fixtures_make_every_registry_concept_observably_reachable() {
    use crate::review_session_contract::CurriculumLearningConcept as Concept;

    let fixtures = [
        (
            "8/4N1pk/8/4R3/8/8/8/7K w - - 0 1",
            "e5h5",
            Concept::AnastasiaMate,
        ),
        (
            "7k/4R3/5N2/8/8/8/8/7K w - - 0 1",
            "e7h7",
            Concept::ArabianMate,
        ),
        (
            "6k1/5ppp/3R4/8/8/8/8/7K w - - 0 1",
            "d6d8",
            Concept::BackRankMate,
        ),
        (
            "6k1/8/7Q/8/6B1/8/8/7K w - - 0 1",
            "g4e6",
            Concept::BalestraMate,
        ),
        (
            "5rk1/2R4R/8/8/8/8/8/7K w - - 0 1",
            "c7g7",
            Concept::BlindSwineMate,
        ),
        (
            "2kr4/3p4/8/8/2B2B2/8/8/7K w - - 1 1",
            "c4a6",
            Concept::BodenMate,
        ),
        (
            "7k/7p/8/6N1/8/8/8/6RK w - - 1 1",
            "g5f7",
            Concept::CornerMate,
        ),
        (
            "7k/7p/8/3B4/8/8/8/4B2K w - - 0 1",
            "e1c3",
            Concept::DoubleBishopMate,
        ),
        (
            "8/8/7Q/8/6p1/5qk1/8/6K1 w - - 0 1",
            "h6h2",
            Concept::DovetailMate,
        ),
        (
            "5rkr/8/8/8/8/8/8/1Q4K1 w - - 0 1",
            "b1g6",
            Concept::EpauletteMate,
        ),
        (
            "6R1/4kp2/5N2/4P3/8/8/8/7K w - - 0 1",
            "g8e8",
            Concept::HookMate,
        ),
        (
            "8/8/2R5/k7/2Q5/8/8/7K w - - 0 1",
            "c6a6",
            Concept::KillBoxMate,
        ),
        (
            "5rk1/5p1p/8/8/8/8/1B6/4R2K w - - 0 1",
            "e1g1",
            Concept::PillsburysMate,
        ),
        (
            "7k/7p/5p2/8/3B4/6R1/8/7K w - - 0 1",
            "d4f6",
            Concept::MorphysMate,
        ),
        (
            "4k3/5p2/8/6B1/8/8/8/3R3K w - - 0 1",
            "d1d8",
            Concept::OperaMate,
        ),
        (
            "3r1r2/4k3/R7/8/2Q5/8/8/7K w - - 0 1",
            "c4e6",
            Concept::SwallowstailMate,
        ),
        (
            "3R4/4kp2/8/8/3Q4/8/8/7K w - - 0 1",
            "d4d6",
            Concept::TriangleMate,
        ),
        (
            "4k3/2R5/4NK2/8/8/8/8/8 w - - 0 1",
            "c7e7",
            Concept::VukovicMate,
        ),
        (
            "6rk/6pp/8/4N3/8/8/8/7K w - - 0 1",
            "e5f7",
            Concept::SmotheredMate,
        ),
        (
            "7k/4B3/6KN/8/8/8/8/8 w - - 0 1",
            "e7f6",
            Concept::KnightAndBishopMate,
        ),
        (
            "7k/8/5KQ1/8/8/8/8/8 w - - 0 1",
            "g6g7",
            Concept::PieceCheckmates,
        ),
        (
            "7k/8/5KQ1/8/8/8/8/N7 w - - 0 1",
            "g6g7",
            Concept::CheckmatePatterns,
        ),
        (
            "6rk/7b/7N/8/8/8/8/6RK w - - 0 1",
            "h6f7",
            Concept::Checkmate,
        ),
    ];

    for (fen, root, expected) in fixtures {
        let fixture = SinglePvFixture {
            fen,
            best_root: root,
            best_line: &[root],
            best_score: 10_000,
            player_root: root,
            player_line: &[root],
            player_score: 10_000,
            classification: positive_highlight(),
        };
        assert!(
            collected_concepts(&fixture)[root].contains(&expected),
            "{root} from {fen}: {expected:?}"
        );
        let (_, tracks) = single_pv_build(fixture);
        assert_eq!(tracks.len(), 1, "{root} from {fen}");
    }
}
