use std::collections::{BTreeMap, BTreeSet};

use petgraph::{
    algo::{has_path_connecting, is_cyclic_directed},
    graph::DiGraph,
};
use serde::Serialize;

use crate::review_session_contract::{
    CurriculumLearningConcept, KnowledgeNodeRef, KnowledgeRuleRef,
};

use super::DecisionExplanationContractError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(super) enum KnowledgeConcept {
    Curriculum(CurriculumLearningConcept),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(super) enum RecognitionRule {
    CurriculumV1(CurriculumLearningConcept),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum GoalTemplate {
    GainMaterial,
}

/// A rule that may be evaluated using only the position before a candidate's
/// root move. Recognition rules remain separate because they validate what a
/// retained variation actually demonstrated afterward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreMoveRule {
    ReachableForkV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GenerationKnowledge {
    pub(super) pre_move_rule: PreMoveRule,
    pub(super) goal_template: GoalTemplate,
}

pub(super) const ATTACK_RELATIONSHIP_CONCEPTS: &[CurriculumLearningConcept] = &[
    CurriculumLearningConcept::Pin,
    CurriculumLearningConcept::Skewer,
    CurriculumLearningConcept::Fork,
    CurriculumLearningConcept::HangingPiece,
    CurriculumLearningConcept::DiscoveredAttack,
    CurriculumLearningConcept::DoubleCheck,
    CurriculumLearningConcept::OverloadedPiece,
    CurriculumLearningConcept::XRayAttack,
    CurriculumLearningConcept::CapturingDefender,
    CurriculumLearningConcept::AttackingF2F7,
    CurriculumLearningConcept::ExposedKing,
    CurriculumLearningConcept::KingsideAttack,
    CurriculumLearningConcept::QueensideAttack,
    CurriculumLearningConcept::TrappedPiece,
    CurriculumLearningConcept::DiscoveredCheck,
];

pub(super) const MATING_CONCEPTS: &[CurriculumLearningConcept] = &[
    CurriculumLearningConcept::PieceCheckmates,
    CurriculumLearningConcept::CheckmatePatterns,
    CurriculumLearningConcept::KnightAndBishopMate,
    CurriculumLearningConcept::AnastasiaMate,
    CurriculumLearningConcept::ArabianMate,
    CurriculumLearningConcept::BackRankMate,
    CurriculumLearningConcept::BalestraMate,
    CurriculumLearningConcept::BlindSwineMate,
    CurriculumLearningConcept::BodenMate,
    CurriculumLearningConcept::CornerMate,
    CurriculumLearningConcept::DoubleBishopMate,
    CurriculumLearningConcept::DovetailMate,
    CurriculumLearningConcept::EpauletteMate,
    CurriculumLearningConcept::HookMate,
    CurriculumLearningConcept::KillBoxMate,
    CurriculumLearningConcept::PillsburysMate,
    CurriculumLearningConcept::MorphysMate,
    CurriculumLearningConcept::OperaMate,
    CurriculumLearningConcept::SwallowstailMate,
    CurriculumLearningConcept::TriangleMate,
    CurriculumLearningConcept::VukovicMate,
    CurriculumLearningConcept::SmotheredMate,
    CurriculumLearningConcept::Checkmate,
];

pub(super) const LINE_TRANSITION_CONCEPTS: &[CurriculumLearningConcept] = &[
    CurriculumLearningConcept::Intermezzo,
    CurriculumLearningConcept::Interference,
    CurriculumLearningConcept::GreekGift,
    CurriculumLearningConcept::Deflection,
    CurriculumLearningConcept::Attraction,
    CurriculumLearningConcept::Desperado,
    CurriculumLearningConcept::CounterCheck,
    CurriculumLearningConcept::Clearance,
    CurriculumLearningConcept::Sacrifice,
    CurriculumLearningConcept::CollinearMove,
    CurriculumLearningConcept::DefensiveMove,
    CurriculumLearningConcept::QuietMove,
    CurriculumLearningConcept::Castling,
    CurriculumLearningConcept::EnPassant,
];

pub(super) const PAWN_ENDGAME_CONCEPTS: &[CurriculumLearningConcept] = &[
    CurriculumLearningConcept::Zugzwang,
    CurriculumLearningConcept::Underpromotion,
    CurriculumLearningConcept::KeySquares,
    CurriculumLearningConcept::Opposition,
    CurriculumLearningConcept::SeventhRankRookPawn,
    CurriculumLearningConcept::PassiveRookDefense,
    CurriculumLearningConcept::Lucena,
    CurriculumLearningConcept::Philidor,
    CurriculumLearningConcept::IntermediateRookEndings,
    CurriculumLearningConcept::PracticalRookEndings,
    CurriculumLearningConcept::AdvancedPawn,
    CurriculumLearningConcept::Promotion,
    CurriculumLearningConcept::RookEndgame,
    CurriculumLearningConcept::BishopEndgame,
    CurriculumLearningConcept::PawnEndgame,
    CurriculumLearningConcept::KnightEndgame,
    CurriculumLearningConcept::QueenEndgame,
    CurriculumLearningConcept::QueenAndRookEndgame,
    CurriculumLearningConcept::Equality,
    CurriculumLearningConcept::Advantage,
    CurriculumLearningConcept::CrushingAdvantage,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KnowledgeRelationship {
    Refines,
    Prerequisite,
    Related,
    Counters,
    RecognizedBy,
    SuggestsGoal,
}

const CYCLIC_RELATIONSHIPS: [KnowledgeRelationship; 2] = [
    KnowledgeRelationship::Related,
    KnowledgeRelationship::Counters,
];

#[derive(Debug, Clone)]
pub(super) struct KnowledgeEdge {
    pub(super) source: KnowledgeEntityRef,
    pub(super) target: KnowledgeEntityRef,
    pub(super) relationship: KnowledgeRelationship,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum KnowledgeEntityRef {
    Concept(KnowledgeNodeRef),
    Rule(KnowledgeRuleRef),
    GoalTemplate(GoalTemplate),
}

#[derive(Debug, Clone)]
pub(super) struct CompiledKnowledgeGraph {
    concepts: BTreeMap<KnowledgeConcept, KnowledgeNodeRef>,
    rules: BTreeMap<RecognitionRule, KnowledgeRuleRef>,
    recognized_by: BTreeSet<(KnowledgeNodeRef, KnowledgeRuleRef)>,
    generation_knowledge: BTreeMap<KnowledgeNodeRef, GenerationKnowledge>,
    /// Transitive `Refines` closure. Each pair points from the more specific
    /// concept to its broader ancestor, including directly authored edges.
    refines: BTreeSet<(KnowledgeNodeRef, KnowledgeNodeRef)>,
    /// Transitive `Prerequisite` closure. Each pair points from the dependent
    /// concept to its prerequisite, including directly authored edges.
    prerequisites: BTreeSet<(KnowledgeNodeRef, KnowledgeNodeRef)>,
}

impl CompiledKnowledgeGraph {
    pub(super) fn compile(
        concepts: impl IntoIterator<Item = KnowledgeConcept>,
        rules: impl IntoIterator<Item = RecognitionRule>,
        edges: &[KnowledgeEdge],
    ) -> Result<Self, DecisionExplanationContractError> {
        let concepts = concepts
            .into_iter()
            .map(|concept| (concept, KnowledgeNodeRef::from_content(&concept)))
            .collect::<BTreeMap<_, _>>();
        let rules = rules
            .into_iter()
            .map(|rule| (rule, KnowledgeRuleRef::from_content(&rule)))
            .collect::<BTreeMap<_, _>>();
        let concept_refs = concepts.values().cloned().collect::<BTreeSet<_>>();
        let rule_refs = rules.values().cloned().collect::<BTreeSet<_>>();

        for edge in edges
            .iter()
            .filter(|edge| CYCLIC_RELATIONSHIPS.contains(&edge.relationship))
        {
            let (KnowledgeEntityRef::Concept(source), KnowledgeEntityRef::Concept(target)) =
                (&edge.source, &edge.target)
            else {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "Related and Counters edges must connect known concepts in V1",
                ));
            };
            if !concept_refs.contains(source) || !concept_refs.contains(target) {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "a cyclic relationship references an unknown concept",
                ));
            }
        }

        let mut refines = BTreeSet::new();
        let mut prerequisites = BTreeSet::new();
        for relationship in [
            KnowledgeRelationship::Refines,
            KnowledgeRelationship::Prerequisite,
        ] {
            let mut graph = DiGraph::<KnowledgeNodeRef, ()>::new();
            let mut indices = BTreeMap::new();
            for reference in &concept_refs {
                indices.insert(reference.clone(), graph.add_node(reference.clone()));
            }
            for edge in edges
                .iter()
                .filter(|edge| edge.relationship == relationship)
            {
                let (
                    KnowledgeEntityRef::Concept(source_ref),
                    KnowledgeEntityRef::Concept(target_ref),
                ) = (&edge.source, &edge.target)
                else {
                    return Err(DecisionExplanationContractError::InvalidKnowledge(
                        "a hierarchy edge must connect two concepts",
                    ));
                };
                let (Some(source), Some(target)) =
                    (indices.get(source_ref), indices.get(target_ref))
                else {
                    return Err(DecisionExplanationContractError::InvalidKnowledge(
                        "a hierarchy edge references an unknown concept",
                    ));
                };
                graph.add_edge(*source, *target, ());
            }
            if is_cyclic_directed(&graph) {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "Refines and Prerequisite must each be acyclic",
                ));
            }
            let closure = if relationship == KnowledgeRelationship::Refines {
                &mut refines
            } else {
                &mut prerequisites
            };
            for (source_ref, source) in &indices {
                for (target_ref, target) in &indices {
                    if source != target && has_path_connecting(&graph, *source, *target, None) {
                        closure.insert((source_ref.clone(), target_ref.clone()));
                    }
                }
            }
        }

        let mut recognized_by = BTreeSet::new();
        for edge in edges
            .iter()
            .filter(|edge| edge.relationship == KnowledgeRelationship::RecognizedBy)
        {
            let KnowledgeEntityRef::Concept(concept) = &edge.source else {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "RecognizedBy source is not a concept",
                ));
            };
            let KnowledgeEntityRef::Rule(rule) = &edge.target else {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "RecognizedBy target is not a recognition rule",
                ));
            };
            if !concept_refs.contains(concept) {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "RecognizedBy source is not a known concept",
                ));
            }
            if !rule_refs.contains(rule) {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "RecognizedBy target is not a recognition rule",
                ));
            }
            recognized_by.insert((concept.clone(), rule.clone()));
        }
        if concept_refs.iter().any(|concept| {
            !recognized_by
                .iter()
                .any(|(recognized, _)| recognized == concept)
        }) {
            return Err(DecisionExplanationContractError::InvalidKnowledge(
                "every concept must have a recognition rule",
            ));
        }

        let mut generation_knowledge = BTreeMap::new();
        for edge in edges
            .iter()
            .filter(|edge| edge.relationship == KnowledgeRelationship::SuggestsGoal)
        {
            let KnowledgeEntityRef::Concept(concept_ref) = &edge.source else {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "SuggestsGoal source is not a concept",
                ));
            };
            let KnowledgeEntityRef::GoalTemplate(goal_template) = &edge.target else {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "SuggestsGoal target is not a goal template",
                ));
            };
            let concept = concepts
                .iter()
                .find_map(|(concept, reference)| (reference == concept_ref).then_some(*concept))
                .ok_or(DecisionExplanationContractError::InvalidKnowledge(
                    "SuggestsGoal source is not a known concept",
                ))?;
            let pre_move_rule = match (concept, goal_template) {
                (
                    KnowledgeConcept::Curriculum(CurriculumLearningConcept::Fork),
                    GoalTemplate::GainMaterial,
                ) => PreMoveRule::ReachableForkV1,
                _ => {
                    return Err(DecisionExplanationContractError::InvalidKnowledge(
                        "a goal template must be backed by a truthful pre-move rule",
                    ));
                }
            };
            if generation_knowledge
                .insert(
                    concept_ref.clone(),
                    GenerationKnowledge {
                        pre_move_rule,
                        goal_template: *goal_template,
                    },
                )
                .is_some()
            {
                return Err(DecisionExplanationContractError::InvalidKnowledge(
                    "a concept may suggest only one goal template in V1",
                ));
            }
        }

        Ok(Self {
            concepts,
            rules,
            recognized_by,
            generation_knowledge,
            refines,
            prerequisites,
        })
    }

    pub(super) fn references(
        &self,
        concept: KnowledgeConcept,
    ) -> (KnowledgeNodeRef, KnowledgeRuleRef) {
        let rule = match concept {
            KnowledgeConcept::Curriculum(concept) => RecognitionRule::CurriculumV1(concept),
        };
        (self.concepts[&concept].clone(), self.rules[&rule].clone())
    }

    pub(super) fn concepts(&self) -> impl Iterator<Item = KnowledgeConcept> + '_ {
        self.concepts.keys().copied()
    }

    pub(super) fn generation_knowledge(
        &self,
    ) -> impl Iterator<Item = (&KnowledgeNodeRef, GenerationKnowledge)> {
        self.generation_knowledge
            .iter()
            .map(|(concept, knowledge)| (concept, *knowledge))
    }

    /// Names the concept a node reference stands for.
    ///
    /// A node reference is the hash of the concept it names, so the graph that
    /// minted it is also the only thing that can read it back. Without this a
    /// delivered proof would carry a concept no reader could speak aloud.
    pub(super) fn concept_for(&self, node: &KnowledgeNodeRef) -> Option<KnowledgeConcept> {
        self.concepts
            .iter()
            .find(|(_, reference)| *reference == node)
            .map(|(concept, _)| *concept)
    }

    pub(super) fn resolves(&self, concept: &KnowledgeNodeRef, rule: &KnowledgeRuleRef) -> bool {
        self.recognized_by
            .contains(&(concept.clone(), rule.clone()))
    }

    /// Returns whether `specific` transitively refines `broader`.
    ///
    /// A relationship is authored and stored from descendant to ancestor:
    /// `Refines(A, B)` means A is more specific than B. Descriptive and
    /// learner-oriented relationships never participate in this query.
    pub(super) fn refines(&self, specific: &KnowledgeNodeRef, broader: &KnowledgeNodeRef) -> bool {
        self.refines.contains(&(specific.clone(), broader.clone()))
    }

    /// Returns whether `dependent` transitively requires `prerequisite`.
    ///
    /// Prerequisite edges are authored from dependent to prerequisite, while
    /// learning-plan order places the prerequisite first.
    pub(super) fn has_prerequisite(
        &self,
        dependent: &KnowledgeNodeRef,
        prerequisite: &KnowledgeNodeRef,
    ) -> bool {
        self.prerequisites
            .contains(&(dependent.clone(), prerequisite.clone()))
    }
}

