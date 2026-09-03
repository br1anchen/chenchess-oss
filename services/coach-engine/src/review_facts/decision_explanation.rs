use std::{
    collections::BTreeMap,
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    decision_explanation::{explain_decision, DecisionExplanationBuild, DecisionExplanationInput},
    decision_learning::{decision_learning_build, decision_provenance, DecisionLearningBuild},
    engine_analysis::{
        EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer, EngineMultiPvOutput,
        EngineProvenance, PositionEvaluation,
    },
    operating_limits::REVIEW_FACTS_ENGINE_CONCURRENCY,
    provider_concurrency::collect_ordered_provider_positions,
    review_session_contract::{
        CandidateEvidence, CandidateGap, CriticalMomentId, DecisionLearningAbstentionReason,
        EngineCandidateEvidence, GameRef, GameReviewMomentClassification,
        GameReviewMomentProvenance, PlayerMoveEvidence, PositionSnapshot,
        RankedAlternativeEvidence,
    },
    rule_extractor::{MomentFact, RuleExtraction},
    types::Game,
};

use super::{
    game_review, ProviderAfterMove, ProviderEvidence, ReviewFactsError, ReviewPositionView,
};

pub(crate) const AUTOMATIC_DECISION_MULTI_PV: u8 = 3;

struct ComparisonCandidate<'a> {
    moment: &'a MomentFact,
    position: String,
    position_snapshot: PositionSnapshot,
    single_pv: CandidateEvidence,
    single_pv_build: DecisionLearningBuild,
}

