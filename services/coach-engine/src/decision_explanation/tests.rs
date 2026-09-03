use crate::{
    decision_explanation::{
        explain_decision,
        knowledge::{
            CompiledKnowledgeGraph, KnowledgeConcept, KnowledgeEdge, KnowledgeEntityRef,
            KnowledgeRelationship, RecognitionRule,
        },
        validate_decision_explanation, DecisionExplanationBuild, DecisionExplanationInput,
    },
    review_session_contract::{
        build_position_snapshot, ArtifactDigest, AtomicChessFactData, CandidateEvidence,
        CandidateGap, Color, CriticalMomentId, DecisionEngineProvenance, EngineCandidateEvidence,
        EngineEvaluation, ExplanationPathAttribution, GameRef, GameReviewMomentClassification,
        GameReviewMomentProvenance, ImprovementCorrection, ImprovementOutcome, KnowledgeNodeRef,
        KnowledgeRuleRef, LearningTrackKey, ObjectiveExcellenceReason, PieceAtSquare, PieceRole,
        PlayerMoveEvidence, PositiveHighlightAchievement, PositiveHighlightGrade,
        PositiveHighlightQualification, PositiveHighlightQualificationReason, ProofCapability,
        RankedAlternativeEvidence, SemanticOutcomeData, Square,
    },
};

const FORK_FEN: &str = "r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1";

#[path = "tests/generation.rs"]
mod generation_tests;

#[test]
fn inconsistent_multipv_evidence_abstains_with_a_structured_diagnostic() {
    let mut evidence = multi_pv_evidence();
    let CandidateEvidence::MultiPv {
        requested_count, ..
    } = &mut evidence
    else {
        unreachable!("fixture is MultiPV");
    };
    *requested_count = 4;

    let build = explain_decision(DecisionExplanationInput {
        game_ref: GameRef::try_from(format!("sha256:{}", "1".repeat(64))).unwrap(),
        critical_moment_id: CriticalMomentId::try_from("review-moment:fork".to_string()).unwrap(),
        position_snapshot: build_position_snapshot(FORK_FEN, &[]).unwrap(),
        classification: improvement(),
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: "e1e2".to_string(),
        candidate_evidence: evidence,
    })
    .unwrap();

    assert_eq!(
        build,
        DecisionExplanationBuild::Abstained {
            diagnostics: vec![super::DecisionExplanationDiagnostic::CandidateEvidenceRejected],
        }
    );
}

/// The Player often plays the engine's second or third choice. That candidate is both
/// engine-ranked and Player-played, and it must still report the SinglePV absolute the
/// Moment measured for the move the Player actually made — a gap alone would leave the
/// Moment with no number for its own move (ADR 0041).
#[test]
fn a_player_move_that_is_also_a_ranked_alternative_keeps_its_single_pv_absolute() {
    let CandidateEvidence::MultiPv {
        authoritative_single_pv,
        requested_count,
        ranked_alternatives,
        ..
    } = multi_pv_evidence()
    else {
        unreachable!("fixture is MultiPV");
    };
    let played = ranked_alternatives[0].root_move_uci.clone();

    let build = explain_decision(DecisionExplanationInput {
        game_ref: game_ref(),
        critical_moment_id: CriticalMomentId::try_from("review-moment:played-rank-two".to_string())
            .unwrap(),
        position_snapshot: build_position_snapshot(FORK_FEN, &[]).unwrap(),
        classification: improvement(),
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: played.clone(),
        candidate_evidence: CandidateEvidence::MultiPv {
            authoritative_single_pv,
            requested_count,
            ranked_alternatives: ranked_alternatives.clone(),
            player_move: player_evidence(&played, &["b5d6", "e8d7"], 275),
        },
    })
    .unwrap();
    let DecisionExplanationBuild::Durable { explanation, .. } = build else {
        panic!("a Player-played ranked alternative must still build a durable explanation");
    };

    let candidate = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.root_move_uci == played)
        .expect("the played root should be one candidate, not two");
    assert_eq!(
        candidate.origins,
        vec![
            crate::review_session_contract::DecisionCandidateOrigin::EngineRanked,
            crate::review_session_contract::DecisionCandidateOrigin::PlayerPlayed,
        ]
    );
    assert_eq!(candidate.assessment.rank, Some(2));
    assert_eq!(
        candidate.assessment.score,
        crate::review_session_contract::EngineAssessmentScore::Absolute {
            evaluation: evaluation(275),
        }
    );
}

