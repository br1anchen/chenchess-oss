use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use chrono::Utc;
use serde_json::{json, Value};

use crate::chess_literal_grounding::ChessLiteralGrounding;
use crate::critical_moment_comment::chess_literal_grounding_for;
use crate::evaluation_fingerprint::{
    CaptureOutcome, CaptureTrigger, EvaluationFingerprint, EvaluationStepObservation,
    HostTurnStepCapability, PinVerificationVerdict,
};
use crate::language_layer_ledger::{
    admit_host_turn_envelope, admit_host_turn_step, cost_micros_from_dollars,
    language_layer_record, Admission, AdmissionRequest, AttemptErrorClass, BudgetDecision,
    DenialReason, HostTurnEnvelopeAdmission, LanguageLayerAdmissionConfig, LanguageLayerLedger,
    LanguageLayerOperationalRecord, LanguageLayerRecordInput, ProviderConcurrency,
    GLOBAL_CEILING_MICROS, HOST_TURN_MAX_STEPS,
};
use crate::language_layer_prompt::{project_comment_facts, CoachingProfileProjection};
use crate::language_layer_provider::{
    ChatMessage, CompletionAttempt, CompletionOutcome, CompletionRequest, DeterminismControls,
    PinnedGenerationContract,
};
use crate::pin_verification::{
    judge_completed_verification, verify_generation_within_deadline, PinVerificationFailure,
    PinVerificationJudgement, PinVerificationStrictness,
};
use crate::quality_capture::{
    hosted_language_layer_capture, HostedGenerationInput, HostedLanguageLayerTask,
};
use crate::review_session_cancellation::ReviewSessionCancellation;
use crate::review_session_coaching::gate_player_message;
use crate::review_session_contract::*;
use crate::review_session_host::{
    compile_web_host_prompt, ground_host_turn_answer, host_capability_call_id,
    host_turn_fingerprint, host_turn_step_schema, parse_host_turn_step, san_from_uci,
    HostCapabilityCall, HostCapabilityDispatch, HostCapabilityError, HostCapabilityEvidence,
    HostTurnAnswerRefs, HostTurnPromptInput, HostTurnStep,
};

use super::{
    events::EventEmitter,
    session::{HostTurnTerminal, HostTurnUnavailableCause, ProcessorSession},
    LiveOperation, ProcessorPrincipal, ReviewSessionProcessor,
};
use crate::lichess::LichessExportClient;

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn start_host_turn(
        &self,
        principal: ProcessorPrincipal,
        operation_id: OperationId,
        game_import_id: GameImportId,
        message: String,
        prior_turns: Vec<HostTurnPriorTurn>,
        idempotency_key: IdempotencyKey,
        emitter: Arc<EventEmitter>,
    ) {
        let Some(session) = self
            .session(
                &game_import_id,
                &principal,
                &emitter,
                OperationKind::HostTurn,
            )
            .await
        else {
            return;
        };
        if let Some(replay) = session.host_turn_replay(&idempotency_key).await {
            emitter.accepted(OperationKind::HostTurn);
            emit_host_turn_terminal(&emitter, replay);
            return;
        }
        // Size and control-character gate — not D6. Off-topic input reaches
        // the model and is refused as `NotAboutThisReview`.
        if gate_player_message(&message).is_err()
            || prior_turns.iter().any(|turn| {
                gate_player_message(&turn.message).is_err()
                    || gate_player_message(&turn.answer).is_err()
            })
        {
            emitter.unavailable(
                OperationKind::HostTurn,
                ProviderUnavailableReason::LanguageLayer,
                RetryDirective::NotRetryable,
            );
            return;
        }
        let Some((player_id, runtime)) = hosted_binding(&principal, self.hosted_comment.as_ref())
        else {
            emitter.unavailable(
                OperationKind::HostTurn,
                ProviderUnavailableReason::LanguageLayer,
                RetryDirective::NotRetryable,
            );
            return;
        };
        let Some(open) = open_host_context(&session).await else {
            emitter.unavailable(
                OperationKind::HostTurn,
                ProviderUnavailableReason::LanguageLayer,
                RetryDirective::NotRetryable,
            );
            return;
        };

        let fingerprint = host_turn_fingerprint(&runtime.pin, runtime.fingerprint.axes.environment);
        let as_of = Utc::now();
        let admission_request = AdmissionRequest {
            player_id: &player_id,
            session: session.spend.as_ref(),
            remaining_deadline: runtime.config.host_turn_authoring_deadline,
            as_of,
        };
        let Some(ledger) = self.language_layer_ledger.as_ref() else {
            emitter.unavailable(
                OperationKind::HostTurn,
                ProviderUnavailableReason::LanguageLayer,
                RetryDirective::NotRetryable,
            );
            return;
        };
        let envelope = match admit_host_turn_envelope(
            &admission_request,
            session.spend.clone(),
            ledger.as_ref(),
            runtime.concurrency.as_ref(),
        )
        .await
        {
            Ok(HostTurnEnvelopeAdmission::Admitted(envelope)) => envelope,
            Ok(HostTurnEnvelopeAdmission::Denied(reason)) => {
                let record = denied_host_turn_record(
                    &player_id,
                    as_of,
                    &fingerprint,
                    reason,
                    runtime.concurrency.honoured_cooldown(),
                );
                let attempt = non_retryable_attempt();
                let settled = settle_or_log(ledger.as_ref(), record.clone()).await;
                emit_host_turn_settled(
                    &record,
                    &HostTurnTerminal::Unavailable {
                        cause: HostTurnUnavailableCause::Other,
                    },
                    &attempt,
                    &PinVerificationJudgement::NotApplicable,
                    settled,
                );
                if reason == DenialReason::GlobalCeiling {
                    runtime.ceiling_alert.global_ceiling_tripped();
                }
                session
                    .captures
                    .push(hosted_language_layer_capture(HostedGenerationInput {
                        fingerprint,
                        attempt: &attempt,
                        trigger: CaptureTrigger::Preference,
                        outcome: capture_outcome_for_denial(reason),
                        pin_verification: PinVerificationVerdict::NotApplicable,
                        served_endpoint: None,
                        served_region: None,
                        routed_service_tier: None,
                        attempts: 0,
                        task: HostedLanguageLayerTask::HostTurn,
                        created_at: as_of,
                        steps: Vec::new(),
                        rejection: None,
                    }));
                self.quality_capture
                    .commit_best_effort(&principal, &session.captures.take())
                    .await;
                emitter.unavailable(
                    OperationKind::HostTurn,
                    ProviderUnavailableReason::LanguageLayer,
                    RetryDirective::NotRetryable,
                );
                return;
            }
            Err(_) => {
                emitter.unavailable(
                    OperationKind::HostTurn,
                    ProviderUnavailableReason::Persistence,
                    RetryDirective::RetryAllowed,
                );
                return;
            }
        };

        let activity = self.coach_turn_activity(&principal, &game_import_id).await;
        let activity_id = host_turn_activity_id(&operation_id);
        let Some(_lease) = activity.acquire(&activity_id) else {
            envelope.release();
            emitter.conflict(
                OperationKind::HostTurn,
                OperationConflictReason::CoachTurnAlreadyActive,
            );
            return;
        };
        let cancellation = ReviewSessionCancellation::default();
        if !self
            .register_live(
                operation_id.clone(),
                LiveOperation::HostTurn {
                    owner: principal.clone(),
                    game_import_id: game_import_id.clone(),
                    idempotency_key: idempotency_key.clone(),
                    cancellation: cancellation.clone(),
                },
            )
            .await
        {
            envelope.release();
            emitter.rejected(
                OperationKind::HostTurn,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            );
            return;
        }

        emitter.accepted(OperationKind::HostTurn);
        let terminal = run_host_turn(RunHostTurn {
            session: session.as_ref(),
            runtime: runtime.as_ref(),
            player_id: &player_id,
            ledger: ledger.as_ref(),
            fingerprint: &fingerprint,
            profile: self.current_coaching_profile(),
            open,
            message: &message,
            prior_turns: &prior_turns,
            cancellation: &cancellation,
            emitter: emitter.as_ref(),
        })
        .await;

        envelope.release();
        session
            .record_host_turn(idempotency_key, terminal.clone())
            .await;
        let captures = session.captures.take();
        self.quality_capture
            .commit_best_effort(&principal, &captures)
            .await;
        self.live.lock().await.remove(&operation_id);
        emit_host_turn_terminal(&emitter, terminal);
        drop(_lease);
    }
}

