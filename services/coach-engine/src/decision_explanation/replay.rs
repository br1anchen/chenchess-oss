use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    thread,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::decision_learning::{automatic_learning_plan, merge_with_opening};
use crate::review_session_contract::{
    CandidateEvidence, ChessKnowledgeGraphVersion, CriticalMomentId, CurriculumLearningConcept,
    DecisionExplanation, DecisionExplanationGeneration, DecisionExplanationRef, GameReview,
    GameReviewMomentClassification, GameReviewMomentProvenance, CHESS_KNOWLEDGE_GRAPH_VERSION,
    DECISION_EXPLANATION_GENERATION,
};

use super::{
    explain_decision,
    knowledge::{self, KnowledgeConcept},
    DecisionExplanationBuild, DecisionExplanationContractError, DecisionExplanationInput,
};

const BASELINE_FILE_NAME: &str = "decision-explanation-concepts.baseline.json";

/// Exact aggregate produced by replaying persisted Decision Explanations.
///
/// The concept map is initialized from the compiled Chess Knowledge Graph so
/// serialized evaluation artifacts retain concepts with zero selected paths.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionExplanationReplayBaseline {
    decision_explanation_generation: DecisionExplanationGeneration,
    knowledge_graph_version: ChessKnowledgeGraphVersion,
    replayed_moment_count: usize,
    explanation_path_count: usize,
    concept_path_counts: BTreeMap<CurriculumLearningConcept, usize>,
}

impl DecisionExplanationReplayBaseline {
    fn empty() -> Result<Self, DecisionExplanationContractError> {
        let concept_path_counts = curriculum_knowledge_concepts()?
            .into_iter()
            .map(|concept| (concept, 0))
            .collect();
        Ok(Self {
            decision_explanation_generation: DECISION_EXPLANATION_GENERATION,
            knowledge_graph_version: CHESS_KNOWLEDGE_GRAPH_VERSION,
            replayed_moment_count: 0,
            explanation_path_count: 0,
            concept_path_counts,
        })
    }

    fn record_concepts(&mut self, concepts: &[CurriculumLearningConcept]) {
        self.replayed_moment_count += 1;
        self.explanation_path_count += concepts.len();
        for concept in concepts {
            let count = self
                .concept_path_counts
                .get_mut(concept)
                .expect("the replay baseline is seeded from the same compiled graph");
            *count += 1;
        }
    }

    /// Number of persisted Decision Explanations replayed into this baseline.
    pub fn replayed_moment_count(&self) -> usize {
        self.replayed_moment_count
    }

    /// Number of selected explanation paths represented by this baseline.
    pub fn explanation_path_count(&self) -> usize {
        self.explanation_path_count
    }

    /// Number of compiled curriculum concepts represented, including zeroes.
    pub fn concept_key_count(&self) -> usize {
        self.concept_path_counts.len()
    }

    fn merge(&mut self, other: Self) {
        self.replayed_moment_count += other.replayed_moment_count;
        self.explanation_path_count += other.explanation_path_count;
        for (concept, count) in other.concept_path_counts {
            *self
                .concept_path_counts
                .get_mut(&concept)
                .expect("partition reports use the same complete concept set") += count;
        }
    }

    /// Describes every difference from another complete replay distribution.
    pub fn differences(
        &self,
        actual: &Self,
    ) -> Result<Vec<String>, DecisionExplanationContractError> {
        let mut differences = Vec::new();
        if self.decision_explanation_generation != actual.decision_explanation_generation {
            differences.push(format!(
                "decisionExplanationGeneration: expected {:?}, actual {:?}",
                self.decision_explanation_generation, actual.decision_explanation_generation
            ));
        }
        if self.knowledge_graph_version != actual.knowledge_graph_version {
            differences.push(format!(
                "knowledgeGraphVersion: expected {:?}, actual {:?}",
                self.knowledge_graph_version, actual.knowledge_graph_version
            ));
        }
        if self.replayed_moment_count != actual.replayed_moment_count {
            differences.push(format!(
                "replayedMomentCount: expected {}, actual {}",
                self.replayed_moment_count, actual.replayed_moment_count
            ));
        }
        if self.explanation_path_count != actual.explanation_path_count {
            differences.push(format!(
                "explanationPathCount: expected {}, actual {}",
                self.explanation_path_count, actual.explanation_path_count
            ));
        }

        let compiled = curriculum_knowledge_concepts()?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let expected = self
            .concept_path_counts
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        let observed = actual
            .concept_path_counts
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        if expected != compiled {
            differences.push(
                "expected conceptPathCounts does not contain the complete compiled concept set"
                    .to_string(),
            );
        }
        if observed != compiled {
            differences.push(
                "actual conceptPathCounts does not contain the complete compiled concept set"
                    .to_string(),
            );
        }
        for concept in compiled {
            match (
                self.concept_path_counts.get(&concept),
                actual.concept_path_counts.get(&concept),
            ) {
                (Some(expected), Some(actual)) if expected != actual => {
                    differences.push(format!("{concept:?}: expected {expected}, actual {actual}"))
                }
                _ => {}
            }
        }
        Ok(differences)
    }
}

