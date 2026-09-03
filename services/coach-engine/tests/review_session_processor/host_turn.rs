use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use chen_chess_coach_engine::{
    critical_moment_comment::HostedCommentRuntime,
    evaluation_fingerprint::{CaptureOutcome, EvaluationEnvironment, HostTurnStepCapability},
    language_layer_ledger::{
        next_request_id, AttemptErrorClass, BudgetDecision, DenialReason,
        LanguageLayerAdmissionConfig, LanguageLayerLedger, LanguageLayerOperationalRecord,
        MemoryLanguageLayerLedger, ProviderConcurrency, GLOBAL_CEILING_MICROS,
    },
    language_layer_provider::LanguageLayerProvider,
    pin_record::{compiled_pin_record, fingerprint_from_pin},
    review_session_contract::*,
    review_session_processor::{ProcessorPrincipal, ReviewSessionProcessor},
};
use chrono::Utc;
use serde_json::json;
use tokio::sync::{Mutex, Notify};

use super::*;

#[tokio::test]
async fn one_step_answer_completes() {
    let (processor, ledger) =
        bound_host_processor(scripted(vec![answer_step("This move lost the exchange.")])).await;
    let events = start_host_turn(&processor, "one-step", "Why was this move a mistake?").await;
    assert_completed_answer(&events, "This move lost the exchange.");
    let records = ledger.records().await.unwrap();
    assert_eq!(records.last().map(|record| record.steps.len()), Some(1));
    assert_eq!(
        records.last().and_then(|record| record.steps[0].capability),
        None
    );
}

#[tokio::test]
async fn call_then_answer_cites_the_returned_result() {
    let other_san = {
        let (processor, _) = bound_host_processor(scripted(vec![])).await;
        let (_, _, other) = host_session_with_two_moments(&processor, "cite-probe").await;
        played_san(&other)
    };
    let answer = format!("{other_san} was the other moment.");
    let (processor, ledger) = bound_host_processor(scripted(vec![
        call_step("listMoments"),
        cited_answer(&answer, "call:listMoments"),
    ]))
    .await;
    let events =
        start_host_turn_on_first_of_two(&processor, "call-then-answer", "Which moments matter?")
            .await;
    assert!(events.iter().any(|event| matches!(
        &event.event,
        ReviewSessionEvent::Progress {
            stage: OperationProgress::HostTurn {
                label: HostTurnStepLabel::Writing
            }
        }
    )));
    assert!(events.iter().any(|event| matches!(
        &event.event,
        ReviewSessionEvent::Progress {
            stage: OperationProgress::HostTurn {
                label: HostTurnStepLabel::LookingAtAnotherMoment
            }
        }
    )));
    assert_completed_answer(&events, &answer);
    let records = ledger.records().await.unwrap();
    let steps = &records.last().expect("HostTurn settles one record").steps;
    assert_eq!(steps.len(), 2);
    assert_eq!(
        steps[0].capability,
        Some(HostTurnStepCapability::ListMoments)
    );
    assert_eq!(steps[1].capability, None);
}

#[tokio::test]
async fn dirty_prior_message_is_unavailable() {
    let (processor, _) = bound_host_processor(scripted(vec![answer_step("should not run")])).await;
    let events = start_host_turn_with_priors(
        &processor,
        "dirty-prior-message",
        "Why was this move a mistake?",
        vec![HostTurnPriorTurn {
            message: "has\u{0007}bell".to_string(),
            answer: "The knight was hanging.".to_string(),
        }],
    )
    .await;
    assert_unavailable(&events);
}

#[tokio::test]
async fn dirty_prior_answer_is_unavailable() {
    let (processor, _) = bound_host_processor(scripted(vec![answer_step("should not run")])).await;
    let events = start_host_turn_with_priors(
        &processor,
        "dirty-prior-answer",
        "Why was this move a mistake?",
        vec![HostTurnPriorTurn {
            message: "Why was this move a mistake?".to_string(),
            answer: "has\u{0007}bell".to_string(),
        }],
    )
    .await;
    assert_unavailable(&events);
}

#[tokio::test]
async fn refuse_completes_as_host_turn_refused() {
    let (processor, _) =
        bound_host_processor(scripted(vec![refuse_step("notAboutThisReview")])).await;
    let events = start_host_turn(
        &processor,
        "refuse",
        "What is the Sicilian Defense in general?",
    )
    .await;
    assert_refused(&events, HostTurnRefusalReason::NotAboutThisReview);
}

#[tokio::test]
async fn idempotent_replay_does_not_resettle() {
    let (processor, ledger) = bound_host_processor(scripted(vec![
        answer_step("This move lost the exchange."),
        answer_step("should not be used"),
    ]))
    .await;
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:host-turn:idempotent".to_string()).unwrap(),
    );
    let (game_import_id, _) = import_and_start(&processor, principal.clone()).await;
    let first = submit_host_turn(
        &processor,
        principal.clone(),
        "idempotent",
        game_import_id.clone(),
        "Why was this move a mistake?",
    )
    .await;
    let second = submit_host_turn(
        &processor,
        principal,
        "idempotent",
        game_import_id,
        "Why was this move a mistake?",
    )
    .await;
    assert_completed_answer(&first, "This move lost the exchange.");
    assert_completed_answer(&second, "This move lost the exchange.");
    let records = ledger.records().await.unwrap();
    let hosts = records
        .iter()
        .filter(|record| record.player_id.as_str() == "player:host-turn:idempotent")
        .count();
    assert_eq!(
        hosts, 1,
        "replay must not settle a second record: {records:?}"
    );
}

