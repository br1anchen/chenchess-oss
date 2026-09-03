use std::{
    collections::BTreeMap,
    fs,
    future::{pending, Future},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use chen_chess_coach_engine::{
    engine_analysis::{
        EngineAnalysis, EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer, EngineProvenance,
        PositionEvaluation,
    },
    evaluation_recording::{
        ReviewSessionProviderRecording, PINNED_STOCKFISH_BINARY_DIGEST, PINNED_STOCKFISH_DEPTH,
        PINNED_STOCKFISH_HASH_MIB, PINNED_STOCKFISH_THREADS, PINNED_STOCKFISH_VERSION,
    },
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction},
    lichess::{
        LichessExportClient, LichessExportError, LichessExportRequest, LichessExportResponse,
        LICHESS_PGN_MEDIA_TYPE,
    },
    pgn::parse_pgn,
    review_session_coaching::{AlternativeMoveAssessmentAuthor, CoachTurnAuthorInput},
    review_session_contract::*,
    review_session_processor::ReviewSessionProcessor,
    types::HumanMoveCandidate,
};
use serde::{de::DeserializeOwned, Deserialize};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};
use tokio::sync::watch;

#[derive(Clone)]
pub struct CapturedLichess {
    response: LichessExportResponse,
}

impl CapturedLichess {
    pub fn new() -> Self {
        Self {
            response: LichessExportResponse {
                body: fs::read(canonical_fixture_root().join("lichess-export.raw.pgn")).unwrap(),
                content_type: LICHESS_PGN_MEDIA_TYPE.to_string(),
                captured_at: "2026-09-03T00:00:00Z".parse().unwrap(),
            },
        }
    }
}

impl LichessExportClient for CapturedLichess {
    fn export<'a>(
        &'a self,
        _request: &'a LichessExportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LichessExportResponse, LichessExportError>> + Send + 'a>>
    {
        Box::pin(async move { Ok(self.response.clone()) })
    }
}

pub struct RecordingEngine {
    responses: BTreeMap<String, EngineAnalysis>,
    calls: AtomicUsize,
}

struct FailingAfterImportEngine {
    recording: RecordingEngine,
    remaining_import_calls: AtomicUsize,
}

impl FailingAfterImportEngine {
    fn new(recording: &ReviewSessionProviderRecording) -> Self {
        Self {
            recording: RecordingEngine::new(recording),
            remaining_import_calls: AtomicUsize::new(canonical_game_plies()),
        }
    }
}

impl EngineAnalyzer for FailingAfterImportEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            if self
                .remaining_import_calls
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                    remaining.checked_sub(1)
                })
                .is_ok()
            {
                return self.recording.analyze(input).await;
            }
            Err(EngineAnalysisError::Protocol(
                "controlled Stockfish failure".to_string(),
            ))
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        self.recording.provenance()
    }
}

impl RecordingEngine {
    pub fn new(recording: &ReviewSessionProviderRecording) -> Self {
        let mut responses = full_game_recording()
            .into_iter()
            .map(|(position, evidence)| (position, evidence.engine_before))
            .collect::<BTreeMap<_, _>>();
        let positions = positions_by_ref(recording);
        responses.extend(
            recording
                .content
                .entries
                .iter()
                .filter_map(|entry| match entry {
                    EvidenceEntry::EngineAnalysis {
                        position_ref,
                        analysis,
                        ..
                    } => Some((
                        positions.get(position_ref).unwrap().clone(),
                        EngineAnalysis {
                            best_move: analysis.best_move_uci.clone(),
                            evaluation: raw_evaluation(&analysis.evaluation),
                            principal_variation: analysis.principal_variation.clone(),
                            depth: PINNED_STOCKFISH_DEPTH,
                        },
                    )),
                    _ => None,
                }),
        );
        Self {
            responses,
            calls: AtomicUsize::new(0),
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl EngineAnalyzer for RecordingEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.responses
                .get(input.position)
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| dynamic_analysis(input.position))
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        Some(pinned_engine_provenance())
    }
}

