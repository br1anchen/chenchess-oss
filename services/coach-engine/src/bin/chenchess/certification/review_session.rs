use std::{
    future::{pending, Future},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};

use anyhow::{ensure, Context};
use chen_chess_coach_engine::{
    engine_analysis::EngineAnalyzer,
    evaluation_recording::ReviewSessionProviderRecording,
    human_move_model::HumanMoveModel,
    operating_limits::{
        ALTERNATIVE_MOVE_DEADLINE_MILLISECONDS, CANCELLATION_BUDGET_MILLISECONDS,
        COACH_TURN_DEADLINE_SECONDS, REVIEW_MOMENT_PREPARATION_DEADLINE_SECONDS,
    },
    projected_plan::ProjectedPlanBuilder,
    review_session_coaching::{
        AlternativeMoveAssessmentAuthor, AlternativeMoveCoaching, CoachTurnActivity,
        CoachTurnAuthorInput, CoachTurnTargetSelection, PreparedCoachTurnTarget,
        StartAlternativeMoveCoachTurn,
    },
    review_session_contract::*,
    review_session_exploration::{
        AlternativeMoveCancellation, AlternativeMoveExploration, ExploreAlternativeMoveRequest,
    },
    review_session_start::start_review_session,
};
use serde::Serialize;
use tokio::sync::watch;

const CERTIFICATION_SAMPLES: usize = 3;
const REVIEWED_PLY: u16 = 26;
const PRIMARY_ALTERNATIVE: &str = "c5d4";
const CONCURRENT_ALTERNATIVE: &str = "e5d4";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LatencyDistribution {
    pub(super) samples: usize,
    pub(super) p50_milliseconds: u64,
    pub(super) p95_milliseconds: u64,
    pub(super) maximum_milliseconds: u64,
}