#[tokio::test]
async fn uncited_literal_from_another_moment_is_rejected() {
    let other_san = {
        let (processor, _) = bound_host_processor(scripted(vec![])).await;
        let (_, _, other) = host_session_with_two_moments(&processor, "uncited-probe").await;
        played_san(&other)
    };
    let ungrounded = format!("{other_san} was the other moment.");
    let (processor, _) = bound_host_processor(scripted(vec![
        answer_step(&ungrounded),
        answer_step(&ungrounded),
    ]))
    .await;
    let events =
        start_host_turn_on_first_of_two(&processor, "uncited-literal", "Which moments matter?")
            .await;
    assert_unavailable(&events);
}

#[tokio::test]
async fn envelope_denial_spends_nothing() {
    let (processor, ledger) =
        bound_host_processor(scripted(vec![answer_step("This move lost the exchange.")])).await;
    seed_global_ceiling(&ledger).await;
    let events = start_host_turn(
        &processor,
        "envelope-denial",
        "Why was this move a mistake?",
    )
    .await;
    assert_unavailable(&events);
    let records = ledger.records().await.unwrap();
    assert!(
        records
            .iter()
            .filter(|record| record.player_id.as_str() != "player:host-turn:ceiling-seed")
            .all(|record| record.cost_micros == 0),
        "envelope denial spends nothing: {records:?}"
    );
    assert!(records.iter().any(|record| {
        record.budget_decision == BudgetDecision::Denied
            && record.denial_reason == Some(DenialReason::GlobalCeiling)
    }));
}

#[tokio::test]
async fn grounding_rejection_retries_then_publishes() {
    let (processor, _) = bound_host_processor(scripted(vec![
        answer_step("See https://example.com for the line."),
        answer_step("This move lost the exchange."),
    ]))
    .await;
    let events = start_host_turn(
        &processor,
        "grounding-retry",
        "Why was this move a mistake?",
    )
    .await;
    assert_completed_answer(&events, "This move lost the exchange.");
}

#[tokio::test]
async fn double_grounding_rejection_is_unavailable() {
    let (processor, ledger) = bound_host_processor(scripted(vec![
        answer_step("See https://example.com for the line."),
        answer_step("Also visit https://evil.example."),
    ]))
    .await;
    let events = start_host_turn(&processor, "double-reject", "Why was this move a mistake?").await;
    assert_unavailable(&events);
    let records = ledger.records().await.unwrap();
    assert_eq!(
        records.last().and_then(|record| record.capture_outcome),
        Some(CaptureOutcome::Rejected),
        "D5-exhausted grounding is Rejected: {records:?}"
    );
}

#[tokio::test]
async fn deadline_makes_the_turn_unavailable() {
    let mut config = LanguageLayerAdmissionConfig::conservative_defaults();
    config.host_turn_authoring_deadline = Duration::from_millis(40);
    let (processor, _) = bound_host_processor_with(
        delayed_script(
            vec![answer_step("This move lost the exchange.")],
            Duration::from_millis(200),
        ),
        config,
    )
    .await;
    let events = start_host_turn(&processor, "deadline", "Why was this move a mistake?").await;
    assert_unavailable(&events);
}

#[tokio::test]
async fn cancellation_between_steps_discards_the_turn() {
    let (processor, ledger) = bound_host_processor(delayed_after_first(
        vec![
            call_step("listMoments"),
            answer_step("This move lost the exchange."),
        ],
        Duration::from_millis(200),
    ))
    .await;
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:host-turn:cancel".to_string()).unwrap(),
    );
    let (game_import_id, _) = import_and_start(&processor, principal.clone()).await;
    let mut envelope = host_envelope(
        &principal,
        "cancel-between",
        game_import_id.clone(),
        "Which moments matter?",
    );
    envelope.surface = DeliverySurface::Web;
    let mut receiver = processor.submit(principal.clone(), &serde_json::to_vec(&envelope).unwrap());
    tokio::time::sleep(Duration::from_millis(80)).await;
    let cancel = ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from("request:processor:cancel-between-cancel".to_string())
            .unwrap(),
        operation_id: envelope.operation_id.clone(),
        surface: DeliverySurface::Web,
        command: ReviewSessionCommand::CancelOperation {
            game_import_id,
            operation_id: envelope.operation_id.clone(),
            idempotency_key: idempotency_key("cancel-between"),
        },
    };
    let cancelled = submit(&processor, principal, cancel).await;
    assert!(cancelled.iter().any(|event| matches!(
        &event.event,
        ReviewSessionEvent::Cancelled {
            operation: OperationKind::Cancellation
        }
    )));
    let events = collect(&mut receiver).await;
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            ReviewSessionEvent::Cancelled {
                operation: OperationKind::HostTurn
            }
        )),
        "cancel between steps emits Cancelled, not Language Layer unavailable: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(&event.event, ReviewSessionEvent::Unavailable { .. })),
        "cancel must not collapse into Language Layer unavailable: {events:?}"
    );
    let records = ledger.records().await.unwrap();
    assert!(
        records.iter().any(|record| record.cost_micros > 0
            && record.error_class == Some(AttemptErrorClass::Cancelled)),
        "cancel between steps records Cancelled: {records:?}"
    );
}

#[tokio::test]
async fn host_turn_uses_the_most_recently_opened_moment() {
    let (first_ply, second_ply) = {
        let (processor, _) = bound_host_processor(scripted(vec![])).await;
        let (_, first, second) = host_session_with_two_moments(&processor, "focus-probe").await;
        (first.review_moment.ply, second.review_moment.ply)
    };

    let (stale, _) = bound_host_processor(scripted(vec![
        focus_answer("This move lost the exchange.", first_ply),
        focus_answer("This move lost the exchange.", first_ply),
    ]))
    .await;
    let stale_events =
        start_host_turn_after_opening_second(&stale, "focus-stale", "Why was this move a mistake?")
            .await;
    assert_unavailable(&stale_events);

    let (fresh, _) = bound_host_processor(scripted(vec![focus_answer(
        "This move lost the exchange.",
        second_ply,
    )]))
    .await;
    let fresh_events =
        start_host_turn_after_opening_second(&fresh, "focus-open", "Why was this move a mistake?")
            .await;
    assert_completed_answer(&fresh_events, "This move lost the exchange.");
}