#[test]
fn durable_fork_round_trip_preserves_content_identities_and_recompute_evidence() {
    let (explanation, _) = canonical_build();
    let encoded = serde_json::to_vec(&explanation).unwrap();
    let decoded = serde_json::from_slice(&encoded).unwrap();

    assert_eq!(decoded, explanation);
    validate_decision_explanation(&decoded).unwrap();
    assert!(matches!(
        decoded.candidate_evidence,
        CandidateEvidence::MultiPv { .. }
    ));

    let (rebuilt, _) = canonical_build();
    assert_eq!(
        rebuilt.decision_explanation_ref,
        explanation.decision_explanation_ref
    );
    assert_eq!(rebuilt.candidates, explanation.candidates);
    assert_eq!(rebuilt.selected_paths, explanation.selected_paths);
}

#[test]
fn cross_candidate_fact_reference_fails_validation() {
    let (mut explanation, _) = canonical_build();
    let selected_candidate = explanation.selected_paths[0].candidate_ref.clone();
    let foreign_candidate = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref != selected_candidate)
        .expect("canonical MultiPV must retain an alternative candidate")
        .candidate_ref
        .clone();
    explanation.selected_paths[0].candidate_ref = foreign_candidate.clone();
    explanation.selected_paths[0]
        .concept_validation_proof
        .candidate_ref = foreign_candidate;

    assert_eq!(
        validate_decision_explanation(&explanation),
        Err(super::DecisionExplanationContractError::InvalidProof(
            "selected path contains a cross-candidate or malformed proof reference"
        ))
    );
}

#[test]
fn unresolved_knowledge_reference_fails_validation() {
    let (mut explanation, _) = canonical_build();
    explanation.selected_paths[0]
        .knowledge_activation
        .concept_node_ref =
        KnowledgeNodeRef::try_from(format!("sha256:{}", "f".repeat(64))).unwrap();

    assert_eq!(
        validate_decision_explanation(&explanation),
        Err(super::DecisionExplanationContractError::InvalidProof(
            "selected path has unresolved Chess Knowledge"
        ))
    );
}

#[test]
fn caller_authored_capability_fails_validation() {
    let (mut explanation, _) = canonical_build();
    explanation.capability = ProofCapability::SemanticPreference;

    assert_eq!(
        validate_decision_explanation(&explanation),
        Err(super::DecisionExplanationContractError::InvalidProof(
            "Proof Capability must be derived from complete comparison coverage"
        ))
    );
}

#[test]
fn malformed_selected_proof_fails_validation() {
    let (mut explanation, _) = canonical_build();
    explanation.selected_paths[0].outcome_refs.clear();

    assert_eq!(
        validate_decision_explanation(&explanation),
        Err(super::DecisionExplanationContractError::InvalidProof(
            "selected path contains a cross-candidate or malformed proof reference"
        ))
    );
}

#[test]
fn illegal_persisted_candidate_evidence_fails_recompute_validation() {
    let (mut explanation, _) = canonical_build();
    let CandidateEvidence::MultiPv {
        authoritative_single_pv,
        ..
    } = &mut explanation.candidate_evidence
    else {
        panic!("the canonical fixture uses MultiPV evidence");
    };
    authoritative_single_pv.variation[1] = "e8e9".to_string();

    assert_eq!(
        validate_decision_explanation(&explanation),
        Err(
            super::DecisionExplanationContractError::InvalidCandidateLine {
                candidate: "b5c7".to_string(),
                index: 1,
            }
        )
    );
}