struct RunHostTurn<'a> {
    session: &'a ProcessorSession,
    runtime: &'a crate::critical_moment_comment::HostedCommentRuntime,
    player_id: &'a PlayerId,
    ledger: &'a dyn crate::language_layer_ledger::LanguageLayerLedger,
    fingerprint: &'a EvaluationFingerprint,
    profile: CoachingProfileProjection,
    open: OpenHostContext,
    message: &'a str,
    prior_turns: &'a [HostTurnPriorTurn],
    cancellation: &'a ReviewSessionCancellation,
    emitter: &'a EventEmitter,
}

struct OpenHostContext {
    ply: u16,
    packet: Value,
    facts_grounding: ChessLiteralGrounding,
    allowed_literals: Vec<String>,
    engine_best_allowed: bool,
    played_refutation_allowed: bool,
    elo: u16,
    active_branch: Value,
    alternative_move_ids: Vec<AlternativeMoveId>,
}

struct RecordedDispatch {
    call_id: String,
    projection: Value,
    allowed_chess_literals: Vec<String>,
}

async fn run_host_turn(args: RunHostTurn<'_>) -> HostTurnTerminal {
    let deadline = Instant::now() + args.runtime.config.host_turn_authoring_deadline;
    let mut allowed_plies = vec![args.open.ply];
    let mut alternative_move_ids = args.open.alternative_move_ids.clone();
    let mut returned = Vec::new();
    let mut observations = TurnObservations::new();
    let mut corrective: Option<String> = None;
    let mut denial: Option<DenialReason> = None;
    let mut provider_cooldown = None;
    let started = Instant::now();

    let terminal = loop {
        if args.cancellation.is_cancelled() {
            break HostTurnTerminal::Unavailable {
                cause: HostTurnUnavailableCause::Cancelled,
            };
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || observations.model_calls >= HOST_TURN_MAX_STEPS {
            break HostTurnTerminal::Unavailable {
                cause: HostTurnUnavailableCause::Other,
            };
        }

        args.emitter.event(ReviewSessionEvent::Progress {
            stage: OperationProgress::HostTurn {
                label: HostTurnStepLabel::Writing,
            },
        });

        let request = completion_request(HostTurnCompletionInput {
            open: &args.open,
            profile: &args.profile,
            prior_turns: args.prior_turns,
            message: args.message,
            returned: &returned,
            corrective: corrective.as_deref(),
            pin: &args.runtime.pin,
            remaining,
        });

        let as_of = Utc::now();
        let admission = match admit_host_turn_step(
            &AdmissionRequest {
                player_id: args.player_id,
                session: args.session.spend.as_ref(),
                remaining_deadline: remaining,
                as_of,
            },
            &args.runtime.config,
            args.ledger,
            args.runtime.concurrency.as_ref(),
        )
        .await
        {
            Ok(Admission::Admitted(permit)) => permit,
            Ok(Admission::Denied(reason)) => {
                if reason == DenialReason::GlobalCeiling {
                    args.runtime.ceiling_alert.global_ceiling_tripped();
                }
                denial = Some(reason);
                break HostTurnTerminal::Unavailable {
                    cause: HostTurnUnavailableCause::Other,
                };
            }
            Err(_) => {
                break HostTurnTerminal::Unavailable {
                    cause: HostTurnUnavailableCause::Other,
                }
            }
        };

        let attempt = match args.runtime.provider.complete(&request).await {
            Ok(attempt) => attempt,
            Err(_) => non_retryable_attempt(),
        };
        provider_cooldown = apply_provider_outcome(
            args.runtime.concurrency.as_ref(),
            &args.runtime.config,
            &attempt.outcome,
        );
        drop(admission);

        let leftover = remaining.saturating_sub(attempt.latency);
        let pin = verify_step_pin(args.runtime, &attempt, leftover).await;
        if let PinVerificationJudgement::Mismatched(report) = &pin {
            args.runtime.pin_mismatch_alert.pin_mismatched(report);
        }
        args.session
            .spend
            .record(cost_micros_from_dollars(attempt.cost));

        if args.cancellation.is_cancelled() {
            observations.record(attempt, pin, None, true);
            break HostTurnTerminal::Unavailable {
                cause: HostTurnUnavailableCause::Cancelled,
            };
        }

        if Instant::now() >= deadline {
            observations.record(attempt, pin, None, true);
            break HostTurnTerminal::Unavailable {
                cause: HostTurnUnavailableCause::Other,
            };
        }

        if attempt.outcome != CompletionOutcome::Completed {
            if observations.record_transport_fault(attempt, pin) {
                continue;
            }
            break HostTurnTerminal::Unavailable {
                cause: HostTurnUnavailableCause::Other,
            };
        }

        let parsed = parse_step(&attempt);
        let capability = parsed.as_ref().and_then(step_capability);
        observations.record(attempt, pin, capability, true);

        let Some(step) = parsed else {
            break HostTurnTerminal::Unavailable {
                cause: HostTurnUnavailableCause::Other,
            };
        };

        match step {
            HostTurnStep::Call(_) if corrective.is_some() => {
                break HostTurnTerminal::Unavailable {
                    cause: HostTurnUnavailableCause::Other,
                };
            }
            HostTurnStep::Call(call) => {
                args.emitter.event(ReviewSessionEvent::Progress {
                    stage: OperationProgress::HostTurn {
                        label: progress_label(&call),
                    },
                });
                let dispatch_budget = deadline.saturating_duration_since(Instant::now());
                match dispatch_within(
                    dispatch_budget,
                    args.session.dispatch_host_capability(args.open.ply, &call),
                )
                .await
                {
                    CapabilityDispatchWindow::Finished(Ok(dispatch)) => {
                        collect_refs(&dispatch, &mut allowed_plies, &mut alternative_move_ids);
                        returned.push(RecordedDispatch {
                            call_id: dispatch.call_id,
                            projection: dispatch.projection,
                            allowed_chess_literals: dispatch.allowed_chess_literals,
                        });
                    }
                    CapabilityDispatchWindow::Finished(Err(error)) => {
                        returned.push(failed_dispatch(args.open.ply, &call, error));
                    }
                    CapabilityDispatchWindow::TurnBudgetGone => {
                        break HostTurnTerminal::Unavailable {
                            cause: HostTurnUnavailableCause::Other,
                        }
                    }
                }
            }
            HostTurnStep::Refuse { reason } => {
                break HostTurnTerminal::Refused { reason };
            }
            HostTurnStep::Answer {
                answer,
                citations,
                focus_moment,
                show_line,
            } => {
                let cited = union_cited(&args.open.facts_grounding, &returned, &citations);
                let refs = HostTurnAnswerRefs {
                    allowed_plies: &allowed_plies,
                    engine_best_allowed: args.open.engine_best_allowed,
                    played_refutation_allowed: args.open.played_refutation_allowed,
                    alternative_move_ids: &alternative_move_ids,
                };
                match ground_host_turn_answer(
                    &cited,
                    &answer,
                    focus_moment,
                    show_line.as_ref(),
                    refs,
                ) {
                    Ok(()) => {
                        break HostTurnTerminal::Completed {
                            answer,
                            focus_moment,
                            show_line,
                        };
                    }
                    Err(rejection) if corrective.is_none() => {
                        corrective = Some(rejection.reason().to_string());
                    }
                    Err(_) => {
                        break HostTurnTerminal::Unavailable {
                            cause: HostTurnUnavailableCause::GroundingRejected,
                        }
                    }
                }
            }
        }
    };

    settle_host_turn(SettleHostTurn {
        args: &args,
        terminal: &terminal,
        observations,
        latency: started.elapsed(),
        denial,
        provider_cooldown,
    })
    .await;
    terminal
}

struct TurnObservations {
    last_attempt: CompletionAttempt,
    pin: PinVerificationJudgement,
    steps: Vec<EvaluationStepObservation>,
    model_calls: u8,
    total_cost: i64,
    total_prompt: u64,
    total_completion: u64,
    transport_retried: bool,
}

impl TurnObservations {
    fn new() -> Self {
        Self {
            last_attempt: non_retryable_attempt(),
            pin: PinVerificationJudgement::Unverified,
            steps: Vec::new(),
            model_calls: 0,
            total_cost: 0,
            total_prompt: 0,
            total_completion: 0,
            transport_retried: false,
        }
    }

    fn record(
        &mut self,
        attempt: CompletionAttempt,
        pin: PinVerificationJudgement,
        capability: Option<HostTurnStepCapability>,
        counts_as_model_call: bool,
    ) {
        self.total_cost = self
            .total_cost
            .saturating_add(cost_micros_from_dollars(attempt.cost));
        self.total_prompt = self
            .total_prompt
            .saturating_add(attempt.prompt_tokens.unwrap_or(0));
        self.total_completion = self
            .total_completion
            .saturating_add(attempt.completion_tokens.unwrap_or(0));
        if !self.pin.pin_mismatched() {
            self.pin = pin;
        }
        self.steps.push(step_observation(&attempt, capability));
        self.last_attempt = attempt;
        if counts_as_model_call {
            self.model_calls += 1;
        }
    }

    fn record_transport_fault(
        &mut self,
        attempt: CompletionAttempt,
        pin: PinVerificationJudgement,
    ) -> bool {
        let retry = transport_retryable(&attempt.outcome) && !self.transport_retried;
        if retry {
            self.transport_retried = true;
        }
        self.record(attempt, pin, None, !retry);
        retry
    }
}

struct SettleHostTurn<'a> {
    args: &'a RunHostTurn<'a>,
    terminal: &'a HostTurnTerminal,
    observations: TurnObservations,
    latency: Duration,
    denial: Option<DenialReason>,
    provider_cooldown: Option<Duration>,
}