#[tokio::test]
async fn pin_mismatch_on_any_step_is_recorded_and_still_publishes() {
    let (processor, ledger) = bound_host_processor(Script {
        bodies: vec![answer_step("This move lost the exchange.")],
        delay: Duration::ZERO,
        delay_from: 0,
        mismatch_pin: true,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: Vec::new(),
        empty_completion: false,
        hold_first: None,
    })
    .await;
    let events = start_host_turn(&processor, "pin-mismatch", "Why was this move a mistake?").await;
    assert_completed_answer(&events, "This move lost the exchange.");
    let records = ledger.records().await.unwrap();
    assert!(
        records.iter().any(|record| {
            record.pin_verification
                == chen_chess_coach_engine::evaluation_fingerprint::PinVerificationVerdict::Failed
                && record.pin_cause
                    == Some(
                        chen_chess_coach_engine::pin_verification::PinVerificationCause::Mismatched,
                    )
                && record.error_class.is_none()
                && record.cost_micros > 0
        }),
        "pin mismatch must settle as telemetry on a published turn: {records:?}"
    );
}

#[tokio::test]
async fn first_step_pin_mismatch_is_latched_when_a_later_step_passes() {
    let (processor, ledger) = bound_host_processor(Script {
        bodies: vec![
            call_step("listMoments"),
            cited_answer("This move lost the exchange.", "call:listMoments"),
        ],
        delay: Duration::ZERO,
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 1,
        hang_after: None,
        faults: Vec::new(),
        empty_completion: false,
        hold_first: None,
    })
    .await;
    let events = start_host_turn(
        &processor,
        "latched-pin-mismatch",
        "Why was this move a mistake?",
    )
    .await;
    assert_completed_answer(&events, "This move lost the exchange.");
    let records = ledger.records().await.unwrap();
    let host = records
        .iter()
        .find(|record| record.steps.len() >= 2)
        .expect("two-step HostTurn record");
    assert_eq!(
        host.pin_verification,
        chen_chess_coach_engine::evaluation_fingerprint::PinVerificationVerdict::Failed
    );
    assert_eq!(
        host.pin_cause,
        Some(chen_chess_coach_engine::pin_verification::PinVerificationCause::Mismatched)
    );
    assert_eq!(host.error_class, None);
}

#[tokio::test]
async fn empty_completion_stays_unavailable() {
    let (processor, _) = bound_host_processor(Script {
        bodies: Vec::new(),
        delay: Duration::ZERO,
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: Vec::new(),
        empty_completion: true,
        hold_first: None,
    })
    .await;
    let events = start_host_turn(
        &processor,
        "empty-completion",
        "Why was this move a mistake?",
    )
    .await;
    assert_unavailable(&events);
}

#[tokio::test]
async fn last_open_intent_grounds_host_turn_when_first_open_authoring_completes_last() {
    let later_san = {
        let (processor, _) = bound_host_processor(scripted(vec![])).await;
        let (_, _, other) = host_session_with_two_moments(&processor, "overlap-probe").await;
        played_san(&other)
    };
    let answer = format!("{later_san} decided this moment.");
    let hold = Arc::new(Notify::new());
    let (processor, _, hits) = bound_host_processor_hits(Script {
        bodies: vec![answer_step(&answer), answer_step(&answer)],
        delay: Duration::ZERO,
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: Vec::new(),
        empty_completion: false,
        hold_first: Some(hold.clone()),
    })
    .await;
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:host-turn:overlap-open".to_string()).unwrap(),
    );
    let (game_import_id, first, second_selection) =
        prepared_two_moment_selections(&processor, principal.clone(), "overlap-open").await;
    let first_selection = first.review_moment.selection.clone();
    let first_open = tokio::spawn({
        let processor = Arc::clone(&processor);
        let principal = principal.clone();
        let game_import_id = game_import_id.clone();
        async move {
            let mut envelope = envelope_for(
                &principal,
                "overlap-open-first",
                ReviewSessionCommand::OpenReviewMoment {
                    game_import_id,
                    selection: first_selection,
                    idempotency_key: idempotency_key("overlap-open-first"),
                },
            );
            envelope.surface = DeliverySurface::Web;
            submit(&processor, principal, envelope).await
        }
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while hits.load(Ordering::SeqCst) < 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first-open authoring must reach the Language Layer");
    let mut second_envelope = envelope_for(
        &principal,
        "overlap-open-second",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection: second_selection,
            idempotency_key: idempotency_key("overlap-open-second"),
        },
    );
    second_envelope.surface = DeliverySurface::Web;
    let second = submit(&processor, principal.clone(), second_envelope).await;
    assert!(
        second.iter().any(|event| matches!(
            &event.event,
            ReviewSessionEvent::Completed { result }
                if matches!(result.as_ref(), OperationCompletion::ReviewMomentOpened { .. })
        )),
        "the later open must complete while first-open authoring is still held: {second:?}"
    );
    hold.notify_one();
    let first_opened = first_open.await.expect("first-open task joins");
    assert!(
        first_opened.iter().any(|event| matches!(
            &event.event,
            ReviewSessionEvent::Completed { result }
                if matches!(result.as_ref(), OperationCompletion::ReviewMomentOpened { .. })
        )),
        "the stale first-open still settles: {first_opened:?}"
    );
    let events = submit_host_turn(
        &processor,
        principal,
        "overlap-open",
        game_import_id,
        "What happened here?",
    )
    .await;
    assert_completed_answer(&events, &answer);
}

