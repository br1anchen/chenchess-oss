use super::*;

#[derive(Clone)]
struct CoachTarget {
    game_import_id: GameImportId,
    review_moment_id: CriticalMomentId,
    context: CoachTurnContext,
}

#[derive(Default)]
struct DurableCoachCheckpointStore {
    inner: InMemoryReviewAnalysisCache,
    replace_attempts: AtomicUsize,
    fail_next_replace: AtomicBool,
}

impl ReviewAnalysisCacheStore for DurableCoachCheckpointStore {
    fn seed<'a>(&'a self, entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a> {
        self.inner.seed(entries)
    }

    fn load<'a>(
        &'a self,
        game_import_id: &'a GameImportId,
        game: &'a ReviewSessionGame,
        now: chrono::DateTime<Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>> {
        self.inner.load(game_import_id, game, now)
    }

    fn replace_moment<'a>(
        &'a self,
        mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            self.replace_attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail_next_replace.swap(false, Ordering::SeqCst) {
                return Err(ReviewAnalysisCacheError::Unavailable);
            }
            self.inner.replace_moment(mutation).await
        })
    }
}

#[tokio::test]
async fn failed_admission_is_never_acknowledged_and_leaves_no_partial_turn() {
    let checkpoints = Arc::new(DurableCoachCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let mut first_transport = JourneySurface::jsonl(first_processor);
    let target = prepared_coach_target(&mut first_transport, "failed-coach-admission").await;
    let coach_turn_id =
        CoachTurnId::try_from("coach-turn:journey:failed-admission".to_string()).unwrap();
    let operation_key = key("failed-coach-admission-operation");
    checkpoints.fail_next_replace.store(true, Ordering::SeqCst);
    let mut context = target.context.clone();
    context.coach_turn_id = coach_turn_id.clone();
    let failed = first_transport
        .submit(
            "failed-coach-admission-start",
            ReviewSessionCommand::StartCoachTurn {
                game_import_id: target.game_import_id.clone(),
                review_moment_id: target.review_moment_id.clone(),
                coach_turn_id: coach_turn_id.clone(),
                context: Box::new(context),
                message: "This admission must not escape failed persistence.".to_string(),
                idempotency_key: operation_key.clone(),
                prior_turn: PriorCoachTurn::None,
            },
        )
        .await;
    assert!(!failed.iter().any(|event| matches!(
        event.event,
        ReviewSessionEvent::Accepted {
            operation: OperationKind::CoachTurn,
            ..
        }
    )));
    assert!(matches!(
        failed.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::CoachTurn,
            reason: ProviderUnavailableReason::Persistence,
            retry: RetryDirective::RetryAllowed,
        })
    ));
    drop(first_transport);

    let second_processor = processor_with_stores(game_imports, checkpoints);
    let mut second_transport = JourneySurface::jsonl(second_processor);
    let retried = start_prepared_turn(
        &mut second_transport,
        &target,
        coach_turn_id,
        operation_key,
        PriorCoachTurn::None,
        "failed-coach-admission-retry",
    )
    .await;
    assert_eq!(retried.context.target, target.context.target);
}