/// One persisted explanation and the review facts needed to rebuild it.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionExplanationReplayInput<Location> {
    pub location: Location,
    pub classification: GameReviewMomentClassification,
    pub provenance: GameReviewMomentProvenance,
    pub persisted: DecisionExplanation,
}

/// Why a persisted explanation could not produce a replay observation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecisionExplanationReplayFailure {
    #[error("rebuild failed: {0}")]
    Rebuild(DecisionExplanationContractError),
    #[error("rebuild abstained instead of producing a durable explanation")]
    Abstained,
}

/// Why a persisted Game Review could not be rebuilt from its recorded
/// Decision Explanation inputs.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GameReviewDecisionExplanationRebuildError {
    #[error("Decision Explanation rebuild failed for {critical_moment_id:?}: {source}")]
    Rebuild {
        critical_moment_id: CriticalMomentId,
        #[source]
        source: DecisionExplanationContractError,
    },
    #[error("Decision Explanation rebuild abstained for {critical_moment_id:?}")]
    Abstained {
        critical_moment_id: CriticalMomentId,
    },
    #[error("persisted opening Learning Material is invalid for {critical_moment_id:?}: {reason}")]
    LearningMaterial {
        critical_moment_id: CriticalMomentId,
        reason: &'static str,
    },
    #[error("rebuilt Learning Plan is invalid: {0}")]
    LearningPlan(&'static str),
}

/// The explicit result of replaying one persisted explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionExplanationReplayOutcome {
    Exact,
    Diverged {
        replayed_ref: DecisionExplanationRef,
        persisted_ref: DecisionExplanationRef,
    },
    Failed(DecisionExplanationReplayFailure),
}

/// A replay outcome kept with its caller-supplied corpus location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionExplanationReplayObservation<Location> {
    location: Location,
    outcome: DecisionExplanationReplayOutcome,
}

impl<Location> DecisionExplanationReplayObservation<Location> {
    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn outcome(&self) -> &DecisionExplanationReplayOutcome {
        &self.outcome
    }
}

/// A finalized batch replay: immutable aggregate plus every typed observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionExplanationReplayReport<Location> {
    baseline: DecisionExplanationReplayBaseline,
    observations: Vec<DecisionExplanationReplayObservation<Location>>,
}

impl<Location> DecisionExplanationReplayReport<Location> {
    pub fn baseline(&self) -> &DecisionExplanationReplayBaseline {
        &self.baseline
    }

    pub fn observations(&self) -> &[DecisionExplanationReplayObservation<Location>] {
        &self.observations
    }

    pub fn divergence_count(&self) -> usize {
        self.observations
            .iter()
            .filter(|observation| {
                matches!(
                    observation.outcome,
                    DecisionExplanationReplayOutcome::Diverged { .. }
                )
            })
            .count()
    }

    pub fn failure_count(&self) -> usize {
        self.observations
            .iter()
            .filter(|observation| {
                matches!(
                    observation.outcome,
                    DecisionExplanationReplayOutcome::Failed(_)
                )
            })
            .count()
    }

    /// Combines finalized partition reports without exposing their mutable
    /// baseline accumulators.
    pub fn combine(
        reports: impl IntoIterator<Item = Self>,
    ) -> Result<Self, DecisionExplanationContractError> {
        let mut combined = Self {
            baseline: DecisionExplanationReplayBaseline::empty()?,
            observations: Vec::new(),
        };
        for report in reports {
            combined.baseline.merge(report.baseline);
            combined.observations.extend(report.observations);
        }
        Ok(combined)
    }
}

/// Resolves the replay artifact beside its selected corpus index.
pub fn decision_explanation_replay_baseline_path(corpus_root: &Path) -> PathBuf {
    corpus_root.join(BASELINE_FILE_NAME)
}