#[tokio::test]
async fn host_turn_without_an_open_review_moment_is_unavailable() {
    let (processor, ledger) =
        bound_host_processor(scripted(vec![answer_step("This move lost the exchange.")])).await;
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:host-turn:no-open".to_string()).unwrap(),
    );
    let imported = submit(
        &processor,
        principal.clone(),
        envelope_for(&principal, "no-open-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let started = submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "no-open-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
    )
    .await;
    assert!(
        started.iter().any(|event| matches!(
            &event.event,
            ReviewSessionEvent::Completed { result }
                if matches!(result.as_ref(), OperationCompletion::ReviewSessionStarted { .. })
        )),
        "session starts without opening a moment: {started:?}"
    );
    let events = submit_host_turn(
        &processor,
        principal,
        "no-open",
        game_import_id,
        "Why was this move a mistake?",
    )
    .await;
    assert_unavailable(&events);
    let records = ledger.records().await.unwrap();
    assert!(
        records
            .iter()
            .all(|record| record.player_id.as_str() != "player:host-turn:no-open"),
        "no open moment spends nothing: {records:?}"
    );
}

#[tokio::test]
async fn one_transport_retry_per_turn() {
    let (processor, ledger) = bound_host_processor(Script {
        bodies: vec![
            call_step("listMoments"),
            answer_step("This move lost the exchange."),
        ],
        delay: Duration::ZERO,
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: vec![Some(500), None, Some(500)],
        empty_completion: false,
        hold_first: None,
    })
    .await;
    let events = start_host_turn(&processor, "one-transport", "Which moments matter?").await;
    assert_unavailable(&events);
    let records = ledger.records().await.unwrap();
    let steps = &records.last().expect("HostTurn settles one record").steps;
    assert_eq!(
        steps.len(),
        3,
        "one transport retry, a completed call, then a second fault with no retry: {steps:?}"
    );
}

#[tokio::test]
async fn transport_error_retries_the_step_once() {
    let (processor, ledger) = bound_host_processor(Script {
        bodies: vec![answer_step("This move lost the exchange.")],
        delay: Duration::ZERO,
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: vec![Some(500)],
        empty_completion: false,
        hold_first: None,
    })
    .await;
    let events = start_host_turn(
        &processor,
        "transport-retry",
        "Why was this move a mistake?",
    )
    .await;
    assert_completed_answer(&events, "This move lost the exchange.");
    let records = ledger.records().await.unwrap();
    let steps = &records.last().expect("HostTurn settles one record").steps;
    assert_eq!(steps.len(), 2);
}

#[tokio::test]
async fn rate_limit_honours_provider_cooldown() {
    let (processor, ledger) = bound_host_processor(Script {
        bodies: vec![answer_step("This move lost the exchange.")],
        delay: Duration::ZERO,
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: vec![Some(429)],
        empty_completion: false,
        hold_first: None,
    })
    .await;
    let events = start_host_turn(&processor, "cooldown", "Why was this move a mistake?").await;
    assert_unavailable(&events);
    let next = start_host_turn(&processor, "cooldown-next", "Why was this move a mistake?").await;
    assert_unavailable(&next);
    let records = ledger.records().await.unwrap();
    assert!(
        records.iter().any(|record| {
            record.budget_decision == BudgetDecision::ProviderCooldown
                && record.provider_cooldown.is_some()
                && record.cost_micros == 0
        }),
        "cooldown envelope denial records the honoured wait: {records:?}"
    );
}

#[tokio::test]
async fn grounding_retry_rejects_a_follow_up_call() {
    let (processor, ledger) = bound_host_processor(scripted(vec![
        answer_step("See https://example.com for the line."),
        call_step("listMoments"),
    ]))
    .await;
    let events =
        start_host_turn(&processor, "d5-answer-only", "Why was this move a mistake?").await;
    assert_unavailable(&events);
    assert!(
        events.iter().all(|event| !matches!(
            &event.event,
            ReviewSessionEvent::Progress {
                stage: OperationProgress::HostTurn {
                    label: HostTurnStepLabel::LookingAtAnotherMoment
                }
            }
        )),
        "D5 retry must not dispatch a capability: {events:?}"
    );
    let records = ledger.records().await.unwrap();
    assert_eq!(
        records.last().map(|record| record.steps.len()),
        Some(2),
        "the rejected call still occupies a step: {records:?}"
    );
}

#[tokio::test]
async fn transport_retry_does_not_consume_the_grounding_retry() {
    let (processor, ledger) = bound_host_processor(Script {
        bodies: vec![
            call_step("listMoments"),
            answer_step("See https://example.com for the line."),
            answer_step("This move lost the exchange."),
        ],
        delay: Duration::ZERO,
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: vec![Some(500)],
        empty_completion: false,
        hold_first: None,
    })
    .await;
    let events = start_host_turn(&processor, "transport-then-d5", "Which moments matter?").await;
    assert_completed_answer(&events, "This move lost the exchange.");
    let records = ledger.records().await.unwrap();
    let steps = &records.last().expect("HostTurn settles one record").steps;
    assert_eq!(
        steps.len(),
        4,
        "transport fault plus call, rejected answer, and D5: {steps:?}"
    );
}

