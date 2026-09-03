use crate::{
    decision_explanation::{
        knowledge::{
            CompiledKnowledgeGraph, GenerationKnowledge, GoalTemplate, KnowledgeConcept,
            KnowledgeEdge, KnowledgeEntityRef, KnowledgeRelationship, PreMoveRule, RecognitionRule,
        },
        validate_decision_explanation, DecisionExplanationContractError,
    },
    review_session_contract::{
        build_position_snapshot, AtomicChessFactData, Color, KnowledgeNodeRef, KnowledgeRuleRef,
        LearningTrackKey, MaterialValuePolicyVersion, PieceRole, PositionGoal, ProofCapability,
        SemanticOutcomeData,
    },
};

use super::{
    canonical_build, collected_concepts_for, improvement, multi_pv_evidence, piece_at,
    single_pv_build, square, SinglePvFixture, FORK_FEN,
};

#[test]
fn canonical_candidate_generation_is_pre_move_fork_knowledge_with_a_later_payoff() {
    let snapshot = build_position_snapshot(FORK_FEN, &[]).unwrap();
    let evidence = multi_pv_evidence();
    assert!(collected_concepts_for(&snapshot, &evidence, "e1e2")["b5c7"]
        .contains(&crate::review_session_contract::CurriculumLearningConcept::Fork));
    let (explanation, tracks) = canonical_build();

    assert_eq!(explanation.candidates.len(), 4);
    let path = explanation.selected_paths.first().unwrap();
    let owner = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == path.candidate_ref)
        .unwrap();
    assert_eq!(owner.root_move_uci, "b5c7");
    assert_eq!(
        path.attribution,
        crate::review_session_contract::ExplanationPathAttribution::MissedBest
    );
    assert_eq!(
        crate::decision_explanation::resolve_knowledge_concept(
            &path.knowledge_activation.concept_node_ref
        ),
        Some(crate::review_session_contract::CurriculumLearningConcept::Advantage)
    );
    let generation = path
        .candidate_generation_proof
        .as_ref()
        .expect("the root position must explain why the fork candidate was generated");
    assert_eq!(
        crate::decision_explanation::resolve_knowledge_concept(&generation.concept_node_ref),
        Some(crate::review_session_contract::CurriculumLearningConcept::Fork)
    );
    assert_eq!(generation.suggested_candidate_ref, path.candidate_ref);
    assert_eq!(
        generation.position_goal,
        PositionGoal::GainMaterial {
            targets: vec![
                piece_at(Color::Black, PieceRole::Rook, "a8"),
                piece_at(Color::Black, PieceRole::King, "e8"),
            ],
        }
    );

    let root_snapshot_ref = &owner.line_steps[0].before_snapshot_ref;
    let mut occupied = Vec::new();
    let mut legal_destination_facts = 0;
    for fact_ref in &generation.supporting_fact_refs {
        assert!(owner.fact_refs.contains(fact_ref));
        let fact = explanation
            .facts
            .iter()
            .find(|fact| &fact.fact_ref == fact_ref)
            .expect("every generation support reference must resolve");
        match &fact.data {
            AtomicChessFactData::PieceOccupancy {
                snapshot_ref,
                piece,
            } => {
                assert_eq!(snapshot_ref, root_snapshot_ref);
                occupied.push(piece.clone());
            }
            AtomicChessFactData::LegalDestinations {
                snapshot_ref,
                piece,
                destinations,
            } => {
                assert_eq!(snapshot_ref, root_snapshot_ref);
                assert_eq!(piece, &piece_at(Color::White, PieceRole::Knight, "b5"));
                assert!(destinations.contains(&square("c7")));
                legal_destination_facts += 1;
            }
            other => panic!("generation proof cited a non-root-position fact: {other:?}"),
        }
    }
    occupied.sort_by(|left, right| left.square.as_str().cmp(right.square.as_str()));
    let mut expected_occupancy = vec![
        piece_at(Color::Black, PieceRole::Rook, "a8"),
        piece_at(Color::Black, PieceRole::King, "e8"),
    ];
    expected_occupancy.sort_by(|left, right| left.square.as_str().cmp(right.square.as_str()));
    assert_eq!(occupied, expected_occupancy);
    assert_eq!(legal_destination_facts, 1);
    assert_eq!(generation.supporting_fact_refs.len(), 3);

    let satisfying_outcome = owner
        .outcomes
        .iter()
        .find(|outcome| generation.position_goal.is_satisfied_by(outcome))
        .expect("the retained line must later satisfy the generated material goal");
    assert_eq!(
        satisfying_outcome.data,
        SemanticOutcomeData::MaterialBalanceChanged {
            conventional_value_delta: 5,
            value_policy_version: MaterialValuePolicyVersion::V1,
            gained: vec![piece_at(Color::Black, PieceRole::Rook, "a8")],
            lost: Vec::new(),
        }
    );
    let payoff_step = owner
        .line_steps
        .iter()
        .find(|step| step.captured == Some(piece_at(Color::Black, PieceRole::Rook, "a8")))
        .expect("the retained line must include the material payoff");
    assert_ne!(payoff_step.step_ref, owner.line_steps[0].step_ref);
    assert!(satisfying_outcome
        .supporting_fact_refs
        .iter()
        .any(|fact_ref| explanation.facts.iter().any(|fact| {
            &fact.fact_ref == fact_ref
                && matches!(
                    &fact.data,
                    AtomicChessFactData::MaterialChanged { step_ref, .. }
                        if step_ref == &payoff_step.step_ref
                )
        })));
    assert_eq!(explanation.capability, ProofCapability::EnginePreference);
    assert_eq!(
        explanation
            .preference
            .as_ref()
            .unwrap()
            .engine_comparisons
            .len(),
        3
    );
    assert!(explanation
        .preference
        .as_ref()
        .unwrap()
        .semantic_comparisons
        .is_empty());
    assert!(owner.outcomes.iter().any(|outcome| matches!(
        outcome.data,
        SemanticOutcomeData::MaterialBalanceChanged { .. }
    )));
    assert_eq!(tracks.len(), 1);
    assert_eq!(
        tracks[0].key,
        LearningTrackKey::Curriculum {
            concept: crate::review_session_contract::CurriculumLearningConcept::Advantage,
        }
    );
    assert_eq!(tracks[0].explanation_path_ref, path.path_ref);
    assert!(!tracks[0].resources.is_empty());
}