#[tokio::test]
async fn a_published_coach_turn_replays_idempotently_and_stays_immutable() {
    let checkpoints = Arc::new(DurableCoachCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports, checkpoints.clone());
    let mut first_transport = JourneySurface::jsonl(first_processor);
    let target = prepared_coach_target(&mut first_transport, "durable-coach").await;
    let first_turn_id =
        CoachTurnId::try_from("coach-turn:journey:durable:first".to_string()).unwrap();
    let first_operation_key = key("durable-coach-first-operation");
    let first_facts = start_prepared_turn(
        &mut first_transport,
        &target,
        first_turn_id.clone(),
        first_operation_key,
        PriorCoachTurn::None,
        "durable-coach-first",
    )
    .await;
    let first_assessment = assessment(&first_facts, "durable-first");
    let first_idempotency_key = key("durable-coach-first-publication");
    let first_publication = publish_turn(
        &mut first_transport,
        &target,
        first_turn_id.clone(),
        first_assessment.clone(),
        first_idempotency_key.clone(),
        "durable-coach-first-publish",
    )
    .await;
    // Publication grounds the prose, so what comes back is the substituted
    // assessment rather than the marker form that was submitted. Everything
    // this test is about — replay, immutability — is asserted against that.
    let first_published = published_assessment(&first_publication);
    assert_eq!(
        first_published.coach_turn_id,
        first_assessment.coach_turn_id
    );
    assert_eq!(
        first_published.alternative_move_id,
        first_assessment.alternative_move_id
    );
    assert!(!first_published.objective_quality.explanation.contains('{'));
    let writes_after_first_publication = checkpoints.replace_attempts.load(Ordering::SeqCst);
    let mut second_transport = first_transport;

    let duplicate = publish_turn(
        &mut second_transport,
        &target,
        first_turn_id,
        first_assessment.clone(),
        first_idempotency_key,
        "durable-coach-first-duplicate",
    )
    .await;
    assert_eq!(published_assessment(&duplicate), first_published);
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        writes_after_first_publication,
        "replaying a published completion must not append or rewrite it"
    );

    let second_turn_id =
        CoachTurnId::try_from("coach-turn:journey:durable:second".to_string()).unwrap();
    let second_facts = start_prepared_turn(
        &mut second_transport,
        &target,
        second_turn_id.clone(),
        key("durable-coach-second-operation"),
        PriorCoachTurn::None,
        "durable-coach-second",
    )
    .await;
    let second_assessment = assessment(&second_facts, "durable-second");
    let second_publication = publish_turn(
        &mut second_transport,
        &target,
        second_turn_id,
        second_assessment.clone(),
        key("durable-coach-second-publication"),
        "durable-coach-second-publish",
    )
    .await;
    let second_published = published_assessment(&second_publication);
    assert_eq!(
        second_published.coach_turn_id,
        second_assessment.coach_turn_id
    );
    assert_ne!(second_published, first_published);

    let first_still_canonical = publish_turn(
        &mut second_transport,
        &target,
        first_facts.coach_turn_id.clone(),
        first_assessment.clone(),
        key("durable-coach-first-publication"),
        "durable-coach-first-after-second",
    )
    .await;
    assert_eq!(
        published_assessment(&first_still_canonical),
        first_published
    );
    assert_objective_coach_facts(&first_facts, "");
    assert_objective_coach_facts(&second_facts, "");
}