#[tokio::test]
async fn mid_turn_admission_denial_still_bills_paid_steps() {
    let (processor, ledger, hits) = bound_host_processor_hits(Script {
        bodies: vec![
            call_step("listMoments"),
            answer_step("This move lost the exchange."),
        ],
        delay: Duration::from_millis(200),
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: Vec::new(),
        empty_completion: false,
        hold_first: None,
    })
    .await;
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:host-turn:mid-deny".to_string()).unwrap(),
    );
    let (game_import_id, _) = import_and_start(&processor, principal.clone()).await;
    let mut envelope = host_envelope(
        &principal,
        "mid-deny",
        game_import_id,
        "Which moments matter?",
    );
    envelope.surface = DeliverySurface::Web;
    let mut receiver = processor.submit(principal, &serde_json::to_vec(&envelope).unwrap());
    let started = std::time::Instant::now();
    while hits.load(Ordering::SeqCst) == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "step 1 never reached the provider"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    seed_global_ceiling(&ledger).await;
    let events = collect(&mut receiver).await;
    assert_unavailable(&events);
    let as_of = Utc::now();
    let global = ledger.global_calendar_month(as_of).await.unwrap();
    let records = ledger.records().await.unwrap();
    let host = records
        .iter()
        .find(|record| record.player_id.as_str() == "player:host-turn:mid-deny")
        .expect("HostTurn settled");
    assert!(host.cost_micros > 0, "step 1 billed: {host:?}");
    assert_eq!(host.budget_decision, BudgetDecision::Admitted);
    assert_eq!(host.denial_reason, Some(DenialReason::GlobalCeiling));
    assert_eq!(
        global,
        GLOBAL_CEILING_MICROS.saturating_add(host.cost_micros),
        "paid HostTurn spend accrues on the global ceiling: {records:?}"
    );
}

#[tokio::test]
async fn capability_error_lets_the_model_answer() {
    let (processor, _) = bound_host_processor(scripted(vec![
        evaluate_line_step(&["not-a-move"]),
        answer_step("This move lost the exchange."),
    ]))
    .await;
    let events =
        start_host_turn(&processor, "capability-error", "What if I had played that?").await;
    assert_completed_answer(&events, "This move lost the exchange.");
}

#[tokio::test]
async fn host_turn_answers_may_name_the_on_screen_alternative_move() {
    let (uci, move_san) = {
        let (probe, _) = bound_host_processor(scripted(vec![])).await;
        let principal = ProcessorPrincipal::Player(
            PlayerId::try_from("player:host-turn:branch-probe".to_string()).unwrap(),
        );
        let (_, core) = import_and_start_labeled(&probe, principal, "branch-probe").await;
        let played = played_san(&core);
        legal_alternatives(&core.position_snapshot)
            .into_iter()
            .find(|(_, san)| san != &played)
            .expect("the fixture position has a legal Alternative Move")
    };
    let answer = format!("{move_san} looks stronger than the played move.");
    let (processor, _) = bound_host_processor(scripted(vec![answer_step(&answer)])).await;
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:host-turn:on-screen-branch".to_string()).unwrap(),
    );
    let (game_import_id, core) =
        import_and_start_labeled(&processor, principal.clone(), "on-screen-branch").await;
    let _explored = explore_uci(
        &processor,
        &principal,
        &game_import_id,
        &core,
        "on-screen-branch",
        &uci,
    )
    .await;
    let events = submit_host_turn(
        &processor,
        principal,
        "on-screen-branch",
        game_import_id,
        "What if I had played that instead?",
    )
    .await;
    assert_completed_answer(&events, &answer);
}

async fn start_host_turn(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    label: &str,
    message: &str,
) -> Vec<ReviewSessionEventEnvelope> {
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from(format!("player:host-turn:{label}")).unwrap(),
    );
    let (game_import_id, _) = import_and_start(processor, principal.clone()).await;
    submit_host_turn(processor, principal, label, game_import_id, message).await
}

async fn start_host_turn_on_first_of_two(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    label: &str,
    message: &str,
) -> Vec<ReviewSessionEventEnvelope> {
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from(format!("player:host-turn:{label}")).unwrap(),
    );
    let (game_import_id, _, _) =
        host_session_with_two_moments_for(processor, principal.clone(), label).await;
    submit_host_turn(processor, principal, label, game_import_id, message).await
}

async fn start_host_turn_after_opening_second(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    label: &str,
    message: &str,
) -> Vec<ReviewSessionEventEnvelope> {
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from(format!("player:host-turn:{label}")).unwrap(),
    );
    let (game_import_id, first, second) =
        host_session_with_two_moments_for(processor, principal.clone(), label).await;
    open_review_moment(
        processor,
        principal.clone(),
        &game_import_id,
        &format!("{label}-reopen-second"),
        second.review_moment.selection.clone(),
    )
    .await;
    let _ = first;
    submit_host_turn(processor, principal, label, game_import_id, message).await
}

async fn submit_host_turn(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    label: &str,
    game_import_id: GameImportId,
    message: &str,
) -> Vec<ReviewSessionEventEnvelope> {
    submit_host_turn_with_priors(
        processor,
        principal,
        label,
        game_import_id,
        message,
        Vec::new(),
    )
    .await
}

async fn start_host_turn_with_priors(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    label: &str,
    message: &str,
    prior_turns: Vec<HostTurnPriorTurn>,
) -> Vec<ReviewSessionEventEnvelope> {
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from(format!("player:host-turn:{label}")).unwrap(),
    );
    let (game_import_id, _) = import_and_start(processor, principal.clone()).await;
    submit_host_turn_with_priors(
        processor,
        principal,
        label,
        game_import_id,
        message,
        prior_turns,
    )
    .await
}

async fn submit_host_turn_with_priors(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    label: &str,
    game_import_id: GameImportId,
    message: &str,
    prior_turns: Vec<HostTurnPriorTurn>,
) -> Vec<ReviewSessionEventEnvelope> {
    let mut envelope =
        host_envelope_with_priors(&principal, label, game_import_id, message, prior_turns);
    envelope.surface = DeliverySurface::Web;
    submit(processor, principal, envelope).await
}