pub(super) async fn analyze(
    engine: Arc<dyn EngineAnalyzer>,
    game: &Game,
    facts: &RuleExtraction,
    evidence: &[ProviderEvidence],
    position_views: &[ReviewPositionView],
    game_ref: &GameRef,
    authoritative_provenance: Option<&EngineProvenance>,
) -> Result<(BTreeMap<usize, DecisionLearningBuild>, Vec<Duration>), ReviewFactsError> {
    let post_move_lines = evidence
        .iter()
        .filter_map(|item| match &item.after_move {
            ProviderAfterMove::Analyzed {
                principal_variation,
                ..
            } if !principal_variation.is_empty() => Some((item.ply, principal_variation.clone())),
            ProviderAfterMove::Analyzed { .. } | ProviderAfterMove::Terminal => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut decision_builds = BTreeMap::new();
    let mut eligible = Vec::new();
    for moment in &facts.critical_moments {
        let position = game
            .moves
            .iter()
            .find(|game_move| game_move.ply == moment.ply)
            .map(|game_move| game_move.position.clone())
            .ok_or(ReviewFactsError::UnknownSelectedPly(moment.ply))?;
        let position_snapshot = position_views
            .iter()
            .find(|view| view.ply == moment.ply)
            .map(|view| view.position_snapshot.clone())
            .ok_or(ReviewFactsError::UnknownSelectedPly(moment.ply))?;
        let Some(single_pv) = single_pv_evidence(
            moment,
            post_move_lines.get(&moment.ply).map(Vec::as_slice),
            authoritative_provenance,
        )?
        else {
            decision_builds.insert(
                moment.ply,
                DecisionLearningBuild::Abstained {
                    reason: DecisionLearningAbstentionReason::CandidateEvidenceUnavailable,
                },
            );
            continue;
        };
        let single_pv_build =
            build_single_pv(game_ref, moment, &position_snapshot, single_pv.clone())?;
        if needs_candidate_comparison(&moment.classification, &single_pv_build) {
            eligible.push(ComparisonCandidate {
                moment,
                position,
                position_snapshot,
                single_pv,
                single_pv_build,
            });
        } else {
            decision_builds.insert(moment.ply, single_pv_build);
        }
    }
    if !engine.supports_multi_pv() {
        for candidate in eligible {
            decision_builds.insert(
                candidate.moment.ply,
                comparison_fallback(
                    candidate.single_pv_build,
                    CandidateComparisonFailure::Unavailable,
                ),
            );
        }
        return Ok((decision_builds, Vec::new()));
    }
    let outputs = collect_ordered_provider_positions(
        eligible
            .iter()
            .map(|candidate| candidate.position.clone())
            .collect(),
        REVIEW_FACTS_ENGINE_CONCURRENCY,
        move |position| {
            let engine = engine.clone();
            async move {
                let started = Instant::now();
                let output = engine
                    .analyze_multi_pv(
                        EngineAnalysisInput {
                            position: &position,
                        },
                        AUTOMATIC_DECISION_MULTI_PV,
                    )
                    .await;
                Ok::<_, Infallible>((output, started.elapsed()))
            }
        },
    )
    .await
    .unwrap_or_else(|failure| match failure.error {});

    let mut timings = Vec::with_capacity(outputs.len());
    for (candidate, (output, duration)) in eligible.into_iter().zip(outputs) {
        let build = match output {
            Ok(output) => build_after_comparison(
                game_ref,
                candidate.moment,
                &candidate.position_snapshot,
                candidate.single_pv,
                candidate.single_pv_build,
                output,
            )?,
            Err(error) => {
                log_comparison_failure(candidate.moment.ply, &error);
                comparison_fallback(
                    candidate.single_pv_build,
                    CandidateComparisonFailure::Unavailable,
                )
            }
        };
        decision_builds.insert(candidate.moment.ply, build);
        timings.push(duration);
    }
    Ok((decision_builds, timings))
}

pub(crate) fn single_pv_evidence(
    moment: &MomentFact,
    post_move_line: Option<&[String]>,
    provenance: Option<&EngineProvenance>,
) -> Result<Option<CandidateEvidence>, ReviewFactsError> {
    let Some(provenance) = provenance.and_then(decision_provenance) else {
        return Ok(None);
    };
    let perspective = game_review::color(moment.side);
    let authoritative = EngineCandidateEvidence {
        rank: 1,
        root_move_uci: moment.objective.best_move.clone(),
        evaluation: game_review::evaluation(moment.objective.best_evaluation, perspective)?,
        variation: moment.objective.principal_variation.clone(),
        provenance: provenance.clone(),
    };
    let mut retained_variation = vec![moment.objective.played_move.clone()];
    retained_variation.extend(post_move_line.unwrap_or_default().iter().cloned());
    if moment.objective.best_move == moment.objective.played_move && retained_variation.len() == 1 {
        retained_variation = moment.objective.principal_variation.clone();
    }
    Ok(Some(CandidateEvidence::SinglePv {
        authoritative,
        player_move: PlayerMoveEvidence {
            root_move_uci: moment.objective.played_move.clone(),
            evaluation: game_review::evaluation(moment.objective.played_evaluation, perspective)?,
            retained_variation,
            provenance,
        },
    }))
}

pub(crate) fn enrich(
    single_pv: &CandidateEvidence,
    output: EngineMultiPvOutput,
    moment: &MomentFact,
) -> Option<CandidateEvidence> {
    let CandidateEvidence::SinglePv {
        authoritative,
        player_move,
    } = single_pv
    else {
        return None;
    };
    let provenance = output.provenance.as_ref().and_then(decision_provenance)?;
    // Rank one is dropped rather than restated: the SinglePV record already
    // scores that move, and a MultiPV rank-one score is a second, noisier
    // reading of the same position. What MultiPV owns is the ordering below it,
    // so alternatives keep only their shortfall against rank one, measured
    // inside this one search (ADR 0041).
    let mut variations = output.variations;
    variations.sort_by_key(|variation| variation.rank);
    let (best, alternatives) = variations.split_first()?;
    if best.rank != 1 || best.analysis.best_move != moment.objective.best_move {
        return None;
    }
    let ranked_alternatives = alternatives
        .iter()
        .map(|alternative| RankedAlternativeEvidence {
            rank: alternative.rank,
            root_move_uci: alternative.analysis.best_move.clone(),
            gap: candidate_gap(best.analysis.evaluation, alternative.analysis.evaluation),
            variation: alternative.analysis.principal_variation.clone(),
            provenance: provenance.clone(),
        })
        .collect();
    Some(CandidateEvidence::MultiPv {
        authoritative_single_pv: authoritative.clone(),
        requested_count: output.requested_variations,
        ranked_alternatives,
        player_move: player_move.clone(),
    })
}

pub(crate) fn build_after_comparison(
    game_ref: &GameRef,
    moment: &MomentFact,
    position_snapshot: &PositionSnapshot,
    single_pv: CandidateEvidence,
    single_pv_build: DecisionLearningBuild,
    output: EngineMultiPvOutput,
) -> Result<DecisionLearningBuild, ReviewFactsError> {
    let Some(candidate_evidence) = enrich(&single_pv, output, moment) else {
        return Ok(comparison_fallback(
            single_pv_build,
            CandidateComparisonFailure::Rejected,
        ));
    };
    let compared = build(game_ref, moment, position_snapshot, candidate_evidence)?;
    let compared = normalize(compared)?;
    if matches!(
        &compared,
        DecisionLearningBuild::Abstained {
            reason: DecisionLearningAbstentionReason::CandidateEvidenceRejected
        }
    ) {
        return Ok(comparison_fallback(
            single_pv_build,
            CandidateComparisonFailure::Rejected,
        ));
    }
    Ok(compared)
}

pub(crate) fn build_single_pv(
    game_ref: &GameRef,
    moment: &MomentFact,
    position_snapshot: &PositionSnapshot,
    candidate_evidence: CandidateEvidence,
) -> Result<DecisionLearningBuild, ReviewFactsError> {
    normalize(build(
        game_ref,
        moment,
        position_snapshot,
        candidate_evidence,
    )?)
}

pub(crate) fn needs_candidate_comparison(
    classification: &GameReviewMomentClassification,
    single_pv_build: &DecisionLearningBuild,
) -> bool {
    match (classification, single_pv_build) {
        (
            GameReviewMomentClassification::ImprovementOpportunity { .. },
            DecisionLearningBuild::TrackSelected { .. }
            | DecisionLearningBuild::ExplanationUnmapped { .. }
            | DecisionLearningBuild::Abstained {
                reason: DecisionLearningAbstentionReason::NoProofValidConcept,
            },
        ) => true,
        (
            GameReviewMomentClassification::PositiveHighlight { .. },
            DecisionLearningBuild::TrackSelected { .. }
            | DecisionLearningBuild::ExplanationUnmapped { .. },
        ) => true,
        (
            GameReviewMomentClassification::Neutral { .. }
            | GameReviewMomentClassification::ImprovementOpportunity { .. }
            | GameReviewMomentClassification::PositiveHighlight { .. },
            DecisionLearningBuild::Abstained { .. },
        )
        | (GameReviewMomentClassification::Neutral { .. }, _) => false,
    }
}

/// Both scores come from the same MultiPV search and are stated for the side to
/// move, so they are directly comparable to each other — and to nothing else.
fn candidate_gap(best: PositionEvaluation, alternative: PositionEvaluation) -> CandidateGap {
    use PositionEvaluation::{Centipawns, MateIn};
    match (best, alternative) {
        // A rank below one that scores above rank one contradicts its own ranking. Report
        // that rather than clamping the negative shortfall to zero, which would publish a
        // tie the search never found.
        (Centipawns(best), Centipawns(alternative)) => {
            match u32::try_from(i64::from(best).saturating_sub(i64::from(alternative))) {
                Ok(behind_best) => CandidateGap::Centipawns { behind_best },
                Err(_) => CandidateGap::Incommensurable,
            }
        }
        (MateIn(best), MateIn(alternative)) if best > 0 && alternative >= best => {
            CandidateGap::SlowerMate {
                extra_plies: u16::try_from(alternative.saturating_sub(best)).unwrap_or(u16::MAX),
            }
        }
        (MateIn(best), Centipawns(_)) if best > 0 => CandidateGap::MissesForcedMate,
        (Centipawns(_), MateIn(alternative)) if alternative < 0 => CandidateGap::ConcedesForcedMate,
        _ => CandidateGap::Incommensurable,
    }
}

pub(crate) fn build(
    game_ref: &GameRef,
    moment: &MomentFact,
    position_snapshot: &PositionSnapshot,
    candidate_evidence: CandidateEvidence,
) -> Result<DecisionExplanationBuild, ReviewFactsError> {
    explain_decision(DecisionExplanationInput {
        game_ref: game_ref.clone(),
        critical_moment_id: CriticalMomentId::for_imported_game(
            game_ref,
            u16::try_from(moment.ply).map_err(|_| {
                ReviewFactsError::Contract("ply exceeds review-session limits".to_string())
            })?,
        ),
        position_snapshot: position_snapshot.clone(),
        classification: moment.classification.clone(),
        provenance: GameReviewMomentProvenance::Automatic,
        player_move_uci: moment.objective.played_move.clone(),
        candidate_evidence,
    })
    .map_err(|error| ReviewFactsError::Contract(error.to_string()))
}

enum CandidateComparisonFailure {
    Unavailable,
    Rejected,
}

fn comparison_fallback(
    build: DecisionLearningBuild,
    failure: CandidateComparisonFailure,
) -> DecisionLearningBuild {
    match build {
        selected @ (DecisionLearningBuild::TrackSelected { .. }
        | DecisionLearningBuild::ExplanationUnmapped { .. }) => selected,
        DecisionLearningBuild::Abstained { .. } => DecisionLearningBuild::Abstained {
            reason: match failure {
                CandidateComparisonFailure::Unavailable => {
                    DecisionLearningAbstentionReason::CandidateComparisonUnavailable
                }
                CandidateComparisonFailure::Rejected => {
                    DecisionLearningAbstentionReason::CandidateComparisonRejected
                }
            },
        },
    }
}

fn normalize(build: DecisionExplanationBuild) -> Result<DecisionLearningBuild, ReviewFactsError> {
    decision_learning_build(build).map_err(|error| ReviewFactsError::Contract(error.to_string()))
}

fn log_comparison_failure(ply: usize, error: &EngineAnalysisError) {
    tracing::warn!(
        category = "decision_candidate_comparison",
        ply,
        %error,
        "optional candidate comparison failed; retained SinglePV decision evidence"
    );
}

#[cfg(test)]
mod tests {
    use super::candidate_gap;
    use crate::{engine_analysis::PositionEvaluation, review_session_contract::CandidateGap};

    /// A rank below one scoring above rank one contradicts its own ranking. Reporting it
    /// as a zero shortfall would publish a tie the search never found, and would let the
    /// alternative keep a lower rank while claiming parity with the best move.
    #[test]
    fn an_alternative_scoring_above_rank_one_is_incommensurable_rather_than_a_tie() {
        assert_eq!(
            candidate_gap(
                PositionEvaluation::Centipawns(30),
                PositionEvaluation::Centipawns(45),
            ),
            CandidateGap::Incommensurable
        );
        assert_eq!(
            candidate_gap(
                PositionEvaluation::Centipawns(30),
                PositionEvaluation::Centipawns(30),
            ),
            CandidateGap::Centipawns { behind_best: 0 }
        );
        assert_eq!(
            candidate_gap(PositionEvaluation::MateIn(3), PositionEvaluation::MateIn(1),),
            CandidateGap::Incommensurable
        );
    }
}