async fn settle_host_turn(args: SettleHostTurn<'_>) {
    let outcome = match args.terminal {
        HostTurnTerminal::Completed { .. } | HostTurnTerminal::Refused { .. } => {
            CaptureOutcome::Published
        }
        HostTurnTerminal::Unavailable { cause } => match cause {
            HostTurnUnavailableCause::GroundingRejected => CaptureOutcome::Rejected,
            HostTurnUnavailableCause::Cancelled | HostTurnUnavailableCause::Other => {
                CaptureOutcome::Failed
            }
        },
    };
    let as_of = Utc::now();
    let capture_outcome = match args.denial {
        Some(reason) if args.observations.total_cost == 0 => capture_outcome_for_denial(reason),
        _ => outcome,
    };
    args.args
        .session
        .captures
        .push(hosted_language_layer_capture(
            HostedGenerationInput {
                fingerprint: args.args.fingerprint.clone(),
                attempt: &args.observations.last_attempt,
                trigger: CaptureTrigger::Preference,
                outcome: capture_outcome,
                pin_verification: args.observations.pin.as_verdict(),
                served_endpoint: None,
                served_region: None,
                routed_service_tier: None,
                attempts: args.observations.model_calls,
                task: HostedLanguageLayerTask::HostTurn,
                created_at: as_of,
                steps: args.observations.steps.clone(),
                rejection: None,
            }
            .with_served_route(args.observations.pin.served_route()),
        ));
    let (budget_decision, denial_reason) = match args.denial {
        Some(reason) if args.observations.total_cost > 0 => {
            (BudgetDecision::Admitted, Some(reason))
        }
        Some(DenialReason::ProviderCooldown) => (
            BudgetDecision::ProviderCooldown,
            Some(DenialReason::ProviderCooldown),
        ),
        Some(reason) => (BudgetDecision::Denied, Some(reason)),
        None => (BudgetDecision::Admitted, None),
    };
    let provider_cooldown = match args.denial {
        Some(DenialReason::ProviderCooldown) => args
            .args
            .runtime
            .concurrency
            .honoured_cooldown()
            .or(args.provider_cooldown),
        _ => args.provider_cooldown,
    };
    let record = language_layer_record(LanguageLayerRecordInput {
        player_id: args.args.player_id.clone(),
        settled_at: as_of,
        latency: args.latency,
        cost_micros: args.observations.total_cost,
        prompt_tokens: (args.observations.total_prompt > 0)
            .then_some(args.observations.total_prompt),
        completion_tokens: (args.observations.total_completion > 0)
            .then_some(args.observations.total_completion),
        budget_decision,
        denial_reason,
        error_class: host_turn_error_class(
            args.terminal,
            &args.observations.last_attempt,
            args.denial,
        ),
        fingerprint_digest: args.args.fingerprint.digest.as_str().to_string(),
        capture_outcome: Some(capture_outcome),
        provider_cooldown,
        steps: args.observations.steps,
        pin: args.observations.pin.clone(),
    });
    let settled = settle_or_log(args.args.ledger, record.clone()).await;
    emit_host_turn_settled(
        &record,
        args.terminal,
        &args.observations.last_attempt,
        &args.observations.pin,
        settled,
    );
    if let Ok(global) = args.args.ledger.global_calendar_month(as_of).await {
        if global >= GLOBAL_CEILING_MICROS {
            args.args.runtime.ceiling_alert.global_ceiling_tripped();
        }
    }
}