#[test]
fn persisted_candidates_must_cover_every_candidate_evidence_root() {
    let (mut explanation, _) = canonical_build();
    let selected = explanation.selected_paths[0].candidate_ref.clone();
    let removable = explanation
        .candidates
        .iter()
        .position(|candidate| {
            candidate.candidate_ref != selected
                && !candidate.origins.contains(
                    &crate::review_session_contract::DecisionCandidateOrigin::PlayerPlayed,
                )
        })
        .expect("canonical MultiPV retains a nonselected ranked alternative");
    explanation.candidates.remove(removable);

    assert_eq!(
        validate_decision_explanation(&explanation),
        Err(super::DecisionExplanationContractError::InvalidProof(
            "persisted Decision Candidates do not reproduce from Candidate Evidence"
        ))
    );
}

#[test]
fn knowledge_hierarchies_reject_cycles_while_descriptive_relationships_allow_them() {
    let fork = crate::review_session_contract::CurriculumLearningConcept::Fork;
    let knowledge = KnowledgeConcept::Curriculum(fork);
    let recognition = RecognitionRule::CurriculumV1(fork);
    let concept = KnowledgeNodeRef::from_content(&knowledge);
    let rule = KnowledgeRuleRef::from_content(&recognition);
    let recognized = KnowledgeEdge {
        source: KnowledgeEntityRef::Concept(concept.clone()),
        target: KnowledgeEntityRef::Rule(rule),
        relationship: KnowledgeRelationship::RecognizedBy,
    };
    let self_edge = |relationship| KnowledgeEdge {
        source: KnowledgeEntityRef::Concept(concept.clone()),
        target: KnowledgeEntityRef::Concept(concept.clone()),
        relationship,
    };

    assert!(CompiledKnowledgeGraph::compile(
        [knowledge],
        [recognition],
        &[
            recognized.clone(),
            self_edge(KnowledgeRelationship::Refines)
        ],
    )
    .is_err());
    assert!(CompiledKnowledgeGraph::compile(
        [knowledge],
        [recognition],
        &[
            recognized.clone(),
            self_edge(KnowledgeRelationship::Prerequisite)
        ],
    )
    .is_err());
    assert!(CompiledKnowledgeGraph::compile(
        [knowledge],
        [recognition],
        &[
            recognized.clone(),
            self_edge(KnowledgeRelationship::Related),
            self_edge(KnowledgeRelationship::Counters),
        ],
    )
    .is_ok());
}

#[test]
fn queen_promotion_contributes_a_valid_match_before_crushing_outcome_selection() {
    let fixture = SinglePvFixture {
        fen: "k7/4P3/8/8/8/8/8/7K w - - 0 1",
        best_root: "e7e8q",
        best_line: &["e7e8q"],
        best_score: 900,
        player_root: "h1h2",
        player_line: &["h1h2"],
        player_score: 0,
        classification: improvement_for("e7e8q"),
    };
    assert!(collected_concepts(&fixture)["e7e8q"]
        .contains(&crate::review_session_contract::CurriculumLearningConcept::Promotion));
    let (explanation, tracks) = single_pv_build(fixture);

    let path = explanation.selected_paths.first().unwrap();
    assert_eq!(path.attribution, ExplanationPathAttribution::MissedBest);
    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::CrushingAdvantage,
        }
    );
    let owner = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == path.candidate_ref)
        .unwrap();
    assert!(path
        .concept_validation_proof
        .supporting_fact_refs
        .iter()
        .all(|reference| owner.fact_refs.contains(reference)));
}

#[test]
fn underpromotion_is_distinct_and_positive_highlights_keep_reinforcement_attribution() {
    let fixture = SinglePvFixture {
        fen: "k7/4P3/8/8/8/8/8/7K w - - 0 1",
        best_root: "e7e8n",
        best_line: &["e7e8n"],
        best_score: 500,
        player_root: "e7e8n",
        player_line: &["e7e8n"],
        player_score: 500,
        classification: positive_highlight(),
    };
    let concepts = &collected_concepts(&fixture)["e7e8n"];
    assert!(concepts
        .contains(&crate::review_session_contract::CurriculumLearningConcept::Underpromotion));
    assert!(
        concepts.contains(&crate::review_session_contract::CurriculumLearningConcept::Promotion)
    );
    let (explanation, tracks) = single_pv_build(fixture);

    assert_eq!(
        explanation.selected_paths[0].attribution,
        ExplanationPathAttribution::Reinforcement
    );
    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::Equality,
        }
    );
}

