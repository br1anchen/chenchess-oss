//! Offline replay of one Coach Turn target, for
//! #359.
//!
//! Task A got this capability from
//! [`super::recorded_comment_case`]: address one recorded moment, compile its
//! prompt, run the real gate, all with no provider running. Task B needs the
//! same thing and its unit is different — a Coach Turn is about an *Alternative
//! Move*, which exists only after the board has been explored, so replaying one
//! means replaying the exploration that produced it.
//!
//! Everything here comes out of the frozen `Synthet1` provider recording:
//! Stockfish for the alternative and its reply, Maia for both positions. No
//! Docker, no Local Pipeline Runtime, and — the point — no re-recording, which
//! is why #345 §5.2 could
//! put Task B's whole cost at zero.

use std::{collections::BTreeMap, fs, future::Future, path::Path, pin::Pin, sync::Arc};

use crate::{
    engine_analysis::{
        EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer,
        PositionEvaluation,
    },
    evaluation_recording::{ReviewSessionProviderRecording, PINNED_STOCKFISH_DEPTH},
    human_move_model::HumanMovePrediction,
    review_session_cancellation::ReviewSessionCancellation,
    review_session_coaching::{
        prepare_recorded_evidence, required_evidence_refs, CoachTurnEvidenceRefs,
        PreparedCoachTurnTarget,
    },
    review_session_contract::*,
    review_session_exploration::{AlternativeMoveExploration, ExploreAlternativeMoveRequest},
    review_session_start::start_review_session,
    types::HumanMoveCandidate,
};

use super::EvaluationError;

/// One Coach Turn target, ready to author against.
///
/// The three frozen turn sets (#345
/// §5.2) all ride one of these — they differ in the Player's message and in
/// whether a prior turn is visible, not in the evidence — so the replay
/// produces the target once and the caller composes turns on it.
pub struct RecordedCoachTurnCase {
    pub case_id: String,
    pub elo: u16,
    pub alternative_move_san: String,
    pub target: PreparedCoachTurnTarget,
    pub packet: ReviewSessionEvidencePacket,
    pub evidence: CoachTurnEvidenceRefs,
}

/// Replays the exploration that produces one Alternative Move, then prepares
/// the Coach Turn target on it.
///
/// `alternative_uci` is explored from the reviewed Position itself, which is
/// the root case; `Synthet1` ply 26 with `c5d4` is the one
/// #345 froze.
pub async fn recorded_coach_turn_case(
    recording_path: &Path,
    imported_game_path: &Path,
    ply: u16,
    alternative_uci: &str,
) -> Result<RecordedCoachTurnCase, EvaluationError> {
    let recording: ReviewSessionProviderRecording = read_json(recording_path)?;
    recording
        .verify()
        .map_err(|error| EvaluationError::Contract(error.to_string()))?;
    let imported_game: ImportedGame = read_json(imported_game_path)?;

    let core = start_review_session(
        request_id(&format!("bake-off:{ply}")),
        coach_turn_id("target"),
        imported_game,
        ReviewMomentSelection::PlayerSelectedMoment { ply },
    )
    .map_err(|error| EvaluationError::Contract(error.to_string()))?;

    let root_engine = recorded_entry(&recording, &core.position_snapshot.position_ref)?;
    let exploration = AlternativeMoveExploration::new(
        core.clone(),
        &root_engine,
        Arc::new(RecordedEngine::new(&recording)),
    )
    .map_err(|error| EvaluationError::Contract(error.to_string()))?;

    let root_position_ref = exploration.current_state().await.root_position.position_ref;
    let committed = exploration
        .explore(
            ExploreAlternativeMoveRequest {
                parent: BranchParent::Root {
                    position_ref: root_position_ref.clone(),
                },
                source_position_ref: root_position_ref,
                move_input: MoveInput::Uci {
                    uci: alternative_uci.to_string(),
                },
                idempotency_key: idempotency_key(alternative_uci),
            },
            ReviewSessionCancellation::default(),
        )
        .await
        .map_err(|error| EvaluationError::Contract(error.to_string()))?;

    let target = PreparedCoachTurnTarget::capture(
        &core,
        &exploration.current_state().await,
        &committed.alternative_move.alternative_move_id,
    )
    .map_err(|error| EvaluationError::Contract(error.to_string()))?;

    // The Human Move Model is consulted per Coach Turn rather than during
    // exploration, so its two predictions are replayed here and folded into the
    // packet exactly as the live path folds live ones.
    let source_prediction = recorded_prediction(
        &recording,
        &target.source_position().position_ref,
        target.elo(),
    )?;
    let resulting_prediction = recorded_prediction(
        &recording,
        &target.target().resulting_position.position_ref,
        target.elo(),
    )?;
    let (_, packet, _) =
        prepare_recorded_evidence(&target, source_prediction, resulting_prediction)
            .map_err(|error| EvaluationError::Contract(format!("{error:?}")))?;
    let evidence = required_evidence_refs(&target, &packet)
        .map_err(|error| EvaluationError::Contract(format!("{error:?}")))?;

    Ok(RecordedCoachTurnCase {
        case_id: format!("turns-own-{}", target.elo().value()),
        elo: target.elo().value(),
        alternative_move_san: alternative_uci.to_string(),
        target,
        packet,
        evidence,
    })
}

