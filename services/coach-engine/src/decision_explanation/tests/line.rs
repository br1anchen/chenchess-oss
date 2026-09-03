use crate::{
    decision_explanation::{
        explain_decision, DecisionExplanationBuild, DecisionExplanationDiagnostic,
        DecisionExplanationInput,
    },
    review_session_contract::{
        build_position_snapshot, AtomicChessFactData, CandidateEvidence, CriticalMomentId,
        CurriculumLearningConcept as Concept, ExplanationPathAttribution,
        GameReviewMomentProvenance, LearningTrackKey, ProofCapability, SemanticComparisonRelation,
    },
};

use super::{
    alternative, collected_concepts, game_ref, improvement_for, player_evidence,
    positive_highlight, ranked, single_pv_build, SinglePvFixture,
};

#[test]
fn castling_and_en_passant_contribute_proof_valid_realizations() {
    let cases = [
        ("4k3/8/8/8/8/8/8/4K2R w K - 0 1", "e1g1", Concept::Castling),
        (
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
            "e5d6",
            Concept::EnPassant,
        ),
    ];

    for (fen, root, expected) in cases {
        let fixture = SinglePvFixture {
            fen,
            best_root: root,
            best_line: &[root],
            best_score: 300,
            player_root: root,
            player_line: &[root],
            player_score: 300,
            classification: positive_highlight(),
        };
        assert!(collected_concepts(&fixture)[root].contains(&expected));
        let (explanation, tracks) = single_pv_build(fixture);

        assert_eq!(tracks.len(), 1);
        assert_eq!(explanation.selected_paths.len(), 1);
    }
}

#[test]
fn castling_neither_captures_its_rook_nor_proves_a_hanging_piece() {
    let fixture = SinglePvFixture {
        fen: "4k3/8/8/8/8/8/8/4K2R w K - 0 1",
        best_root: "e1g1",
        best_line: &["e1g1"],
        best_score: 300,
        player_root: "e1g1",
        player_line: &["e1g1"],
        player_score: 300,
        classification: positive_highlight(),
    };
    assert!(!collected_concepts(&fixture)["e1g1"].contains(&Concept::HangingPiece));

    let (explanation, tracks) = single_pv_build(fixture);
    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: Concept::Castling,
        }
    );
    let [candidate] = explanation.candidates.as_slice() else {
        panic!("the shared best and Player move should build one candidate");
    };
    let [step] = candidate.line_steps.as_slice() else {
        panic!("the castling fixture should retain one line step");
    };
    assert!(step.captured.is_none());
    assert!(!explanation.facts.iter().any(|fact| {
        matches!(
            &fact.data,
            AtomicChessFactData::MaterialChanged { step_ref, .. }
                if step_ref == &step.step_ref
        )
    }));
}

#[test]
fn line_family_preserves_missed_conceded_and_reinforcement_candidate_ownership() {
    let fen = "r2qk2r/pppppppp/2n2n2/8/8/2N2N2/PPPQPPPP/R3K2R w KQkq - 0 1";
    let cases = [
        (
            "e1g1",
            &["e1g1"][..],
            "e1f1",
            &["e1f1"][..],
            improvement_for("e1g1"),
            ExplanationPathAttribution::MissedBest,
        ),
        (
            "e1f1",
            &["e1f1"][..],
            "e1g1",
            &["e1g1"][..],
            improvement_for("e1f1"),
            ExplanationPathAttribution::ConcededRefutation,
        ),
        (
            "e1g1",
            &["e1g1"][..],
            "e1g1",
            &["e1g1"][..],
            positive_highlight(),
            ExplanationPathAttribution::Reinforcement,
        ),
    ];

    for (best, best_line, player, player_line, classification, attribution) in cases {
        let fixture = SinglePvFixture {
            fen,
            best_root: best,
            best_line,
            best_score: 300,
            player_root: player,
            player_line,
            player_score: 100,
            classification,
        };
        assert!(collected_concepts(&fixture)["e1g1"].contains(&Concept::Castling));
        let (explanation, tracks) = single_pv_build(fixture);
        assert_eq!(tracks.len(), 1);
        let path = &explanation.selected_paths[0];
        assert_eq!(path.attribution, attribution);
        let owner = explanation
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_ref == path.candidate_ref)
            .unwrap();
        assert_eq!(owner.root_move_uci, "e1g1");
    }
}