struct HostTurnCompletionInput<'a> {
    open: &'a OpenHostContext,
    profile: &'a CoachingProfileProjection,
    prior_turns: &'a [HostTurnPriorTurn],
    message: &'a str,
    returned: &'a [RecordedDispatch],
    corrective: Option<&'a str>,
    pin: &'a crate::pin_record::PinRecord,
    remaining: Duration,
}

fn completion_request(args: HostTurnCompletionInput<'_>) -> CompletionRequest {
    let allowed_literals = union_prompt_literals(&args.open.allowed_literals, args.returned);
    let (system, mut user) = compile_web_host_prompt(HostTurnPromptInput {
        elo: args.open.elo,
        profile: args.profile,
        open_moment_packet: &args.open.packet,
        active_branch: &args.open.active_branch,
        prior_turns: args.prior_turns,
        allowed_chess_literals: &allowed_literals,
    });
    insert_before_player_message(
        &mut user,
        &turn_context_block(args.returned, args.corrective),
    );
    CompletionRequest {
        contract: PinnedGenerationContract {
            model: args.pin.model.clone(),
            provider_only: args.pin.endpoint_tag.clone(),
            max_tokens: args.pin.max_tokens,
            determinism: DeterminismControls {
                temperature: args.pin.determinism.temperature,
                seed: args.pin.determinism.seed,
            },
        },
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: system,
            },
            ChatMessage {
                role: "user".into(),
                content: user,
            },
            ChatMessage {
                role: "user".into(),
                content: format!("playerMessage:\n{}", args.message),
            },
        ],
        schema_name: "host_turn_step".to_string(),
        schema: host_turn_step_schema(),
        remaining_deadline: args.remaining,
    }
}