pub struct RecordingHuman {
    responses: BTreeMap<String, HumanMovePrediction>,
    blocking: bool,
    hold: AtomicBool,
    nonblocking_calls: AtomicUsize,
    calls: AtomicUsize,
    exposes_provenance: bool,
    started: watch::Sender<bool>,
}

impl RecordingHuman {
    pub fn new(recording: &ReviewSessionProviderRecording, blocking: bool) -> Self {
        Self::new_with_provenance(recording, blocking, false)
    }

    pub fn new_with_provenance(
        recording: &ReviewSessionProviderRecording,
        blocking: bool,
        exposes_provenance: bool,
    ) -> Self {
        let mut responses = full_game_recording()
            .into_iter()
            .map(|(position, evidence)| (position, evidence.human_before))
            .collect::<BTreeMap<_, _>>();
        let positions = positions_by_ref(recording);
        responses.extend(recording.content.entries.iter().filter_map(|entry| {
            match entry {
                EvidenceEntry::HumanMoveModel {
                    position_ref,
                    analysis,
                    ..
                } => Some((
                    positions.get(position_ref).unwrap().clone(),
                    HumanMovePrediction {
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
                            ProbabilityState::Available { probability } => {
                                Some(probability.value())
                            }
                            ProbabilityState::Unavailable => None,
                        },
                    },
                )),
                _ => None,
            }
        }));
        let (started, _) = watch::channel(false);
        let nonblocking_calls = if blocking { canonical_game_plies() } else { 0 };
        Self {
            responses,
            blocking,
            hold: AtomicBool::new(false),
            nonblocking_calls: AtomicUsize::new(nonblocking_calls),
            calls: AtomicUsize::new(0),
            exposes_provenance,
            started,
        }
    }

    /// Hold every later Maia predict so a Coach Turn stays in flight after setup.
    pub fn begin_holding(&self) {
        self.hold.store(true, Ordering::SeqCst);
    }

    pub async fn wait_until_started(&self) {
        let mut started = self.started.subscribe();
        while !*started.borrow_and_update() {
            started.changed().await.unwrap();
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl HumanMoveModel for RecordingHuman {
    fn predict<'a>(
        &'a self,
        input: HumanMoveInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<HumanMovePrediction, HumanMoveModelError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let should_block = self.hold.load(Ordering::SeqCst)
                || (self.blocking
                    && self
                        .nonblocking_calls
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                            remaining.checked_sub(1)
                        })
                        .is_err());
            if should_block {
                self.started.send_replace(true);
                return pending().await;
            }
            Ok(self
                .responses
                .get(input.position)
                .cloned()
                .unwrap_or_else(|| dynamic_human_prediction(input.position)))
        })
    }

    fn cache_identity(
        &self,
    ) -> Option<chen_chess_coach_engine::human_move_model::HumanMoveCacheIdentity> {
        self.exposes_provenance
            .then(|| {
                chen_chess_coach_engine::human_move_model::MaiaHttpAdapter::new(
                    "http://unused.test",
                )
            })
            .and_then(|adapter| adapter.cache_identity())
    }
}

#[derive(Default)]
pub struct GroundedAuthor;

impl AlternativeMoveAssessmentAuthor for GroundedAuthor {
    fn assess<'a>(
        &'a self,
        input: CoachTurnAuthorInput,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<AlternativeMoveAssessment, ProviderUnavailableReason>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { Ok(coach_assessment(&input)) })
    }
}

pub struct BlockingAuthor {
    started: watch::Sender<bool>,
}

impl BlockingAuthor {
    fn new() -> Self {
        let (started, _) = watch::channel(false);
        Self { started }
    }

    pub async fn wait_until_started(&self) {
        let mut started = self.started.subscribe();
        while !*started.borrow_and_update() {
            started.changed().await.unwrap();
        }
    }
}

impl AlternativeMoveAssessmentAuthor for BlockingAuthor {
    fn assess<'a>(
        &'a self,
        _input: CoachTurnAuthorInput,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<AlternativeMoveAssessment, ProviderUnavailableReason>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.started.send_replace(true);
            pending().await
        })
    }
}