#[test]
fn counter_check_uses_the_exact_checker_transition() {
    let fixture = SinglePvFixture {
        fen: "4r2k/8/8/8/Q7/8/8/4K3 w - - 0 1",
        best_root: "a4e8",
        best_line: &["a4e8"],
        best_score: 500,
        player_root: "a4e8",
        player_line: &["a4e8"],
        player_score: 500,
        classification: positive_highlight(),
    };
    assert!(collected_concepts(&fixture)["a4e8"].contains(&Concept::CounterCheck));
    let (explanation, tracks) = single_pv_build(fixture);

    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: Concept::CrushingAdvantage,
        }
    );
    assert_eq!(explanation.selected_paths.len(), 1);
}

#[test]
fn sequential_line_concepts_name_their_causal_and_payoff_steps() {
    let cases = [
        (
            "6k1/7p/8/8/8/3B1N2/8/4K3 w - - 0 1",
            &["d3h7", "g8h7", "f3g5"][..],
            Concept::GreekGift,
        ),
        (
            "6k1/7p/8/8/8/8/8/3QK2R w - - 0 1",
            &["h1h7", "g8h7", "d1h5"][..],
            Concept::Sacrifice,
        ),
        (
            "3qr2k/1N6/8/8/8/8/8/3R2K1 w - - 0 1",
            &["b7d8", "e8d8", "d1d8"][..],
            Concept::Intermezzo,
        ),
        (
            "3qk3/8/7p/6B1/8/8/8/4K3 w - - 0 1",
            &["g5d8", "e8d8"][..],
            Concept::Desperado,
        ),
        (
            "k3q3/8/8/8/8/8/4B3/4R2K w - - 0 1",
            &["e2c4", "a8b8", "e1e8"][..],
            Concept::Clearance,
        ),
        (
            "q6k/2b5/8/8/8/8/2N5/R5K1 w - - 0 1",
            &["c2e3", "c7a5", "a1a5"][..],
            Concept::Interference,
        ),
        (
            "3q4/7k/8/8/5N2/8/8/R6K w - - 0 1",
            &["a1a8", "d8d5", "f4d5"][..],
            Concept::Attraction,
        ),
        (
            "7k/5p2/3q4/8/2B5/8/8/R6K w - - 0 1",
            &["a1a8", "d6d8", "c4f7"][..],
            Concept::Deflection,
        ),
        (
            "7k/5p2/8/8/2B5/8/8/R6K w - - 0 1",
            &["a1a8", "h8g7", "c4f7"][..],
            Concept::CollinearMove,
        ),
    ];

    for (fen, line, expected) in cases {
        let root = line[0];
        let fixture = SinglePvFixture {
            fen,
            best_root: root,
            best_line: line,
            best_score: 500,
            player_root: root,
            player_line: line,
            player_score: 500,
            classification: positive_highlight(),
        };
        assert!(
            collected_concepts(&fixture)[root].contains(&expected),
            "{expected:?}"
        );
        let (explanation, tracks) = single_pv_build(fixture);
        assert_eq!(tracks.len(), 1);
        let proof = &explanation.selected_paths[0].concept_validation_proof;
        assert!(explanation.candidates[0]
            .line_steps
            .iter()
            .any(|step| step.step_ref == proof.causal_step_ref));
        assert!(explanation.candidates[0]
            .line_steps
            .iter()
            .any(|step| step.step_ref == proof.payoff_step_ref));
    }
}

#[test]
fn quiet_move_uses_complete_absence_facts_and_a_later_payoff() {
    let (explanation, tracks) = single_pv_build(SinglePvFixture {
        fen: "7k/8/8/8/8/8/6N1/R5K1 w - - 0 1",
        best_root: "g2f4",
        best_line: &["g2f4", "h8g8", "a1a8"],
        best_score: 400,
        player_root: "g2f4",
        player_line: &["g2f4", "h8g8", "a1a8"],
        player_score: 400,
        classification: positive_highlight(),
    });

    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: Concept::QuietMove,
        }
    );
    let proof = &explanation.selected_paths[0].concept_validation_proof;
    assert_ne!(proof.causal_step_ref, proof.payoff_step_ref);
    assert!(proof.supporting_fact_refs.iter().any(|reference| {
        explanation.facts.iter().any(|fact| {
            &fact.fact_ref == reference
                && matches!(fact.data, AtomicChessFactData::MaterialInventory { .. })
        })
    }));
}