async fn verify_step_pin(
    runtime: &crate::critical_moment_comment::HostedCommentRuntime,
    attempt: &CompletionAttempt,
    remaining: Duration,
) -> PinVerificationJudgement {
    let completed = attempt.outcome == CompletionOutcome::Completed;
    let Some(generation_id) = attempt.generation_id.as_deref() else {
        if !completed {
            return PinVerificationJudgement::NotApplicable;
        }
        return judge_completed_verification(
            Err(PinVerificationFailure::MissingIdentity),
            &runtime.pin.model,
            &runtime.pin.endpoint_tag,
            completed,
            PinVerificationStrictness::Runtime,
        );
    };
    let result =
        verify_generation_within_deadline(runtime.provider.as_ref(), generation_id, remaining)
            .await;
    judge_completed_verification(
        result,
        &runtime.pin.model,
        &runtime.pin.endpoint_tag,
        completed,
        PinVerificationStrictness::Runtime,
    )
}

async fn open_host_context(session: &ProcessorSession) -> Option<OpenHostContext> {
    let open_ply = session.open_review_moment_ply().await?;
    let entries = session.review_moment_entries().await;
    let entry = entries.into_iter().find(|entry| entry.ply() == open_ply)?;
    let prepared = entry.prepared_moment().await?;
    let facts = prepared.comment_facts()?.clone();
    let packet = project_comment_facts(&facts);
    let mut grounding = chess_literal_grounding_for(&facts, None);
    let engine_best_allowed = facts.moment().objective.has_engine_best_line();
    let played_refutation_allowed = facts.moment().objective.has_played_refutation();
    let core = prepared.core_snapshot().await;
    let exploration = prepared.exploration.current_state().await;
    let alternative_move_ids = exploration
        .committed_moves
        .iter()
        .map(|commit| commit.alternative_move_id.clone())
        .collect();
    let active_branch = project_active_branch(&exploration, &mut grounding);
    let allowed_literals: Vec<String> = grounding.allowed().map(str::to_string).collect();
    Some(OpenHostContext {
        ply: open_ply,
        packet,
        facts_grounding: grounding,
        allowed_literals,
        engine_best_allowed,
        played_refutation_allowed,
        elo: core.imported_game.elo_profile.rating.value(),
        active_branch,
        alternative_move_ids,
    })
}

fn project_active_branch(
    exploration: &crate::review_session_exploration::AlternativeMoveExplorationState,
    grounding: &mut ChessLiteralGrounding,
) -> Value {
    if exploration.committed_moves.is_empty() {
        return json!(null);
    }
    let mut fen = exploration.root_position.fen.as_str();
    let mut moves = Vec::new();
    for commit in &exploration.committed_moves {
        let move_san = san_from_uci(fen, &commit.move_uci);
        if let Some(san) = move_san.as_deref() {
            grounding.allow_move_san(san);
        }
        let reply_san = match &commit.strongest_reply {
            StrongestReply::Offered { uci } => san_from_uci(&commit.resulting_position.fen, uci),
            StrongestReply::Terminal => None,
        };
        if let Some(san) = reply_san.as_deref() {
            grounding.allow_move_san(san);
        }
        moves.push(json!({
            "moveSan": move_san,
            "alternativeMoveId": commit.alternative_move_id,
            "comparison": commit.evaluation.comparison,
            "strongestReplySan": reply_san,
        }));
        fen = commit.resulting_position.fen.as_str();
    }
    json!({ "moves": moves })
}

fn hosted_binding(
    principal: &ProcessorPrincipal,
    hosted: Option<&Arc<crate::critical_moment_comment::HostedCommentRuntime>>,
) -> Option<(
    PlayerId,
    Arc<crate::critical_moment_comment::HostedCommentRuntime>,
)> {
    match (principal, hosted) {
        (ProcessorPrincipal::Player(player_id), Some(runtime)) => {
            Some((player_id.clone(), Arc::clone(runtime)))
        }
        _ => None,
    }
}

fn collect_refs(
    dispatch: &HostCapabilityDispatch,
    allowed_plies: &mut Vec<u16>,
    alternative_move_ids: &mut Vec<AlternativeMoveId>,
) {
    match &dispatch.evidence {
        HostCapabilityEvidence::Moment { ply, .. } => {
            if !allowed_plies.contains(ply) {
                allowed_plies.push(*ply);
            }
        }
        HostCapabilityEvidence::MomentList { moments } => {
            for moment in moments {
                if !allowed_plies.contains(&moment.ply) {
                    allowed_plies.push(moment.ply);
                }
            }
        }
        HostCapabilityEvidence::EvaluatedLine { commits, .. } => {
            for commit in commits {
                if !alternative_move_ids.contains(&commit.alternative_move_id) {
                    alternative_move_ids.push(commit.alternative_move_id.clone());
                }
            }
        }
        HostCapabilityEvidence::LearningMaterial { ply, .. } => {
            if !allowed_plies.contains(ply) {
                allowed_plies.push(*ply);
            }
        }
    }
}

fn union_cited(
    preloaded: &ChessLiteralGrounding,
    returned: &[RecordedDispatch],
    citations: &[String],
) -> ChessLiteralGrounding {
    let mut grounding = ChessLiteralGrounding::empty();
    for literal in preloaded.allowed() {
        grounding.allow(literal);
    }
    for citation in citations {
        if let Some(dispatch) = returned
            .iter()
            .find(|dispatch| dispatch.call_id == *citation)
        {
            grounding.allow_all(&dispatch.allowed_chess_literals);
        }
    }
    grounding
}

fn parse_step(attempt: &CompletionAttempt) -> Option<HostTurnStep> {
    let raw = attempt.raw_content.as_deref()?;
    let value: Value = serde_json::from_str(raw).ok()?;
    parse_host_turn_step(&value).ok()
}