#[test]
fn player_line_concept_keeps_conceded_refutation_attribution_when_best_line_abstains() {
    let fixture = SinglePvFixture {
        fen: "k7/4P3/8/8/8/8/8/7K w - - 0 1",
        best_root: "h1h2",
        best_line: &["h1h2"],
        best_score: 100,
        player_root: "e7e8q",
        player_line: &["e7e8q"],
        player_score: 0,
        classification: improvement_for("h1h2"),
    };
    assert!(collected_concepts(&fixture)["e7e8q"]
        .contains(&crate::review_session_contract::CurriculumLearningConcept::Promotion));
    let (explanation, tracks) = single_pv_build(fixture);

    assert_eq!(
        explanation.selected_paths[0].attribution,
        ExplanationPathAttribution::ConcededRefutation
    );
    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::CrushingAdvantage,
        }
    );
}

#[test]
fn engine_magnitude_without_a_typed_transition_abstains() {
    let build = explain_decision(DecisionExplanationInput {
        game_ref: game_ref(),
        critical_moment_id: CriticalMomentId::try_from(
            "review-moment:engine-magnitude-only".to_string(),
        )
        .unwrap(),
        position_snapshot: build_position_snapshot("k7/8/8/8/8/8/2N5/7K w - - 0 1", &[]).unwrap(),
        classification: improvement_for("c2b4"),
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: "h1h2".to_string(),
        candidate_evidence: CandidateEvidence::SinglePv {
            authoritative: ranked(1, "c2b4", 10_000, &["c2b4"]),
            player_move: player_evidence("h1h2", &["h1h2"], -10_000),
        },
    })
    .unwrap();

    assert_eq!(
        build,
        DecisionExplanationBuild::Abstained {
            diagnostics: vec![super::DecisionExplanationDiagnostic::NoProofValidConcept],
        }
    );
}

#[test]
fn conventional_material_transition_drives_outcome_concept_independent_of_engine_score() {
    let (explanation, tracks) = single_pv_build(SinglePvFixture {
        fen: "k7/rbn4p/5N2/8/8/8/4Q1BR/7K w - - 0 1",
        best_root: "f6h7",
        best_line: &["f6h7"],
        best_score: -10_000,
        player_root: "h1g1",
        player_line: &["h1g1"],
        player_score: 10_000,
        classification: improvement_for("f6h7"),
    });

    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::Advantage,
        }
    );
    let selected = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == explanation.selected_paths[0].candidate_ref)
        .unwrap();
    assert!(selected.outcomes.iter().any(|outcome| matches!(
        outcome.data,
        SemanticOutcomeData::MaterialBalanceChanged {
            conventional_value_delta: 1,
            ..
        }
    )));
}

#[test]
fn hanging_piece_uses_the_same_candidate_owned_curriculum_path() {
    let (explanation, tracks) = single_pv_build(SinglePvFixture {
        fen: "k7/7r/5N2/8/8/8/8/6K1 w - - 0 1",
        best_root: "f6h7",
        best_line: &["f6h7"],
        best_score: 500,
        player_root: "g1f1",
        player_line: &["g1f1"],
        player_score: 0,
        classification: improvement_for("f6h7"),
    });

    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::HangingPiece,
        }
    );
    let path = &explanation.selected_paths[0];
    let owner = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == path.candidate_ref)
        .unwrap();
    assert!(path
        .concept_validation_proof
        .supporting_fact_refs
        .iter()
        .all(|reference| owner.fact_refs.contains(reference)));
}