#[test]
fn defensive_move_is_not_selected_from_single_pv_evidence() {
    let build = explain_decision(DecisionExplanationInput {
        game_ref: game_ref(),
        critical_moment_id: crate::review_session_contract::CriticalMomentId::try_from(
            "review-moment:defensive-single-pv".to_string(),
        )
        .unwrap(),
        position_snapshot: build_position_snapshot("4r2k/8/8/8/8/8/8/2B1K3 w - - 0 1", &[])
            .unwrap(),
        classification: improvement_for("c1e3"),
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: "e1d1".to_string(),
        candidate_evidence: CandidateEvidence::SinglePv {
            authoritative: ranked(1, "c1e3", 100, &["c1e3"]),
            player_move: player_evidence("e1d1", &["e1d1"], 0),
        },
    })
    .unwrap();

    match build {
        DecisionExplanationBuild::Durable {
            projected_tracks, ..
        } => assert!(projected_tracks.iter().all(|track| {
            track.key
                != LearningTrackKey::Curriculum {
                    concept: Concept::DefensiveMove,
                }
        })),
        DecisionExplanationBuild::Abstained { diagnostics } => assert_eq!(
            diagnostics,
            vec![DecisionExplanationDiagnostic::NoProofValidConcept]
        ),
    }
}

#[test]
fn defensive_move_requires_refutes_coverage_for_every_retained_alternative() {
    let evidence = CandidateEvidence::MultiPv {
        authoritative_single_pv: ranked(1, "d2e2", 300, &["d2e2"]),
        requested_count: 3,
        ranked_alternatives: vec![
            alternative(2, "e1d1", 200, &["e1d1", "e8e1"]),
            alternative(3, "e1f1", 300, &["e1f1", "e8e1"]),
        ],
        player_move: player_evidence("e1d1", &["e1d1", "e8e1"], 100),
    };
    let build = explain_decision(DecisionExplanationInput {
        game_ref: game_ref(),
        critical_moment_id: CriticalMomentId::try_from(
            "review-moment:defensive-multipv".to_string(),
        )
        .unwrap(),
        position_snapshot: build_position_snapshot("4r2k/8/8/8/8/8/3R4/4K3 w - - 0 1", &[])
            .unwrap(),
        classification: improvement_for("d2e2"),
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: "e1d1".to_string(),
        candidate_evidence: evidence,
    })
    .unwrap();
    let DecisionExplanationBuild::Durable {
        explanation,
        projected_tracks,
        ..
    } = build
    else {
        panic!("complete comparative refutation evidence must produce a defensive path");
    };

    assert_eq!(
        projected_tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: Concept::DefensiveMove,
        }
    );
    assert_eq!(explanation.capability, ProofCapability::SemanticPreference);
    let comparisons = &explanation
        .preference
        .as_ref()
        .unwrap()
        .semantic_comparisons;
    assert_eq!(comparisons.len(), 2);
    assert!(comparisons
        .iter()
        .all(|comparison| comparison.relation == SemanticComparisonRelation::Refutes));
}

#[test]
fn every_selected_line_path_is_candidate_local_and_has_a_closed_outcome() {
    let (explanation, _) = single_pv_build(SinglePvFixture {
        fen: "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
        best_root: "e5d6",
        best_line: &["e5d6"],
        best_score: 300,
        player_root: "e5d6",
        player_line: &["e5d6"],
        player_score: 300,
        classification: positive_highlight(),
    });
    let path = &explanation.selected_paths[0];
    let owner = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == path.candidate_ref)
        .unwrap();

    assert!(!path.outcome_refs.is_empty());
    assert!(path
        .concept_validation_proof
        .supporting_fact_refs
        .iter()
        .all(|reference| owner.fact_refs.contains(reference)));
    assert_eq!(explanation.capability, ProofCapability::ValidationOnly);
}
