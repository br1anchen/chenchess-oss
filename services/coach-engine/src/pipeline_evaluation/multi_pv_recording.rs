//! Recorded MultiPV comparison searches for the deterministic Pipeline Evaluation gate.
//!
//! The corpus records single-PV provider output per ply, which is everything the Rule
//! Extractor needs. The candidate-comparison stage of a Decision Explanation needs a
//! second, MultiPV search per Critical Moment, and until that search was recorded the
//! offline gate never exercised it — a Ranked Alternative could contradict the
//! authoritative evaluation without any baseline moving (ADR 0041).
//!
//! Recording that search here keeps the gate offline: `enrich` and `explain_decision`
//! are pure over the recorded output, so `evaluate-fast` replays the comparison with no
//! engine at all.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    decision_learning::DecisionLearningBuild,
    engine_analysis::{
        EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer, EngineMultiPvOutput,
        EngineProvenance,
    },
    review_facts::{decision_explanation, ProviderAfterMove, ProviderEvidence},
    review_session_contract::{
        build_position_snapshot, CandidateEvidence, DecisionExplanationRef,
        DecisionLearningAbstentionReason, GameRef, PositionSnapshot, PreferenceProof,
        ProofCapability,
    },
    rule_extractor::{MomentFact, RuleExtraction},
    types::Game,
};

/// One MultiPV comparison search recorded against the Position of one Critical Moment.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RecordedMultiPv {
    pub(super) ply: usize,
    pub(super) output: EngineMultiPvOutput,
}

/// A Critical Moment that reached the candidate-comparison stage, paired with the
/// single-PV evidence the comparison is measured against.
pub(super) struct ComparableMoment<'a> {
    pub(super) moment: &'a MomentFact,
    pub(super) position: String,
    position_snapshot: PositionSnapshot,
    single_pv: CandidateEvidence,
    single_pv_build: DecisionLearningBuild,
}

/// The Decision Explanation the recorded comparison produces, as it lands in the baseline.
///
/// The comparison stage decides the candidate evidence, the preference proof, and the
/// capability, so those are stated in full and read as a diff. Everything else the
/// explanation carries — its snapshots, facts, candidates, and paths — is covered by
/// `decisionExplanationRef`, which digests the whole explanation, so a change anywhere
/// still moves the baseline without burying the comparison in the surrounding proof.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecordedDecisionProof {
    ply: usize,
    #[serde(flatten)]
    outcome: RecordedDecisionOutcome,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum RecordedDecisionOutcome {
    Durable {
        decision_explanation_ref: DecisionExplanationRef,
        candidate_evidence: Box<CandidateEvidence>,
        preference: Option<PreferenceProof>,
        capability: ProofCapability,
        diagnostics: Vec<&'static str>,
    },
    Abstained {
        diagnostics: Vec<&'static str>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum MultiPvRecordingError {
    #[error("the corpus Game has no move at ply {0}")]
    UnknownPly(usize),
    #[error("the Position at ply {0} is not a legal Chess Position")]
    InvalidPosition(usize),
    #[error("ply {0} reached candidate comparison with no recorded MultiPV evidence")]
    MissingMultiPv(usize),
    #[error("recorded MultiPV evidence at ply {0} belongs to no compared Critical Moment")]
    UnusedMultiPv(usize),
    #[error("ply {0} carries more than one recorded MultiPV search")]
    DuplicateMultiPv(usize),
    #[error("the Game has Critical Moments but records no authoritative engine provenance")]
    MissingEngineProvenance,
    #[error("Decision Explanation at ply {ply} is not durable: {source}")]
    Explanation {
        ply: usize,
        source: crate::review_facts::ReviewFactsError,
    },
}

/// Selects the Critical Moments whose Decision Explanation needs a MultiPV comparison.
///
/// Mirrors the eligibility the live pipeline applies before it spends a MultiPV search,
/// so the recorder asks the engine for exactly the Positions the offline gate replays.
///
/// A Game with Critical Moments and no authoritative provenance is refused rather than
/// treated as having nothing to compare: that silence is exactly the coverage hole this
/// module exists to close, and it would otherwise return the moment a fixture is added
/// or migrated without recording its comparison.
pub(super) fn comparable_moments<'a>(
    game_ref: &GameRef,
    game: &Game,
    facts: &'a RuleExtraction,
    evidence: &[ProviderEvidence],
    engine_provenance: Option<&EngineProvenance>,
) -> Result<Vec<ComparableMoment<'a>>, MultiPvRecordingError> {
    if engine_provenance.is_none() && !facts.critical_moments.is_empty() {
        return Err(MultiPvRecordingError::MissingEngineProvenance);
    }
    let mut comparable = Vec::new();
    for moment in &facts.critical_moments {
        let position = position_at(game, moment.ply)?;
        let position_snapshot = position_snapshot_at(game, moment.ply)?;
        let Some(single_pv) = decision_explanation::single_pv_evidence(
            moment,
            post_move_line(evidence, moment.ply),
            engine_provenance,
        )
        .map_err(|_| MultiPvRecordingError::UnknownPly(moment.ply))?
        else {
            continue;
        };
        let single_pv_build = decision_explanation::build_single_pv(
            game_ref,
            moment,
            &position_snapshot,
            single_pv.clone(),
        )
        .map_err(|source| MultiPvRecordingError::Explanation {
            ply: moment.ply,
            source,
        })?;
        if !decision_explanation::needs_candidate_comparison(
            &moment.classification,
            &single_pv_build,
        ) {
            continue;
        }
        comparable.push(ComparableMoment {
            moment,
            position,
            position_snapshot,
            single_pv,
            single_pv_build,
        });
    }
    Ok(comparable)
}