#[tokio::test]
async fn player_plan_evaluation_never_propagates_into_branch_descendants_or_coach_turns() {
    let processor = processor_with_stores(
        Arc::new(InMemoryGameImportStore::default()),
        Arc::new(InMemoryReviewAnalysisCache::default()),
    );
    let mut transport = JourneySurface::jsonl(processor);
    let target = prepared_coach_target(&mut transport, "plan-isolation").await;
    let prepared = transport
        .submit(
            "plan-isolation-prepare",
            ReviewSessionCommand::EvaluatePlayerPlan {
                game_import_id: target.game_import_id.clone(),
                review_moment_id: target.review_moment_id.clone(),
                request: PlayerPlanEvaluationRequest::Prepare,
            },
        )
        .await;
    let evaluation_context = match completion(&prepared) {
        OperationCompletion::PlayerPlanEvaluationPrepared { context } => context.as_ref(),
        completion => panic!("expected Player Plan Evaluation facts, got {completion:?}"),
    };
    let counterplay = evaluation_context
        .facts
        .objective_counterplay_san
        .first()
        .expect("fixture Review Moment has Objective Counterplay");
    let evaluation_text = format!(
        "{} supports the plan, while {counterplay} is the objective challenge.",
        evaluation_context.facts.reviewed_move_san
    );
    let evaluated = transport
        .submit(
            "plan-isolation-admit",
            ReviewSessionCommand::EvaluatePlayerPlan {
                game_import_id: target.game_import_id.clone(),
                review_moment_id: target.review_moment_id.clone(),
                request: PlayerPlanEvaluationRequest::Admit {
                    draft: PlayerPlanEvaluationDraft {
                        facts_ref: evaluation_context.facts_ref.clone(),
                        text: evaluation_text.clone(),
                    },
                },
            },
        )
        .await;
    assert!(matches!(
        completion(&evaluated),
        OperationCompletion::PlayerPlanEvaluated { .. }
    ));

    let first_facts = start_prepared_turn(
        &mut transport,
        &target,
        CoachTurnId::try_from("coach-turn:journey:plan-isolation:first".to_string()).unwrap(),
        key("plan-isolation-first"),
        PriorCoachTurn::None,
        "plan-isolation-first",
    )
    .await;
    assert_objective_coach_facts(&first_facts, &evaluation_text);

    let parent = &first_facts.alternative_move;
    let child = transport
        .submit(
            "plan-isolation-child",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: target.game_import_id.clone(),
                review_moment_id: target.review_moment_id.clone(),
                parent: BranchParent::Move {
                    branch_ref: parent.branch_ref.clone(),
                },
                source_position_ref: parent.resulting_position.position_ref.clone(),
                move_input: MoveInput::Uci {
                    uci: first_legal_uci(&parent.resulting_position.fen),
                },
                idempotency_key: key("plan-isolation-child"),
            },
        )
        .await;
    let child = match completion(&child) {
        OperationCompletion::AlternativeMoveEvaluated { alternative_move } => {
            alternative_move.as_ref()
        }
        completion => panic!("expected descendant Alternative Move, got {completion:?}"),
    };
    let inspected = transport
        .submit(
            "plan-isolation-child-inspect",
            ReviewSessionCommand::InspectPosition {
                game_import_id: target.game_import_id.clone(),
                review_moment_id: target.review_moment_id.clone(),
                target: PositionInspectionTarget::AlternativeMove {
                    alternative_move_id: child.alternative_move_id.clone(),
                },
            },
        )
        .await;
    let child_context = match completion(&inspected) {
        OperationCompletion::PositionInspected { inspection } => inspection.context.clone(),
        completion => panic!("expected descendant Position inspection, got {completion:?}"),
    };
    let child_target = CoachTarget {
        game_import_id: target.game_import_id,
        review_moment_id: target.review_moment_id,
        context: child_context,
    };
    let child_facts = start_prepared_turn(
        &mut transport,
        &child_target,
        CoachTurnId::try_from("coach-turn:journey:plan-isolation:child".to_string()).unwrap(),
        key("plan-isolation-child-turn"),
        PriorCoachTurn::None,
        "plan-isolation-child-turn",
    )
    .await;
    assert_eq!(child_facts.ancestor_branch.len(), 2);
    assert_objective_coach_facts(&child_facts, &evaluation_text);
}

#[tokio::test]
async fn prepared_coach_turn_replays_by_operation_without_new_work() {
    let checkpoints = Arc::new(DurableCoachCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports, checkpoints.clone());
    let mut first_transport = JourneySurface::jsonl(first_processor);
    let target = prepared_coach_target(&mut first_transport, "prepared-replay").await;
    let coach_turn_id =
        CoachTurnId::try_from("coach-turn:journey:prepared-replay".to_string()).unwrap();
    let operation_key = key("prepared-replay");
    let first = start_prepared_turn(
        &mut first_transport,
        &target,
        coach_turn_id.clone(),
        operation_key.clone(),
        PriorCoachTurn::None,
        "prepared-replay",
    )
    .await;
    let writes_after_preparation = checkpoints.replace_attempts.load(Ordering::SeqCst);
    let mut second_transport = first_transport;
    let replayed = start_prepared_turn(
        &mut second_transport,
        &target,
        coach_turn_id,
        operation_key,
        PriorCoachTurn::None,
        "prepared-replay",
    )
    .await;

    assert_eq!(replayed, first);
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        writes_after_preparation,
        "replaying the same operation must not prepare or persist twice"
    );
}

