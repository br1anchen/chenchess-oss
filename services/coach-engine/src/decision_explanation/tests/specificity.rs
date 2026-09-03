use std::collections::BTreeMap;

use super::*;
use crate::{
    decision_explanation::{
        candidate, detectors,
        facts::{self, select_detected_realization, suppress_broader_realizations},
        knowledge::{self, CompiledKnowledgeGraph},
        validation,
    },
    review_session_contract::{
        AtomicFactRef, CurriculumLearningConcept as Concept, LineStepRef, SemanticOutcomeRef,
    },
};

fn detected(
    concept: Concept,
    causal: &str,
    payoff: &str,
    outcome: &str,
    fact_count: usize,
) -> detectors::DetectedConcept {
    detectors::DetectedConcept {
        concept,
        causal_step_ref: LineStepRef::from_content(&causal),
        payoff_step_ref: LineStepRef::from_content(&payoff),
        supporting_fact_refs: (0..fact_count)
            .map(|index| AtomicFactRef::from_content(&(concept, causal, payoff, index)))
            .collect(),
        outcome_refs: vec![SemanticOutcomeRef::from_content(&outcome)],
        semantic_comparisons: Vec::new(),
    }
}

fn selected(
    detected: Vec<detectors::DetectedConcept>,
    steps: &[&str],
) -> detectors::DetectedConcept {
    let indices = steps
        .iter()
        .enumerate()
        .map(|(index, label)| (LineStepRef::from_content(label), index))
        .collect::<BTreeMap<_, _>>();
    select_detected_realization(
        detected,
        &knowledge::compiled_graph().unwrap(),
        |reference| indices.get(reference).copied(),
    )
    .unwrap()
    .unwrap()
    .1
}

#[test]
fn synthetic_specificity_suppresses_named_mate_underpromotion_and_discovered_check_ancestors() {
    let graph = knowledge::compiled_graph().unwrap();
    let cases = [
        (
            vec![
                detected(Concept::Checkmate, "mate", "mate", "mate", 1),
                detected(Concept::CheckmatePatterns, "mate", "mate", "mate", 1),
                detected(Concept::AnastasiaMate, "mate", "mate", "mate", 1),
            ],
            Concept::AnastasiaMate,
        ),
        (
            vec![
                detected(Concept::Promotion, "promote", "promote", "promote", 1),
                detected(Concept::Underpromotion, "promote", "promote", "promote", 1),
            ],
            Concept::Underpromotion,
        ),
        (
            vec![
                detected(
                    Concept::DiscoveredAttack,
                    "discover",
                    "discover",
                    "check",
                    1,
                ),
                detected(Concept::DiscoveredCheck, "discover", "discover", "check", 1),
            ],
            Concept::DiscoveredCheck,
        ),
    ];

    for (matches, expected) in cases {
        let survivors = suppress_broader_realizations(matches, &graph);
        assert_eq!(survivors.len(), 1);
        assert_eq!(survivors[0].concept, expected);
    }
}

#[test]
fn specificity_never_crosses_causal_payoff_or_outcome_realizations() {
    let graph = knowledge::compiled_graph().unwrap();
    let matches = vec![
        detected(
            Concept::DiscoveredCheck,
            "specific-causal",
            "specific-payoff",
            "shared",
            1,
        ),
        detected(
            Concept::DiscoveredAttack,
            "other-causal",
            "specific-payoff",
            "shared",
            1,
        ),
        detected(
            Concept::DiscoveredAttack,
            "specific-causal",
            "other-payoff",
            "shared",
            1,
        ),
        detected(
            Concept::DiscoveredAttack,
            "specific-causal",
            "specific-payoff",
            "disjoint",
            1,
        ),
    ];

    assert_eq!(suppress_broader_realizations(matches, &graph).len(), 4);
}