pub fn processor(
    blocking_human: bool,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<RecordingHuman>,
    ReviewSessionProviderRecording,
) {
    processor_with_options(blocking_human, None)
}

pub fn processor_with_runtime_startup(
    runtime_startup: Duration,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<RecordingHuman>,
    ReviewSessionProviderRecording,
) {
    processor_with_options(false, Some(runtime_startup))
}

fn processor_with_options(
    blocking_human: bool,
    runtime_startup: Option<Duration>,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<RecordingHuman>,
    ReviewSessionProviderRecording,
) {
    let recording = provider_recording();
    let human = Arc::new(RecordingHuman::new(&recording, blocking_human));
    let processor = ReviewSessionProcessor::new(
        CapturedLichess::new(),
        recording.clone(),
        Arc::new(RecordingEngine::new(&recording)),
        human.clone(),
        Arc::new(GroundedAuthor),
    )
    .unwrap();
    let processor = match runtime_startup {
        Some(runtime_startup) => processor.with_runtime_startup(runtime_startup),
        None => processor,
    };
    (Arc::new(processor), human, recording)
}

pub fn processor_with_failing_engine() -> Arc<ReviewSessionProcessor<CapturedLichess>> {
    let recording = provider_recording();
    Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(FailingAfterImportEngine::new(&recording)),
            Arc::new(RecordingHuman::new(&recording, false)),
            Arc::new(GroundedAuthor),
        )
        .unwrap(),
    )
}

pub fn processor_with_blocking_author() -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<BlockingAuthor>,
) {
    let recording = provider_recording();
    let author = Arc::new(BlockingAuthor::new());
    let processor = ReviewSessionProcessor::new(
        CapturedLichess::new(),
        recording.clone(),
        Arc::new(RecordingEngine::new(&recording)),
        Arc::new(RecordingHuman::new(&recording, false)),
        author.clone(),
    )
    .unwrap();
    (Arc::new(processor), author)
}

pub fn provider_recording() -> ReviewSessionProviderRecording {
    fixture(canonical_fixture_root().join("review-session-provider-recording.json"))
}

fn coach_assessment(input: &CoachTurnAuthorInput) -> AlternativeMoveAssessment {
    grounded_coach_assessment(
        &input.coach_turn_id,
        &input.target.target().alternative_move_id,
        &input.evidence,
    )
}

pub fn grounded_coach_assessment(
    coach_turn_id: &CoachTurnId,
    alternative_move_id: &AlternativeMoveId,
    evidence: &AlternativeMoveAssessmentEvidenceRefs,
) -> AlternativeMoveAssessment {
    AlternativeMoveAssessment {
        coach_turn_id: coach_turn_id.clone(),
        alternative_move_id: alternative_move_id.clone(),
        objective_quality: dimension(
            OBJECTIVE_QUALITY_PROSE,
            [
                evidence.target_branch.clone(),
                evidence.source_engine.clone(),
                evidence.resulting_engine.clone(),
            ],
        ),
        findability: dimension(
            FINDABILITY_PROSE,
            [
                evidence.target_branch.clone(),
                evidence.source_human.clone(),
            ],
        ),
        resilience: dimension(
            RESILIENCE_PROSE,
            [
                evidence.target_branch.clone(),
                evidence.resulting_engine.clone(),
                evidence.resulting_human.clone(),
            ],
        ),
    }
}

/// Marker-form assessment prose, one vocabulary per dimension.
///
/// Each dimension may only name what it cites, so the objective-quality
/// explanation is the only one that can carry an evaluation.
fn dimension(explanation: &str, ids: impl IntoIterator<Item = EvidenceId>) -> AssessmentDimension {
    AssessmentDimension {
        explanation: explanation.to_string(),
        evidence_refs: ids.into_iter().collect(),
    }
}

const OBJECTIVE_QUALITY_PROSE: &str =
    "By the engine's reckoning {alternativeMove} lands at {alternativeEval}, against {bestMove} at {bestEval}.";
const FINDABILITY_PROSE: &str =
    "Whether {alternativeMove} turns up at the board is the real question here.";
const RESILIENCE_PROSE: &str =
    "After {alternativeMove} the reply that decides it is {strongestReply}.";