#[tokio::test]
/// Coaching is ephemeral, so losing the process loses the in-flight Coach Turn.
///
/// What matters to the Player is that the loss is total and clean: the next
/// process has no active authority to reconcile, admits a fresh turn with no
/// prior, and aims it at the same Review Moment.
#[allow(clippy::too_many_lines)]
async fn process_loss_discards_active_authority_and_a_fresh_turn_keeps_its_target() {
    let checkpoints = Arc::new(DurableCoachCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let (first_processor, blocking_human) =
        processor_with_blocking_coach_turn(game_imports.clone(), checkpoints.clone());
    let mut setup_transport = JourneySurface::jsonl(first_processor.clone());
    let target = prepared_coach_target(&mut setup_transport, "interrupted-coach").await;
    let interrupted_id =
        CoachTurnId::try_from("coach-turn:journey:interrupted".to_string()).unwrap();
    let mut interrupted_context = target.context.clone();
    interrupted_context.coach_turn_id = interrupted_id.clone();
    let command = ReviewSessionCommand::StartCoachTurn {
        game_import_id: target.game_import_id.clone(),
        review_moment_id: target.review_moment_id.clone(),
        coach_turn_id: interrupted_id.clone(),
        context: Box::new(interrupted_context),
        message: "Assess this branch before the process disappears.".to_string(),
        idempotency_key: key("interrupted-coach-operation"),
        prior_turn: PriorCoachTurn::None,
    };
    let mut receiver = ReviewSessionProcessor::submit(
        &first_processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &serde_json::to_vec(&transport_support::envelope(
            DeliverySurface::CoachSkill,
            "interrupted-coach-start",
            command,
        ))
        .unwrap(),
    );
    let mut accepted = false;
    while let Some(event) = receiver.recv().await {
        if matches!(
            event.event,
            ReviewSessionEvent::Accepted {
                operation: OperationKind::CoachTurn,
                ..
            }
        ) {
            accepted = true;
            break;
        }
    }
    assert!(
        accepted,
        "active authority must be durable before acknowledgement"
    );
    blocking_human.wait_until_started().await;
    drop(receiver);
    drop(setup_transport);

    let second_processor = processor_with_stores(game_imports, checkpoints.clone());
    let mut second_transport = JourneySurface::jsonl(second_processor);
    let resumed = second_transport
        .submit(
            "interrupted-coach-resume",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: target.game_import_id.clone(),
            },
        )
        .await;
    assert!(matches!(
        completion(&resumed),
        OperationCompletion::ReviewSessionStarted { .. }
    ));

    let retry_id =
        CoachTurnId::try_from("coach-turn:journey:interrupted:retry".to_string()).unwrap();
    let retried = start_prepared_turn(
        &mut second_transport,
        &target,
        retry_id,
        key("interrupted-coach-retry-operation"),
        PriorCoachTurn::None,
        "interrupted-coach-retry",
    )
    .await;
    assert_eq!(
        retried.context.target, target.context.target,
        "a fresh turn must aim at the same Review Moment"
    );
    let _ = interrupted_id;
}