#[test]
fn selection_applies_payoff_prefix_facts_then_curriculum_order_and_is_input_order_invariant() {
    let earliest_payoff = selected(
        vec![
            detected(Concept::Pin, "zero", "one", "pin", 1),
            detected(Concept::Skewer, "zero", "zero", "skewer", 8),
        ],
        &["zero", "one"],
    );
    assert_eq!(earliest_payoff.concept, Concept::Skewer);

    let shortest_prefix = selected(
        vec![
            detected(Concept::Pin, "two", "zero", "pin", 1),
            detected(Concept::Skewer, "zero", "zero", "skewer", 8),
        ],
        &["zero", "one", "two"],
    );
    assert_eq!(shortest_prefix.concept, Concept::Skewer);

    let fewest_facts = selected(
        vec![
            detected(Concept::Pin, "zero", "zero", "pin", 2),
            detected(Concept::Skewer, "zero", "zero", "skewer", 1),
        ],
        &["zero"],
    );
    assert_eq!(fewest_facts.concept, Concept::Skewer);

    let enum_order = vec![
        detected(Concept::Fork, "zero", "zero", "fork", 1),
        detected(Concept::Skewer, "zero", "zero", "skewer", 1),
    ];
    let forward = selected(enum_order.clone(), &["zero"]);
    let reverse = selected(enum_order.into_iter().rev().collect(), &["zero"]);
    assert_eq!(forward, reverse);
    assert_eq!(forward.concept, Concept::Skewer);
}

fn recognized_edges(concepts: &[KnowledgeConcept]) -> Vec<KnowledgeEdge> {
    concepts
        .iter()
        .map(|concept| {
            let KnowledgeConcept::Curriculum(curriculum) = concept;
            KnowledgeEdge {
                source: KnowledgeEntityRef::Concept(KnowledgeNodeRef::from_content(concept)),
                target: KnowledgeEntityRef::Rule(KnowledgeRuleRef::from_content(
                    &RecognitionRule::CurriculumV1(*curriculum),
                )),
                relationship: KnowledgeRelationship::RecognizedBy,
            }
        })
        .collect()
}

fn concept_ref(concept: KnowledgeConcept) -> KnowledgeEntityRef {
    KnowledgeEntityRef::Concept(KnowledgeNodeRef::from_content(&concept))
}

#[test]
fn refines_compilation_rejects_bad_graphs_and_retains_transitive_direction_only() {
    let concepts = [
        KnowledgeConcept::Curriculum(Concept::DiscoveredCheck),
        KnowledgeConcept::Curriculum(Concept::DiscoveredAttack),
        KnowledgeConcept::Curriculum(Concept::HangingPiece),
    ];
    let rules = [
        RecognitionRule::CurriculumV1(Concept::DiscoveredCheck),
        RecognitionRule::CurriculumV1(Concept::DiscoveredAttack),
        RecognitionRule::CurriculumV1(Concept::HangingPiece),
    ];
    let mut edges = recognized_edges(&concepts);
    edges.extend([
        KnowledgeEdge {
            source: concept_ref(concepts[0]),
            target: concept_ref(concepts[1]),
            relationship: KnowledgeRelationship::Refines,
        },
        KnowledgeEdge {
            source: concept_ref(concepts[1]),
            target: concept_ref(concepts[2]),
            relationship: KnowledgeRelationship::Refines,
        },
    ]);
    let graph = CompiledKnowledgeGraph::compile(concepts, rules, &edges).unwrap();
    let specific = KnowledgeNodeRef::from_content(&concepts[0]);
    let middle = KnowledgeNodeRef::from_content(&concepts[1]);
    let broad = KnowledgeNodeRef::from_content(&concepts[2]);
    assert!(graph.refines(&specific, &middle));
    assert!(graph.refines(&specific, &broad));
    assert!(!graph.refines(&broad, &specific));

    let one = [KnowledgeConcept::Curriculum(Concept::Fork)];
    let one_rule = [RecognitionRule::CurriculumV1(Concept::Fork)];
    let mut unknown = recognized_edges(&one);
    unknown.push(KnowledgeEdge {
        source: concept_ref(KnowledgeConcept::Curriculum(Concept::Pin)),
        target: concept_ref(one[0]),
        relationship: KnowledgeRelationship::Refines,
    });
    assert!(CompiledKnowledgeGraph::compile(one, one_rule, &unknown).is_err());

    let mut self_refinement = recognized_edges(&one);
    self_refinement.push(KnowledgeEdge {
        source: concept_ref(one[0]),
        target: concept_ref(one[0]),
        relationship: KnowledgeRelationship::Refines,
    });
    assert!(CompiledKnowledgeGraph::compile(one, one_rule, &self_refinement).is_err());

    let mut cycle = edges.clone();
    cycle.push(KnowledgeEdge {
        source: concept_ref(concepts[2]),
        target: concept_ref(concepts[0]),
        relationship: KnowledgeRelationship::Refines,
    });
    assert!(CompiledKnowledgeGraph::compile(concepts, rules, &cycle).is_err());
}