#[test]
fn attack_family_preserves_conceded_refutation_and_reinforcement_attribution() {
    let (conceded, conceded_tracks) = single_pv_build(SinglePvFixture {
        fen: "k7/7r/5N2/8/8/8/8/6K1 w - - 0 1",
        best_root: "g1f1",
        best_line: &["g1f1"],
        best_score: 100,
        player_root: "f6h7",
        player_line: &["f6h7"],
        player_score: 0,
        classification: improvement_for("g1f1"),
    });
    assert_eq!(
        conceded.selected_paths[0].attribution,
        ExplanationPathAttribution::ConcededRefutation
    );
    assert_eq!(
        conceded_tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::HangingPiece,
        }
    );

    let (reinforced, reinforced_tracks) = single_pv_build(SinglePvFixture {
        fen: "k7/7r/5N2/8/8/8/8/6K1 w - - 0 1",
        best_root: "f6h7",
        best_line: &["f6h7"],
        best_score: 500,
        player_root: "f6h7",
        player_line: &["f6h7"],
        player_score: 500,
        classification: positive_highlight(),
    });
    assert_eq!(
        reinforced.selected_paths[0].attribution,
        ExplanationPathAttribution::Reinforcement
    );
    assert_eq!(
        reinforced_tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::HangingPiece,
        }
    );
}

#[test]
fn sole_ray_blocker_and_attack_transition_prove_a_pin() {
    let (explanation, tracks) = single_pv_build(SinglePvFixture {
        fen: "4k3/8/8/8/8/8/4r3/R6K w - - 0 1",
        best_root: "a1e1",
        best_line: &["a1e1"],
        best_score: 300,
        player_root: "h1g1",
        player_line: &["h1g1"],
        player_score: 0,
        classification: improvement_for("a1e1"),
    });

    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::Pin,
        }
    );
    assert!(explanation
        .facts
        .iter()
        .any(|fact| matches!(fact.data, AtomicChessFactData::SoleRayBlocker { .. })));
    let owner = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == explanation.selected_paths[0].candidate_ref)
        .unwrap();
    assert!(owner.outcomes.iter().any(|outcome| matches!(
        outcome.data,
        SemanticOutcomeData::AttackAccessChanged { .. }
    )));
}

#[test]
fn stationary_checker_after_a_blocker_move_proves_discovered_check() {
    let fixture = SinglePvFixture {
        fen: "4k3/8/8/8/8/8/4B3/4R2K w - - 0 1",
        best_root: "e2c4",
        best_line: &["e2c4"],
        best_score: 300,
        player_root: "h1g1",
        player_line: &["h1g1"],
        player_score: 0,
        classification: improvement_for("e2c4"),
    };
    let concepts = &collected_concepts(&fixture)["e2c4"];
    assert!(concepts
        .contains(&crate::review_session_contract::CurriculumLearningConcept::DiscoveredCheck));
    assert!(concepts
        .contains(&crate::review_session_contract::CurriculumLearningConcept::DiscoveredAttack));
    let (_, tracks) = single_pv_build(fixture);

    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::ExposedKing,
        }
    );
}

#[test]
fn f7_detector_requires_a_new_attack_on_the_occupied_weak_square() {
    let (_, tracks) = single_pv_build(SinglePvFixture {
        fen: "k7/5p2/8/8/2N5/8/8/7K w - - 0 1",
        best_root: "c4e5",
        best_line: &["c4e5"],
        best_score: 200,
        player_root: "h1g1",
        player_line: &["h1g1"],
        player_score: 0,
        classification: improvement_for("c4e5"),
    });
    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::AttackingF2F7,
        }
    );
}

#[path = "tests/line.rs"]
mod line;
#[path = "tests/mating.rs"]
mod mating;
#[path = "tests/position.rs"]
mod position;
#[path = "tests/skewer.rs"]
mod skewer;
#[path = "tests/specificity.rs"]
mod specificity;

#[test]
fn every_migrated_concept_has_versioned_knowledge_and_an_exact_resource_mapping() {
    let graph = super::knowledge::compiled_graph().unwrap();
    for concept in super::knowledge::ATTACK_RELATIONSHIP_CONCEPTS
        .iter()
        .chain(super::knowledge::MATING_CONCEPTS)
        .chain(super::knowledge::LINE_TRANSITION_CONCEPTS)
        .chain(super::knowledge::PAWN_ENDGAME_CONCEPTS)
    {
        let knowledge = KnowledgeConcept::Curriculum(*concept);
        let (node, rule) = graph.references(knowledge);
        assert!(graph.resolves(&node, &rule), "{concept:?}");
        assert!(
            !crate::learning_plan::catalog::resources_for(&LearningTrackKey::Curriculum {
                concept: *concept,
            })
            .unwrap()
            .is_empty(),
            "{concept:?}"
        );
    }
}