pub(super) fn compiled_graph() -> Result<CompiledKnowledgeGraph, DecisionExplanationContractError> {
    use CurriculumLearningConcept as Concept;

    let migrated = ATTACK_RELATIONSHIP_CONCEPTS
        .iter()
        .chain(MATING_CONCEPTS)
        .chain(LINE_TRANSITION_CONCEPTS)
        .chain(PAWN_ENDGAME_CONCEPTS)
        .copied()
        .collect::<Vec<_>>();
    let concepts = migrated
        .iter()
        .copied()
        .map(KnowledgeConcept::Curriculum)
        .collect::<Vec<_>>();
    let rules = migrated
        .iter()
        .copied()
        .map(RecognitionRule::CurriculumV1)
        .collect::<Vec<_>>();
    let mut edges = concepts
        .iter()
        .copied()
        .zip(rules.iter().copied())
        .map(|(concept, rule)| KnowledgeEdge {
            source: KnowledgeEntityRef::Concept(KnowledgeNodeRef::from_content(&concept)),
            target: KnowledgeEntityRef::Rule(KnowledgeRuleRef::from_content(&rule)),
            relationship: KnowledgeRelationship::RecognizedBy,
        })
        .collect::<Vec<_>>();
    // The catalog is deliberately authored rather than inferred from shared
    // resources. `Refines(A, B)` points from specific A to broader B.
    let refinements = [
        (Concept::DiscoveredCheck, Concept::DiscoveredAttack),
        (Concept::Underpromotion, Concept::Promotion),
        (Concept::KnightAndBishopMate, Concept::PieceCheckmates),
        (Concept::PieceCheckmates, Concept::Checkmate),
        (Concept::CheckmatePatterns, Concept::Checkmate),
        (Concept::AnastasiaMate, Concept::CheckmatePatterns),
        (Concept::ArabianMate, Concept::CheckmatePatterns),
        (Concept::BackRankMate, Concept::CheckmatePatterns),
        (Concept::BalestraMate, Concept::CheckmatePatterns),
        (Concept::BlindSwineMate, Concept::CheckmatePatterns),
        (Concept::BodenMate, Concept::CheckmatePatterns),
        (Concept::CornerMate, Concept::CheckmatePatterns),
        (Concept::DoubleBishopMate, Concept::CheckmatePatterns),
        (Concept::DovetailMate, Concept::CheckmatePatterns),
        (Concept::EpauletteMate, Concept::CheckmatePatterns),
        (Concept::HookMate, Concept::CheckmatePatterns),
        (Concept::KillBoxMate, Concept::CheckmatePatterns),
        (Concept::PillsburysMate, Concept::CheckmatePatterns),
        (Concept::MorphysMate, Concept::CheckmatePatterns),
        (Concept::OperaMate, Concept::CheckmatePatterns),
        (Concept::SwallowstailMate, Concept::CheckmatePatterns),
        (Concept::TriangleMate, Concept::CheckmatePatterns),
        (Concept::VukovicMate, Concept::CheckmatePatterns),
        (Concept::SmotheredMate, Concept::CheckmatePatterns),
        (Concept::Lucena, Concept::RookEndgame),
        (Concept::Philidor, Concept::RookEndgame),
    ];
    edges.extend(refinements.map(|(specific, broader)| KnowledgeEdge {
        source: KnowledgeEntityRef::Concept(KnowledgeNodeRef::from_content(
            &KnowledgeConcept::Curriculum(specific),
        )),
        target: KnowledgeEntityRef::Concept(KnowledgeNodeRef::from_content(
            &KnowledgeConcept::Curriculum(broader),
        )),
        relationship: KnowledgeRelationship::Refines,
    }));
    // `Prerequisite(A, B)` points from dependent A to prerequisite B. Learning
    // Plan assembly reverses that direction so foundations appear first.
    let prerequisites = [
        (Concept::CheckmatePatterns, Concept::Checkmate),
        (Concept::PieceCheckmates, Concept::Checkmate),
        (Concept::AnastasiaMate, Concept::CheckmatePatterns),
        (Concept::ArabianMate, Concept::CheckmatePatterns),
        (Concept::BackRankMate, Concept::CheckmatePatterns),
        (Concept::BalestraMate, Concept::CheckmatePatterns),
        (Concept::BlindSwineMate, Concept::CheckmatePatterns),
        (Concept::BodenMate, Concept::CheckmatePatterns),
        (Concept::CornerMate, Concept::CheckmatePatterns),
        (Concept::DoubleBishopMate, Concept::CheckmatePatterns),
        (Concept::DovetailMate, Concept::CheckmatePatterns),
        (Concept::EpauletteMate, Concept::CheckmatePatterns),
        (Concept::HookMate, Concept::CheckmatePatterns),
        (Concept::KillBoxMate, Concept::CheckmatePatterns),
        (Concept::PillsburysMate, Concept::CheckmatePatterns),
        (Concept::MorphysMate, Concept::CheckmatePatterns),
        (Concept::OperaMate, Concept::CheckmatePatterns),
        (Concept::SwallowstailMate, Concept::CheckmatePatterns),
        (Concept::TriangleMate, Concept::CheckmatePatterns),
        (Concept::VukovicMate, Concept::CheckmatePatterns),
        (Concept::SmotheredMate, Concept::CheckmatePatterns),
        (Concept::Lucena, Concept::RookEndgame),
        (Concept::Philidor, Concept::RookEndgame),
        (Concept::PassiveRookDefense, Concept::RookEndgame),
        (Concept::IntermediateRookEndings, Concept::RookEndgame),
        (Concept::PracticalRookEndings, Concept::RookEndgame),
    ];
    edges.extend(
        prerequisites.map(|(dependent, prerequisite)| KnowledgeEdge {
            source: KnowledgeEntityRef::Concept(KnowledgeNodeRef::from_content(
                &KnowledgeConcept::Curriculum(dependent),
            )),
            target: KnowledgeEntityRef::Concept(KnowledgeNodeRef::from_content(
                &KnowledgeConcept::Curriculum(prerequisite),
            )),
            relationship: KnowledgeRelationship::Prerequisite,
        }),
    );
    let fork = KnowledgeConcept::Curriculum(Concept::Fork);
    edges.push(KnowledgeEdge {
        source: KnowledgeEntityRef::Concept(KnowledgeNodeRef::from_content(&fork)),
        target: KnowledgeEntityRef::GoalTemplate(GoalTemplate::GainMaterial),
        relationship: KnowledgeRelationship::SuggestsGoal,
    });
    CompiledKnowledgeGraph::compile(concepts, rules, &edges)
}