fn recorded_entry(
    recording: &ReviewSessionProviderRecording,
    position_ref: &PositionRef,
) -> Result<EvidenceEntry, EvaluationError> {
    recording
        .content
        .entries
        .iter()
        .find(|entry| {
            matches!(entry, EvidenceEntry::EngineAnalysis { position_ref: recorded, .. }
                if recorded == position_ref)
        })
        .cloned()
        .ok_or_else(|| {
            EvaluationError::Contract(format!(
                "the provider recording has no Engine Analysis for {}",
                position_ref.as_str()
            ))
        })
}

fn recorded_prediction(
    recording: &ReviewSessionProviderRecording,
    position_ref: &PositionRef,
    elo: EloRating,
) -> Result<HumanMovePrediction, EvaluationError> {
    let analysis = recording
        .replay()
        .map_err(|error| EvaluationError::Contract(error.to_string()))?
        .maia(position_ref, elo)
        .map_err(|error| {
            EvaluationError::Contract(format!("{} at {}", error, position_ref.as_str()))
        })?
        .clone();
    Ok(HumanMovePrediction {
        candidates: analysis
            .candidates
            .iter()
            .map(|candidate| HumanMoveCandidate {
                uci: candidate.uci.clone(),
                probability: candidate.probability.value(),
                rank: usize::from(candidate.rank),
            })
            .collect(),
        win_probability: match analysis.win_probability {
            ProbabilityState::Available { probability } => Some(probability.value()),
            ProbabilityState::Unavailable => None,
        },
    })
}

/// Stockfish, replayed. Exploration asks by FEN, so the recording's positions
/// are indexed by FEN rather than by reference.
struct RecordedEngine {
    responses: BTreeMap<String, EngineAnalysis>,
}

impl RecordedEngine {
    fn new(recording: &ReviewSessionProviderRecording) -> Self {
        let fens = recording
            .content
            .entries
            .iter()
            .filter_map(|entry| match entry {
                EvidenceEntry::Position { position, .. } => {
                    Some((position.position_ref.clone(), position.fen.clone()))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let responses = recording
            .content
            .entries
            .iter()
            .filter_map(|entry| match entry {
                EvidenceEntry::EngineAnalysis {
                    position_ref,
                    analysis,
                    ..
                } => Some((
                    fens.get(position_ref)?.clone(),
                    EngineAnalysis {
                        best_move: analysis.best_move_uci.clone(),
                        evaluation: raw_evaluation(&analysis.evaluation),
                        principal_variation: analysis.principal_variation.clone(),
                        depth: PINNED_STOCKFISH_DEPTH,
                    },
                )),
                _ => None,
            })
            .collect();
        Self { responses }
    }
}

impl EngineAnalyzer for RecordedEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.responses.get(input.position).cloned().ok_or_else(|| {
                EngineAnalysisError::Protocol(
                    "the provider recording has no search for this Position".to_string(),
                )
            })
        })
    }
}

fn raw_evaluation(evaluation: &EngineEvaluation) -> PositionEvaluation {
    match evaluation {
        EngineEvaluation::Centipawns { value, .. } => PositionEvaluation::Centipawns(*value),
        EngineEvaluation::Mate {
            outcome,
            distance_plies,
            ..
        } => PositionEvaluation::MateIn(match outcome {
            MateOutcome::Win => i32::from(*distance_plies),
            MateOutcome::Loss => -i32::from(*distance_plies),
        }),
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, EvaluationError> {
    let body = fs::read(path).map_err(|source| EvaluationError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&body).map_err(|source| EvaluationError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn request_id(suffix: &str) -> RequestId {
    RequestId::try_from(format!("request:{suffix}")).expect("a harness request id is valid")
}

fn coach_turn_id(suffix: &str) -> CoachTurnId {
    CoachTurnId::try_from(format!("coach-turn:bake-off:{suffix}"))
        .expect("a harness Coach Turn id is valid")
}

fn idempotency_key(suffix: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(format!("key:bake-off:{suffix}"))
        .expect("a harness idempotency key is valid")
}