#[tokio::test]
/// A steering write that cannot be persisted fails both turns and takes the
/// runtime with it, and coaching is ephemeral, so the prior authority goes too.
/// The Player's next question is admitted as a fresh turn at the same target
/// rather than as a retry of something that no longer exists.
async fn failed_steering_persistence_discards_the_prior_authority_and_admits_a_fresh_turn() {
    let checkpoints = Arc::new(DurableCoachCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let (first_processor, blocking_human) =
        processor_with_blocking_coach_turn(game_imports.clone(), checkpoints.clone());
    let mut setup_transport = JourneySurface::jsonl(first_processor.clone());
    let target = prepared_coach_target(&mut setup_transport, "failed-steering").await;
    let prior_id = CoachTurnId::try_from("coach-turn:journey:steering:prior".to_string()).unwrap();
    let prior_key = key("failed-steering-prior");
    let mut prior_context = target.context.clone();
    prior_context.coach_turn_id = prior_id.clone();
    let mut prior_receiver = ReviewSessionProcessor::submit(
        &first_processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &serde_json::to_vec(&transport_support::envelope(
            DeliverySurface::CoachSkill,
            "failed-steering-prior",
            ReviewSessionCommand::StartCoachTurn {
                game_import_id: target.game_import_id.clone(),
                review_moment_id: target.review_moment_id.clone(),
                coach_turn_id: prior_id.clone(),
                context: Box::new(prior_context),
                message: "Keep assessing until a steering message arrives.".to_string(),
                idempotency_key: prior_key,
                prior_turn: PriorCoachTurn::None,
            },
        ))
        .unwrap(),
    );
    while let Some(event) = prior_receiver.recv().await {
        if matches!(
            event.event,
            ReviewSessionEvent::Accepted {
                operation: OperationKind::CoachTurn,
                ..
            }
        ) {
            break;
        }
    }
    blocking_human.wait_until_started().await;
    checkpoints.fail_next_replace.store(true, Ordering::SeqCst);

    let replacement_id =
        CoachTurnId::try_from("coach-turn:journey:steering:replacement".to_string()).unwrap();
    let mut replacement_context = target.context.clone();
    replacement_context.coach_turn_id = replacement_id.clone();
    let failed_replacement = transport_support::collect_receiver(ReviewSessionProcessor::submit(
        &first_processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &serde_json::to_vec(&transport_support::envelope(
            DeliverySurface::CoachSkill,
            "failed-steering-replacement",
            ReviewSessionCommand::StartCoachTurn {
                game_import_id: target.game_import_id.clone(),
                review_moment_id: target.review_moment_id.clone(),
                coach_turn_id: replacement_id,
                context: Box::new(replacement_context),
                message: "Steer toward the same target with a sharper question.".to_string(),
                idempotency_key: key("failed-steering-replacement"),
                prior_turn: PriorCoachTurn::Steers {
                    coach_turn_id: prior_id.clone(),
                },
            },
        ))
        .unwrap(),
    ))
    .await;
    assert!(!failed_replacement.iter().any(|event| matches!(
        event.event,
        ReviewSessionEvent::Accepted {
            operation: OperationKind::CoachTurn,
            ..
        }
    )));
    assert!(matches!(
        failed_replacement.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::CoachTurn,
            reason: ProviderUnavailableReason::Persistence,
            retry: RetryDirective::RetryAllowed,
        })
    ));
    let prior_terminal = transport_support::collect_receiver(prior_receiver).await;
    assert!(matches!(
        prior_terminal.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::CoachTurn,
            reason: ProviderUnavailableReason::Persistence,
            retry: RetryDirective::RetryAllowed,
        })
    ));
    drop(setup_transport);

    let second_processor = processor_with_stores(game_imports, checkpoints);
    let mut second_transport = JourneySurface::jsonl(second_processor);
    let restarted = second_transport
        .submit(
            "failed-steering-restart",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: target.game_import_id.clone(),
            },
        )
        .await;
    assert!(matches!(
        completion(&restarted),
        OperationCompletion::ReviewSessionStarted { .. }
    ));
    let retry_id = CoachTurnId::try_from("coach-turn:journey:steering:retry".to_string()).unwrap();
    let retried = start_prepared_turn(
        &mut second_transport,
        &target,
        retry_id,
        key("failed-steering-retry"),
        PriorCoachTurn::None,
        "failed-steering-retry",
    )
    .await;
    assert_eq!(retried.context.target, target.context.target);
    let _ = prior_id;
}

fn processor_with_blocking_coach_turn(
    game_imports: Arc<dyn GameImportStore>,
    checkpoints: Arc<dyn ReviewAnalysisCacheStore>,
) -> (
    Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    Arc<processor_support::RecordingHuman>,
) {
    let recording = processor_support::provider_recording();
    let human = Arc::new(processor_support::RecordingHuman::new(&recording, true));
    let processor = ReviewSessionProcessor::new(
        processor_support::CapturedLichess::new(),
        recording.clone(),
        Arc::new(processor_support::RecordingEngine::new(&recording)),
        human.clone(),
        Arc::new(processor_support::GroundedAuthor),
    )
    .unwrap()
    .with_game_import_store(game_imports)
    .with_review_analysis_cache(checkpoints);
    (Arc::new(processor), human)
}

