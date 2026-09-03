use std::{cmp::Ordering, collections::BTreeSet, sync::Arc};

use serde::{Deserialize, Serialize};
use shakmaty::{
    fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position,
};

use crate::{
    engine_analysis::{EngineAnalysisInput, EngineAnalyzer},
    evaluation_recording::PINNED_MAIA_CANDIDATE_LIMIT,
    human_move_model::{HumanMoveModel, HumanMovePrediction},
    operating_limits::{
        PROJECTED_PLAN_BEAM_WIDTH, PROJECTED_PLAN_ENGINE_CONCURRENCY,
        PROJECTED_PLAN_REQUIRED_HALF_MOVES,
    },
    provider_concurrency::collect_ordered_provider_positions,
    review_session_contract::{
        build_position_snapshot, Color, EloRating, EngineEvaluation, EvidenceProvenance,
        MateOutcome, PositionSnapshot, PositionStatus, ReviewSessionCoreContract,
    },
    review_session_exploration::normalize_live_engine_analysis,
    review_session_start::reconstruct_selected_position,
    types::EloProfile,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedPlan {
    san: Vec<String>,
    objective_counterplay_san: Vec<String>,
    provenance: Option<ProjectedPlanProvenance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectedPlanProvenance {
    pub(crate) stockfish: EvidenceProvenance,
    pub(crate) maia: EvidenceProvenance,
}

impl ProjectedPlan {
    pub fn projected_plan_san(&self) -> &[String] {
        &self.san
    }

    pub fn objective_counterplay_san(&self) -> &[String] {
        &self.objective_counterplay_san
    }

    pub(crate) fn provenance(&self) -> Option<&ProjectedPlanProvenance> {
        self.provenance.as_ref()
    }
}

pub struct ProjectedPlanBuilder {
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectedPlanError {
    #[error("Projected Plan input is invalid: {0}")]
    Binding(&'static str),
    #[error("Projected Plan Maia evidence is unavailable")]
    MaiaUnavailable,
    #[error("Projected Plan Stockfish evidence is unavailable")]
    StockfishUnavailable,
    #[error("Projected Plan evidence is invalid: {0}")]
    Evidence(&'static str),
}

impl ProjectedPlanBuilder {
    pub fn new(engine: Arc<dyn EngineAnalyzer>, human: Arc<dyn HumanMoveModel>) -> Self {
        Self { engine, human }
    }

    pub async fn build(
        &self,
        core: &ReviewSessionCoreContract,
    ) -> Result<ProjectedPlan, ProjectedPlanError> {
        let provenance = projected_plan_provenance(
            self.engine.as_ref(),
            self.human.as_ref(),
            core.imported_game.elo_profile.rating,
        );
        let seed = ProjectionSeed::from_core(core)?;
        self.build_from_seed(seed, provenance).await
    }

    async fn build_from_seed(
        &self,
        seed: ProjectionSeed,
        provenance: Option<ProjectedPlanProvenance>,
    ) -> Result<ProjectedPlan, ProjectedPlanError> {
        let objective_engine = self.engine.clone();
        let objective_fen = seed.position.fen.clone();
        let (objective, candidates) = tokio::join!(
            async move {
                objective_engine
                    .analyze(EngineAnalysisInput {
                        position: &objective_fen,
                    })
                    .await
                    .map_err(|_| ProjectedPlanError::StockfishUnavailable)
            },
            build_maia_candidates(seed.clone(), self.human.clone()),
        );
        let objective = normalize_live_engine_analysis(&seed.position, objective?)
            .map_err(|_| ProjectedPlanError::Evidence("Objective Counterplay is invalid"))?;
        let candidates = candidates?;

        let analyses = self
            .engine
            .clone()
            .analyze_positions(
                candidates
                    .iter()
                    .map(|candidate| candidate.position.fen.clone())
                    .collect(),
                PROJECTED_PLAN_ENGINE_CONCURRENCY,
            )
            .await
            .map_err(|_| ProjectedPlanError::StockfishUnavailable)?;
        if analyses.len() != candidates.len() {
            return Err(ProjectedPlanError::Evidence(
                "Stockfish returned the wrong number of candidate leaf evaluations",
            ));
        }
        let mut scored = candidates
            .into_iter()
            .zip(analyses)
            .map(|(candidate, timed)| {
                let analysis = normalize_live_engine_analysis(&candidate.position, timed.analysis)
                    .map_err(|_| {
                        ProjectedPlanError::Evidence("candidate leaf evaluation is invalid")
                    })?;
                let evaluation = evaluation_for_player(&analysis.evaluation, seed.player)?;
                Ok(ScoredCandidate {
                    candidate,
                    evaluation,
                })
            })
            .collect::<Result<Vec<_>, ProjectedPlanError>>()?;
        scored.sort_by(compare_candidates);
        let selected = scored
            .into_iter()
            .next()
            .ok_or(ProjectedPlanError::Evidence(
                "Maia produced no complete candidate line",
            ))?;

        Ok(ProjectedPlan {
            san: san_line(&seed.position, &selected.candidate.moves)?,
            objective_counterplay_san: san_line(&seed.position, &objective.principal_variation)?,
            provenance,
        })
    }
}

fn projected_plan_provenance(
    engine: &dyn EngineAnalyzer,
    human: &dyn HumanMoveModel,
    elo: EloRating,
) -> Option<ProjectedPlanProvenance> {
    Some(ProjectedPlanProvenance {
        stockfish: crate::provider_provenance::stockfish(engine.provenance()?)?,
        maia: crate::provider_provenance::identified_maia(&human.cache_identity()?, elo)?,
    })
}

#[derive(Clone)]
struct ProjectionSeed {
    position: PositionSnapshot,
    history: Vec<String>,
    player: Color,
    elo: EloProfile,
}

impl ProjectionSeed {
    fn from_core(core: &ReviewSessionCoreContract) -> Result<Self, ProjectedPlanError> {
        let selected_index = usize::from(
            core.coach_turn_context
                .reviewed_move
                .ply
                .checked_sub(1)
                .ok_or(ProjectedPlanError::Binding("reviewed ply must be positive"))?,
        );
        let (root, root_history) =
            reconstruct_selected_position(&core.imported_game, selected_index).map_err(|_| {
                ProjectedPlanError::Binding(
                    "reviewed Position cannot be reconstructed from the imported Game",
                )
            })?;
        if root != core.position_snapshot {
            return Err(ProjectedPlanError::Binding(
                "reconstructed Position does not match the Review Session",
            ));
        }
        let (_, prior_history) = root_history
            .split_last()
            .ok_or(ProjectedPlanError::Binding(
                "reviewed Position history is empty",
            ))?;
        let position = play(
            &root,
            prior_history,
            &core.coach_turn_context.reviewed_move.played_move_uci,
        )?;
        let elo = EloProfile::try_from(core.imported_game.elo_profile.rating.value())
            .map_err(|_| ProjectedPlanError::Binding("resolved Elo is outside Maia limits"))?;
        Ok(Self {
            position,
            history: root_history,
            player: core.coach_turn_context.reviewed_move.side,
            elo,
        })
    }
}

#[derive(Clone)]
struct Candidate {
    position: PositionSnapshot,
    history: Vec<String>,
    moves: Vec<String>,
    joint_probability: f64,
}

struct ScoredCandidate {
    candidate: Candidate,
    evaluation: EngineEvaluation,
}

async fn build_maia_candidates(
    seed: ProjectionSeed,
    human: Arc<dyn HumanMoveModel>,
) -> Result<Vec<Candidate>, ProjectedPlanError> {
    let mut beam = vec![Candidate {
        position: seed.position,
        history: seed.history,
        moves: Vec::new(),
        joint_probability: 1.0,
    }];

    for _ in 0..PROJECTED_PLAN_REQUIRED_HALF_MOVES {
        if beam
            .iter()
            .any(|candidate| !matches!(candidate.position.status, PositionStatus::Ongoing { .. }))
        {
            return Err(ProjectedPlanError::Evidence(
                "a Maia candidate ended before four half-moves",
            ));
        }
        let positions = beam
            .iter()
            .map(|candidate| candidate.position.fen.clone())
            .collect::<Vec<_>>();
        let elo = seed.elo;
        let model = human.clone();
        let predictions = collect_ordered_provider_positions(
            positions,
            PROJECTED_PLAN_BEAM_WIDTH,
            move |position| {
                let human = model.clone();
                async move {
                    human
                        .predict(crate::human_move_model::HumanMoveInput {
                            position: &position,
                            elo,
                            limit: PINNED_MAIA_CANDIDATE_LIMIT,
                        })
                        .await
                }
            },
        )
        .await
        .map_err(|_| ProjectedPlanError::MaiaUnavailable)?;

        let mut expanded = Vec::new();
        for (candidate, prediction) in beam.into_iter().zip(predictions) {
            for (move_uci, probability) in normalized_candidates(&candidate.position, &prediction)?
            {
                let position = play(&candidate.position, &candidate.history, &move_uci)?;
                let mut history = candidate.history.clone();
                history.push(candidate.position.fen.clone());
                let mut moves = candidate.moves.clone();
                moves.push(move_uci);
                expanded.push(Candidate {
                    position,
                    history,
                    moves,
                    joint_probability: candidate.joint_probability * probability,
                });
            }
        }
        expanded.sort_by(|left, right| {
            right
                .joint_probability
                .total_cmp(&left.joint_probability)
                .then_with(|| left.moves.cmp(&right.moves))
        });
        expanded.truncate(PROJECTED_PLAN_BEAM_WIDTH);
        if expanded.is_empty() {
            return Err(ProjectedPlanError::Evidence(
                "the global Maia beam became empty",
            ));
        }
        beam = expanded;
    }

    if beam.len() != PROJECTED_PLAN_BEAM_WIDTH
        || beam
            .iter()
            .any(|candidate| candidate.moves.len() != PROJECTED_PLAN_REQUIRED_HALF_MOVES)
    {
        return Err(ProjectedPlanError::Evidence(
            "Maia did not produce three complete four-half-move lines",
        ));
    }
    Ok(beam)
}

fn normalized_candidates(
    position: &PositionSnapshot,
    prediction: &HumanMovePrediction,
) -> Result<Vec<(String, f64)>, ProjectedPlanError> {
    let chess = parse_position(position)?;
    let mut seen = BTreeSet::new();
    let mut candidates = prediction
        .candidates
        .iter()
        .map(|candidate| {
            if !candidate.probability.is_finite() || !(0.0..=1.0).contains(&candidate.probability) {
                return Err(ProjectedPlanError::Evidence(
                    "a Maia probability is outside zero through one",
                ));
            }
            if !seen.insert(candidate.uci.as_str()) {
                return Err(ProjectedPlanError::Evidence(
                    "Maia candidates contain a duplicate move",
                ));
            }
            if candidate.probability == 0.0 {
                return Ok(None);
            }
            let uci = UciMove::from_ascii(candidate.uci.as_bytes())
                .map_err(|_| ProjectedPlanError::Evidence("a Maia move is malformed"))?;
            uci.to_move(&chess)
                .map_err(|_| ProjectedPlanError::Evidence("a Maia move is illegal"))?;
            Ok(Some((candidate.uci.clone(), candidate.probability)))
        })
        .collect::<Result<Vec<_>, ProjectedPlanError>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    candidates.truncate(usize::from(PINNED_MAIA_CANDIDATE_LIMIT));
    if candidates.is_empty() {
        return Err(ProjectedPlanError::Evidence(
            "Maia returned no legal positive-probability candidates",
        ));
    }
    Ok(candidates)
}

fn compare_candidates(left: &ScoredCandidate, right: &ScoredCandidate) -> Ordering {
    compare_evaluations(&right.evaluation, &left.evaluation)
        .then_with(|| {
            right
                .candidate
                .joint_probability
                .total_cmp(&left.candidate.joint_probability)
        })
        .then_with(|| left.candidate.moves.cmp(&right.candidate.moves))
}

fn compare_evaluations(left: &EngineEvaluation, right: &EngineEvaluation) -> Ordering {
    match (left, right) {
        (
            EngineEvaluation::Mate {
                outcome: MateOutcome::Win,
                distance_plies: left,
                ..
            },
            EngineEvaluation::Mate {
                outcome: MateOutcome::Win,
                distance_plies: right,
                ..
            },
        ) => right.cmp(left),
        (
            EngineEvaluation::Mate {
                outcome: MateOutcome::Loss,
                distance_plies: left,
                ..
            },
            EngineEvaluation::Mate {
                outcome: MateOutcome::Loss,
                distance_plies: right,
                ..
            },
        ) => left.cmp(right),
        (
            EngineEvaluation::Centipawns { value: left, .. },
            EngineEvaluation::Centipawns { value: right, .. },
        ) => left.cmp(right),
        (
            EngineEvaluation::Mate {
                outcome: MateOutcome::Win,
                ..
            },
            _,
        )
        | (
            EngineEvaluation::Centipawns { .. },
            EngineEvaluation::Mate {
                outcome: MateOutcome::Loss,
                ..
            },
        ) => Ordering::Greater,
        (
            EngineEvaluation::Mate {
                outcome: MateOutcome::Loss,
                ..
            },
            _,
        )
        | (
            EngineEvaluation::Centipawns { .. },
            EngineEvaluation::Mate {
                outcome: MateOutcome::Win,
                ..
            },
        ) => Ordering::Less,
    }
}

fn evaluation_for_player(
    evaluation: &EngineEvaluation,
    player: Color,
) -> Result<EngineEvaluation, ProjectedPlanError> {
    match evaluation {
        EngineEvaluation::Centipawns { perspective, .. } if *perspective == player => {
            Ok(evaluation.clone())
        }
        EngineEvaluation::Centipawns { value, .. } => Ok(EngineEvaluation::Centipawns {
            value: value.checked_neg().ok_or(ProjectedPlanError::Evidence(
                "a centipawn evaluation cannot change perspective",
            ))?,
            perspective: player,
        }),
        EngineEvaluation::Mate { perspective, .. } if *perspective == player => {
            Ok(evaluation.clone())
        }
        EngineEvaluation::Mate {
            outcome,
            distance_plies,
            ..
        } => Ok(EngineEvaluation::Mate {
            outcome: match outcome {
                MateOutcome::Win => MateOutcome::Loss,
                MateOutcome::Loss => MateOutcome::Win,
            },
            distance_plies: *distance_plies,
            perspective: player,
        }),
    }
}

fn play(
    source: &PositionSnapshot,
    history: &[String],
    uci: &str,
) -> Result<PositionSnapshot, ProjectedPlanError> {
    let mut chess = parse_position(source)?;
    let chess_move = UciMove::from_ascii(uci.as_bytes())
        .map_err(|_| ProjectedPlanError::Evidence("a projected move is malformed"))?
        .to_move(&chess)
        .map_err(|_| ProjectedPlanError::Evidence("a projected move is illegal"))?;
    chess.play_unchecked(&chess_move);
    let fen = Fen::from_position(chess, EnPassantMode::Legal).to_string();
    let mut preceding = history.to_vec();
    preceding.push(source.fen.clone());
    let preceding = preceding.iter().map(String::as_str).collect::<Vec<_>>();
    build_position_snapshot(&fen, &preceding)
        .map_err(|_| ProjectedPlanError::Evidence("a projected Position cannot be normalized"))
}

fn san_line(
    source: &PositionSnapshot,
    moves: &[String],
) -> Result<Vec<String>, ProjectedPlanError> {
    let mut chess = parse_position(source)?;
    let mut san = Vec::with_capacity(moves.len());
    for uci in moves {
        let chess_move = UciMove::from_ascii(uci.as_bytes())
            .map_err(|_| ProjectedPlanError::Evidence("a projected line move is malformed"))?
            .to_move(&chess)
            .map_err(|_| ProjectedPlanError::Evidence("a projected line move is illegal"))?;
        san.push(SanPlus::from_move(chess.clone(), &chess_move).to_string());
        chess.play_unchecked(&chess_move);
    }
    if san.is_empty() {
        return Err(ProjectedPlanError::Evidence(
            "a projected SAN line is empty",
        ));
    }
    Ok(san)
}

fn parse_position(position: &PositionSnapshot) -> Result<Chess, ProjectedPlanError> {
    Fen::from_ascii(position.fen.as_bytes())
        .map_err(|_| ProjectedPlanError::Evidence("a projected Position FEN is invalid"))?
        .into_position(CastlingMode::Standard)
        .map_err(|_| ProjectedPlanError::Evidence("a projected Position is illegal"))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        future::Future,
        path::Path,
        pin::Pin,
        sync::{Arc, Mutex},
    };

    use crate::{
        engine_analysis::{
            EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, PositionEvaluation,
        },
        human_move_model::{HumanMoveInput, HumanMoveModelError},
        review_session_contract::{
            CoachTurnId, ImportedGame, RequestId, ReviewMomentSelection, ReviewSessionCoreContract,
            STANDARD_STARTING_FEN,
        },
        review_session_start::start_review_session,
        types::HumanMoveCandidate,
    };

    use super::*;

    /// The Position the canonical Game reaches after its first move, which is
    /// where a Projected Plan for the ply-1 moment is rooted.
    const AFTER_FIRST_MOVE: &str = "rnbqkbnr/pppppppp/8/8/8/2P5/PP1PPPPP/RNBQKBNR b KQkq - 0 1";

    #[tokio::test]
    async fn a_lower_probability_line_wins_on_leaf_evaluation_without_stockfish_injection() {
        let fixture = Fixture::new([0, -100, -50]);
        let plan = fixture.build().await;

        assert_eq!(plan.projected_plan_san(), ["c5", "d4", "cxd4", "cxd4"]);
        assert_eq!(plan.objective_counterplay_san(), ["e5", "Nf3"]);
        assert_ne!(
            plan.projected_plan_san()[0],
            plan.objective_counterplay_san()[0]
        );
        fixture.assert_provider_boundary();
    }

    #[tokio::test]
    async fn joint_maia_probability_breaks_an_objective_evaluation_tie() {
        let fixture = Fixture::new([0, 0, 0]);
        let plan = fixture.build().await;

        assert_eq!(plan.projected_plan_san(), ["c5", "Nf3", "Nc6", "d4"]);
        assert_eq!(plan.objective_counterplay_san(), ["e5", "Nf3"]);
        fixture.assert_provider_boundary();
    }

    struct Fixture {
        core: ReviewSessionCoreContract,
        builder: ProjectedPlanBuilder,
        human_observations: Arc<Mutex<Vec<(u16, u8)>>>,
        engine_positions: Arc<Mutex<Vec<String>>>,
    }

    impl Fixture {
        fn new(leaf_scores_from_side_to_move: [i32; 3]) -> Self {
            let starting = build_position_snapshot(STANDARD_STARTING_FEN, &[]).unwrap();
            let post_move =
                build_position_snapshot(AFTER_FIRST_MOVE, &[starting.fen.as_str()]).unwrap();
            let mut predictions = BTreeMap::new();
            predictions.insert(
                post_move.fen.clone(),
                prediction(vec![("e7e5", 0.45), ("c7c5", 0.40)]),
            );

            let history = vec![starting.fen.clone()];
            let after_e5 = position_after(&post_move, &history, &["e7e5"]);
            let after_c5 = position_after(&post_move, &history, &["c7c5"]);
            predictions.insert(
                after_e5.fen.clone(),
                prediction(vec![("g1f3", 0.50), ("d2d4", 0.20)]),
            );
            predictions.insert(
                after_c5.fen.clone(),
                prediction(vec![("g1f3", 0.60), ("d2d4", 0.40)]),
            );

            let after_c5_nf3 = position_after(&post_move, &history, &["c7c5", "g1f3"]);
            let after_c5_d4 = position_after(&post_move, &history, &["c7c5", "d2d4"]);
            let after_e5_nf3 = position_after(&post_move, &history, &["e7e5", "g1f3"]);
            predictions.insert(after_c5_nf3.fen.clone(), prediction(vec![("b8c6", 1.0)]));
            predictions.insert(after_c5_d4.fen.clone(), prediction(vec![("c5d4", 1.0)]));
            predictions.insert(after_e5_nf3.fen.clone(), prediction(vec![("b8c6", 1.0)]));

            let after_c5_nf3_nc6 = position_after(&post_move, &history, &["c7c5", "g1f3", "b8c6"]);
            let after_c5_d4_cxd4 = position_after(&post_move, &history, &["c7c5", "d2d4", "c5d4"]);
            let after_e5_nf3_nc6 = position_after(&post_move, &history, &["e7e5", "g1f3", "b8c6"]);
            predictions.insert(
                after_c5_nf3_nc6.fen.clone(),
                prediction(vec![("d2d4", 1.0)]),
            );
            predictions.insert(
                after_c5_d4_cxd4.fen.clone(),
                prediction(vec![("c3d4", 1.0)]),
            );
            predictions.insert(
                after_e5_nf3_nc6.fen.clone(),
                prediction(vec![("d2d4", 1.0)]),
            );

            let paths = [
                vec!["c7c5", "g1f3", "b8c6", "d2d4"],
                vec!["c7c5", "d2d4", "c5d4", "c3d4"],
                vec!["e7e5", "g1f3", "b8c6", "d2d4"],
            ];
            let mut leaves = BTreeMap::new();
            for (moves, score) in paths.iter().zip(leaf_scores_from_side_to_move) {
                leaves.insert(position_after(&post_move, &history, moves).fen, score);
            }

            let human_observations = Arc::new(Mutex::new(Vec::new()));
            let engine_positions = Arc::new(Mutex::new(Vec::new()));
            let human = Arc::new(FakeHuman {
                predictions,
                observations: human_observations.clone(),
            });
            let engine = Arc::new(FakeEngine {
                leaves,
                positions: engine_positions.clone(),
            });
            let snapshot: ImportedGame = serde_json::from_slice(
                &fs::read(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../packages/coach-engine-sdk/fixtures/imported-game.json"),
                )
                .unwrap(),
            )
            .unwrap();
            let core = start_review_session(
                RequestId::try_from("request:projected-plan:test".to_string()).unwrap(),
                CoachTurnId::try_from("coach-turn:projected-plan:test".to_string()).unwrap(),
                snapshot,
                ReviewMomentSelection::PlayerSelectedMoment { ply: 1 },
            )
            .unwrap();
            Self {
                core,
                builder: ProjectedPlanBuilder::new(engine, human),
                human_observations,
                engine_positions,
            }
        }

        async fn build(&self) -> ProjectedPlan {
            self.builder.build(&self.core).await.unwrap()
        }

        fn assert_provider_boundary(&self) {
            let observations = self.human_observations.lock().unwrap();
            assert_eq!(observations.len(), 9);
            assert!(observations
                .iter()
                .all(|observation| *observation == (1246, PINNED_MAIA_CANDIDATE_LIMIT)));
            assert_eq!(self.engine_positions.lock().unwrap().len(), 4);
        }
    }

    struct FakeHuman {
        predictions: BTreeMap<String, HumanMovePrediction>,
        observations: Arc<Mutex<Vec<(u16, u8)>>>,
    }

    impl HumanMoveModel for FakeHuman {
        fn predict<'a>(
            &'a self,
            input: HumanMoveInput<'a>,
        ) -> Pin<
            Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>,
        > {
            Box::pin(async move {
                self.observations
                    .lock()
                    .unwrap()
                    .push((input.elo.rating(), input.limit));
                self.predictions
                    .get(input.position)
                    .cloned()
                    .ok_or_else(|| {
                        HumanMoveModelError::InvalidInput("unexpected test Position".to_string())
                    })
            })
        }
    }

    struct FakeEngine {
        leaves: BTreeMap<String, i32>,
        positions: Arc<Mutex<Vec<String>>>,
    }

    impl EngineAnalyzer for FakeEngine {
        fn analyze<'a>(
            &'a self,
            input: EngineAnalysisInput<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
        {
            Box::pin(async move {
                self.positions
                    .lock()
                    .unwrap()
                    .push(input.position.to_string());
                if input.position == AFTER_FIRST_MOVE {
                    return Ok(analysis(input.position, 0, Some(vec!["e7e5", "g1f3"])));
                }
                let score = self.leaves.get(input.position).copied().ok_or_else(|| {
                    EngineAnalysisError::InvalidInput("unexpected test Position".to_string())
                })?;
                Ok(analysis(input.position, score, None))
            })
        }
    }

    fn prediction(moves: Vec<(&str, f64)>) -> HumanMovePrediction {
        HumanMovePrediction {
            candidates: moves
                .into_iter()
                .enumerate()
                .map(|(index, (uci, probability))| HumanMoveCandidate {
                    uci: uci.to_string(),
                    probability,
                    rank: index + 1,
                })
                .collect(),
            win_probability: None,
        }
    }

    fn position_after(
        source: &PositionSnapshot,
        preceding: &[String],
        moves: &[&str],
    ) -> PositionSnapshot {
        let mut position = source.clone();
        let mut history = preceding.to_vec();
        for uci in moves {
            let next = play(&position, &history, uci).unwrap();
            history.push(position.fen.clone());
            position = next;
        }
        position
    }

    fn analysis(
        fen: &str,
        centipawns: i32,
        principal_variation: Option<Vec<&str>>,
    ) -> EngineAnalysis {
        let chess: Chess = Fen::from_ascii(fen.as_bytes())
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        let principal_variation = principal_variation.map_or_else(
            || {
                vec![UciMove::from_move(
                    chess.legal_moves().first().unwrap(),
                    CastlingMode::Standard,
                )
                .to_string()]
            },
            |moves| moves.into_iter().map(str::to_string).collect(),
        );
        EngineAnalysis {
            best_move: principal_variation[0].clone(),
            evaluation: PositionEvaluation::Centipawns(centipawns),
            principal_variation,
            depth: crate::evaluation_recording::PINNED_STOCKFISH_DEPTH,
        }
    }
}