async fn host_session_with_two_moments(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    label: &str,
) -> (
    GameImportId,
    ReviewSessionCoreContract,
    ReviewSessionCoreContract,
) {
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from(format!("player:host-turn:{label}")).unwrap(),
    );
    host_session_with_two_moments_for(processor, principal, label).await
}

async fn prepared_two_moment_selections(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    label: &str,
) -> (
    GameImportId,
    ReviewSessionCoreContract,
    ReviewMomentSelection,
) {
    let (game_import_id, first) =
        import_and_start_labeled(processor, principal.clone(), label).await;
    let started = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            &format!("{label}-moments"),
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
    )
    .await;
    let second_selection = started
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewSessionStarted { review_moments, .. } => review_moments
                    .iter()
                    .find(|moment| moment.review_moment.ply != first.review_moment.ply)
                    .map(|moment| moment.review_moment.selection.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("the canonical fixture has a second Automatic Review Moment");
    (game_import_id, first, second_selection)
}

async fn host_session_with_two_moments_for(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    label: &str,
) -> (
    GameImportId,
    ReviewSessionCoreContract,
    ReviewSessionCoreContract,
) {
    let (game_import_id, first) =
        import_and_start_labeled(processor, principal.clone(), label).await;
    let started = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            &format!("{label}-moments"),
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
    )
    .await;
    let other_selection = started
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewSessionStarted { review_moments, .. } => review_moments
                    .iter()
                    .find(|moment| moment.review_moment.ply != first.review_moment.ply)
                    .map(|moment| moment.review_moment.selection.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("the canonical fixture has a second Automatic Review Moment");
    let second = open_review_moment(
        processor,
        principal.clone(),
        &game_import_id,
        &format!("{label}-open-other"),
        other_selection,
    )
    .await;
    let first = open_review_moment(
        processor,
        principal,
        &game_import_id,
        &format!("{label}-reopen-first"),
        first.review_moment.selection.clone(),
    )
    .await;
    (game_import_id, first, second)
}

async fn open_review_moment(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    game_import_id: &GameImportId,
    label: &str,
    selection: ReviewMomentSelection,
) -> ReviewSessionCoreContract {
    let events = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            label,
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection,
                idempotency_key: idempotency_key(label),
            },
        ),
    )
    .await;
    events
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
        .expect("opening a Review Moment prepares it")
}

fn played_san(core: &ReviewSessionCoreContract) -> String {
    core.imported_game
        .game
        .moves
        .iter()
        .find(|game_move| game_move.ply == core.review_moment.ply)
        .map(|game_move| game_move.san.clone())
        .expect("the opened ply is a move in the imported game")
}

fn legal_alternatives(position: &PositionSnapshot) -> Vec<(String, String)> {
    use shakmaty::{fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, Position};

    let chess: Chess = Fen::from_ascii(position.fen.as_bytes())
        .expect("test position should be valid FEN")
        .into_position(CastlingMode::Standard)
        .expect("test position should be legal");
    chess
        .legal_moves()
        .into_iter()
        .map(|chess_move| {
            let uci = UciMove::from_move(&chess_move, CastlingMode::Standard).to_string();
            let san = SanPlus::from_move(chess.clone(), &chess_move).to_string();
            (uci, san)
        })
        .collect()
}

async fn explore_uci(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: &ProcessorPrincipal,
    game_import_id: &GameImportId,
    core: &ReviewSessionCoreContract,
    label: &str,
    uci: &str,
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
                    uci: uci.to_string(),
                },
                idempotency_key: idempotency_key(&format!("{label}-explore")),
            },
        ),
    )
    .await;
    events
        .iter()
        .find_map(explored_move)
        .expect("exploring the Alternative Move commits it")
}

fn host_envelope(
    principal: &ProcessorPrincipal,
    label: &str,
    game_import_id: GameImportId,
    message: &str,
) -> ReviewSessionCommandEnvelope {
    host_envelope_with_priors(principal, label, game_import_id, message, Vec::new())
}

fn host_envelope_with_priors(
    principal: &ProcessorPrincipal,
    label: &str,
    game_import_id: GameImportId,
    message: &str,
    prior_turns: Vec<HostTurnPriorTurn>,
) -> ReviewSessionCommandEnvelope {
    let mut envelope = envelope_for(
        principal,
        label,
        ReviewSessionCommand::StartHostTurn {
            game_import_id,
            message: message.to_string(),
            prior_turns,
            idempotency_key: idempotency_key(label),
        },
    );
    envelope.surface = DeliverySurface::Web;
    envelope
}

fn assert_completed_answer(events: &[ReviewSessionEventEnvelope], answer: &str) {
    assert!(
        events
            .iter()
            .any(|event| matches!(event.event, ReviewSessionEvent::Accepted { .. })),
        "HostTurn must accept: {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            ReviewSessionEvent::Completed { result }
                if matches!(
                    result.as_ref(),
                    OperationCompletion::HostTurnCompleted { answer: published, .. }
                        if published == answer
                )
        )),
        "expected HostTurnCompleted {answer:?}: {events:?}"
    );
}

fn assert_refused(events: &[ReviewSessionEventEnvelope], reason: HostTurnRefusalReason) {
    assert!(
        events
            .iter()
            .any(|event| matches!(event.event, ReviewSessionEvent::Accepted { .. })),
        "HostTurn must accept: {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            ReviewSessionEvent::Completed { result }
                if matches!(
                    result.as_ref(),
                    OperationCompletion::HostTurnRefused { reason: published }
                        if *published == reason
                )
        )),
        "expected HostTurnRefused {reason:?}: {events:?}"
    );
}

