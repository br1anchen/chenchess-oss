use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    decision_learning::DecisionLearningBuild,
    engine_analysis::{
        EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer, EngineProvenance,
        TimedEngineAnalysis,
    },
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction},
    operating_limits::{REVIEW_FACTS_ENGINE_CONCURRENCY, REVIEW_FACTS_HUMAN_CONCURRENCY},
    pgn::{parse_pgn, PgnImportError},
    provider_concurrency::{collect_ordered_provider_positions, IndexedProviderError},
    review_session_board::coordinate_text_board,
    review_session_contract::{
        build_position_snapshot, GameRef, GameReview, OpeningMetadata, PositionSnapshot,
    },
    rule_extractor::{self, AfterMoveEvidence, MoveEvidence, RuleExtraction, RuleExtractorError},
    types::{EloProfile, Game, ReviewSide},
};
use serde::{Deserialize, Serialize};

pub(crate) mod decision_explanation;
pub(crate) mod game_review;

pub struct ReviewFactsInput<'a> {
    pub pgn: &'a str,
    pub player_elo: EloProfile,
    pub review_side: ReviewSide,
    pub opening_identification: &'a OpeningMetadata,
}

struct ReviewAnalysis {
    game: Game,
    facts: RuleExtraction,
    position_views: Vec<ReviewPositionView>,
    opening_identification: OpeningMetadata,
    decision_builds: BTreeMap<usize, DecisionLearningBuild>,
}

struct ReviewPositionView {
    ply: usize,
    position_snapshot: PositionSnapshot,
    text_board: String,
    evaluation: crate::engine_analysis::PositionEvaluation,
}

pub(crate) struct TimedGameReview {
    pub(crate) review: GameReview,
    pub(crate) player_selected_moments:
        Vec<crate::review_session_contract::GameReviewCriticalMoment>,
    pub(crate) engine_provenance: Option<EngineProvenance>,
    pub(crate) provider_timings: ReviewProviderTimings,
}

pub(crate) struct ReviewProviderTimings {
    pub(crate) engine_analysis: Vec<Duration>,
    pub(crate) human_move_model: Vec<Duration>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderEvidence {
    pub ply: usize,
    pub engine_before: EngineAnalysis,
    pub after_move: ProviderAfterMove,
    pub human_before: HumanMovePrediction,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum ProviderAfterMove {
    Analyzed {
        evaluation: crate::engine_analysis::PositionEvaluation,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        principal_variation: Vec<String>,
    },
    Terminal,
}

impl ProviderEvidence {
    pub(crate) fn as_rule_evidence(&self) -> MoveEvidence<'_> {
        MoveEvidence {
            ply: self.ply,
            engine_before: &self.engine_before,
            after_move: match &self.after_move {
                ProviderAfterMove::Analyzed { evaluation, .. } => {
                    AfterMoveEvidence::Analyzed(*evaluation)
                }
                ProviderAfterMove::Terminal => AfterMoveEvidence::Terminal,
            },
            human_before: &self.human_before,
        }
    }
}

#[derive(Clone)]
pub struct ReviewFactsService {
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
}

impl ReviewFactsService {
    pub fn new(engine: Arc<dyn EngineAnalyzer>, human: Arc<dyn HumanMoveModel>) -> Self {
        Self { engine, human }
    }

    pub(crate) async fn review_session_game(
        &self,
        input: ReviewFactsInput<'_>,
        game_ref: &GameRef,
    ) -> Result<TimedGameReview, ReviewFactsError> {
        let player_elo = input.player_elo;
        let (output, evidence, provider_timings, engine_provenance) =
            self.analyze_game_review(input, game_ref).await?;
        let player_selected_moments = game_review::player_selected_moments(
            &output.game,
            player_elo,
            &evidence,
            game_ref,
            &output.opening_identification,
        )?;
        Ok(TimedGameReview {
            review: game_review::build(output, evidence, game_ref)?,
            player_selected_moments,
            engine_provenance,
            provider_timings,
        })
    }

    async fn analyze_game_review(
        &self,
        input: ReviewFactsInput<'_>,
        game_ref: &GameRef,
    ) -> Result<
        (
            ReviewAnalysis,
            Vec<ProviderEvidence>,
            ReviewProviderTimings,
            Option<EngineProvenance>,
        ),
        ReviewFactsError,
    > {
        let game = parse_pgn(input.pgn)?;
        let analyzed = self.analyze_game(&game, input.player_elo).await?;
        let evidence = analyzed.game.into_evidence();
        let rule_evidence = evidence
            .iter()
            .map(ProviderEvidence::as_rule_evidence)
            .collect::<Vec<_>>();
        let facts =
            rule_extractor::extract(&game, input.player_elo, input.review_side, &rule_evidence)?;
        let position_views = facts
            .critical_moments
            .iter()
            .map(|moment| review_position_view(&game, &evidence, moment.ply))
            .collect::<Result<Vec<_>, ReviewFactsError>>()?;
        let (decision_builds, multi_pv_timings) = decision_explanation::analyze(
            self.engine.clone(),
            &game,
            &facts,
            &evidence,
            &position_views,
            game_ref,
            analyzed.engine_provenance.as_ref(),
        )
        .await?;
        Ok((
            ReviewAnalysis {
                game,
                facts,
                position_views,
                opening_identification: input.opening_identification.clone(),
                decision_builds,
            },
            evidence,
            ReviewProviderTimings {
                engine_analysis: analyzed
                    .provider_timings
                    .engine_analysis
                    .into_iter()
                    .chain(multi_pv_timings)
                    .collect(),
                human_move_model: analyzed.provider_timings.human_move_model,
            },
            analyzed.engine_provenance,
        ))
    }

    /// Returns the recorded provider evidence beside the provenance of the Engine
    /// Analysis that produced it, which is what a published candidate is stamped with.
    pub(crate) async fn collect_game_evidence(
        &self,
        game: &Game,
        elo: EloProfile,
    ) -> Result<(Vec<ProviderEvidence>, Option<EngineProvenance>), ReviewFactsError> {
        let analyzed = self.analyze_game(game, elo).await?;
        Ok((
            analyzed.game.into_evidence(),
            analyzed.engine_provenance.clone(),
        ))
    }

    pub(crate) fn engine(&self) -> Arc<dyn EngineAnalyzer> {
        self.engine.clone()
    }