fn prerequisite_chain() -> (
    [KnowledgeConcept; 3],
    [RecognitionRule; 3],
    Vec<KnowledgeEdge>,
) {
    let concepts = [
        KnowledgeConcept::Curriculum(Concept::AnastasiaMate),
        KnowledgeConcept::Curriculum(Concept::CheckmatePatterns),
        KnowledgeConcept::Curriculum(Concept::Checkmate),
    ];
    let rules = [
        RecognitionRule::CurriculumV1(Concept::AnastasiaMate),
        RecognitionRule::CurriculumV1(Concept::CheckmatePatterns),
        RecognitionRule::CurriculumV1(Concept::Checkmate),
    ];
    let mut edges = recognized_edges(&concepts);
    edges.extend([
        KnowledgeEdge {
            source: concept_ref(concepts[0]),
            target: concept_ref(concepts[1]),
            relationship: KnowledgeRelationship::Prerequisite,
        },
        KnowledgeEdge {
            source: concept_ref(concepts[1]),
            target: concept_ref(concepts[2]),
            relationship: KnowledgeRelationship::Prerequisite,
        },
    ]);
    (concepts, rules, edges)
}

#[test]
fn prerequisite_compilation_retains_transitive_direction_only() {
    let (concepts, rules, edges) = prerequisite_chain();
    let graph = CompiledKnowledgeGraph::compile(concepts, rules, &edges).unwrap();
    let dependent = KnowledgeNodeRef::from_content(&concepts[0]);
    let direct = KnowledgeNodeRef::from_content(&concepts[1]);
    let transitive = KnowledgeNodeRef::from_content(&concepts[2]);

    assert!(graph.has_prerequisite(&dependent, &direct));
    assert!(graph.has_prerequisite(&dependent, &transitive));
    assert!(!graph.has_prerequisite(&transitive, &dependent));
}

#[test]
fn prerequisite_compilation_rejects_an_unknown_endpoint() {
    let one = [KnowledgeConcept::Curriculum(Concept::Fork)];
    let one_rule = [RecognitionRule::CurriculumV1(Concept::Fork)];
    let mut unknown = recognized_edges(&one);
    unknown.push(KnowledgeEdge {
        source: concept_ref(KnowledgeConcept::Curriculum(Concept::Pin)),
        target: concept_ref(one[0]),
        relationship: KnowledgeRelationship::Prerequisite,
    });
    assert!(CompiledKnowledgeGraph::compile(one, one_rule, &unknown).is_err());
}

#[test]
fn prerequisite_compilation_rejects_a_cycle() {
    let (concepts, rules, edges) = prerequisite_chain();
    let mut cycle = edges;
    cycle.push(KnowledgeEdge {
        source: concept_ref(concepts[2]),
        target: concept_ref(concepts[0]),
        relationship: KnowledgeRelationship::Prerequisite,
    });
    assert!(CompiledKnowledgeGraph::compile(concepts, rules, &cycle).is_err());
}