fn assert_unavailable(events: &[ReviewSessionEventEnvelope]) {
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            ReviewSessionEvent::Unavailable {
                operation: OperationKind::HostTurn,
                reason: ProviderUnavailableReason::LanguageLayer,
                ..
            }
        )),
        "expected HostTurn unavailable: {events:?}"
    );
}

struct Script {
    bodies: Vec<String>,
    delay: Duration,
    delay_from: usize,
    mismatch_pin: bool,
    mismatch_first_lookups: usize,
    hang_after: Option<usize>,
    faults: Vec<Option<u16>>,
    empty_completion: bool,
    hold_first: Option<Arc<Notify>>,
}

fn scripted(bodies: Vec<String>) -> Script {
    Script {
        bodies,
        delay: Duration::ZERO,
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: Vec::new(),
        empty_completion: false,
        hold_first: None,
    }
}

fn delayed_script(bodies: Vec<String>, delay: Duration) -> Script {
    Script {
        bodies,
        delay,
        delay_from: 0,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: Vec::new(),
        empty_completion: false,
        hold_first: None,
    }
}

fn delayed_after_first(bodies: Vec<String>, delay: Duration) -> Script {
    Script {
        bodies,
        delay,
        delay_from: 1,
        mismatch_pin: false,
        mismatch_first_lookups: 0,
        hang_after: None,
        faults: Vec::new(),
        empty_completion: false,
        hold_first: None,
    }
}

async fn bound_host_processor(
    script: Script,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<MemoryLanguageLayerLedger>,
) {
    let (processor, ledger, _) = bound_host_processor_hits(script).await;
    (processor, ledger)
}

async fn bound_host_processor_with(
    script: Script,
    config: LanguageLayerAdmissionConfig,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<MemoryLanguageLayerLedger>,
) {
    let (processor, ledger, _) = bound_host_processor_configured(script, config).await;
    (processor, ledger)
}

async fn bound_host_processor_hits(
    script: Script,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<MemoryLanguageLayerLedger>,
    Arc<AtomicUsize>,
) {
    bound_host_processor_configured(
        script,
        LanguageLayerAdmissionConfig::conservative_defaults(),
    )
    .await
}

async fn bound_host_processor_configured(
    script: Script,
    config: LanguageLayerAdmissionConfig,
) -> (
    Arc<ReviewSessionProcessor<CapturedLichess>>,
    Arc<MemoryLanguageLayerLedger>,
    Arc<AtomicUsize>,
) {
    let (base, hits) = spawn_host_server(script).await;
    let recording = support::provider_recording();
    let pin = compiled_pin_record();
    let ledger = Arc::new(MemoryLanguageLayerLedger::new());
    let hosted = Arc::new(HostedCommentRuntime::new(
        Arc::new(LanguageLayerProvider::from_client_at(
            reqwest::Client::new(),
            "test",
            &base,
        )),
        pin.clone(),
        fingerprint_from_pin(&pin, EvaluationEnvironment::Staging),
        ledger.clone(),
        Arc::new(ProviderConcurrency::new(4)),
        config,
    ));
    let built = ReviewSessionProcessor::new(
        CapturedLichess::new(),
        recording.clone(),
        Arc::new(support::RecordingEngine::new(&recording)),
        Arc::new(support::RecordingHuman::new(&recording, false)),
        Arc::new(support::GroundedAuthor),
    )
    .unwrap()
    .with_language_layer_ledger(ledger.clone())
    .with_hosted_comment(hosted);
    (Arc::new(built), ledger, hits)
}

async fn seed_global_ceiling(ledger: &MemoryLanguageLayerLedger) {
    let as_of = Utc::now();
    let fingerprint = fingerprint_from_pin(&compiled_pin_record(), EvaluationEnvironment::Staging);
    ledger
        .settle(LanguageLayerOperationalRecord {
            request_id: next_request_id(),
            player_id: PlayerId::try_from("player:host-turn:ceiling-seed".to_string()).unwrap(),
            settled_at: as_of,
            latency: Duration::ZERO,
            cost_micros: GLOBAL_CEILING_MICROS,
            prompt_tokens: None,
            completion_tokens: None,
            budget_decision: BudgetDecision::Admitted,
            denial_reason: None,
            error_class: None,
            pin_verification: chen_chess_coach_engine::evaluation_fingerprint::PinVerificationVerdict::NotApplicable,
            pin_cause: None,
            fingerprint_digest: fingerprint.digest.as_str().to_string(),
            capture_outcome: None,
            provider_cooldown: None,
            steps: Vec::new(),
        })
        .await
        .unwrap();
}