fn progress_label(call: &HostCapabilityCall) -> HostTurnStepLabel {
    match HostTurnStepCapability::from(call) {
        HostTurnStepCapability::EvaluateLine => HostTurnStepLabel::CheckingThatLine,
        HostTurnStepCapability::ReadMoment | HostTurnStepCapability::ListMoments => {
            HostTurnStepLabel::LookingAtAnotherMoment
        }
        HostTurnStepCapability::LearningMaterial => HostTurnStepLabel::Writing,
    }
}

impl From<&HostCapabilityCall> for HostTurnStepCapability {
    fn from(call: &HostCapabilityCall) -> Self {
        match call {
            HostCapabilityCall::ReadMoment { .. } => Self::ReadMoment,
            HostCapabilityCall::ListMoments => Self::ListMoments,
            HostCapabilityCall::EvaluateLine(_) => Self::EvaluateLine,
            HostCapabilityCall::LearningMaterial => Self::LearningMaterial,
        }
    }
}

fn step_capability(step: &HostTurnStep) -> Option<HostTurnStepCapability> {
    match step {
        HostTurnStep::Call(call) => Some(HostTurnStepCapability::from(call)),
        HostTurnStep::Answer { .. } | HostTurnStep::Refuse { .. } => None,
    }
}

fn step_observation(
    attempt: &CompletionAttempt,
    capability: Option<HostTurnStepCapability>,
) -> EvaluationStepObservation {
    EvaluationStepObservation {
        served_model: attempt.served_model.clone(),
        served_provider: attempt.served_provider.clone(),
        prompt_tokens: attempt.prompt_tokens,
        completion_tokens: attempt.completion_tokens,
        cost_micros: cost_micros_from_dollars(attempt.cost),
        capability,
    }
}

async fn settle_or_log(
    ledger: &dyn LanguageLayerLedger,
    record: LanguageLayerOperationalRecord,
) -> bool {
    match ledger.settle(record).await {
        Ok(()) => true,
        Err(error) => {
            tracing::error!(
                error = %error,
                "Language Layer ledger settle failed for HostTurn"
            );
            false
        }
    }
}

fn host_turn_settled_diagnostic(
    record: &LanguageLayerOperationalRecord,
    terminal: &HostTurnTerminal,
    attempt: &CompletionAttempt,
    pin: &PinVerificationJudgement,
    settled: bool,
) -> Value {
    let (terminal_kind, unavailable_cause) = match terminal {
        HostTurnTerminal::Completed { .. } => ("completed", None),
        HostTurnTerminal::Refused { .. } => ("refused", None),
        HostTurnTerminal::Unavailable { cause } => (
            "unavailable",
            Some(match cause {
                HostTurnUnavailableCause::GroundingRejected => "groundingRejected",
                HostTurnUnavailableCause::Cancelled => "cancelled",
                HostTurnUnavailableCause::Other => "other",
            }),
        ),
    };
    json!({
        "boundary": "coach-engine",
        "budgetDecision": record.budget_decision.as_str(),
        "completionOutcome": attempt.outcome.as_str(),
        "costMicros": record.cost_micros,
        "event": "host_turn_settled",
        "httpStatus": attempt.http_status,
        "pin": serde_json::to_value(pin.as_verdict())
            .expect("PinVerificationVerdict is serializable"),
        "pinCause": pin.cause().map(crate::pin_verification::PinVerificationCause::as_str),
        "pinMismatch": pin.mismatch_report(),
        "requestId": record.request_id,
        "schemaVersion": 1,
        "settled": settled,
        "stepCount": record.steps.len(),
        "terminal": terminal_kind,
        "unavailableCause": unavailable_cause,
    })
}

fn emit_host_turn_settled(
    record: &LanguageLayerOperationalRecord,
    terminal: &HostTurnTerminal,
    attempt: &CompletionAttempt,
    pin: &PinVerificationJudgement,
    settled: bool,
) {
    let diagnostic = host_turn_settled_diagnostic(record, terminal, attempt, pin, settled);
    eprintln!(
        "{}",
        serde_json::to_string(&diagnostic).expect("HostTurn settle telemetry is serializable")
    );
}

fn emit_host_turn_terminal(emitter: &EventEmitter, terminal: HostTurnTerminal) {
    match terminal {
        HostTurnTerminal::Completed {
            answer,
            focus_moment,
            show_line,
        } => emitter.completed(OperationCompletion::HostTurnCompleted {
            answer,
            focus_moment,
            show_line,
        }),
        HostTurnTerminal::Refused { reason } => {
            emitter.completed(OperationCompletion::HostTurnRefused { reason });
        }
        HostTurnTerminal::Unavailable {
            cause: HostTurnUnavailableCause::Cancelled,
        } => emitter.cancelled(OperationKind::HostTurn),
        HostTurnTerminal::Unavailable { .. } => emitter.unavailable(
            OperationKind::HostTurn,
            ProviderUnavailableReason::LanguageLayer,
            RetryDirective::NotRetryable,
        ),
    }
}

fn host_turn_error_class(
    terminal: &HostTurnTerminal,
    attempt: &CompletionAttempt,
    denial: Option<DenialReason>,
) -> Option<AttemptErrorClass> {
    match terminal {
        HostTurnTerminal::Unavailable {
            cause: HostTurnUnavailableCause::Cancelled,
        } => Some(AttemptErrorClass::Cancelled),
        HostTurnTerminal::Unavailable { .. } if denial.is_none() => {
            AttemptErrorClass::from_completion(&attempt.outcome)
        }
        _ => None,
    }
}

fn denied_host_turn_record(
    player_id: &PlayerId,
    as_of: chrono::DateTime<Utc>,
    fingerprint: &EvaluationFingerprint,
    reason: DenialReason,
    provider_cooldown: Option<Duration>,
) -> LanguageLayerOperationalRecord {
    language_layer_record(LanguageLayerRecordInput {
        player_id: player_id.clone(),
        settled_at: as_of,
        latency: Duration::ZERO,
        cost_micros: 0,
        prompt_tokens: None,
        completion_tokens: None,
        budget_decision: if reason == DenialReason::ProviderCooldown {
            BudgetDecision::ProviderCooldown
        } else {
            BudgetDecision::Denied
        },
        denial_reason: Some(reason),
        error_class: None,
        fingerprint_digest: fingerprint.digest.as_str().to_string(),
        capture_outcome: Some(capture_outcome_for_denial(reason)),
        provider_cooldown: if reason == DenialReason::ProviderCooldown {
            provider_cooldown
        } else {
            None
        },
        steps: Vec::new(),
        pin: PinVerificationJudgement::NotApplicable,
    })
}