#[test]
fn compiled_graph_seeds_learning_plan_prerequisites() {
    let graph = knowledge::compiled_graph().unwrap();
    let requires = |dependent: Concept, prerequisite: Concept| {
        let dependent = KnowledgeNodeRef::from_content(&KnowledgeConcept::Curriculum(dependent));
        let prerequisite =
            KnowledgeNodeRef::from_content(&KnowledgeConcept::Curriculum(prerequisite));
        graph.has_prerequisite(&dependent, &prerequisite)
    };

    for dependent in [Concept::CheckmatePatterns, Concept::PieceCheckmates] {
        assert!(requires(dependent, Concept::Checkmate));
    }
    for dependent in [
        Concept::AnastasiaMate,
        Concept::ArabianMate,
        Concept::BackRankMate,
        Concept::BalestraMate,
        Concept::BlindSwineMate,
        Concept::BodenMate,
        Concept::CornerMate,
        Concept::DoubleBishopMate,
        Concept::DovetailMate,
        Concept::EpauletteMate,
        Concept::HookMate,
        Concept::KillBoxMate,
        Concept::PillsburysMate,
        Concept::MorphysMate,
        Concept::OperaMate,
        Concept::SwallowstailMate,
        Concept::TriangleMate,
        Concept::VukovicMate,
        Concept::SmotheredMate,
    ] {
        assert!(requires(dependent, Concept::CheckmatePatterns));
    }
    for dependent in [
        Concept::Lucena,
        Concept::Philidor,
        Concept::PassiveRookDefense,
        Concept::IntermediateRookEndings,
        Concept::PracticalRookEndings,
    ] {
        assert!(requires(dependent, Concept::RookEndgame));
    }
    assert!(!requires(
        Concept::SeventhRankRookPawn,
        Concept::RookEndgame
    ));
}

#[test]
fn non_refines_relationships_have_no_selection_effect() {
    let concepts = [
        KnowledgeConcept::Curriculum(Concept::Pin),
        KnowledgeConcept::Curriculum(Concept::Skewer),
    ];
    let rules = [
        RecognitionRule::CurriculumV1(Concept::Pin),
        RecognitionRule::CurriculumV1(Concept::Skewer),
    ];
    let mut edges = recognized_edges(&concepts);
    for relationship in [
        KnowledgeRelationship::Related,
        KnowledgeRelationship::Prerequisite,
        KnowledgeRelationship::Counters,
    ] {
        edges.push(KnowledgeEdge {
            source: concept_ref(concepts[0]),
            target: concept_ref(concepts[1]),
            relationship,
        });
    }
    let graph = CompiledKnowledgeGraph::compile(concepts, rules, &edges).unwrap();
    let matches = vec![
        detected(Concept::Pin, "same", "same", "same", 1),
        detected(Concept::Skewer, "same", "same", "same", 1),
    ];
    assert_eq!(suppress_broader_realizations(matches, &graph).len(), 2);
}

#[test]
fn reversing_detector_families_preserves_the_serialized_explanation() {
    let position_snapshot = build_position_snapshot(FORK_FEN, &[]).unwrap();
    let candidate_evidence = multi_pv_evidence();
    let input = DecisionExplanationInput {
        game_ref: game_ref(),
        critical_moment_id: CriticalMomentId::try_from("review-moment:order".to_string()).unwrap(),
        position_snapshot: position_snapshot.clone(),
        classification: improvement(),
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: "e1e2".to_string(),
        candidate_evidence: candidate_evidence.clone(),
    };
    let normalized =
        candidate::normalize_evidence(&candidate_evidence, "e1e2", &position_snapshot.fen).unwrap();
    let construction = candidate::replay_candidates(&position_snapshot, normalized).unwrap();
    let graph = knowledge::compiled_graph().unwrap();
    let forward = facts::select_concept_proof_with_family_order(
        &construction,
        &graph,
        &input.classification,
        &detectors::DETECTOR_FAMILIES,
    )
    .unwrap()
    .unwrap();
    let mut reverse_order = detectors::DETECTOR_FAMILIES;
    reverse_order.reverse();
    let reverse = facts::select_concept_proof_with_family_order(
        &construction,
        &graph,
        &input.classification,
        &reverse_order,
    )
    .unwrap()
    .unwrap();
    let (forward, _) =
        validation::assemble_and_validate(input.clone(), construction.clone(), forward, &graph)
            .unwrap();
    let (reverse, _) =
        validation::assemble_and_validate(input, construction, reverse, &graph).unwrap();

    assert_eq!(
        serde_json::to_value(forward).unwrap(),
        serde_json::to_value(reverse).unwrap()
    );
}