#[test]
fn gain_material_goal_matches_only_a_named_piece_gained_at_positive_delta() {
    let (explanation, _) = canonical_build();
    let path = &explanation.selected_paths[0];
    let owner = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == path.candidate_ref)
        .unwrap();
    let rook_gain = owner
        .outcomes
        .iter()
        .find(|outcome| {
            matches!(
                &outcome.data,
                SemanticOutcomeData::MaterialBalanceChanged { gained, .. }
                    if gained.contains(&piece_at(Color::Black, PieceRole::Rook, "a8"))
            )
        })
        .expect("the canonical line gains the rook on a8");

    let matching = PositionGoal::GainMaterial {
        targets: vec![piece_at(Color::Black, PieceRole::Rook, "a8")],
    };
    assert!(matching.is_satisfied_by(rook_gain));
    assert!(!PositionGoal::GainMaterial {
        targets: vec![piece_at(Color::Black, PieceRole::Rook, "a7")],
    }
    .is_satisfied_by(rook_gain));

    let mut target_only_lost = rook_gain.clone();
    let SemanticOutcomeData::MaterialBalanceChanged { gained, lost, .. } =
        &mut target_only_lost.data
    else {
        unreachable!("the selected outcome is a material transition");
    };
    gained.clear();
    *lost = vec![piece_at(Color::Black, PieceRole::Rook, "a8")];
    assert!(!matching.is_satisfied_by(&target_only_lost));

    let mut negative_delta = rook_gain.clone();
    let SemanticOutcomeData::MaterialBalanceChanged {
        conventional_value_delta,
        ..
    } = &mut negative_delta.data
    else {
        unreachable!("the selected outcome is a material transition");
    };
    *conventional_value_delta = -5;
    assert!(!matching.is_satisfied_by(&negative_delta));

    let mut unrelated = rook_gain.clone();
    unrelated.data = SemanticOutcomeData::MaterialConfigurationChanged {
        before_inventory_refs: Vec::new(),
        after_inventory_refs: Vec::new(),
    };
    assert!(!matching.is_satisfied_by(&unrelated));
}

#[test]
fn reachable_fork_without_a_retained_material_payoff_omits_generation_proof() {
    let fixture = SinglePvFixture {
        fen: FORK_FEN,
        best_root: "b5c7",
        best_line: &["b5c7", "e8d7"],
        best_score: 500,
        player_root: "e1e2",
        player_line: &["e1e2"],
        player_score: 0,
        classification: improvement(),
    };
    let (explanation, _) = single_pv_build(fixture);
    let path = &explanation.selected_paths[0];
    let owner = explanation
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_ref == path.candidate_ref)
        .unwrap();
    assert_eq!(owner.root_move_uci, "b5c7");
    assert!(path.candidate_generation_proof.is_none());
}