fn capture_outcome_for_denial(reason: DenialReason) -> CaptureOutcome {
    match reason {
        DenialReason::ProviderCooldown => CaptureOutcome::ProviderCooldown,
        DenialReason::ReviewSessionCeiling
        | DenialReason::PlayerCeiling
        | DenialReason::GlobalCeiling
        | DenialReason::ConcurrencyUnavailable => CaptureOutcome::BudgetRefused,
    }
}

fn non_retryable_attempt() -> CompletionAttempt {
    CompletionAttempt {
        latency: Duration::ZERO,
        http_status: None,
        generation_id: None,
        served_model: None,
        served_provider: None,
        prompt_tokens: None,
        completion_tokens: None,
        reasoning_tokens: None,
        cost: None,
        finish_reason: None,
        raw_content: None,
        outcome: CompletionOutcome::InvalidRequest,
    }
}

/// Hashed so a long `OperationId` still fits `CoachTurnId`.
fn host_turn_activity_id(operation_id: &OperationId) -> CoachTurnId {
    let digest = format!("{:x}", Sha256::digest(operation_id.as_str().as_bytes()));
    CoachTurnId::try_from(format!("coach-turn:host-{digest}"))
        .expect("hashed host-turn activity id is a valid semantic id")
}

fn apply_provider_outcome(
    concurrency: &ProviderConcurrency,
    config: &LanguageLayerAdmissionConfig,
    outcome: &CompletionOutcome,
) -> Option<Duration> {
    match outcome {
        CompletionOutcome::RateLimited {
            retry_after,
            source,
        } => Some(concurrency.honor_rate_limit(*retry_after, *source, config)),
        _ => {
            concurrency.note_non_rate_limited();
            None
        }
    }
}

fn transport_retryable(outcome: &CompletionOutcome) -> bool {
    matches!(
        outcome,
        CompletionOutcome::TransportError
            | CompletionOutcome::TimedOut
            | CompletionOutcome::HttpError
            | CompletionOutcome::EmptyCompletion
    )
}

fn union_prompt_literals(open: &[String], returned: &[RecordedDispatch]) -> Vec<String> {
    let mut literals = open.to_vec();
    for dispatch in returned {
        for literal in &dispatch.allowed_chess_literals {
            if !literals.contains(literal) {
                literals.push(literal.clone());
            }
        }
    }
    literals
}

fn turn_context_block(returned: &[RecordedDispatch], corrective: Option<&str>) -> String {
    let mut block = String::new();
    if !returned.is_empty() {
        block.push_str("CAPABILITY_RESULTS:\n");
        for dispatch in returned {
            block.push_str(&format!("{}: {}\n", dispatch.call_id, dispatch.projection));
        }
        block.push('\n');
    }
    if let Some(reason) = corrective {
        block.push_str("GROUNDING_REJECTION:\n");
        block.push_str(reason);
        block.push_str("\nAnswer again using only the allowed chess literals.\n\n");
    }
    block
}

fn insert_before_player_message(user: &mut String, block: &str) {
    if block.is_empty() {
        return;
    }
    const MARKER: &str = "PLAYER_MESSAGE:";
    if let Some(index) = user.rfind(MARKER) {
        user.insert_str(index, block);
    } else {
        user.push_str(block);
    }
}

enum CapabilityDispatchWindow {
    Finished(Result<HostCapabilityDispatch, HostCapabilityError>),
    TurnBudgetGone,
}

async fn dispatch_within<F>(remaining: Duration, dispatch: F) -> CapabilityDispatchWindow
where
    F: Future<Output = Result<HostCapabilityDispatch, HostCapabilityError>>,
{
    if remaining.is_zero() {
        return CapabilityDispatchWindow::TurnBudgetGone;
    }
    match tokio::time::timeout(remaining, dispatch).await {
        Ok(result) => CapabilityDispatchWindow::Finished(result),
        Err(_) => CapabilityDispatchWindow::TurnBudgetGone,
    }
}