/// Runs the comparison search for one Critical Moment and captures it verbatim.
pub(super) async fn record(
    engine: &dyn EngineAnalyzer,
    moment: &ComparableMoment<'_>,
    variation_count: u8,
) -> Result<RecordedMultiPv, EngineAnalysisError> {
    let output = engine
        .analyze_multi_pv(
            EngineAnalysisInput {
                position: &moment.position,
            },
            variation_count,
        )
        .await?;
    Ok(RecordedMultiPv {
        ply: moment.moment.ply,
        output,
    })
}

/// Replays every recorded comparison into the Decision Explanation it produces.
///
/// Every comparable moment must have a recording and every recording must be consumed:
/// a corpus that drifts into a new Critical Moment fails the gate loudly rather than
/// silently dropping the comparison it never recorded.
pub(super) fn decision_proofs(
    game_ref: &GameRef,
    comparable: Vec<ComparableMoment<'_>>,
    recordings: &[RecordedMultiPv],
) -> Result<Vec<RecordedDecisionProof>, MultiPvRecordingError> {
    let mut seen = BTreeSet::new();
    for recording in recordings {
        if !comparable
            .iter()
            .any(|candidate| candidate.moment.ply == recording.ply)
        {
            return Err(MultiPvRecordingError::UnusedMultiPv(recording.ply));
        }
        // Only the first recording for a ply is ever read, so a duplicate would ride in a
        // fixture unexercised and unreported by the baseline diff.
        if !seen.insert(recording.ply) {
            return Err(MultiPvRecordingError::DuplicateMultiPv(recording.ply));
        }
    }
    comparable
        .into_iter()
        .map(|candidate| {
            let ply = candidate.moment.ply;
            let recorded = recordings
                .iter()
                .find(|recording| recording.ply == ply)
                .ok_or(MultiPvRecordingError::MissingMultiPv(ply))?;
            let build = decision_explanation::build_after_comparison(
                game_ref,
                candidate.moment,
                &candidate.position_snapshot,
                candidate.single_pv,
                candidate.single_pv_build,
                recorded.output.clone(),
            )
            .map_err(|source| MultiPvRecordingError::Explanation { ply, source })?;
            Ok(RecordedDecisionProof {
                ply,
                outcome: outcome(build),
            })
        })
        .collect()
}

fn outcome(build: DecisionLearningBuild) -> RecordedDecisionOutcome {
    match build {
        DecisionLearningBuild::TrackSelected { explanation, .. } => {
            let explanation = *explanation;
            RecordedDecisionOutcome::Durable {
                decision_explanation_ref: explanation.decision_explanation_ref,
                candidate_evidence: Box::new(explanation.candidate_evidence),
                preference: explanation.preference,
                capability: explanation.capability,
                diagnostics: Vec::new(),
            }
        }
        DecisionLearningBuild::ExplanationUnmapped { explanation } => {
            let explanation = *explanation;
            RecordedDecisionOutcome::Durable {
                decision_explanation_ref: explanation.decision_explanation_ref,
                candidate_evidence: Box::new(explanation.candidate_evidence),
                preference: explanation.preference,
                capability: explanation.capability,
                diagnostics: vec!["resourceMappingUnavailable"],
            }
        }
        DecisionLearningBuild::Abstained { reason } => RecordedDecisionOutcome::Abstained {
            diagnostics: vec![reason_name(reason)],
        },
    }
}

fn reason_name(reason: DecisionLearningAbstentionReason) -> &'static str {
    match reason {
        DecisionLearningAbstentionReason::CandidateEvidenceUnavailable => {
            "candidateEvidenceUnavailable"
        }
        DecisionLearningAbstentionReason::CandidateEvidenceRejected => "candidateEvidenceRejected",
        DecisionLearningAbstentionReason::CandidateComparisonUnavailable => {
            "candidateComparisonUnavailable"
        }
        DecisionLearningAbstentionReason::CandidateComparisonRejected => {
            "candidateComparisonRejected"
        }
        DecisionLearningAbstentionReason::NoProofValidConcept => "noProofValidConcept",
    }
}

fn post_move_line(evidence: &[ProviderEvidence], ply: usize) -> Option<&[String]> {
    evidence
        .iter()
        .find(|item| item.ply == ply)
        .and_then(|item| match &item.after_move {
            ProviderAfterMove::Analyzed {
                principal_variation,
                ..
            } if !principal_variation.is_empty() => Some(principal_variation.as_slice()),
            ProviderAfterMove::Analyzed { .. } | ProviderAfterMove::Terminal => None,
        })
}

fn position_at(game: &Game, ply: usize) -> Result<String, MultiPvRecordingError> {
    game.moves
        .iter()
        .find(|game_move| game_move.ply == ply)
        .map(|game_move| game_move.position.clone())
        .ok_or(MultiPvRecordingError::UnknownPly(ply))
}

fn position_snapshot_at(
    game: &Game,
    ply: usize,
) -> Result<PositionSnapshot, MultiPvRecordingError> {
    let (index, game_move) = game
        .moves
        .iter()
        .enumerate()
        .find(|(_, game_move)| game_move.ply == ply)
        .ok_or(MultiPvRecordingError::UnknownPly(ply))?;
    let preceding = game
        .moves
        .iter()
        .take(index)
        .map(|game_move| game_move.position.as_str())
        .collect::<Vec<_>>();
    build_position_snapshot(&game_move.position, &preceding)
        .map_err(|_| MultiPvRecordingError::InvalidPosition(ply))
}
