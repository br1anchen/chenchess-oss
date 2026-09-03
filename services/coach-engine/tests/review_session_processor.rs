use std::sync::Arc;

use chen_chess_coach_engine::{
    game_import_store::{
        GameImportLookup, GameImportRecord, GameImportReference, GameImportReferenceLookup,
        GameImportStore, GameImportStoreFuture, InMemoryGameImportStore,
    },
    review_session_contract::*,
    review_session_processor::{ProcessorPrincipal, ReviewSessionProcessor},
};
use tokio::sync::{mpsc, Mutex};

use crate::processor_support::{
    self as support, processor, processor_with_failing_engine, processor_with_runtime_startup,
    CapturedLichess,
};

#[path = "review_session_processor/addressed_reads.rs"]
mod addressed_reads;
#[path = "review_session_processor/deletion.rs"]
mod deletion;
#[path = "review_session_processor/failures.rs"]
mod failures;
#[path = "review_session_processor/feedback.rs"]
mod feedback;
#[path = "review_session_processor/first_open_hosted_comment.rs"]
mod first_open_hosted_comment;
#[path = "review_session_processor/host_turn.rs"]
mod host_turn;
#[path = "review_session_processor/hosted_coach_turn.rs"]
mod hosted_coach_turn;
#[path = "review_session_processor/journey.rs"]
mod journey;
#[path = "review_session_processor/limits.rs"]
mod limits;
#[path = "review_session_processor/open_review_moment.rs"]
mod open_review_moment;

async fn import_and_start(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
) -> (GameImportId, ReviewSessionCoreContract) {
    import_and_start_labeled(processor, principal, "setup").await
}

async fn import_and_start_labeled(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    label: &str,
) -> (GameImportId, ReviewSessionCoreContract) {
    import_and_start_game(processor, principal, label, REVIEWED_GAME_URL).await
}

/// Imports one Lichess game and opens a Review Moment on it. A second URL is a
/// second Game Import, which is what separates the per-game scopes.
async fn import_and_start_game(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    label: &str,
    url: &str,
) -> (GameImportId, ReviewSessionCoreContract) {
    let imported = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            &format!("{label}-import"),
            import_command_for(url),
        ),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let started = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            &format!("{label}-start"),
            ReviewSessionCommand::StartReviewSession { game_import_id },
        ),
    )
    .await;
    if let Some(prepared) = started.iter().find_map(started_session) {
        return prepared;
    }
    let (game_import_id, selection) = started
        .iter()
        .find_map(started_admission)
        .expect("Review Session start admits at least one moment");
    let opened = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            &format!("{label}-open"),
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection,
                idempotency_key: idempotency_key(&format!("{label}-open")),
            },
        ),
    )
    .await;
    let core = opened
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewMomentOpened { review_moment, .. } => {
                    Some(review_moment.as_ref().clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("opening a pending Coach App moment prepares it");
    (game_import_id, core)
}

async fn coach_command(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: &ProcessorPrincipal,
    game_import_id: GameImportId,
    core: &ReviewSessionCoreContract,
    label: &str,
) -> ReviewSessionCommandEnvelope {
    let alternative =
        explore_root_labeled(processor, principal, &game_import_id, core, label).await;
    let inspection = inspect_alternative(
        processor,
        principal.clone(),
        &game_import_id,
        &core.review_moment.moment_id,
        &alternative.alternative_move_id,
        &format!("{label}-inspect"),
    )
    .await;
    let coach_turn_id = CoachTurnId::try_from(format!("coach-turn:processor:{label}")).unwrap();
    let mut context = inspection.context;
    context.coach_turn_id = coach_turn_id.clone();
    envelope_for(
        principal,
        label,
        ReviewSessionCommand::StartCoachTurn {
            game_import_id,
            review_moment_id: core.review_moment.moment_id.clone(),
            coach_turn_id,
            context: Box::new(context),
            message: "Please assess this branch.".to_string(),
            idempotency_key: idempotency_key(label),
            prior_turn: PriorCoachTurn::None,
        },
    )
}

async fn explore_root(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: &ProcessorPrincipal,
    game_import_id: &GameImportId,
    core: &ReviewSessionCoreContract,
) -> AlternativeMoveResult {
    explore_root_labeled(processor, principal, game_import_id, core, "setup-explore").await
}

async fn explore_root_labeled(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: &ProcessorPrincipal,
    game_import_id: &GameImportId,
    core: &ReviewSessionCoreContract,
    label: &str,
) -> AlternativeMoveResult {
    let events = submit(
        processor,
        principal.clone(),
        envelope_for(
            principal,
            &format!("{label}-explore"),
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: game_import_id.clone(),
                review_moment_id: core.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: core.position_snapshot.position_ref.clone(),
                },
                source_position_ref: core.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci {
                    uci: first_legal_uci(&core.position_snapshot),
                },
                idempotency_key: idempotency_key(&format!("{label}-explore")),
            },
        ),
    )
    .await;
    events.iter().find_map(explored_move).unwrap()
}

async fn submit(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    envelope: ReviewSessionCommandEnvelope,
) -> Vec<ReviewSessionEventEnvelope> {
    let mut receiver = processor.submit(principal, &serde_json::to_vec(&envelope).unwrap());
    collect(&mut receiver).await
}

async fn collect(
    receiver: &mut mpsc::UnboundedReceiver<ReviewSessionEventEnvelope>,
) -> Vec<ReviewSessionEventEnvelope> {
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    events
}

fn envelope(label: &str, command: ReviewSessionCommand) -> ReviewSessionCommandEnvelope {
    envelope_for(&ProcessorPrincipal::LocalCoach, label, command)
}