/// Replays a complete in-memory batch without consulting an Engine Analysis
/// provider. Results retain input order for every concurrency value.
pub fn replay_decision_explanations<Location: Send>(
    inputs: Vec<DecisionExplanationReplayInput<Location>>,
    concurrency: NonZeroUsize,
) -> Result<DecisionExplanationReplayReport<Location>, DecisionExplanationContractError> {
    replay_decision_explanations_with(inputs, concurrency, replay_decision_explanation)
}

type ReplayFunction = fn(
    &DecisionExplanation,
    &GameReviewMomentClassification,
    GameReviewMomentProvenance,
) -> Result<DecisionExplanationBuild, DecisionExplanationContractError>;

fn replay_decision_explanations_with<Location: Send>(
    inputs: Vec<DecisionExplanationReplayInput<Location>>,
    concurrency: NonZeroUsize,
    replay: ReplayFunction,
) -> Result<DecisionExplanationReplayReport<Location>, DecisionExplanationContractError> {
    let mut baseline = DecisionExplanationReplayBaseline::empty()?;
    if inputs.is_empty() {
        return Ok(DecisionExplanationReplayReport {
            baseline,
            observations: Vec::new(),
        });
    }

    let worker_count = concurrency.get().min(inputs.len());
    let mut buckets = (0..worker_count).map(|_| Vec::new()).collect::<Vec<_>>();
    for (index, input) in inputs.into_iter().enumerate() {
        buckets[index % worker_count].push((index, input));
    }
    let mut completed = thread::scope(|scope| {
        let handles = buckets
            .into_iter()
            .map(|bucket| {
                scope.spawn(move || {
                    bucket
                        .into_iter()
                        .map(|(index, input)| (index, replay_one(input, replay)))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let mut completed = Vec::new();
        for handle in handles {
            completed.extend(
                handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            );
        }
        completed
    });
    completed.sort_by_key(|(index, _)| *index);

    let mut observations = Vec::with_capacity(completed.len());
    for (_, completed) in completed {
        if let Some(concepts) = completed.concepts {
            baseline.record_concepts(&concepts);
        }
        observations.push(completed.observation);
    }
    Ok(DecisionExplanationReplayReport {
        baseline,
        observations,
    })
}

struct CompletedReplay<Location> {
    observation: DecisionExplanationReplayObservation<Location>,
    concepts: Option<Vec<CurriculumLearningConcept>>,
}

fn replay_one<Location>(
    input: DecisionExplanationReplayInput<Location>,
    replay: ReplayFunction,
) -> CompletedReplay<Location> {
    let DecisionExplanationReplayInput {
        location,
        classification,
        provenance,
        persisted,
    } = input;
    let build = match replay(&persisted, &classification, provenance) {
        Ok(build) => build,
        Err(error) => {
            return failed_replay(location, DecisionExplanationReplayFailure::Rebuild(error));
        }
    };
    let DecisionExplanationBuild::Durable {
        explanation: replayed,
        ..
    } = build
    else {
        return failed_replay(location, DecisionExplanationReplayFailure::Abstained);
    };
    let concepts = explanation_concepts(&replayed);
    let outcome = if *replayed == persisted {
        DecisionExplanationReplayOutcome::Exact
    } else {
        DecisionExplanationReplayOutcome::Diverged {
            replayed_ref: replayed.decision_explanation_ref.clone(),
            persisted_ref: persisted.decision_explanation_ref,
        }
    };
    CompletedReplay {
        observation: DecisionExplanationReplayObservation { location, outcome },
        concepts: Some(concepts),
    }
}

fn failed_replay<Location>(
    location: Location,
    failure: DecisionExplanationReplayFailure,
) -> CompletedReplay<Location> {
    CompletedReplay {
        observation: DecisionExplanationReplayObservation {
            location,
            outcome: DecisionExplanationReplayOutcome::Failed(failure),
        },
        concepts: None,
    }
}

fn explanation_concepts(explanation: &DecisionExplanation) -> Vec<CurriculumLearningConcept> {
    let graph = knowledge::compiled_graph()
        .expect("a durable replay was validated against the compiled Knowledge Graph");
    explanation
        .selected_paths
        .iter()
        .map(|path| {
            graph
                .concept_for(&path.knowledge_activation.concept_node_ref)
                .map(|concept| match concept {
                    KnowledgeConcept::Curriculum(concept) => concept,
                })
                .expect("a durable replay resolves every selected path concept")
        })
        .collect()
}

fn replay_decision_explanation(
    explanation: &DecisionExplanation,
    classification: &GameReviewMomentClassification,
    provenance: GameReviewMomentProvenance,
) -> Result<DecisionExplanationBuild, DecisionExplanationContractError> {
    let player_move_uci = match &explanation.candidate_evidence {
        CandidateEvidence::SinglePv { player_move, .. }
        | CandidateEvidence::MultiPv { player_move, .. } => player_move.root_move_uci.clone(),
    };
    explain_decision(DecisionExplanationInput {
        game_ref: explanation.game_ref.clone(),
        critical_moment_id: explanation.critical_moment_id.clone(),
        position_snapshot: explanation.position_snapshot.clone(),
        classification: classification.clone(),
        provenance,
        player_move_uci,
        candidate_evidence: explanation.candidate_evidence.clone(),
    })
}

/// Rebuilds every persisted Decision Explanation and its derived learning
/// projections without consulting an Engine Analysis provider.
pub fn rebuild_game_review_decision_explanations(
    mut review: GameReview,
) -> Result<GameReview, GameReviewDecisionExplanationRebuildError> {
    for moment in &mut review.critical_moments {
        let previous_learning_material = moment.learning_material.clone();
        let Some(persisted) = moment.decision_explanation.take() else {
            moment.set_decision_explanation(None);
            moment.learning_material = merge_with_opening(Vec::new(), &previous_learning_material)
                .map_err(
                    |reason| GameReviewDecisionExplanationRebuildError::LearningMaterial {
                        critical_moment_id: moment.critical_moment_id.clone(),
                        reason,
                    },
                )?;
            continue;
        };
        let game_ref = persisted.game_ref.clone();
        let build =
            replay_decision_explanation(&persisted, &moment.classification, moment.provenance)
                .map_err(
                    |source| GameReviewDecisionExplanationRebuildError::Rebuild {
                        critical_moment_id: moment.critical_moment_id.clone(),
                        source,
                    },
                )?;
        if matches!(&build, DecisionExplanationBuild::Abstained { .. }) {
            return Err(GameReviewDecisionExplanationRebuildError::Abstained {
                critical_moment_id: moment.critical_moment_id.clone(),
            });
        }
        let build = crate::decision_learning::decision_learning_build(build).map_err(|reason| {
            GameReviewDecisionExplanationRebuildError::LearningMaterial {
                critical_moment_id: moment.critical_moment_id.clone(),
                reason,
            }
        })?;
        crate::decision_learning::apply_decision_learning(
            &game_ref,
            moment,
            build,
            &previous_learning_material,
        )
        .map_err(
            |reason| GameReviewDecisionExplanationRebuildError::LearningMaterial {
                critical_moment_id: moment.critical_moment_id.clone(),
                reason,
            },
        )?;
    }
    review.learning_plan = automatic_learning_plan(&review.critical_moments)
        .map_err(GameReviewDecisionExplanationRebuildError::LearningPlan)?;
    Ok(review)
}

fn curriculum_knowledge_concepts(
) -> Result<Vec<CurriculumLearningConcept>, DecisionExplanationContractError> {
    let graph = knowledge::compiled_graph()?;
    Ok(graph
        .concepts()
        .map(|concept| match concept {
            KnowledgeConcept::Curriculum(concept) => concept,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision_explanation::tests::{canonical_build, improvement};

    fn exact_input(location: usize) -> DecisionExplanationReplayInput<usize> {
        let (persisted, _) = canonical_build();
        DecisionExplanationReplayInput {
            location,
            classification: improvement(),
            provenance: GameReviewMomentProvenance::Automatic,
            persisted,
        }
    }

    fn injected_failure(
        _: &DecisionExplanation,
        _: &GameReviewMomentClassification,
        _: GameReviewMomentProvenance,
    ) -> Result<DecisionExplanationBuild, DecisionExplanationContractError> {
        Err(DecisionExplanationContractError::InvalidProof(
            "injected replay failure",
        ))
    }

    #[test]
    fn empty_baseline_contains_every_compiled_concept_at_zero() {
        let baseline = DecisionExplanationReplayBaseline::empty().unwrap();

        assert_eq!(baseline.concept_path_counts.len(), 73);
        assert!(baseline
            .concept_path_counts
            .values()
            .all(|count| *count == 0));
    }

    #[test]
    fn differences_report_changed_totals() {
        let expected = DecisionExplanationReplayBaseline::empty().unwrap();
        let mut actual = expected.clone();
        actual.replayed_moment_count = 1;

        assert_eq!(
            expected.differences(&actual).unwrap(),
            vec!["replayedMomentCount: expected 0, actual 1"]
        );
    }

    #[test]
    fn differences_report_incomplete_concept_sets() {
        let expected = DecisionExplanationReplayBaseline::empty().unwrap();
        let mut actual = expected.clone();
        actual.concept_path_counts.pop_first();

        assert_eq!(
            expected.differences(&actual).unwrap(),
            vec!["actual conceptPathCounts does not contain the complete compiled concept set"]
        );
    }

    #[test]
    fn batch_reports_exact_diverged_abstained_and_failed_outcomes() {
        let exact =
            replay_decision_explanations(vec![exact_input(1)], NonZeroUsize::new(1).unwrap())
                .unwrap();
        assert_eq!(
            exact.observations[0].outcome,
            DecisionExplanationReplayOutcome::Exact
        );

        let mut diverged_input = exact_input(2);
        diverged_input.persisted.decision_explanation_ref =
            DecisionExplanationRef::try_from(format!("sha256:{}", "2".repeat(64))).unwrap();
        let diverged =
            replay_decision_explanations(vec![diverged_input], NonZeroUsize::new(1).unwrap())
                .unwrap();
        assert!(matches!(
            diverged.observations[0].outcome,
            DecisionExplanationReplayOutcome::Diverged { .. }
        ));

        let mut abstained_input = exact_input(3);
        abstained_input.provenance = GameReviewMomentProvenance::PlayerSelected;
        let abstained =
            replay_decision_explanations(vec![abstained_input], NonZeroUsize::new(1).unwrap())
                .unwrap();
        assert_eq!(
            abstained.observations[0].outcome,
            DecisionExplanationReplayOutcome::Failed(DecisionExplanationReplayFailure::Abstained)
        );

        let failed = replay_decision_explanations_with(
            vec![exact_input(4)],
            NonZeroUsize::new(1).unwrap(),
            injected_failure,
        )
        .unwrap();
        assert_eq!(
            failed.observations[0].outcome,
            DecisionExplanationReplayOutcome::Failed(DecisionExplanationReplayFailure::Rebuild(
                DecisionExplanationContractError::InvalidProof("injected replay failure")
            ))
        );
    }

    #[test]
    fn batch_includes_every_input_and_is_deterministic_across_concurrency() {
        let inputs = (1..=6).map(exact_input).collect::<Vec<_>>();
        let serial =
            replay_decision_explanations(inputs.clone(), NonZeroUsize::new(1).unwrap()).unwrap();
        let parallel = replay_decision_explanations(inputs, NonZeroUsize::new(4).unwrap()).unwrap();

        assert_eq!(parallel, serial);
        assert_eq!(parallel.observations.len(), 6);
        assert_eq!(parallel.baseline.replayed_moment_count, 6);
        assert_eq!(parallel.baseline.explanation_path_count, 6);
        assert_eq!(
            parallel
                .baseline
                .concept_path_counts
                .values()
                .sum::<usize>(),
            6
        );
    }

    #[test]
    fn combined_partition_reports_preserve_complete_aggregation() {
        let first = replay_decision_explanations(
            vec![exact_input(1), exact_input(2)],
            NonZeroUsize::new(2).unwrap(),
        )
        .unwrap();
        let second =
            replay_decision_explanations(vec![exact_input(3)], NonZeroUsize::new(2).unwrap())
                .unwrap();

        let combined = DecisionExplanationReplayReport::combine([first, second]).unwrap();

        assert_eq!(combined.observations.len(), 3);
        assert_eq!(combined.baseline.replayed_moment_count, 3);
        assert_eq!(combined.baseline.explanation_path_count, 3);
        assert_eq!(
            combined
                .baseline
                .concept_path_counts
                .values()
                .sum::<usize>(),
            3
        );
    }

    #[test]
    fn baseline_path_follows_selected_index() {
        let alternate_root = Path::new("alternate-corpus");
        assert_eq!(
            decision_explanation_replay_baseline_path(alternate_root),
            Path::new("alternate-corpus").join(BASELINE_FILE_NAME)
        );
    }
}
