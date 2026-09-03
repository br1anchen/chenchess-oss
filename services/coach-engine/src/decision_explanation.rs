mod candidate;
mod detectors;
mod facts;
mod generation;
mod knowledge;
mod minimization;
mod outcomes;
mod projection;
mod replay;
mod validation;

use thiserror::Error;

use crate::review_session_contract::{
    CandidateEvidence, CriticalMomentId, CurriculumLearningConcept, DecisionExplanation,
    DecisionLearningTrackProjection, GameRef, GameReviewMomentClassification,
    GameReviewMomentProvenance, KnowledgeNodeRef, PositionSnapshot,
};

pub use replay::{
    decision_explanation_replay_baseline_path, rebuild_game_review_decision_explanations,
    replay_decision_explanations, DecisionExplanationReplayBaseline,
    DecisionExplanationReplayFailure, DecisionExplanationReplayInput,
    DecisionExplanationReplayObservation, DecisionExplanationReplayOutcome,
    DecisionExplanationReplayReport, GameReviewDecisionExplanationRebuildError,
};
pub use validation::validate_decision_explanation;

/// Names the concept a Knowledge Activation's node reference stands for.
///
/// The reference is the hash of the concept, so only the graph that minted it
/// can read it back. A reference outside the compiled graph — a proof from an
/// older knowledge generation — names nothing rather than guessing.
pub fn resolve_knowledge_concept(
    concept_node_ref: &crate::review_session_contract::KnowledgeNodeRef,
) -> Option<crate::review_session_contract::CurriculumLearningConcept> {
    let graph = knowledge::compiled_graph().ok()?;
    match graph.concept_for(concept_node_ref)? {
        knowledge::KnowledgeConcept::Curriculum(concept) => Some(concept),
    }
}

/// The curriculum relationships Learning Plan assembly is allowed to use.
///
/// Proof recognition owns the compiled graph. Exposing this narrow view keeps
/// plan policy from depending on graph storage, recognition rules, or
/// descriptive relationships such as `Related` and `Counters`.
pub(crate) struct LearningConceptRelationships {
    graph: knowledge::CompiledKnowledgeGraph,
}

impl LearningConceptRelationships {
    pub(crate) fn refines(
        &self,
        specific: CurriculumLearningConcept,
        broader: CurriculumLearningConcept,
    ) -> bool {
        self.graph.refines(
            &curriculum_node_ref(specific),
            &curriculum_node_ref(broader),
        )
    }

    pub(crate) fn has_prerequisite(
        &self,
        dependent: CurriculumLearningConcept,
        prerequisite: CurriculumLearningConcept,
    ) -> bool {
        self.graph.has_prerequisite(
            &curriculum_node_ref(dependent),
            &curriculum_node_ref(prerequisite),
        )
    }
}

pub(crate) fn learning_concept_relationships() -> Result<LearningConceptRelationships, &'static str>
{
    knowledge::compiled_graph()
        .map(|graph| LearningConceptRelationships { graph })
        .map_err(|_| "the compiled chess knowledge graph is invalid")
}

fn curriculum_node_ref(concept: CurriculumLearningConcept) -> KnowledgeNodeRef {
    KnowledgeNodeRef::from_content(&knowledge::KnowledgeConcept::Curriculum(concept))
}