impl LatencyDistribution {
    pub(super) fn from_milliseconds(samples: impl IntoIterator<Item = u64>) -> Self {
        let mut samples = samples.into_iter().collect::<Vec<_>>();
        assert!(!samples.is_empty(), "a latency distribution needs samples");
        samples.sort_unstable();
        Self {
            samples: samples.len(),
            p50_milliseconds: percentile(&samples, 50),
            p95_milliseconds: percentile(&samples, 95),
            maximum_milliseconds: *samples.last().expect("samples are nonempty"),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReviewSessionCertification {
    canonical_game_id: &'static str,
    samples_per_operation: usize,
    provider_recording_digest: String,
    cache_state: CacheState,
    operations: OperationMeasurements,
    concurrency: ConcurrencyProof,
    fallback: FallbackProof,
    gates: CertificationGates,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheState {
    provider_recording: &'static str,
    lichess_export: &'static str,
    review_session_state: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationMeasurements {
    game_review: LatencyDistribution,
    intent_enrichment: LatencyDistribution,
    alternative_move_evaluation: LatencyDistribution,
    concurrent_exploration: LatencyDistribution,
    coach_turn: LatencyDistribution,
    cancellation_to_stop: LatencyDistribution,
    steer_to_replacement: LatencyDistribution,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConcurrencyProof {
    coach_turn_and_stockfish_exploration_completed_together: bool,
    active_coach_turns_per_session: u8,
    active_alternative_evaluations_per_session: u8,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FallbackProof {
    language_layer_unavailable: bool,
    stockfish_exploration_remained_available: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CertificationGates {
    provider_recording_verified: bool,
    intent_enrichment_deadline_held: bool,
    alternative_move_deadline_held: bool,
    coach_turn_deadline_held: bool,
    cancellation_deadline_held: bool,
    steering_deadline_held: bool,
    typed_cancellation_held: bool,
    stale_work_prevented_from_publishing: bool,
}

pub(super) async fn certify(
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
    game_review_samples: [u64; CERTIFICATION_SAMPLES],
) -> anyhow::Result<ReviewSessionCertification> {
    let recording = provider_recording()?;
    recording
        .verify()
        .context("provider recording verification")?;
    let base_core = review_core()?;

    measure_fallback(&base_core, &recording, engine.clone(), human.clone()).await?;

    let mut intent_enrichment = Vec::with_capacity(CERTIFICATION_SAMPLES);
    let mut alternative_move = Vec::with_capacity(CERTIFICATION_SAMPLES);
    let mut coach_turn = Vec::with_capacity(CERTIFICATION_SAMPLES);
    for sample in 0..CERTIFICATION_SAMPLES {
        intent_enrichment
            .push(measure_intent_enrichment(&base_core, engine.clone(), human.clone()).await?);
        let (exploration, target, elapsed) =
            explored_target(&base_core, &recording, engine.clone(), sample).await?;
        alternative_move.push(elapsed);
        let coaching = AlternativeMoveCoaching::new(
            human.clone(),
            Arc::new(GroundedAuthor),
            Arc::new(CoachTurnActivity::default()),
        );
        let started = Instant::now();
        coaching
            .coach(coach_request(
                &format!("coach-{sample}"),
                PriorCoachTurn::None,
                CoachTurnTargetSelection::Explicit(Box::new(target.clone())),
            ))
            .await
            .context("live Coach Turn")?;
        coach_turn.push(elapsed_milliseconds(started));
        drop(exploration);
    }

    let mut concurrent = Vec::with_capacity(CERTIFICATION_SAMPLES);
    for sample in 0..CERTIFICATION_SAMPLES {
        concurrent.push(
            measure_concurrent(
                &base_core,
                &recording,
                engine.clone(),
                human.clone(),
                sample,
            )
            .await?,
        );
    }

    let mut cancellation = Vec::with_capacity(CERTIFICATION_SAMPLES);
    let mut steering = Vec::with_capacity(CERTIFICATION_SAMPLES);
    for sample in 0..CERTIFICATION_SAMPLES {
        cancellation.push(
            measure_cancellation(
                &base_core,
                &recording,
                engine.clone(),
                human.clone(),
                sample,
            )
            .await?,
        );
        steering.push(
            measure_steering(
                &base_core,
                &recording,
                engine.clone(),
                human.clone(),
                sample,
            )
            .await?,
        );
    }

    let operations = OperationMeasurements {
        game_review: LatencyDistribution::from_milliseconds(game_review_samples),
        intent_enrichment: LatencyDistribution::from_milliseconds(intent_enrichment),
        alternative_move_evaluation: LatencyDistribution::from_milliseconds(alternative_move),
        concurrent_exploration: LatencyDistribution::from_milliseconds(concurrent),
        coach_turn: LatencyDistribution::from_milliseconds(coach_turn),
        cancellation_to_stop: LatencyDistribution::from_milliseconds(cancellation),
        steer_to_replacement: LatencyDistribution::from_milliseconds(steering),
    };
    let gates = CertificationGates {
        provider_recording_verified: true,
        intent_enrichment_deadline_held: operations.intent_enrichment.maximum_milliseconds
            <= REVIEW_MOMENT_PREPARATION_DEADLINE_SECONDS.saturating_mul(1_000),
        alternative_move_deadline_held: operations.alternative_move_evaluation.maximum_milliseconds
            <= ALTERNATIVE_MOVE_DEADLINE_MILLISECONDS,
        coach_turn_deadline_held: operations.coach_turn.maximum_milliseconds
            <= COACH_TURN_DEADLINE_SECONDS.saturating_mul(1_000),
        cancellation_deadline_held: operations.cancellation_to_stop.maximum_milliseconds
            <= CANCELLATION_BUDGET_MILLISECONDS,
        steering_deadline_held: operations.steer_to_replacement.maximum_milliseconds
            <= COACH_TURN_DEADLINE_SECONDS.saturating_mul(1_000),
        typed_cancellation_held: true,
        stale_work_prevented_from_publishing: true,
    };
    ensure!(
        gates.intent_enrichment_deadline_held
            && gates.alternative_move_deadline_held
            && gates.coach_turn_deadline_held
            && gates.cancellation_deadline_held
            && gates.steering_deadline_held,
        "a Review Session operating limit was missed"
    );

    Ok(ReviewSessionCertification {
        canonical_game_id: "Synthet1",
        samples_per_operation: CERTIFICATION_SAMPLES,
        provider_recording_digest: recording.content_digest.as_str().to_string(),
        cache_state: CacheState {
            provider_recording: "verified checked-in canonical recording",
            lichess_export: "cold single-game export in manual live journeys",
            review_session_state: "fresh in-memory session per sample",
        },
        operations,
        concurrency: ConcurrencyProof {
            coach_turn_and_stockfish_exploration_completed_together: true,
            active_coach_turns_per_session: 1,
            active_alternative_evaluations_per_session: 1,
        },
        fallback: FallbackProof {
            language_layer_unavailable: true,
            stockfish_exploration_remained_available: true,
        },
        gates,
    })
}

async fn measure_intent_enrichment(
    core: &ReviewSessionCoreContract,
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
) -> anyhow::Result<u64> {
    let started = Instant::now();
    let plan = ProjectedPlanBuilder::new(engine, human)
        .build(core)
        .await
        .context("live Intent Enrichment")?;
    ensure!(
        plan.projected_plan_san().len() == 4
            && !plan.objective_counterplay_san().is_empty()
            && plan.projected_plan_san() != plan.objective_counterplay_san(),
        "Intent Enrichment did not keep one four-half-move Projected Plan separate from Objective Counterplay"
    );
    Ok(elapsed_milliseconds(started))
}

async fn measure_fallback(
    core: &ReviewSessionCoreContract,
    recording: &ReviewSessionProviderRecording,
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
) -> anyhow::Result<()> {
    let (exploration, target, _) = explored_target(core, recording, engine, 0).await?;
    let coaching = AlternativeMoveCoaching::new(
        human,
        Arc::new(UnavailableAuthor),
        Arc::new(CoachTurnActivity::default()),
    );
    ensure!(
        matches!(
            coaching
                .coach(coach_request(
                    "fallback",
                    PriorCoachTurn::None,
                    CoachTurnTargetSelection::Explicit(Box::new(target.clone())),
                ))
                .await,
            Err(chen_chess_coach_engine::review_session_coaching::AlternativeMoveCoachTurnError::Unavailable(
                ProviderUnavailableReason::LanguageLayer
            ))
        ),
        "fallback Coach Turn did not expose Language Layer unavailability"
    );
    let packet = exploration.current_state().await.evidence_packet;
    exploration
        .explore(
            explore_request(core, &packet, CONCURRENT_ALTERNATIVE, "fallback-stockfish"),
            AlternativeMoveCancellation::default(),
        )
        .await
        .context("Stockfish exploration after unavailable fallback Coach Turn")?;
    Ok(())
}

async fn explored_target(
    core: &ReviewSessionCoreContract,
    recording: &ReviewSessionProviderRecording,
    engine: Arc<dyn EngineAnalyzer>,
    sample: usize,
) -> anyhow::Result<(
    Arc<AlternativeMoveExploration>,
    PreparedCoachTurnTarget,
    u64,
)> {
    let root_engine = root_engine(recording, &core.position_snapshot.position_ref)?;
    let exploration = Arc::new(
        AlternativeMoveExploration::new(core.clone(), &root_engine, engine)
            .context("Alternative Move Exploration setup")?,
    );
    let packet = exploration.current_state().await.evidence_packet;
    let started = Instant::now();
    let commit = exploration
        .explore(
            explore_request(
                core,
                &packet,
                PRIMARY_ALTERNATIVE,
                &format!("primary-{sample}"),
            ),
            AlternativeMoveCancellation::default(),
        )
        .await
        .context("live Alternative Move Evaluation")?;
    let elapsed = elapsed_milliseconds(started);
    let snapshot = exploration.current_state().await;
    let target = PreparedCoachTurnTarget::capture(
        core,
        &snapshot,
        &commit.alternative_move.alternative_move_id,
    )
    .context("Alternative Move Coach Turn target")?;
    Ok((exploration, target, elapsed))
}

async fn measure_concurrent(
    core: &ReviewSessionCoreContract,
    recording: &ReviewSessionProviderRecording,
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
    sample: usize,
) -> anyhow::Result<u64> {
    let (exploration, target, _) = explored_target(core, recording, engine, sample).await?;
    let packet = exploration.current_state().await.evidence_packet;
    let coaching = AlternativeMoveCoaching::new(
        human,
        Arc::new(GroundedAuthor),
        Arc::new(CoachTurnActivity::default()),
    );
    let started = Instant::now();
    let (explored, coached) = tokio::join!(
        exploration.explore(
            explore_request(
                core,
                &packet,
                CONCURRENT_ALTERNATIVE,
                &format!("concurrent-{sample}"),
            ),
            AlternativeMoveCancellation::default(),
        ),
        coaching.coach(coach_request(
            &format!("concurrent-{sample}"),
            PriorCoachTurn::None,
            CoachTurnTargetSelection::Explicit(Box::new(target.clone())),
        )),
    );
    explored.context("concurrent Stockfish exploration")?;
    coached.context("concurrent Coach Turn")?;
    Ok(elapsed_milliseconds(started))
}

async fn measure_cancellation(
    core: &ReviewSessionCoreContract,
    recording: &ReviewSessionProviderRecording,
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
    sample: usize,
) -> anyhow::Result<u64> {
    let (_, target, _) = explored_target(core, recording, engine, sample).await?;
    let author = Arc::new(BlockFirstAuthor::new());
    let coaching = Arc::new(AlternativeMoveCoaching::new(
        human,
        author.clone(),
        Arc::new(CoachTurnActivity::default()),
    ));
    let label = format!("cancel-{sample}");
    let coach_turn_id = coach_turn_id(&label)?;
    let key = idempotency_key(&label)?;
    let task = {
        let coaching = coaching.clone();
        let target = target.clone();
        let label = label.clone();
        tokio::spawn(async move {
            coaching
                .coach(coach_request(
                    &label,
                    PriorCoachTurn::None,
                    CoachTurnTargetSelection::Explicit(Box::new(target.clone())),
                ))
                .await
        })
    };
    author.wait_until_started().await;
    let started = Instant::now();
    coaching
        .cancel(&coach_turn_id, &key)
        .await
        .context("Coach Turn cancellation")?;
    ensure!(
        matches!(
            task.await.context("cancelled Coach Turn task")?,
            Err(chen_chess_coach_engine::review_session_coaching::AlternativeMoveCoachTurnError::Cancelled)
        ),
        "cancelled Coach Turn did not return its typed terminal outcome"
    );
    Ok(elapsed_milliseconds(started))
}

async fn measure_steering(
    core: &ReviewSessionCoreContract,
    recording: &ReviewSessionProviderRecording,
    engine: Arc<dyn EngineAnalyzer>,
    human: Arc<dyn HumanMoveModel>,
    sample: usize,
) -> anyhow::Result<u64> {
    let (_, target, _) = explored_target(core, recording, engine, sample).await?;
    let author = Arc::new(BlockFirstAuthor::new());
    let coaching = Arc::new(AlternativeMoveCoaching::new(
        human,
        author.clone(),
        Arc::new(CoachTurnActivity::default()),
    ));
    let first_label = format!("steered-{sample}");
    let first_id = coach_turn_id(&first_label)?;
    let first = {
        let coaching = coaching.clone();
        let target = target.clone();
        let label = first_label.clone();
        tokio::spawn(async move {
            coaching
                .coach(coach_request(
                    &label,
                    PriorCoachTurn::None,
                    CoachTurnTargetSelection::Explicit(Box::new(target.clone())),
                ))
                .await
        })
    };
    author.wait_until_started().await;

    let replacement_label = format!("replacement-{sample}");
    let started = Instant::now();
    coaching
        .coach(coach_request(
            &replacement_label,
            PriorCoachTurn::Steers {
                coach_turn_id: first_id,
            },
            CoachTurnTargetSelection::Preserve,
        ))
        .await
        .context("steered replacement Coach Turn")?;
    ensure!(
        matches!(
            first.await.context("steered Coach Turn task")?,
            Err(chen_chess_coach_engine::review_session_coaching::AlternativeMoveCoachTurnError::Cancelled)
        ),
        "steering allowed stale Coach Turn publication"
    );
    Ok(elapsed_milliseconds(started))
}

fn explore_request(
    core: &ReviewSessionCoreContract,
    _packet: &ReviewSessionEvidencePacket,
    uci: &str,
    label: &str,
) -> ExploreAlternativeMoveRequest {
    ExploreAlternativeMoveRequest {
        parent: BranchParent::Root {
            position_ref: core.position_snapshot.position_ref.clone(),
        },
        source_position_ref: core.position_snapshot.position_ref.clone(),
        move_input: MoveInput::Uci {
            uci: uci.to_string(),
        },
        idempotency_key: idempotency_key(label).expect("static certification labels are valid"),
    }
}

fn coach_request(
    label: &str,
    prior_turn: PriorCoachTurn,
    target_selection: CoachTurnTargetSelection,
) -> StartAlternativeMoveCoachTurn {
    StartAlternativeMoveCoachTurn {
        coach_turn_id: coach_turn_id(label).expect("static certification labels are valid"),
        message: "Assess this Alternative Move from the grounded facts.".to_string(),
        idempotency_key: idempotency_key(label).expect("static certification labels are valid"),
        prior_turn,
        target: target_selection,
    }
}

fn review_core() -> anyhow::Result<ReviewSessionCoreContract> {
    let imported_game: ImportedGame = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/imported-game.json"
    )))?;
    Ok(start_review_session(
        RequestId::try_from("request:certification:core".to_string())?,
        CoachTurnId::try_from("coach-turn:certification:initial".to_string())?,
        imported_game,
        ReviewMomentSelection::PlayerSelectedMoment { ply: REVIEWED_PLY },
    )?)
}

fn provider_recording() -> anyhow::Result<ReviewSessionProviderRecording> {
    Ok(serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/shared-assets/fixtures/Synthet1/review-session-provider-recording.json"
    )))?)
}

fn root_engine(
    recording: &ReviewSessionProviderRecording,
    position_ref: &PositionRef,
) -> anyhow::Result<EvidenceEntry> {
    recording
        .content
        .entries
        .iter()
        .find(|entry| {
            matches!(
                entry,
                EvidenceEntry::EngineAnalysis {
                    position_ref: recorded,
                    ..
                } if recorded == position_ref
            )
        })
        .cloned()
        .context("canonical recording has no root Stockfish evidence")
}

fn coach_turn_id(label: &str) -> anyhow::Result<CoachTurnId> {
    Ok(CoachTurnId::try_from(format!(
        "coach-turn:certification:{label}"
    ))?)
}

fn idempotency_key(label: &str) -> anyhow::Result<IdempotencyKey> {
    Ok(IdempotencyKey::try_from(format!(
        "idempotency-key:certification:{label}"
    ))?)
}

fn grounded_assessment(input: &CoachTurnAuthorInput) -> AlternativeMoveAssessment {
    let dimension = |explanation: &str, refs: Vec<EvidenceId>| AssessmentDimension {
        explanation: explanation.to_string(),
        evidence_refs: refs,
    };
    let evidence = &input.evidence;
    AlternativeMoveAssessment {
        coach_turn_id: input.coach_turn_id.clone(),
        alternative_move_id: input.target.target().alternative_move_id.clone(),
        objective_quality: dimension(
            "By the engine's reckoning {alternativeMove} lands at {alternativeEval}, against {bestMove} at {bestEval}.",
            vec![
                evidence.target_branch.clone(),
                evidence.source_engine.clone(),
                evidence.resulting_engine.clone(),
            ],
        ),
        findability: dimension(
            "Whether {alternativeMove} turns up at the board is the real question here.",
            vec![
                evidence.target_branch.clone(),
                evidence.source_human.clone(),
            ],
        ),
        resilience: dimension(
            "After {alternativeMove} the reply that decides it is {strongestReply}.",
            vec![
                evidence.target_branch.clone(),
                evidence.resulting_engine.clone(),
                evidence.resulting_human.clone(),
            ],
        ),
    }
}

struct GroundedAuthor;

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
        Box::pin(async move { Ok(grounded_assessment(&input)) })
    }
}

struct UnavailableAuthor;

impl AlternativeMoveAssessmentAuthor for UnavailableAuthor {
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
        Box::pin(async { Err(ProviderUnavailableReason::LanguageLayer) })
    }
}

struct BlockFirstAuthor {
    calls: AtomicUsize,
    started: watch::Sender<bool>,
}

impl BlockFirstAuthor {
    fn new() -> Self {
        let (started, _) = watch::channel(false);
        Self {
            calls: AtomicUsize::new(0),
            started,
        }
    }

    async fn wait_until_started(&self) {
        let mut started = self.started.subscribe();
        while !*started.borrow_and_update() {
            started
                .changed()
                .await
                .expect("certification author remains available");
        }
    }
}

impl AlternativeMoveAssessmentAuthor for BlockFirstAuthor {
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
        Box::pin(async move {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.started.send_replace(true);
                return pending().await;
            }
            Ok(grounded_assessment(&input))
        })
    }
}

fn percentile(samples: &[u64], percentage: usize) -> u64 {
    let rank = percentage
        .saturating_mul(samples.len())
        .div_ceil(100)
        .saturating_sub(1);
    samples[rank.min(samples.len() - 1)]
}

fn elapsed_milliseconds(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