pub(crate) fn canonical_build() -> (
    crate::review_session_contract::DecisionExplanation,
    Vec<crate::review_session_contract::DecisionLearningTrackProjection>,
) {
    let build = explain_decision(DecisionExplanationInput {
        game_ref: GameRef::try_from(format!("sha256:{}", "1".repeat(64))).unwrap(),
        critical_moment_id: CriticalMomentId::try_from("review-moment:fork".to_string()).unwrap(),
        position_snapshot: build_position_snapshot(FORK_FEN, &[]).unwrap(),
        classification: improvement(),
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: "e1e2".to_string(),
        candidate_evidence: multi_pv_evidence(),
    })
    .unwrap();
    let DecisionExplanationBuild::Durable {
        explanation,
        projected_tracks,
        diagnostics,
    } = build
    else {
        panic!("the canonical fork must produce a durable explanation");
    };
    assert!(diagnostics.is_empty());
    (*explanation, projected_tracks)
}

#[derive(Clone)]
struct SinglePvFixture<'a> {
    fen: &'a str,
    best_root: &'a str,
    best_line: &'a [&'a str],
    best_score: i32,
    player_root: &'a str,
    player_line: &'a [&'a str],
    player_score: i32,
    classification: GameReviewMomentClassification,
}

fn collected_concepts(
    fixture: &SinglePvFixture<'_>,
) -> std::collections::BTreeMap<
    String,
    Vec<crate::review_session_contract::CurriculumLearningConcept>,
> {
    let position_snapshot = build_position_snapshot(fixture.fen, &[]).unwrap();
    let evidence = CandidateEvidence::SinglePv {
        authoritative: ranked(1, fixture.best_root, fixture.best_score, fixture.best_line),
        player_move: player_evidence(
            fixture.player_root,
            fixture.player_line,
            fixture.player_score,
        ),
    };
    collected_concepts_for(&position_snapshot, &evidence, fixture.player_root)
}

fn collected_concepts_for(
    position_snapshot: &crate::review_session_contract::PositionSnapshot,
    evidence: &CandidateEvidence,
    player_move_uci: &str,
) -> std::collections::BTreeMap<
    String,
    Vec<crate::review_session_contract::CurriculumLearningConcept>,
> {
    let normalized =
        super::candidate::normalize_evidence(evidence, player_move_uci, &position_snapshot.fen)
            .unwrap();
    let construction = super::candidate::replay_candidates(position_snapshot, normalized).unwrap();
    construction
        .candidates
        .iter()
        .map(|candidate| {
            let mut concepts = super::detectors::detect_all(candidate, &construction)
                .unwrap()
                .into_iter()
                .map(|detected| detected.concept)
                .collect::<Vec<_>>();
            concepts.sort();
            concepts.dedup();
            (candidate.contract.root_move_uci.clone(), concepts)
        })
        .collect()
}

fn single_pv_build(
    fixture: SinglePvFixture<'_>,
) -> (
    crate::review_session_contract::DecisionExplanation,
    Vec<crate::review_session_contract::DecisionLearningTrackProjection>,
) {
    let build = explain_decision(DecisionExplanationInput {
        game_ref: game_ref(),
        critical_moment_id: CriticalMomentId::try_from(format!(
            "review-moment:{}-{}",
            fixture.best_root, fixture.player_root
        ))
        .unwrap(),
        position_snapshot: build_position_snapshot(fixture.fen, &[]).unwrap(),
        classification: fixture.classification,
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: fixture.player_root.to_string(),
        candidate_evidence: CandidateEvidence::SinglePv {
            authoritative: ranked(1, fixture.best_root, fixture.best_score, fixture.best_line),
            player_move: player_evidence(
                fixture.player_root,
                fixture.player_line,
                fixture.player_score,
            ),
        },
    })
    .unwrap();
    let DecisionExplanationBuild::Durable {
        explanation,
        projected_tracks,
        diagnostics,
    } = build
    else {
        panic!("fixture must produce a durable Decision Explanation");
    };
    assert!(diagnostics.is_empty());
    (*explanation, projected_tracks)
}