fn failed_dispatch(
    open_ply: u16,
    call: &HostCapabilityCall,
    error: HostCapabilityError,
) -> RecordedDispatch {
    RecordedDispatch {
        call_id: host_capability_call_id(call, open_ply),
        projection: json!({ "error": error.message }),
        allowed_chess_literals: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_results_extend_the_prompt_vocabulary() {
        let returned = [RecordedDispatch {
            call_id: "call:listMoments".to_owned(),
            projection: json!({ "playedSan": "Nxd4" }),
            allowed_chess_literals: vec!["Nxd4".to_owned()],
        }];
        assert_eq!(
            union_prompt_literals(&["e4".to_owned()], &returned),
            vec!["e4".to_owned(), "Nxd4".to_owned()]
        );
    }

    #[test]
    fn capability_results_sit_before_the_player_message_pointer() {
        let mut user = String::from("ALLOWED_CHESS_LITERALS:\ne4\n\nPLAYER_MESSAGE:\n(next)");
        insert_before_player_message(
            &mut user,
            "CAPABILITY_RESULTS:\ncall:listMoments: {\"playedSan\":\"Nxd4\"}\n\n",
        );
        let results = user.find("CAPABILITY_RESULTS:").expect("results inserted");
        let pointer = user.rfind("PLAYER_MESSAGE:").expect("pointer remains");
        assert!(results < pointer);
    }

    #[test]
    fn capability_results_ignore_player_message_text_in_prior_turns() {
        let mut user = String::from(
            "PRIOR_TURNS:\nPLAYER_MESSAGE: earlier\n\nALLOWED_CHESS_LITERALS:\ne4\n\nPLAYER_MESSAGE:\n(next)",
        );
        insert_before_player_message(&mut user, "CAPABILITY_RESULTS:\ncall:listMoments\n\n");
        let results = user.find("CAPABILITY_RESULTS:").expect("results inserted");
        let pointer = user.rfind("PLAYER_MESSAGE:").expect("real pointer remains");
        assert!(results < pointer);
        assert!(user[..results].contains("PLAYER_MESSAGE: earlier"));
    }

    #[test]
    fn host_turn_activity_id_is_unique_for_a_long_operation_id() {
        let first = OperationId::try_from(format!("op:{}", "a".repeat(120))).unwrap();
        let second = OperationId::try_from(format!("op:{}", "b".repeat(120))).unwrap();
        let left = host_turn_activity_id(&first);
        let right = host_turn_activity_id(&second);
        assert_ne!(left, right);
        assert!(left.as_str().starts_with("coach-turn:host-"));
        assert!(right.as_str().len() <= 128);
    }

    #[test]
    fn host_turn_settled_diagnostic_names_outcome_pin_cause_and_settle() {
        let record = LanguageLayerOperationalRecord {
            request_id: "ll-diagnostic".into(),
            player_id: PlayerId::try_from("firebase-player-host-turn".to_string()).unwrap(),
            settled_at: Utc::now(),
            latency: Duration::from_millis(12),
            cost_micros: 0,
            prompt_tokens: None,
            completion_tokens: None,
            budget_decision: BudgetDecision::Admitted,
            denial_reason: None,
            error_class: None,
            pin_verification: crate::evaluation_fingerprint::PinVerificationVerdict::NotApplicable,
            pin_cause: None,
            fingerprint_digest: "sha256:host-turn".into(),
            capture_outcome: Some(CaptureOutcome::Failed),
            provider_cooldown: None,
            steps: Vec::new(),
        };
        let diagnostic = host_turn_settled_diagnostic(
            &record,
            &HostTurnTerminal::Unavailable {
                cause: HostTurnUnavailableCause::Other,
            },
            &non_retryable_attempt(),
            &PinVerificationJudgement::NotApplicable,
            false,
        );
        assert_eq!(diagnostic["event"], "host_turn_settled");
        assert_eq!(diagnostic["completionOutcome"], "invalidRequest");
        assert_eq!(diagnostic["pin"], "notApplicable");
        assert_eq!(diagnostic["pinCause"], serde_json::Value::Null);
        assert_eq!(diagnostic["unavailableCause"], "other");
        assert_eq!(diagnostic["settled"], false);

        let verify_failed = host_turn_settled_diagnostic(
            &record,
            &HostTurnTerminal::Unavailable {
                cause: HostTurnUnavailableCause::Other,
            },
            &non_retryable_attempt(),
            &PinVerificationJudgement::Failed(PinVerificationFailure::VerifyError),
            true,
        );
        assert_eq!(verify_failed["pin"], "failed");
        assert_eq!(verify_failed["pinCause"], "verifyError");
        assert_eq!(verify_failed["pinMismatch"], serde_json::Value::Null);

        let mismatched = host_turn_settled_diagnostic(
            &record,
            &HostTurnTerminal::Completed {
                answer: "ok".into(),
                focus_moment: None,
                show_line: None,
            },
            &non_retryable_attempt(),
            &PinVerificationJudgement::Mismatched(crate::pin_verification::PinMismatchReport {
                pinned_model: "pinned/model".into(),
                pinned_provider_family: "google-vertex".into(),
                observed_permaslug: Some("other/model".into()),
                observed_provider: Some("Google Vertex".into()),
                observed_provider_family: Some("google-vertex".into()),
                served_endpoint: None,
                served_region: None,
                routed_service_tier: None,
            }),
            true,
        );
        assert_eq!(mismatched["pin"], "failed");
        assert_eq!(mismatched["pinCause"], "mismatched");
        assert_eq!(mismatched["pinMismatch"]["pinnedModel"], "pinned/model");
        assert_eq!(
            mismatched["pinMismatch"]["observedPermaslug"],
            "other/model"
        );
        assert!(mismatched.get("pinnedModel").is_none());
    }

    #[test]
    fn cancelled_host_turn_keeps_error_class_when_pin_mismatched() {
        assert_eq!(
            host_turn_error_class(
                &HostTurnTerminal::Unavailable {
                    cause: HostTurnUnavailableCause::Cancelled,
                },
                &non_retryable_attempt(),
                None,
            ),
            Some(AttemptErrorClass::Cancelled)
        );
    }

    #[test]
    fn transport_retry_covers_the_provider_faults() {
        assert!(transport_retryable(&CompletionOutcome::TimedOut));
        assert!(transport_retryable(&CompletionOutcome::HttpError));
        assert!(transport_retryable(&CompletionOutcome::TransportError));
        assert!(!transport_retryable(&CompletionOutcome::InvalidRequest));
        assert!(!transport_retryable(&CompletionOutcome::DeadlineExhausted));
        assert!(!transport_retryable(&CompletionOutcome::Completed));
    }

    #[tokio::test]
    async fn capability_dispatch_times_out_when_the_turn_budget_is_gone() {
        let timed_out = dispatch_within(
            Duration::from_millis(10),
            std::future::pending::<Result<HostCapabilityDispatch, HostCapabilityError>>(),
        )
        .await;
        assert!(matches!(
            timed_out,
            CapabilityDispatchWindow::TurnBudgetGone
        ));
        let already_gone = dispatch_within(Duration::ZERO, async {
            Err(HostCapabilityError::new("unused"))
        })
        .await;
        assert!(matches!(
            already_gone,
            CapabilityDispatchWindow::TurnBudgetGone
        ));
    }
}