#[test]
fn undefended_fork_targets_retain_complete_root_occupancy() {
    let fixture = SinglePvFixture {
        fen: "b3b2k/8/8/1N6/8/8/8/4K3 w - - 0 1",
        best_root: "b5c7",
        best_line: &["b5c7", "h8g7", "c7a8"],
        best_score: 300,
        player_root: "e1e2",
        player_line: &["e1e2"],
        player_score: 0,
        classification: improvement(),
    };
    let (explanation, _) = single_pv_build(fixture);
    let path = &explanation.selected_paths[0];
    let proof = path
        .candidate_generation_proof
        .as_ref()
        .expect("the retained line gains an undefended fork target");

    let root_occupancy_count = proof
        .supporting_fact_refs
        .iter()
        .filter(|fact_ref| {
            explanation.facts.iter().any(|fact| {
                &fact.fact_ref == *fact_ref
                    && matches!(fact.data, AtomicChessFactData::PieceOccupancy { .. })
            })
        })
        .count();
    assert_eq!(root_occupancy_count, 5);
    assert_eq!(proof.supporting_fact_refs.len(), 6);
}

#[test]
fn post_move_fact_cannot_support_candidate_generation() {
    let (mut explanation, _) = canonical_build();
    let post_move_fact_ref = {
        let path = &explanation.selected_paths[0];
        let owner = explanation
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_ref == path.candidate_ref)
            .unwrap();
        let payoff_step = owner
            .line_steps
            .iter()
            .find(|step| step.captured == Some(piece_at(Color::Black, PieceRole::Rook, "a8")))
            .unwrap();
        explanation
            .facts
            .iter()
            .find_map(|fact| match &fact.data {
                AtomicChessFactData::MaterialChanged { step_ref, .. }
                    if step_ref == &payoff_step.step_ref
                        && owner.fact_refs.contains(&fact.fact_ref) =>
                {
                    Some(fact.fact_ref.clone())
                }
                _ => None,
            })
            .expect("the retained payoff must have a candidate-owned post-move fact")
    };

    explanation.selected_paths[0]
        .candidate_generation_proof
        .as_mut()
        .unwrap()
        .supporting_fact_refs[0] = post_move_fact_ref;

    assert_eq!(
        validate_decision_explanation(&explanation),
        Err(DecisionExplanationContractError::InvalidProof(
            "Candidate Generation Proof may cite only pre-move position facts"
        ))
    );
}

#[test]
fn suggests_goal_requires_a_truthful_pre_move_rule() {
    let fork = crate::review_session_contract::CurriculumLearningConcept::Fork;
    let fork_knowledge = KnowledgeConcept::Curriculum(fork);
    let fork_rule = RecognitionRule::CurriculumV1(fork);
    let fork_ref = KnowledgeNodeRef::from_content(&fork_knowledge);
    let fork_rule_ref = KnowledgeRuleRef::from_content(&fork_rule);
    let backed = CompiledKnowledgeGraph::compile(
        [fork_knowledge],
        [fork_rule],
        &[
            KnowledgeEdge {
                source: KnowledgeEntityRef::Concept(fork_ref.clone()),
                target: KnowledgeEntityRef::Rule(fork_rule_ref),
                relationship: KnowledgeRelationship::RecognizedBy,
            },
            KnowledgeEdge {
                source: KnowledgeEntityRef::Concept(fork_ref.clone()),
                target: KnowledgeEntityRef::GoalTemplate(GoalTemplate::GainMaterial),
                relationship: KnowledgeRelationship::SuggestsGoal,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        backed.generation_knowledge().collect::<Vec<_>>(),
        vec![(
            &fork_ref,
            GenerationKnowledge {
                pre_move_rule: PreMoveRule::ReachableForkV1,
                goal_template: GoalTemplate::GainMaterial,
            },
        )]
    );

    let advantage = crate::review_session_contract::CurriculumLearningConcept::Advantage;
    let advantage_knowledge = KnowledgeConcept::Curriculum(advantage);
    let advantage_rule = RecognitionRule::CurriculumV1(advantage);
    let advantage_ref = KnowledgeNodeRef::from_content(&advantage_knowledge);
    let advantage_rule_ref = KnowledgeRuleRef::from_content(&advantage_rule);
    assert_eq!(
        CompiledKnowledgeGraph::compile(
            [advantage_knowledge],
            [advantage_rule],
            &[
                KnowledgeEdge {
                    source: KnowledgeEntityRef::Concept(advantage_ref.clone()),
                    target: KnowledgeEntityRef::Rule(advantage_rule_ref),
                    relationship: KnowledgeRelationship::RecognizedBy,
                },
                KnowledgeEdge {
                    source: KnowledgeEntityRef::Concept(advantage_ref),
                    target: KnowledgeEntityRef::GoalTemplate(GoalTemplate::GainMaterial),
                    relationship: KnowledgeRelationship::SuggestsGoal,
                },
            ],
        )
        .unwrap_err(),
        DecisionExplanationContractError::InvalidKnowledge(
            "a goal template must be backed by a truthful pre-move rule"
        )
    );
}