async fn spawn_host_server(script: Script) -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let pin = compiled_pin_record();
    let wrong_model = "openrouter/wrong-model".to_string();
    let bodies = Arc::new(Mutex::new(script.bodies.into_iter()));
    let faults = Arc::new(script.faults);
    let delay = script.delay;
    let delay_from = script.delay_from;
    let hang_after = script.hang_after;
    let empty_completion = script.empty_completion;
    let hold_first = script.hold_first.clone();
    let mismatch_pin = script.mismatch_pin;
    let mismatch_first_lookups = script.mismatch_first_lookups;
    let generation_hits = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route(
            "/generation",
            axum::routing::get({
                let pinned = pin.model.clone();
                let wrong = wrong_model.clone();
                let generation_hits = generation_hits.clone();
                move || {
                    let pinned = pinned.clone();
                    let wrong = wrong.clone();
                    let generation_hits = generation_hits.clone();
                    async move {
                        let lookup = generation_hits.fetch_add(1, Ordering::SeqCst);
                        let served = if mismatch_pin || lookup < mismatch_first_lookups {
                            wrong
                        } else {
                            pinned
                        };
                        Json(json!({
                            "data": {
                                "model": served,
                                "provider_name": "Google Vertex",
                                "data_region": "global",
                                "provider_responses": [{
                                    "endpoint_id": "ep-host-turn",
                                    "routed_service_tier": null
                                }]
                            }
                        }))
                    }
                }
            }),
        )
        .route(
            "/chat/completions",
            post({
                let bodies = bodies.clone();
                let counted = counted.clone();
                let faults = faults.clone();
                let hold_first = hold_first.clone();
                let first_comment = Arc::new(AtomicBool::new(false));
                move |Json(payload): Json<serde_json::Value>| {
                    let bodies = bodies.clone();
                    let counted = counted.clone();
                    let faults = faults.clone();
                    let hold_first = hold_first.clone();
                    let first_comment = first_comment.clone();
                    async move {
                        let index = counted.fetch_add(1, Ordering::SeqCst);
                        let schema_name = payload["response_format"]["json_schema"]["name"]
                            .as_str()
                            .unwrap_or("");
                        if schema_name != "host_turn_step" {
                            if let Some(hold) = hold_first {
                                if !first_comment.swap(true, Ordering::SeqCst) {
                                    hold.notified().await;
                                }
                            }
                            return Json(json!({
                                "id": format!("gen-first-open-{index}"),
                                "choices": [{
                                    "message": { "content": hosted_comment_body() },
                                    "finish_reason": "stop"
                                }]
                            }))
                            .into_response();
                        }
                        if hang_after == Some(index) {
                            std::future::pending::<()>().await;
                        }
                        if index >= delay_from && !delay.is_zero() {
                            tokio::time::sleep(delay).await;
                        }
                        if let Some(status) = faults.get(index).copied().flatten() {
                            let status = StatusCode::from_u16(status)
                                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                            if status == StatusCode::TOO_MANY_REQUESTS {
                                /* Long enough that a loaded test host cannot
                                outrun the cooldown between the two turns;
                                no test waits for this window to expire. */
                                return (status, [(header::RETRY_AFTER, "600")], "rate limited")
                                    .into_response();
                            }
                            return (status, "provider fault").into_response();
                        }
                        if empty_completion {
                            return Json(json!({
                                "id": format!("gen-host-turn-{index}"),
                                "choices": [{
                                    "message": {},
                                    "finish_reason": "stop"
                                }],
                                "usage": { "prompt_tokens": 12, "completion_tokens": 8, "cost": 0.001 }
                            }))
                            .into_response();
                        }
                        let body = bodies
                            .lock()
                            .await
                            .next()
                            .unwrap_or_else(|| answer_step("This move lost the exchange."));
                        Json(json!({
                            "id": format!("gen-host-turn-{index}"),
                            "choices": [{
                                "message": { "content": body },
                                "finish_reason": "stop"
                            }],
                            "usage": { "prompt_tokens": 12, "completion_tokens": 8, "cost": 0.001 }
                        }))
                        .into_response()
                    }
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    (format!("http://{addr}"), hits)
}

fn hosted_comment_body() -> String {
    json!({ "comment": "a first-open comment" }).to_string()
}

fn answer_step(answer: &str) -> String {
    flattened_step(json!({
        "kind": "answer",
        "capability": "",
        "ply": 0,
        "next": false,
        "classification": "",
        "moves": [],
        "opponentReplies": "",
        "answer": answer,
        "citations": [],
        "focusMoment": 0,
        "showLineKind": "",
        "alternativeMoveId": "",
        "refusalReason": "none"
    }))
}

fn focus_answer(answer: &str, focus_moment: u16) -> String {
    flattened_step(json!({
        "kind": "answer",
        "capability": "",
        "ply": 0,
        "next": false,
        "classification": "",
        "moves": [],
        "opponentReplies": "",
        "answer": answer,
        "citations": [],
        "focusMoment": focus_moment,
        "showLineKind": "",
        "alternativeMoveId": "",
        "refusalReason": "none"
    }))
}

fn cited_answer(answer: &str, citation: &str) -> String {
    flattened_step(json!({
        "kind": "answer",
        "capability": "",
        "ply": 0,
        "next": false,
        "classification": "",
        "moves": [],
        "opponentReplies": "",
        "answer": answer,
        "citations": [citation],
        "focusMoment": 0,
        "showLineKind": "",
        "alternativeMoveId": "",
        "refusalReason": "none"
    }))
}

fn call_step(capability: &str) -> String {
    flattened_step(json!({
        "kind": "call",
        "capability": capability,
        "ply": 0,
        "next": false,
        "classification": "",
        "moves": [],
        "opponentReplies": "",
        "answer": "",
        "citations": [],
        "focusMoment": 0,
        "showLineKind": "",
        "alternativeMoveId": "",
        "refusalReason": "none"
    }))
}

fn refuse_step(reason: &str) -> String {
    flattened_step(json!({
        "kind": "refuse",
        "capability": "",
        "ply": 0,
        "next": false,
        "classification": "",
        "moves": [],
        "opponentReplies": "",
        "answer": "",
        "citations": [],
        "focusMoment": 0,
        "showLineKind": "",
        "alternativeMoveId": "",
        "refusalReason": reason
    }))
}

fn evaluate_line_step(moves: &[&str]) -> String {
    flattened_step(json!({
        "kind": "call",
        "capability": "evaluateLine",
        "ply": 0,
        "next": false,
        "classification": "",
        "moves": moves,
        "opponentReplies": "engineBest",
        "answer": "",
        "citations": [],
        "focusMoment": 0,
        "showLineKind": "",
        "alternativeMoveId": "",
        "refusalReason": "none"
    }))
}

fn flattened_step(value: serde_json::Value) -> String {
    serde_json::to_string(&value).expect("host turn step fixture serializes")
}