    pub(crate) async fn collect_selected_evidence(
        &self,
        game: &Game,
        elo: EloProfile,
        selected_ply: usize,
    ) -> Result<ProviderEvidence, ReviewFactsError> {
        let selected_index = game
            .moves
            .iter()
            .position(|game_move| game_move.ply == selected_ply)
            .ok_or(ReviewFactsError::UnknownSelectedPly(selected_ply))?;
        let selected_move = &game.moves[selected_index];
        let position_analysis = self.analyze_position(&selected_move.position, elo).await?;
        let after_position = game
            .moves
            .get(selected_index + 1)
            .map(|next| next.position.as_str())
            .or_else(|| (!game.is_terminal).then_some(game.final_position.as_str()));
        let after_move = match after_position {
            Some(position) => {
                let analysis = self
                    .engine
                    .analyze(EngineAnalysisInput { position })
                    .await?;
                ProviderAfterMove::Analyzed {
                    evaluation: analysis.evaluation,
                    principal_variation: analysis.principal_variation,
                }
            }
            None => ProviderAfterMove::Terminal,
        };
        Ok(ProviderEvidence {
            ply: selected_move.ply,
            engine_before: position_analysis.engine,
            after_move,
            human_before: position_analysis.human,
        })
    }

    async fn analyze_game(
        &self,
        game: &Game,
        elo: EloProfile,
    ) -> Result<TimedAnalyzedGame, ReviewFactsError> {
        // ADR 0003 allows the Local Pipeline Runtime to sequence full-Game provider phases when
        // measured CPU contention makes overlap slower. If Engine Analysis fails, probe Maia only
        // through that Position so provider-error precedence remains identical to the old
        // per-Position pipeline.
        let engine_analyses = match self.analyze_game_engine_positions(game).await {
            Ok(analyses) => analyses,
            Err(engine_failure) => {
                match self
                    .predict_game_positions(game, elo, engine_failure.index + 1)
                    .await
                {
                    Err(human_failure) if human_failure.index < engine_failure.index => {
                        return Err(human_failure.error);
                    }
                    _ => return Err(engine_failure.error),
                }
            }
        };
        let engine_provenance = common_engine_provenance(&engine_analyses)?;
        let human_analyses = self
            .predict_game_positions(game, elo, game.moves.len())
            .await
            .map_err(|failure| failure.error)?;
        let mut engine_analyses = engine_analyses.into_iter();
        let mut moves = Vec::with_capacity(game.moves.len());
        let mut engine_analysis =
            Vec::with_capacity(game.moves.len() + usize::from(!game.is_terminal));
        let mut human_move_model = Vec::with_capacity(game.moves.len());
        for (game_move, human) in game.moves.iter().zip(human_analyses) {
            let engine = engine_analyses
                .next()
                .expect("every Game move should have Engine Analysis");
            engine_analysis.push(engine.duration);
            human_move_model.push(human.duration);
            moves.push(AnalyzedMove {
                ply: game_move.ply,
                engine: engine.analysis,
                human: human.prediction,
            });
        }
        let final_position = if game.is_terminal {
            FinalPositionEvidence::Terminal
        } else {
            let final_engine = engine_analyses
                .next()
                .expect("a nonterminal Game should have final-position Engine Analysis");
            engine_analysis.push(final_engine.duration);
            FinalPositionEvidence::Analyzed(final_engine.analysis)
        };
        assert!(
            engine_analyses.next().is_none(),
            "Game analysis should consume every Engine Analysis result"
        );
        Ok(TimedAnalyzedGame {
            game: AnalyzedGame {
                moves,
                final_position,
            },
            provider_timings: ReviewProviderTimings {
                engine_analysis,
                human_move_model,
            },
            engine_provenance,
        })
    }

    async fn predict_game_positions(
        &self,
        game: &Game,
        elo: EloProfile,
        position_count: usize,
    ) -> Result<Vec<TimedHumanMovePrediction>, IndexedProviderError<ReviewFactsError>> {
        let service = self.clone();
        let positions = game
            .moves
            .iter()
            .take(position_count)
            .map(|game_move| game_move.position.clone())
            .collect();
        collect_ordered_provider_positions(
            positions,
            REVIEW_FACTS_HUMAN_CONCURRENCY,
            move |position| {
                let service = service.clone();
                async move { service.predict_human_move(&position, elo).await }
            },
        )
        .await
    }

    async fn analyze_game_engine_positions(
        &self,
        game: &Game,
    ) -> Result<Vec<TimedEngineAnalysis>, IndexedProviderError<ReviewFactsError>> {
        let positions = game
            .moves
            .iter()
            .map(|game_move| game_move.position.clone())
            .chain((!game.is_terminal).then(|| game.final_position.clone()))
            .collect::<Vec<_>>();
        self.engine
            .clone()
            .analyze_positions(positions, REVIEW_FACTS_ENGINE_CONCURRENCY)
            .await
            .map_err(|failure| IndexedProviderError {
                index: failure.index,
                error: failure.error.into(),
            })
    }

    async fn analyze_position(
        &self,
        position: &str,
        elo: EloProfile,
    ) -> Result<TimedPositionAnalysis, ReviewFactsError> {
        let (engine, human) = tokio::join!(
            self.analyze_engine(position),
            self.predict_human_move(position, elo)
        );
        let engine = engine?;
        let human = human?;
        Ok(TimedPositionAnalysis {
            engine: engine.analysis,
            human: human.prediction,
        })
    }

    async fn analyze_engine(
        &self,
        position: &str,
    ) -> Result<TimedEngineAnalysis, ReviewFactsError> {
        let started = Instant::now();
        let output = self
            .engine
            .analyze_with_provenance(EngineAnalysisInput { position })
            .await?;
        Ok(TimedEngineAnalysis {
            analysis: output.analysis,
            provenance: output.provenance,
            duration: started.elapsed(),
        })
    }