#[derive(Debug, Clone)]
pub struct DecisionExplanationInput {
    pub game_ref: GameRef,
    pub critical_moment_id: CriticalMomentId,
    pub position_snapshot: PositionSnapshot,
    pub classification: GameReviewMomentClassification,
    pub provenance: GameReviewMomentProvenance,
    pub player_move_uci: String,
    pub candidate_evidence: CandidateEvidence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DecisionExplanationBuild {
    Durable {
        explanation: Box<DecisionExplanation>,
        projected_tracks: Vec<DecisionLearningTrackProjection>,
        diagnostics: Vec<DecisionExplanationDiagnostic>,
    },
    Abstained {
        diagnostics: Vec<DecisionExplanationDiagnostic>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionExplanationDiagnostic {
    NoProofValidConcept,
    CandidateEvidenceRejected,
    ResourceMappingUnavailable,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DecisionExplanationContractError {
    #[error("candidate evidence is inconsistent: {0}")]
    InvalidCandidateEvidence(&'static str),
    #[error("the supplied position is invalid")]
    InvalidPosition,
    #[error("candidate {candidate} contains an illegal move at index {index}")]
    InvalidCandidateLine { candidate: String, index: usize },
    #[error("the constructed Decision Explanation violates its contract: {0}")]
    InvalidProof(&'static str),
    #[error("the compiled Chess Knowledge Graph is invalid: {0}")]
    InvalidKnowledge(&'static str),
    #[error("the selected concept has no exact Learning Resource mapping")]
    MissingResourceMapping,
}

pub fn explain_decision(
    input: DecisionExplanationInput,
) -> Result<DecisionExplanationBuild, DecisionExplanationContractError> {
    if matches!(
        input.classification,
        GameReviewMomentClassification::Neutral { .. }
    ) {
        return Ok(DecisionExplanationBuild::Abstained {
            diagnostics: vec![DecisionExplanationDiagnostic::NoProofValidConcept],
        });
    }
    if input.provenance == GameReviewMomentProvenance::PlayerSelected
        && matches!(&input.candidate_evidence, CandidateEvidence::MultiPv { .. })
    {
        return Ok(DecisionExplanationBuild::Abstained {
            diagnostics: vec![DecisionExplanationDiagnostic::CandidateEvidenceRejected],
        });
    }

    let normalized = match candidate::normalize_evidence(
        &input.candidate_evidence,
        &input.player_move_uci,
        &input.position_snapshot.fen,
    ) {
        Ok(normalized) => normalized,
        Err(
            DecisionExplanationContractError::InvalidCandidateEvidence(_)
            | DecisionExplanationContractError::InvalidCandidateLine { .. }
            | DecisionExplanationContractError::InvalidPosition,
        ) => {
            return Ok(DecisionExplanationBuild::Abstained {
                diagnostics: vec![DecisionExplanationDiagnostic::CandidateEvidenceRejected],
            });
        }
        Err(error) => return Err(error),
    };
    let graph = knowledge::compiled_graph()?;
    let construction = match candidate::replay_candidates(&input.position_snapshot, normalized) {
        Ok(construction) => construction,
        Err(
            DecisionExplanationContractError::InvalidCandidateEvidence(_)
            | DecisionExplanationContractError::InvalidCandidateLine { .. }
            | DecisionExplanationContractError::InvalidPosition,
        ) => {
            return Ok(DecisionExplanationBuild::Abstained {
                diagnostics: vec![DecisionExplanationDiagnostic::CandidateEvidenceRejected],
            });
        }
        Err(error) => return Err(error),
    };
    let Some(selected) = facts::select_concept_proof(&construction, &graph, &input.classification)?
    else {
        return Ok(DecisionExplanationBuild::Abstained {
            diagnostics: vec![DecisionExplanationDiagnostic::NoProofValidConcept],
        });
    };
    let (explanation, concept) =
        validation::assemble_and_validate(input, construction, selected, &graph)?;
    let (projected_tracks, diagnostics) = match projection::project(&concept, &explanation) {
        Ok(track) => (vec![track], Vec::new()),
        Err(DecisionExplanationContractError::MissingResourceMapping) => (
            Vec::new(),
            vec![DecisionExplanationDiagnostic::ResourceMappingUnavailable],
        ),
        Err(error) => return Err(error),
    };
    Ok(DecisionExplanationBuild::Durable {
        explanation: Box::new(explanation),
        projected_tracks,
        diagnostics,
    })
}

#[cfg(test)]
#[path = "decision_explanation/tests.rs"]
pub(crate) mod tests;