fn envelope_for(
    principal: &ProcessorPrincipal,
    label: &str,
    command: ReviewSessionCommand,
) -> ReviewSessionCommandEnvelope {
    ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from(format!("request:processor:{label}")).unwrap(),
        operation_id: OperationId::try_from(format!("operation:processor:{label}")).unwrap(),
        surface: match principal {
            ProcessorPrincipal::LocalCoach => DeliverySurface::CoachSkill,
            ProcessorPrincipal::Player(_) => DeliverySurface::CoachApp,
        },
        command,
    }
}

const REVIEWED_GAME_URL: &str = "https://lichess.org/Synthet1Demo/black";

fn import_command() -> ReviewSessionCommand {
    import_command_for(REVIEWED_GAME_URL)
}

fn import_command_for(url: &str) -> ReviewSessionCommand {
    ReviewSessionCommand::ImportGame {
        source: GameInputSource::LichessUrl {
            url: url.to_string(),
        },
        review_side: RequestedReviewSide::FromQualifiedUrl,
        elo_profile: RequestedEloProfile::FromImportedMetadata,
    }
}

fn imported_game(event: &ReviewSessionEventEnvelope) -> Option<GameImportId> {
    match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::GameImported { game_import_id, .. } => {
                Some(game_import_id.clone())
            }
            _ => None,
        },
        _ => None,
    }
}

fn imported_timing(event: &ReviewSessionEventEnvelope) -> Option<&GameImportTiming> {
    match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::GameImported {
                timing: Some(timing),
                ..
            } => Some(timing),
            _ => None,
        },
        _ => None,
    }
}

fn started_session(
    event: &ReviewSessionEventEnvelope,
) -> Option<(GameImportId, ReviewSessionCoreContract)> {
    match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewSessionStarted {
                game_import_id,
                review_moments,
                ..
            } => review_moments.first().and_then(|moment| {
                moment
                    .prepared_core()
                    .cloned()
                    .map(|core| (game_import_id.clone(), core))
            }),
            _ => None,
        },
        _ => None,
    }
}

fn started_admission(
    event: &ReviewSessionEventEnvelope,
) -> Option<(GameImportId, ReviewMomentSelection)> {
    match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewSessionStarted {
                game_import_id,
                review_moments,
                ..
            } => review_moments.first().map(|moment| {
                (
                    game_import_id.clone(),
                    moment.review_moment.selection.clone(),
                )
            }),
            _ => None,
        },
        _ => None,
    }
}

fn explored_move(event: &ReviewSessionEventEnvelope) -> Option<AlternativeMoveResult> {
    match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::AlternativeMoveEvaluated { alternative_move } => {
                Some(alternative_move.as_ref().clone())
            }
            _ => None,
        },
        _ => None,
    }
}

fn position_inspection(event: &ReviewSessionEventEnvelope) -> Option<PositionInspection> {
    match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::PositionInspected { inspection } => {
                Some(inspection.as_ref().clone())
            }
            _ => None,
        },
        _ => None,
    }
}

fn coach_turn_preparation(event: &ReviewSessionEventEnvelope) -> Option<CoachTurnFacts> {
    match &event.event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::CoachTurnPrepared { facts } => Some(facts.as_ref().clone()),
            _ => None,
        },
        _ => None,
    }
}

async fn inspect_alternative(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    game_import_id: &GameImportId,
    review_moment_id: &CriticalMomentId,
    alternative_move_id: &AlternativeMoveId,
    label: &str,
) -> PositionInspection {
    let events = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            label,
            ReviewSessionCommand::InspectPosition {
                game_import_id: game_import_id.clone(),
                review_moment_id: review_moment_id.clone(),
                target: PositionInspectionTarget::AlternativeMove {
                    alternative_move_id: alternative_move_id.clone(),
                },
            },
        ),
    )
    .await;
    assert_event_stream(&events, OperationKind::PositionInspection);
    events.iter().find_map(position_inspection).unwrap()
}

fn alternative_assessment(facts: &CoachTurnFacts) -> AlternativeMoveAssessment {
    let dimension = |explanation: &str, refs: Vec<EvidenceId>| AssessmentDimension {
        explanation: explanation.to_string(),
        evidence_refs: refs,
    };
    let evidence = &facts.evidence;
    AlternativeMoveAssessment {
        coach_turn_id: facts.coach_turn_id.clone(),
        alternative_move_id: facts.alternative_move.alternative_move_id.clone(),
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

fn idempotency_key(label: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(format!("idempotency-key:processor:{label}")).unwrap()
}

fn first_legal_uci(position: &PositionSnapshot) -> String {
    use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};

    let chess: Chess = Fen::from_ascii(position.fen.as_bytes())
        .expect("test position should be valid FEN")
        .into_position(CastlingMode::Standard)
        .expect("test position should be legal");
    let chess_move = chess
        .legal_moves()
        .into_iter()
        .next()
        .expect("test position should have a legal move");
    UciMove::from_move(&chess_move, CastlingMode::Standard).to_string()
}

fn assert_event_stream(events: &[ReviewSessionEventEnvelope], operation: OperationKind) {
    assert!(
        matches!(
            events.first().map(|event| &event.event),
            Some(ReviewSessionEvent::Accepted {
                operation: accepted,
                ..
            }) if *accepted == operation
        ),
        "expected accepted {operation:?}, got {events:#?}"
    );
    assert!(
        matches!(
            events.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Completed { .. })
        ),
        "{events:#?}"
    );
    assert!(events
        .iter()
        .enumerate()
        .all(|(sequence, event)| event.sequence == sequence as u32));
}