fn positions_by_ref(recording: &ReviewSessionProviderRecording) -> BTreeMap<PositionRef, String> {
    recording
        .content
        .entries
        .iter()
        .filter_map(|entry| match entry {
            EvidenceEntry::Position { position, .. } => {
                Some((position.position_ref.clone(), position.fen.clone()))
            }
            _ => None,
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FullGameRecording {
    input: FullGameInput,
    evidence: Vec<FullGameEvidence>,
}

#[derive(Deserialize)]
struct FullGameInput {
    pgn: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FullGameEvidence {
    ply: usize,
    engine_before: EngineAnalysis,
    human_before: HumanMovePrediction,
}

fn full_game_recording() -> Vec<(String, FullGameEvidence)> {
    let recording: FullGameRecording =
        fixture(canonical_fixture_root().join("provider-recordings/full-game.case.json"));
    let game = parse_pgn(&recording.input.pgn).expect("captured full Game PGN is valid");
    assert_eq!(
        game.moves.len(),
        recording.evidence.len(),
        "captured provider evidence covers every Game ply"
    );
    game.moves
        .into_iter()
        .zip(recording.evidence)
        .map(|(game_move, evidence)| {
            assert_eq!(game_move.ply, evidence.ply);
            (game_move.position, evidence)
        })
        .collect()
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

fn dynamic_analysis(position: &str) -> Result<EngineAnalysis, EngineAnalysisError> {
    let chess: Chess = Fen::from_ascii(position.as_bytes())
        .map_err(|_| EngineAnalysisError::InvalidInput("bad FEN".to_string()))?
        .into_position(CastlingMode::Standard)
        .map_err(|_| EngineAnalysisError::InvalidInput("illegal FEN".to_string()))?;
    let best_move = chess
        .legal_moves()
        .first()
        .map(|chess_move| UciMove::from_move(chess_move, CastlingMode::Standard).to_string())
        .unwrap_or_else(|| "0000".to_string());
    Ok(EngineAnalysis {
        principal_variation: if best_move == "0000" {
            Vec::new()
        } else {
            vec![best_move.clone()]
        },
        best_move,
        evaluation: if chess.is_checkmate() {
            PositionEvaluation::MateIn(0)
        } else {
            PositionEvaluation::Centipawns(0)
        },
        depth: PINNED_STOCKFISH_DEPTH,
    })
}

fn dynamic_human_prediction(position: &str) -> HumanMovePrediction {
    let chess: Chess = Fen::from_ascii(position.as_bytes())
        .expect("test position should be valid FEN")
        .into_position(CastlingMode::Standard)
        .expect("test position should be legal");
    let move_uci = chess
        .legal_moves()
        .first()
        .map(|chess_move| UciMove::from_move(chess_move, CastlingMode::Standard).to_string())
        .expect("test position should have a legal move");
    HumanMovePrediction {
        candidates: vec![HumanMoveCandidate {
            uci: move_uci,
            probability: 1.0,
            rank: 1,
        }],
        win_probability: Some(0.5),
    }
}

fn pinned_engine_provenance() -> EngineProvenance {
    EngineProvenance {
        version: PINNED_STOCKFISH_VERSION.to_string(),
        binary_sha256: PINNED_STOCKFISH_BINARY_DIGEST
            .strip_prefix("sha256:")
            .expect("pinned Stockfish digest is canonical")
            .to_string(),
        depth: PINNED_STOCKFISH_DEPTH,
        threads: PINNED_STOCKFISH_THREADS,
        hash_mib: PINNED_STOCKFISH_HASH_MIB,
    }
}

fn fixture<T: DeserializeOwned>(path: impl AsRef<Path>) -> T {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn canonical_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/shared-assets/fixtures/Synthet1")
}

pub fn canonical_game_plies() -> usize {
    let pgn = fs::read_to_string(canonical_fixture_root().join("lichess-export.raw.pgn"))
        .expect("captured Lichess fixture is UTF-8");
    parse_pgn(&pgn)
        .expect("captured Lichess fixture is valid")
        .moves
        .len()
}