    async fn predict_human_move(
        &self,
        position: &str,
        elo: EloProfile,
    ) -> Result<TimedHumanMovePrediction, ReviewFactsError> {
        let started = Instant::now();
        let prediction = self
            .human
            .predict(HumanMoveInput {
                position,
                elo,
                limit: 5,
            })
            .await?;
        Ok(TimedHumanMovePrediction {
            prediction,
            duration: started.elapsed(),
        })
    }
}

struct TimedPositionAnalysis {
    engine: EngineAnalysis,
    human: HumanMovePrediction,
}

struct TimedHumanMovePrediction {
    prediction: HumanMovePrediction,
    duration: Duration,
}

fn review_position_view(
    game: &Game,
    evidence: &[ProviderEvidence],
    ply: usize,
) -> Result<ReviewPositionView, ReviewFactsError> {
    let (index, game_move) = game
        .moves
        .iter()
        .enumerate()
        .find(|(_, game_move)| game_move.ply == ply)
        .ok_or(ReviewFactsError::UnknownSelectedPly(ply))?;
    let preceding = game
        .moves
        .iter()
        .take(index)
        .map(|game_move| game_move.position.as_str())
        .collect::<Vec<_>>();
    let position_snapshot = build_position_snapshot(&game_move.position, &preceding)
        .map_err(|_| ReviewFactsError::InvalidPosition(ply))?;
    let evaluation = evidence
        .iter()
        .find(|item| item.ply == ply)
        .map(|item| item.engine_before.evaluation)
        .ok_or(ReviewFactsError::MissingPositionEvidence(ply))?;
    Ok(ReviewPositionView {
        ply,
        text_board: coordinate_text_board(&position_snapshot),
        position_snapshot,
        evaluation,
    })
}

fn common_engine_provenance(
    analyses: &[TimedEngineAnalysis],
) -> Result<Option<EngineProvenance>, ReviewFactsError> {
    let provenance = analyses
        .first()
        .and_then(|analysis| analysis.provenance.clone());
    if analyses
        .iter()
        .any(|analysis| analysis.provenance != provenance)
    {
        return Err(ReviewFactsError::Contract(
            "Game Review Engine Analysis used inconsistent provider provenance".to_string(),
        ));
    }
    Ok(provenance)
}

struct AnalyzedGame {
    moves: Vec<AnalyzedMove>,
    final_position: FinalPositionEvidence,
}

struct TimedAnalyzedGame {
    game: AnalyzedGame,
    provider_timings: ReviewProviderTimings,
    engine_provenance: Option<EngineProvenance>,
}

enum FinalPositionEvidence {
    Terminal,
    Analyzed(EngineAnalysis),
}

struct AnalyzedMove {
    ply: usize,
    engine: EngineAnalysis,
    human: HumanMovePrediction,
}

impl AnalyzedGame {
    fn into_evidence(self) -> Vec<ProviderEvidence> {
        let after_moves = self
            .moves
            .iter()
            .enumerate()
            .map(|(index, _)| match self.moves.get(index + 1) {
                Some(next) => ProviderAfterMove::Analyzed {
                    evaluation: next.engine.evaluation,
                    principal_variation: next.engine.principal_variation.clone(),
                },
                None => match &self.final_position {
                    FinalPositionEvidence::Terminal => ProviderAfterMove::Terminal,
                    FinalPositionEvidence::Analyzed(analysis) => ProviderAfterMove::Analyzed {
                        evaluation: analysis.evaluation,
                        principal_variation: analysis.principal_variation.clone(),
                    },
                },
            })
            .collect::<Vec<_>>();
        self.moves
            .into_iter()
            .zip(after_moves)
            .map(|(analyzed, after_move)| ProviderEvidence {
                ply: analyzed.ply,
                engine_before: analyzed.engine,
                after_move,
                human_before: analyzed.human,
            })
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewFactsError {
    #[error(transparent)]
    Pgn(#[from] PgnImportError),
    #[error("selected ply {0} does not exist in the imported Game")]
    UnknownSelectedPly(usize),
    #[error("selected ply {0} could not be represented as a Position Snapshot")]
    InvalidPosition(usize),
    #[error("selected ply {0} has no Engine Analysis evidence")]
    MissingPositionEvidence(usize),
    #[error("engine line for selected ply {0} is not legal in its recorded Position")]
    InvalidEngineLine(usize),
    #[error(transparent)]
    Engine(#[from] EngineAnalysisError),
    #[error(transparent)]
    Human(#[from] HumanMoveModelError),
    #[error(transparent)]
    Rule(#[from] RuleExtractorError),
    #[error("Review Session contract value is invalid: {0}")]
    Contract(String),
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::Pin,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    use tokio::sync::Semaphore;

    use crate::{
        engine_analysis::{
            EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer,
            EngineMultiPvOutput, EngineProvenance, PositionEvaluation, RankedEngineAnalysis,
        },
        human_move_model::{
            HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction,
        },
        operating_limits::{REVIEW_FACTS_ENGINE_CONCURRENCY, REVIEW_FACTS_HUMAN_CONCURRENCY},
        pgn::parse_pgn,
        review_session_contract::{
            build_position_snapshot, BoardTerminalOutcome, CandidateEvidence, CandidateGap, Color,
            CurriculumLearningConcept, DecisionLearningOutcome, EngineEvaluation, GameRef,
            GameReview, GameReviewMomentClassification, GameReviewMomentProvenance,
            LearningResourceKind, LearningResourceRole, LearningTrackKey, LearningTrackSupport,
            LearningTrackSupportBasis, OpeningMetadata, PlayedMoveOutcomeEvidence, ProofCapability,
        },
        types::{EloProfile, HumanMoveCandidate, ReviewSide},
    };

    use super::{
        ProviderAfterMove, ProviderEvidence, ReviewFactsError, ReviewFactsInput, ReviewFactsService,
    };

    #[tokio::test]
    async fn game_review_respects_white_review_side() {
        let output = review(ReviewSide::White).await;

        assert_eq!(
            output
                .critical_moments
                .iter()
                .map(|moment| moment.ply)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(output.position_views.len(), 1);
        assert_eq!(output.position_views[0].ply, 1);
        assert!(output.position_views[0]
            .text_board
            .contains("Side to move: White"));
    }

    #[tokio::test]
    async fn game_review_respects_black_review_side() {
        let output = review(ReviewSide::Black).await;

        assert_eq!(
            output
                .critical_moments
                .iter()
                .map(|moment| moment.ply)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[tokio::test]
    async fn game_review_can_explicitly_review_both_sides() {
        let output = review(ReviewSide::Both).await;

        assert_eq!(
            output
                .critical_moments
                .iter()
                .map(|moment| moment.ply)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[tokio::test]
    async fn learning_paths_request_exactly_one_root_multi_pv_search_per_eligible_moment() {
        let calls = Arc::new(AtomicUsize::new(0));
        let service = ReviewFactsService::new(
            Arc::new(MultiPvProbeEngine {
                calls: calls.clone(),
                fails_comparison: false,
                mismatched_provenance: false,
                rank_one_offset: 0,
            }),
            Arc::new(ForkHumanMoveModel),
        );
        let game_ref = GameRef::try_from(format!("sha256:{}", "b".repeat(64))).unwrap();
        let opening_identification = OpeningMetadata::Absent;

        let output = service
            .review_session_game(
                ReviewFactsInput {
                    pgn: "[SetUp \"1\"]\n[FEN \"r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1\"]\n\n1. Nxc7+ *",
                    player_elo: EloProfile::try_from(1500).unwrap(),
                    review_side: ReviewSide::Both,
                    opening_identification: &opening_identification,
                },
                &game_ref,
            )
            .await
            .unwrap();

        let enriched = output
            .review
            .critical_moments
            .iter()
            .filter(|moment| moment.decision_explanation.is_some())
            .count();
        assert!(
            enriched > 0,
            "the fixture must exercise Decision Explanation enrichment"
        );
        assert_eq!(calls.load(Ordering::Acquire), enriched);
        assert_eq!(output.provider_timings.engine_analysis.len(), 2 + enriched);
        let moment = &output.review.critical_moments[0];
        let explanation = moment
            .decision_explanation
            .as_ref()
            .expect("a proof-valid Automatic Fork should be durable");
        assert!(matches!(
            explanation.candidate_evidence,
            CandidateEvidence::MultiPv {
                requested_count: 3,
                ..
            }
        ));
        crate::decision_explanation::validate_decision_explanation(explanation)
            .expect("the persisted proof must pass the public validator");
        let rebuilt = crate::decision_explanation::explain_decision(
            crate::decision_explanation::DecisionExplanationInput {
                game_ref: explanation.game_ref.clone(),
                critical_moment_id: explanation.critical_moment_id.clone(),
                position_snapshot: explanation.position_snapshot.clone(),
                classification: moment.classification.clone(),
                provenance: moment.provenance,
                player_move_uci: moment.objective.played_move_uci.clone(),
                candidate_evidence: explanation.candidate_evidence.clone(),
            },
        )
        .expect("persisted Candidate Evidence must remain sufficient for offline recomputation");
        let crate::decision_explanation::DecisionExplanationBuild::Durable {
            explanation: rebuilt,
            ..
        } = rebuilt
        else {
            panic!("persisted Candidate Evidence unexpectedly abstained on recomputation");
        };
        assert_eq!(rebuilt.as_ref(), explanation);
        assert_eq!(output.review.learning_plan.tracks.len(), 1);
        assert_eq!(
            output.review.learning_plan.tracks[0],
            moment.learning_material.tracks[0]
        );
    }

    /// The MultiPV search scores the Fork's best root 30cp below the SinglePV
    /// screening pass, as the two searches genuinely do at identical settings.
    /// That number must not reach the payload anywhere.
    #[tokio::test]
    async fn a_disagreeing_multi_pv_rank_one_score_reaches_no_part_of_the_review() {
        let service = ReviewFactsService::new(
            Arc::new(MultiPvProbeEngine {
                calls: Arc::new(AtomicUsize::new(0)),
                fails_comparison: false,
                mismatched_provenance: false,
                rank_one_offset: -30,
            }),
            Arc::new(ForkHumanMoveModel),
        );
        let game_ref = GameRef::try_from(format!("sha256:{}", "f".repeat(64))).unwrap();
        let opening_identification = OpeningMetadata::Absent;
        let review = service
            .review_session_game(
                ReviewFactsInput {
                    pgn: "[SetUp \"1\"]\n[FEN \"r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1\"]\n\n1. Nxc7+ *",
                    player_elo: EloProfile::try_from(1500).unwrap(),
                    review_side: ReviewSide::Both,
                    opening_identification: &opening_identification,
                },
                &game_ref,
            )
            .await
            .expect("a disagreeing MultiPV search must not fail the Game Review")
            .review;

        let moment = &review.critical_moments[0];
        // The SinglePV screening pass keeps the absolute, untouched.
        assert_eq!(
            moment.objective.best_evaluation,
            EngineEvaluation::Centipawns {
                value: 500,
                perspective: Color::White,
            },
        );

        let explanation = moment
            .decision_explanation
            .as_ref()
            .expect("the fixture must produce a durable comparison");
        let CandidateEvidence::MultiPv {
            authoritative_single_pv,
            ranked_alternatives,
            ..
        } = &explanation.candidate_evidence
        else {
            panic!("the fixture must exercise MultiPV enrichment");
        };
        assert_eq!(
            authoritative_single_pv.evaluation, moment.objective.best_evaluation,
            "the authoritative record restates the Moment's own evaluation",
        );
        // Rank one is not restated as an alternative, so there is no second
        // absolute for the best move to contradict the first.
        assert!(ranked_alternatives
            .iter()
            .all(|alternative| alternative.rank >= 2));
        assert!(!ranked_alternatives
            .iter()
            .any(|alternative| alternative.root_move_uci == moment.objective.best_move_uci));
        // Gaps are measured inside the MultiPV search: rank one at 470 against
        // alternatives at 400 and 300.
        assert_eq!(
            ranked_alternatives
                .iter()
                .map(|alternative| alternative.gap)
                .collect::<Vec<_>>(),
            vec![
                CandidateGap::Centipawns { behind_best: 70 },
                CandidateGap::Centipawns { behind_best: 170 },
            ],
        );

        // 470 is the MultiPV reading and belongs to no published absolute.
        let serialized = serde_json::to_string(&review).expect("the review must serialize");
        assert!(
            !serialized.contains("470"),
            "the MultiPV rank-one absolute must not appear anywhere in the review",
        );

        // Every published absolute states one perspective.
        for evaluation in [
            &moment.objective.best_evaluation,
            &moment.objective.played_evaluation,
            &authoritative_single_pv.evaluation,
        ] {
            assert_eq!(evaluation.perspective(), moment.side);
        }
    }

    #[tokio::test]
    async fn invalid_multi_pv_keeps_the_authoritative_single_pv_learning_path() {
        let calls = Arc::new(AtomicUsize::new(0));
        let service = ReviewFactsService::new(
            Arc::new(MultiPvProbeEngine {
                calls: calls.clone(),
                fails_comparison: false,
                mismatched_provenance: true,
                rank_one_offset: 0,
            }),
            Arc::new(ForkHumanMoveModel),
        );
        let game_ref = GameRef::try_from(format!("sha256:{}", "d".repeat(64))).unwrap();
        let opening_identification = OpeningMetadata::Absent;

        let output = service
            .review_session_game(
                ReviewFactsInput {
                    pgn: "[SetUp \"1\"]\n[FEN \"r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1\"]\n\n1. Nxc7+ *",
                    player_elo: EloProfile::try_from(1500).unwrap(),
                    review_side: ReviewSide::Both,
                    opening_identification: &opening_identification,
                },
                &game_ref,
            )
            .await
            .expect("invalid comparative evidence must not fail the ordinary Game Review");

        assert_eq!(calls.load(Ordering::Acquire), 1);
        let moment = &output.review.critical_moments[0];
        assert_eq!(moment.objective.best_move_uci, "b5c7");
        assert_eq!(
            moment.decision_learning_outcome,
            DecisionLearningOutcome::TrackSelected
        );
        assert!(matches!(
            moment
                .decision_explanation
                .as_ref()
                .map(|explanation| &explanation.candidate_evidence),
            Some(CandidateEvidence::SinglePv { .. })
        ));
        assert_eq!(moment.learning_material.tracks.len(), 1);
        assert_eq!(output.review.learning_plan.tracks.len(), 1);
    }

    #[tokio::test]
    async fn failed_multi_pv_keeps_single_pv_learning_and_records_the_attempt() {
        let calls = Arc::new(AtomicUsize::new(0));
        let service = ReviewFactsService::new(
            Arc::new(MultiPvProbeEngine {
                calls: calls.clone(),
                fails_comparison: true,
                mismatched_provenance: false,
                rank_one_offset: 0,
            }),
            Arc::new(ForkHumanMoveModel),
        );
        let game_ref = GameRef::try_from(format!("sha256:{}", "9".repeat(64))).unwrap();
        let opening_identification = OpeningMetadata::Absent;

        let output = service
            .review_session_game(
                ReviewFactsInput {
                    pgn: "[SetUp \"1\"]\n[FEN \"r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1\"]\n\n1. Nxc7+ *",
                    player_elo: EloProfile::try_from(1500).unwrap(),
                    review_side: ReviewSide::Both,
                    opening_identification: &opening_identification,
                },
                &game_ref,
            )
            .await
            .expect("an optional comparison failure must not fail the Game Review");

        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert_eq!(output.provider_timings.engine_analysis.len(), 3);
        let moment = &output.review.critical_moments[0];
        assert_eq!(
            moment.decision_learning_outcome,
            DecisionLearningOutcome::TrackSelected
        );
        assert!(matches!(
            moment
                .decision_explanation
                .as_ref()
                .map(|explanation| &explanation.candidate_evidence),
            Some(CandidateEvidence::SinglePv { .. })
        ));
        assert_eq!(moment.learning_material.tracks.len(), 1);
    }

    #[tokio::test]
    async fn improvement_opportunities_use_the_same_automatic_preflight_and_enrichment_path() {
        let calls = Arc::new(AtomicUsize::new(0));
        let service = ReviewFactsService::new(
            Arc::new(MultiPvProbeEngine {
                calls: calls.clone(),
                fails_comparison: false,
                mismatched_provenance: false,
                rank_one_offset: 0,
            }),
            Arc::new(ForkHumanMoveModel),
        );
        let game_ref = GameRef::try_from(format!("sha256:{}", "e".repeat(64))).unwrap();
        let opening_identification = OpeningMetadata::Absent;

        let output = service
            .review_session_game(
                ReviewFactsInput {
                    pgn: "[SetUp \"1\"]\n[FEN \"r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1\"]\n\n1. Nc3 *",
                    player_elo: EloProfile::try_from(1500).unwrap(),
                    review_side: ReviewSide::White,
                    opening_identification: &opening_identification,
                },
                &game_ref,
            )
            .await
            .expect("the missed Fork should remain a valid Improvement Opportunity");

        assert_eq!(calls.load(Ordering::Acquire), 1);
        let moment = &output.review.critical_moments[0];
        assert!(matches!(
            moment.classification,
            GameReviewMomentClassification::ImprovementOpportunity { .. }
        ));
        assert!(moment.decision_explanation.is_some());
        assert_eq!(moment.learning_material.tracks.len(), 1);
    }

    #[tokio::test]
    async fn review_session_game_links_the_complete_grounded_review() {
        let output = review(ReviewSide::White).await;

        assert_eq!(output.evaluation_timeline.len(), 2);
        assert_eq!(output.critical_moments.len(), 1);
        let moment = &output.critical_moments[0];
        assert_eq!(moment.ply, 1);
        assert_eq!(
            output.position_views[0].critical_moment_id,
            moment.critical_moment_id
        );
        let lines = moment
            .objective
            .lines
            .as_ref()
            .expect("live Game Reviews should preserve both engine lines");
        assert_eq!(lines.best[0].uci, "a2a3");
        assert_eq!(lines.best[0].san, "a3");
        assert_eq!(lines.refutation[0].uci, "a7a6");
        assert_eq!(lines.refutation[0].san, "a6");
    }

    #[test]
    fn player_selected_terminal_facts_preserve_the_board_outcome_and_classification() {
        let game = parse_pgn("[FEN \"7k/8/5KQ1/8/8/8/8/8 w - - 0 1\"]\n\n1. Qg7# *")
            .expect("test PGN should be valid");
        let evidence = vec![ProviderEvidence {
            ply: 1,
            engine_before: EngineAnalysis {
                best_move: "g6g7".to_string(),
                evaluation: PositionEvaluation::MateIn(1),
                principal_variation: vec!["g6g7".to_string()],
                depth: 16,
            },
            after_move: ProviderAfterMove::Terminal,
            human_before: HumanMovePrediction {
                candidates: vec![HumanMoveCandidate {
                    uci: "g6g7".to_string(),
                    probability: 0.61,
                    rank: 1,
                }],
                win_probability: Some(0.99),
            },
        }];
        let game_ref = GameRef::try_from(format!("sha256:{}", "b".repeat(64))).unwrap();

        let moments = super::game_review::player_selected_moments(
            &game,
            EloProfile::try_from(1200).unwrap(),
            &evidence,
            &game_ref,
            &OpeningMetadata::Absent,
        )
        .expect("terminal Player-selected facts should remain presentable");

        assert_eq!(moments.len(), 1);
        assert_eq!(
            moments[0].provenance,
            GameReviewMomentProvenance::PlayerSelected
        );
        assert!(matches!(
            moments[0].classification,
            GameReviewMomentClassification::PositiveHighlight { .. }
        ));
        assert_eq!(
            moments[0].played_move_outcome,
            PlayedMoveOutcomeEvidence::Terminal {
                outcome: BoardTerminalOutcome::Checkmate {
                    winner: Color::White
                }
            }
        );
    }

    #[test]
    fn player_selected_decision_stays_lightweight_until_the_shared_module_materializes_it() {
        let game =
            parse_pgn("[SetUp \"1\"]\n[FEN \"r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1\"]\n\n1. Nxc7+ *")
                .expect("the audited Fork position should be a legal Game");
        let evidence = vec![ProviderEvidence {
            ply: 1,
            engine_before: EngineAnalysis {
                best_move: "b5c7".to_string(),
                evaluation: PositionEvaluation::Centipawns(500),
                principal_variation: vec![
                    "b5c7".to_string(),
                    "e8d7".to_string(),
                    "c7a8".to_string(),
                ],
                depth: 16,
            },
            after_move: ProviderAfterMove::Analyzed {
                evaluation: PositionEvaluation::Centipawns(-500),
                principal_variation: vec!["e8d7".to_string(), "c7a8".to_string()],
            },
            human_before: HumanMovePrediction {
                candidates: vec![HumanMoveCandidate {
                    uci: "b5c7".to_string(),
                    probability: 0.4,
                    rank: 1,
                }],
                win_probability: Some(0.9),
            },
        }];
        let game_ref = GameRef::try_from(format!("sha256:{}", "c".repeat(64))).unwrap();

        let mut moments = super::game_review::player_selected_moments(
            &game,
            EloProfile::try_from(1200).unwrap(),
            &evidence,
            &game_ref,
            &OpeningMetadata::Absent,
        )
        .expect("the audited Fork should produce lightweight Player-Selected facts");

        let moment = &moments[0];
        assert_eq!(
            moment.provenance,
            GameReviewMomentProvenance::PlayerSelected
        );
        assert_eq!(
            moment.decision_learning_outcome,
            DecisionLearningOutcome::NotAttempted
        );
        assert!(moment.learning_material.tracks.is_empty());

        let materialized = crate::player_selected_decision::materialize(
            &game_ref,
            &build_position_snapshot("r2qk3/2p5/8/1N6/8/8/8/4K3 w - - 0 1", &[]).unwrap(),
            crate::game_import_store::ImportedCriticalMoment {
                moment: moments.remove(0),
                engine_provenance: Some(EngineProvenance {
                    version: "Stockfish 18".to_string(),
                    binary_sha256: "e".repeat(64),
                    depth: 16,
                    threads: 1,
                    hash_mib: 16,
                }),
                decision_explanation: None,
            },
        )
        .expect("the on-demand Decision Explanation should be valid");
        let explanation = materialized
            .decision_explanation
            .as_ref()
            .expect("a proof-valid Fork should retain its Decision Explanation");
        assert_eq!(explanation.capability, ProofCapability::ValidationOnly);
        assert_eq!(
            materialized.moment.decision_learning_outcome,
            DecisionLearningOutcome::TrackSelected
        );
        assert_eq!(materialized.moment.learning_material.tracks.len(), 1);
        let track = &materialized.moment.learning_material.tracks[0];
        assert_eq!(
            track.key,
            LearningTrackKey::Curriculum {
                concept: CurriculumLearningConcept::Advantage
            }
        );
        assert!(matches!(
            track.support.as_slice(),
            [LearningTrackSupport::Reinforcement {
                ply: 1,
                basis: LearningTrackSupportBasis::DecisionExplanation {
                    explanation_path_ref,
                },
                ..
            }] if explanation_path_ref == &explanation.selected_paths[0].path_ref
        ));
        assert_eq!(track.resources.len(), 1);
        assert_eq!(track.resources[0].role, LearningResourceRole::Drill);
        assert_eq!(track.resources[0].kind, LearningResourceKind::PuzzleStream);
        assert_eq!(
            track.resources[0].canonical_url,
            "https://lichess.org/training/advantage"
        );
    }

    #[test]
    fn neutral_player_selected_moments_have_empty_local_learning_material() {
        let game = parse_pgn("1. e4 *").expect("the quiet fixture should be a legal Game");
        let evidence = vec![ProviderEvidence {
            ply: 1,
            engine_before: EngineAnalysis {
                best_move: "e2e4".to_string(),
                evaluation: PositionEvaluation::Centipawns(100),
                principal_variation: vec!["e2e4".to_string()],
                depth: 16,
            },
            after_move: ProviderAfterMove::Analyzed {
                evaluation: PositionEvaluation::Centipawns(-100),
                principal_variation: vec!["a7a6".to_string()],
            },
            human_before: HumanMovePrediction {
                candidates: vec![HumanMoveCandidate {
                    uci: "e2e4".to_string(),
                    probability: 0.4,
                    rank: 1,
                }],
                win_probability: Some(0.55),
            },
        }];
        let game_ref = GameRef::try_from(format!("sha256:{}", "d".repeat(64))).unwrap();
        let moments = super::game_review::player_selected_moments(
            &game,
            EloProfile::try_from(1500).unwrap(),
            &evidence,
            &game_ref,
            &OpeningMetadata::Absent,
        )
        .expect("the quiet move should produce Player-selected facts");
        let neutral = &moments[0];

        assert!(matches!(
            neutral.classification,
            GameReviewMomentClassification::Neutral { .. }
        ));
        assert!(neutral.learning_material.tracks.is_empty());
    }

    #[tokio::test]
    async fn game_evidence_orders_bounded_provider_work() {
        let engine_probe = ConcurrencyProbe::default();
        let human_probe = ConcurrencyProbe::default();
        let service = ReviewFactsService::new(
            Arc::new(OutOfOrderEngine {
                probe: engine_probe.clone(),
            }),
            Arc::new(ObservedHumanMoveModel {
                probe: human_probe.clone(),
            }),
        );
        let game = parse_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 *")
            .expect("test PGN should be valid");

        let (evidence, _) = service
            .collect_game_evidence(&game, EloProfile::try_from(1500).unwrap())
            .await
            .expect("out-of-order provider completion should still produce evidence");

        assert_eq!(
            evidence.iter().map(|item| item.ply).collect::<Vec<_>>(),
            (1..=8).collect::<Vec<_>>()
        );
        assert_eq!(engine_probe.peak(), REVIEW_FACTS_ENGINE_CONCURRENCY);
        assert_eq!(human_probe.peak(), REVIEW_FACTS_HUMAN_CONCURRENCY);
        assert_eq!(engine_probe.active(), 0);
        assert_eq!(human_probe.active(), 0);
    }

    #[tokio::test]
    async fn cancelling_game_analysis_drops_every_in_flight_provider_call() {
        let probe = ConcurrencyProbe::default();
        let started = Arc::new(Semaphore::new(0));
        let service = ReviewFactsService::new(
            Arc::new(BlockingEngine {
                probe: probe.clone(),
                started: started.clone(),
            }),
            Arc::new(StaticHumanMoveModel),
        );
        let game = parse_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 *")
            .expect("test PGN should be valid");
        let analysis = tokio::spawn(async move {
            service
                .collect_game_evidence(&game, EloProfile::try_from(1500).unwrap())
                .await
        });
        let permits = tokio::time::timeout(
            Duration::from_secs(1),
            started.acquire_many_owned(REVIEW_FACTS_ENGINE_CONCURRENCY as u32),
        )
        .await
        .expect("the bounded Engine Analysis tasks should start")
        .expect("start signal should stay open");
        permits.forget();

        analysis.abort();
        assert!(analysis
            .await
            .expect_err("analysis should be cancelled")
            .is_cancelled());
        tokio::time::timeout(Duration::from_secs(1), async {
            while probe.active() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled provider calls should be dropped");
    }

    #[tokio::test]
    async fn cancelling_game_analysis_drops_every_in_flight_human_move_model_call() {
        let probe = ConcurrencyProbe::default();
        let started = Arc::new(Semaphore::new(0));
        let service = ReviewFactsService::new(
            Arc::new(StaticEngine),
            Arc::new(BlockingHumanMoveModel {
                probe: probe.clone(),
                started: started.clone(),
            }),
        );
        let game = parse_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 *")
            .expect("test PGN should be valid");
        let analysis = tokio::spawn(async move {
            service
                .collect_game_evidence(&game, EloProfile::try_from(1500).unwrap())
                .await
        });
        let permits = tokio::time::timeout(
            Duration::from_secs(1),
            started.acquire_many_owned(REVIEW_FACTS_HUMAN_CONCURRENCY as u32),
        )
        .await
        .expect("the bounded Human Move Model tasks should start")
        .expect("start signal should stay open");
        permits.forget();

        analysis.abort();
        assert!(analysis
            .await
            .expect_err("analysis should be cancelled")
            .is_cancelled());
        tokio::time::timeout(Duration::from_secs(1), async {
            while probe.active() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled Human Move Model calls should be dropped");
    }

    #[tokio::test]
    async fn provider_failures_keep_earliest_ply_precedence_with_bounded_engine_work() {
        let service = ReviewFactsService::new(
            Arc::new(OutOfOrderFailingEngine),
            Arc::new(DelayedFirstPositionHumanFailure),
        );
        let game = parse_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 *")
            .expect("test PGN should be valid");

        let engine_error = match service.analyze_game_engine_positions(&game).await {
            Ok(_) => panic!("controlled Engine Analysis calls should fail"),
            Err(error) => error,
        };
        assert_eq!(engine_error.index, 1);
        assert!(matches!(
            engine_error.error,
            ReviewFactsError::Engine(EngineAnalysisError::Protocol(message))
                if message == "earlier Engine Analysis failure"
        ));

        let error = service
            .collect_game_evidence(&game, EloProfile::try_from(1500).unwrap())
            .await
            .expect_err("controlled providers should fail");
        assert!(matches!(
            error,
            ReviewFactsError::Human(HumanMoveModelError::InvalidResponse(message))
                if message == "first-position Human Move Model failure"
        ));
    }

    struct StaticEngine;

    struct MultiPvProbeEngine {
        calls: Arc<AtomicUsize>,
        fails_comparison: bool,
        mismatched_provenance: bool,
        /// Shifts the MultiPV rank-one score away from the SinglePV screening
        /// score, reproducing the pruning difference between the two searches.
        rank_one_offset: i32,
    }

    impl EngineAnalyzer for StaticEngine {
        fn analyze<'a>(
            &'a self,
            input: EngineAnalysisInput<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
        {
            Box::pin(async move {
                let best_move = if input.position.split_whitespace().nth(1) == Some("b") {
                    "a7a6"
                } else {
                    "a2a3"
                };
                Ok(engine(best_move, 100))
            })
        }
    }

    impl EngineAnalyzer for MultiPvProbeEngine {
        fn analyze<'a>(
            &'a self,
            input: EngineAnalysisInput<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
        {
            Box::pin(async move {
                if input.position.split_whitespace().nth(1) == Some("b") {
                    if input.position.starts_with("r2qk3/2N5") {
                        Ok(EngineAnalysis {
                            best_move: "e8d7".to_string(),
                            evaluation: PositionEvaluation::Centipawns(-500),
                            principal_variation: vec!["e8d7".to_string(), "c7a8".to_string()],
                            depth: 16,
                        })
                    } else {
                        Ok(engine("a8b8", 0))
                    }
                } else {
                    Ok(EngineAnalysis {
                        best_move: "b5c7".to_string(),
                        evaluation: PositionEvaluation::Centipawns(500),
                        principal_variation: vec![
                            "b5c7".to_string(),
                            "e8d7".to_string(),
                            "c7a8".to_string(),
                        ],
                        depth: 16,
                    })
                }
            })
        }

        fn provenance(&self) -> Option<EngineProvenance> {
            Some(engine_provenance())
        }

        fn supports_multi_pv(&self) -> bool {
            true
        }

        fn analyze_multi_pv<'a>(
            &'a self,
            input: EngineAnalysisInput<'a>,
            variation_count: u8,
        ) -> Pin<
            Box<dyn Future<Output = Result<EngineMultiPvOutput, EngineAnalysisError>> + Send + 'a>,
        > {
            self.calls.fetch_add(1, Ordering::AcqRel);
            let provenance = if self.mismatched_provenance {
                EngineProvenance {
                    binary_sha256: "c".repeat(64),
                    ..engine_provenance()
                }
            } else {
                engine_provenance()
            };
            let fails_comparison = self.fails_comparison;
            Box::pin(async move {
                assert_eq!(variation_count, 3);
                assert_eq!(input.position.split_whitespace().nth(1), Some("w"));
                if fails_comparison {
                    return Err(EngineAnalysisError::Protocol(
                        "controlled MultiPV failure".to_string(),
                    ));
                }
                let candidates = [
                    (
                        "b5c7",
                        vec!["b5c7".to_string(), "e8d7".to_string(), "c7a8".to_string()],
                    ),
                    ("b5d6", vec!["b5d6".to_string()]),
                    ("b5c3", vec!["b5c3".to_string()]),
                ];
                Ok(EngineMultiPvOutput {
                    requested_variations: variation_count,
                    variations: candidates
                        .into_iter()
                        .enumerate()
                        .map(
                            |(index, (root, principal_variation))| RankedEngineAnalysis {
                                rank: u8::try_from(index + 1).unwrap(),
                                analysis: EngineAnalysis {
                                    best_move: root.to_string(),
                                    evaluation: PositionEvaluation::Centipawns(
                                        500 - 100 * i32::try_from(index).unwrap()
                                            + if index == 0 { self.rank_one_offset } else { 0 },
                                    ),
                                    principal_variation,
                                    depth: 16,
                                },
                            },
                        )
                        .collect(),
                    provenance: Some(provenance),
                })
            })
        }
    }

    fn engine_provenance() -> EngineProvenance {
        EngineProvenance {
            version: "18".to_string(),
            binary_sha256: "b".repeat(64),
            depth: 16,
            threads: 1,
            hash_mib: 16,
        }
    }

    struct StaticHumanMoveModel;
    struct ForkHumanMoveModel;

    impl HumanMoveModel for StaticHumanMoveModel {
        fn predict<'a>(
            &'a self,
            _input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async { Ok(human_prediction()) })
        }
    }

    impl HumanMoveModel for ForkHumanMoveModel {
        fn predict<'a>(
            &'a self,
            _input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async {
                Ok(HumanMovePrediction {
                    candidates: vec![HumanMoveCandidate {
                        uci: "b5c7".to_string(),
                        probability: 0.4,
                        rank: 1,
                    }],
                    win_probability: Some(0.9),
                })
            })
        }
    }

    struct DelayedFirstPositionHumanFailure;

    impl HumanMoveModel for DelayedFirstPositionHumanFailure {
        fn predict<'a>(
            &'a self,
            input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async move {
                let mut fields = input.position.split_whitespace();
                let side = fields.nth(1);
                let full_move = input.position.split_whitespace().last();
                if side == Some("w") && full_move == Some("1") {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    return Err(HumanMoveModelError::InvalidResponse(
                        "first-position Human Move Model failure".to_string(),
                    ));
                }
                Ok(human_prediction())
            })
        }
    }

    struct ObservedHumanMoveModel {
        probe: ConcurrencyProbe,
    }

    struct BlockingHumanMoveModel {
        probe: ConcurrencyProbe,
        started: Arc<Semaphore>,
    }

    impl HumanMoveModel for ObservedHumanMoveModel {
        fn predict<'a>(
            &'a self,
            _input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async move {
                let _call = self.probe.enter();
                tokio::task::yield_now().await;
                Ok(human_prediction())
            })
        }
    }

    impl HumanMoveModel for BlockingHumanMoveModel {
        fn predict<'a>(
            &'a self,
            _input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async move {
                let _call = self.probe.enter();
                self.started.add_permits(1);
                std::future::pending().await
            })
        }
    }

    struct OutOfOrderEngine {
        probe: ConcurrencyProbe,
    }

    impl EngineAnalyzer for OutOfOrderEngine {
        fn analyze<'a>(
            &'a self,
            input: EngineAnalysisInput<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
        {
            Box::pin(async move {
                let _call = self.probe.enter();
                let full_move = input
                    .position
                    .split_whitespace()
                    .last()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(1);
                tokio::time::sleep(Duration::from_millis(12_u64.saturating_sub(full_move))).await;
                Ok(engine("a2a3", 100))
            })
        }
    }

    struct BlockingEngine {
        probe: ConcurrencyProbe,
        started: Arc<Semaphore>,
    }

    struct OutOfOrderFailingEngine;

    impl EngineAnalyzer for OutOfOrderFailingEngine {
        fn analyze<'a>(
            &'a self,
            input: EngineAnalysisInput<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
        {
            Box::pin(async move {
                let mut fields = input.position.split_whitespace();
                let side = fields.nth(1);
                let full_move = input.position.split_whitespace().last();
                match (side, full_move) {
                    (Some("b"), Some("1")) => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        Err(EngineAnalysisError::Protocol(
                            "earlier Engine Analysis failure".to_string(),
                        ))
                    }
                    (Some("w"), Some("2")) => Err(EngineAnalysisError::Protocol(
                        "later Engine Analysis failure".to_string(),
                    )),
                    _ => Ok(engine("a2a3", 100)),
                }
            })
        }
    }

    impl EngineAnalyzer for BlockingEngine {
        fn analyze<'a>(
            &'a self,
            _input: EngineAnalysisInput<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
        {
            Box::pin(async move {
                let _call = self.probe.enter();
                self.started.add_permits(1);
                std::future::pending().await
            })
        }
    }

    #[derive(Clone, Default)]
    struct ConcurrencyProbe {
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    impl ConcurrencyProbe {
        fn enter(&self) -> ActiveCall {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            ActiveCall(self.active.clone())
        }

        fn active(&self) -> usize {
            self.active.load(Ordering::SeqCst)
        }

        fn peak(&self) -> usize {
            self.peak.load(Ordering::SeqCst)
        }
    }

    struct ActiveCall(Arc<AtomicUsize>);

    impl Drop for ActiveCall {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn engine(best_move: &str, centipawns: i32) -> EngineAnalysis {
        EngineAnalysis {
            best_move: best_move.to_string(),
            evaluation: PositionEvaluation::Centipawns(centipawns),
            principal_variation: vec![best_move.to_string()],
            depth: 16,
        }
    }

    fn human_prediction() -> HumanMovePrediction {
        HumanMovePrediction {
            candidates: vec![HumanMoveCandidate {
                uci: "a2a3".to_string(),
                probability: 0.4,
                rank: 1,
            }],
            win_probability: Some(0.5),
        }
    }

    fn two_ply_service() -> ReviewFactsService {
        ReviewFactsService::new(Arc::new(StaticEngine), Arc::new(StaticHumanMoveModel))
    }

    async fn review(side: ReviewSide) -> GameReview {
        let game_ref = GameRef::try_from(format!("sha256:{}", "a".repeat(64))).unwrap();
        let opening_identification = OpeningMetadata::Absent;
        two_ply_service()
            .review_session_game(
                ReviewFactsInput {
                    pgn: "1. e4 e5 *",
                    player_elo: EloProfile::try_from(1500).unwrap(),
                    review_side: side,
                    opening_identification: &opening_identification,
                },
                &game_ref,
            )
            .await
            .expect("complete provider evidence should produce a Review Session Game Review")
            .review
    }
}