fn game_ref() -> GameRef {
    GameRef::try_from(format!("sha256:{}", "1".repeat(64))).unwrap()
}

fn improvement_for(best_move: &str) -> GameReviewMomentClassification {
    GameReviewMomentClassification::ImprovementOpportunity {
        correction: ImprovementCorrection {
            better_move_uci: best_move.to_string(),
            better_move_san: best_move.to_string(),
            outcome: ImprovementOutcome::ImprovedAnalyzed {
                better_evaluation: evaluation(500),
            },
        },
    }
}

fn positive_highlight() -> GameReviewMomentClassification {
    GameReviewMomentClassification::PositiveHighlight {
        qualification: PositiveHighlightQualification {
            reasons: vec![PositiveHighlightQualificationReason::Objective {
                reason: ObjectiveExcellenceReason::ExactBestMajorAchievement,
            }],
            achievements: vec![PositiveHighlightAchievement::AdvancedPassedPawn {
                to_square: "e8".to_string(),
            }],
        },
        grade: PositiveHighlightGrade::Good,
    }
}

pub(crate) fn improvement() -> GameReviewMomentClassification {
    GameReviewMomentClassification::ImprovementOpportunity {
        correction: ImprovementCorrection {
            better_move_uci: "b5c7".to_string(),
            better_move_san: "Nxc7+".to_string(),
            outcome: ImprovementOutcome::ImprovedAnalyzed {
                better_evaluation: evaluation(500),
            },
        },
    }
}

fn multi_pv_evidence() -> CandidateEvidence {
    CandidateEvidence::MultiPv {
        authoritative_single_pv: ranked(1, "b5c7", 500, &["b5c7", "e8d7", "c7a8"]),
        requested_count: 3,
        ranked_alternatives: vec![
            alternative(2, "b5d6", 200, &["b5d6", "e8d7"]),
            alternative(3, "b5a7", 300, &["b5a7", "e8d7"]),
        ],
        player_move: player(),
    }
}

fn alternative(
    rank: u8,
    root: &str,
    behind_best: u32,
    variation: &[&str],
) -> RankedAlternativeEvidence {
    RankedAlternativeEvidence {
        rank,
        root_move_uci: root.to_string(),
        gap: CandidateGap::Centipawns { behind_best },
        variation: variation.iter().map(|uci| (*uci).to_string()).collect(),
        provenance: provenance(),
    }
}

fn ranked(rank: u8, root: &str, score: i32, variation: &[&str]) -> EngineCandidateEvidence {
    EngineCandidateEvidence {
        rank,
        root_move_uci: root.to_string(),
        evaluation: evaluation(score),
        variation: variation.iter().map(|uci| (*uci).to_string()).collect(),
        provenance: provenance(),
    }
}

fn player() -> PlayerMoveEvidence {
    player_evidence("e1e2", &["e1e2"], 0)
}

fn player_evidence(root: &str, variation: &[&str], score: i32) -> PlayerMoveEvidence {
    PlayerMoveEvidence {
        root_move_uci: root.to_string(),
        evaluation: evaluation(score),
        retained_variation: variation.iter().map(|uci| (*uci).to_string()).collect(),
        provenance: provenance(),
    }
}

fn piece_at(color: Color, role: PieceRole, square_name: &str) -> PieceAtSquare {
    PieceAtSquare {
        color,
        role,
        square: square(square_name),
    }
}

fn square(name: &str) -> Square {
    Square::try_from(name.to_string()).unwrap()
}

fn provenance() -> DecisionEngineProvenance {
    DecisionEngineProvenance {
        engine: "Stockfish 18 fixture".to_string(),
        binary_digest: ArtifactDigest::try_from(format!("sha256:{}", "2".repeat(64))).unwrap(),
        depth: 16,
        threads: 1,
        hash_mib: 16,
    }
}

fn evaluation(value: i32) -> EngineEvaluation {
    EngineEvaluation::Centipawns {
        value,
        perspective: Color::White,
    }
}