async fn prepared_coach_target(transport: &mut JourneySurface, label: &str) -> CoachTarget {
    let imported = transport
        .submit(&format!("{label}-import"), import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = transport
        .submit(
            &format!("{label}-session"),
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (game_import_id, moments) = started_session(&started);
    let moment = moments.first().unwrap();
    let explored = transport
        .submit(
            &format!("{label}-explore"),
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci {
                    uci: first_legal_uci(&moment.position_snapshot.fen),
                },
                idempotency_key: key(&format!("{label}-explore")),
            },
        )
        .await;
    let alternative = match completion(&explored) {
        OperationCompletion::AlternativeMoveEvaluated { alternative_move } => {
            alternative_move.as_ref()
        }
        completion => panic!("expected Alternative Move completion, got {completion:?}"),
    };
    let inspected = transport
        .submit(
            &format!("{label}-inspect"),
            ReviewSessionCommand::InspectPosition {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment.review_moment.moment_id.clone(),
                target: PositionInspectionTarget::AlternativeMove {
                    alternative_move_id: alternative.alternative_move_id.clone(),
                },
            },
        )
        .await;
    let context = match completion(&inspected) {
        OperationCompletion::PositionInspected { inspection } => inspection.context.clone(),
        completion => panic!("expected Position Inspection completion, got {completion:?}"),
    };
    CoachTarget {
        game_import_id,
        review_moment_id: moment.review_moment.moment_id.clone(),
        context,
    }
}

async fn start_prepared_turn(
    transport: &mut JourneySurface,
    target: &CoachTarget,
    coach_turn_id: CoachTurnId,
    idempotency_key: IdempotencyKey,
    prior_turn: PriorCoachTurn,
    label: &str,
) -> CoachTurnFacts {
    let mut context = target.context.clone();
    context.coach_turn_id = coach_turn_id.clone();
    let events = transport
        .submit(
            label,
            ReviewSessionCommand::StartCoachTurn {
                game_import_id: target.game_import_id.clone(),
                review_moment_id: target.review_moment_id.clone(),
                coach_turn_id,
                context: Box::new(context),
                message: format!("Assess this branch for {label}."),
                idempotency_key,
                prior_turn,
            },
        )
        .await;
    match completion(&events) {
        OperationCompletion::CoachTurnPrepared { facts } => facts.as_ref().clone(),
        completion => panic!("expected prepared Coach Turn, got {completion:?}"),
    }
}

async fn publish_turn(
    transport: &mut JourneySurface,
    target: &CoachTarget,
    coach_turn_id: CoachTurnId,
    assessment: AlternativeMoveAssessment,
    idempotency_key: IdempotencyKey,
    label: &str,
) -> Vec<ReviewSessionEventEnvelope> {
    transport
        .submit(
            label,
            ReviewSessionCommand::PublishCoachTurn {
                game_import_id: target.game_import_id.clone(),
                review_moment_id: target.review_moment_id.clone(),
                coach_turn_id,
                assessment: Box::new(assessment),
                idempotency_key,
            },
        )
        .await
}

fn published_assessment(events: &[ReviewSessionEventEnvelope]) -> AlternativeMoveAssessment {
    match completion(events) {
        OperationCompletion::CoachTurnCompleted { assessment } => assessment.as_ref().clone(),
        completion => panic!("expected a published Coach Turn, got {completion:?}"),
    }
}

fn assessment(facts: &CoachTurnFacts, label: &str) -> AlternativeMoveAssessment {
    // The label rides in the marker-free prose so each journey's assessment
    // stays distinguishable while still passing the gate.
    let dimension = |explanation: &str, evidence_refs| AssessmentDimension {
        explanation: format!("{explanation} Recorded for {label}."),
        evidence_refs,
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

fn assert_objective_coach_facts(facts: &CoachTurnFacts, forbidden_text: &str) {
    let wire = serde_json::to_string(facts).expect("Coach Turn facts serialize");
    assert!(!wire.to_ascii_lowercase().contains("intent"));
    assert!(!wire.to_ascii_lowercase().contains("hypothesis"));
    if !forbidden_text.is_empty() {
        assert!(!wire.contains(forbidden_text));
    }
    assert!(facts.evidence_packet.entries.iter().all(|entry| matches!(
        entry,
        EvidenceEntry::Position { .. }
            | EvidenceEntry::EngineAnalysis { .. }
            | EvidenceEntry::HumanMoveModel { .. }
            | EvidenceEntry::Branch { .. }
    )));
}

fn key(label: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(format!("idempotency-key:journey:{label}")).unwrap()
}
