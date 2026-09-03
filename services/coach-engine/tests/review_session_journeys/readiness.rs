use super::*;

#[tokio::test]
async fn acknowledged_session_resumes_after_processor_restart_with_its_retained_import() {
    let checkpoints = Arc::new(InMemoryReviewAnalysisCache::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let imported = surface_events(
        &first_processor,
        DeliverySurface::CoachSkill,
        "restart-import",
        import_command(),
    )
    .await;
    let game_import_id = imported_game_id(&imported);
    let started = surface_events(
        &first_processor,
        DeliverySurface::CoachSkill,
        "restart-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    let (game_import_id, started_moments) = started_readiness(&started);
    drop(first_processor);

    let second_processor = processor_with_stores(game_imports, checkpoints);
    let resumed = surface_events(
        &second_processor,
        DeliverySurface::CoachSkill,
        "restart-resume",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    let (resumed_id, resumed_moments) = resumed_readiness(&resumed);

    assert_eq!(resumed_id, game_import_id);
    assert_eq!(resumed_moments, started_moments);
    assert!(resumed_moments
        .windows(2)
        .all(|window| window[0].review_moment.ply < window[1].review_moment.ply));

    let repeated = surface_events(
        &second_processor,
        DeliverySurface::CoachSkill,
        "restart-resume-repeat",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    assert_eq!(
        resumed_readiness(&repeated),
        (game_import_id.clone(), started_moments.clone())
    );

    let repeated_start = surface_events(
        &second_processor,
        DeliverySurface::CoachSkill,
        "restart-start-repeat",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    let (next_session_id, next_moments) = started_readiness(&repeated_start);
    assert_eq!(
        next_session_id, game_import_id,
        "the address is the handle, so starting again names the same review"
    );
    assert_eq!(
        next_moments
            .iter()
            .map(|moment| &moment.review_moment)
            .collect::<Vec<_>>(),
        started_moments
            .iter()
            .map(|moment| &moment.review_moment)
            .collect::<Vec<_>>(),
        "a rebuilt session must remain grounded in the frozen import"
    );
}

#[tokio::test]
async fn restart_fails_closed_for_missing_unauthorized_and_mismatched_imports() {
    let checkpoints = Arc::new(ControlledReferenceCheckpointStore::default());
    let game_imports = Arc::new(ControlledReferenceGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let imported = surface_events(
        &first_processor,
        DeliverySurface::CoachApp,
        "reference-failure-import",
        import_command(),
    )
    .await;
    let started = surface_events(
        &first_processor,
        DeliverySurface::CoachApp,
        "reference-failure-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: imported_game_id(&imported),
        },
    )
    .await;
    let (game_import_id, admitted) = started_readiness(&started);
    drop(first_processor);

    for (mode, label, expected) in [
        (1, "missing", CommandRejectionReason::UnknownGameImport),
        (2, "unauthorized", CommandRejectionReason::UnknownGameImport),
    ] {
        game_imports.mode.store(mode, Ordering::SeqCst);
        let processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
        let events = surface_events(
            &processor,
            DeliverySurface::CoachApp,
            &format!("reference-failure-{label}"),
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
        assert!(
            matches!(
                events.last().map(|event| &event.event),
                Some(ReviewSessionEvent::Rejected {
                    operation: OperationKind::ReviewSessionStart,
                    reason,
                    ..
                }) if reason == &expected
            ),
            "{events:#?}"
        );
    }

    game_imports.mode.store(0, Ordering::SeqCst);
    let processor = processor_with_stores(game_imports, checkpoints);
    let resumed = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "reference-failure-recovered",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    assert_eq!(resumed_readiness(&resumed), (game_import_id, admitted));
}

#[tokio::test]
async fn coach_app_presents_and_restores_the_display_set_without_eager_intent_enrichment() {
    let checkpoints = Arc::new(CountingCheckpointStore::available());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let engine = Arc::new(CountingAfterImportEngine::new());
    let first_processor = live_processor_with_engine_and_stores(
        engine.clone(),
        game_imports.clone(),
        checkpoints.clone(),
    );
    let imported = surface_events(
        &first_processor,
        DeliverySurface::CoachApp,
        "display-admission-import",
        import_command(),
    )
    .await;
    let game_import_id = imported_game_id(&imported);

    let started = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        surface_events(
            &first_processor,
            DeliverySurface::CoachApp,
            "display-admission-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        ),
    )
    .await
    .expect("Coach App admission must not wait for rich authoring");
    let (game_import_id, admitted) = started_readiness(&started);

    assert!(!admitted.is_empty());
    assert!(admitted
        .iter()
        .all(|moment| matches!(moment.authoring, ReviewMomentAuthoringReadiness::Pending)));
    assert!(admitted
        .windows(2)
        .all(|window| window[0].review_moment.ply < window[1].review_moment.ply));
    assert_eq!(
        engine.preparation_calls.load(Ordering::SeqCst),
        0,
        "session admission and objective prefetch must not run Intent Enrichment"
    );
    assert_eq!(
        checkpoints.replaces.load(Ordering::SeqCst),
        0,
        "objective prefetch must not persist or advance the presented revision"
    );
    drop(first_processor);

    let second_processor = processor_with_stores(game_imports, checkpoints);
    let resumed = surface_events(
        &second_processor,
        DeliverySurface::CoachApp,
        "display-admission-resume",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;

    assert_eq!(resumed_readiness(&resumed), (game_import_id, admitted));
}

#[tokio::test]
async fn one_opened_coach_app_moment_restores_as_mixed_readiness_without_peer_drift() {
    let checkpoints = Arc::new(InMemoryReviewAnalysisCache::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let imported = surface_events(
        &first_processor,
        DeliverySurface::CoachApp,
        "mixed-readiness-import",
        import_command(),
    )
    .await;
    let started = surface_events(
        &first_processor,
        DeliverySurface::CoachApp,
        "mixed-readiness-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: imported_game_id(&imported),
        },
    )
    .await;
    let (game_import_id, admitted) = started_readiness(&started);
    assert!(admitted.len() >= 2);
    let target = admitted[0].review_moment.clone();

    let opened = surface_events(
        &first_processor,
        DeliverySurface::CoachApp,
        "mixed-readiness-open",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection: target.selection.clone(),
            idempotency_key: idempotency_key("mixed-readiness-open"),
        },
    )
    .await;
    let opened_core = match completion(&opened) {
        OperationCompletion::ReviewMomentOpened { review_moment, .. } => review_moment.as_ref(),
        completion => panic!("expected Review Moment completion, got {completion:?}"),
    };
    assert_eq!(opened_core.review_moment, target);
    drop(first_processor);

    let second_processor = processor_with_stores(game_imports, checkpoints.clone());
    let resumed = surface_events(
        &second_processor,
        DeliverySurface::CoachApp,
        "mixed-readiness-resume",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    let (resumed_id, restored) = resumed_readiness(&resumed);

    assert_eq!(resumed_id, game_import_id);
    assert_eq!(restored.len(), admitted.len());
    assert_eq!(
        restored
            .iter()
            .filter(|moment| {
                matches!(
                    moment.authoring,
                    ReviewMomentAuthoringReadiness::Prepared { .. }
                )
            })
            .count(),
        1
    );
    assert_eq!(
        restored
            .iter()
            .map(|moment| (&moment.review_moment, &moment.position_snapshot))
            .collect::<Vec<_>>(),
        admitted
            .iter()
            .map(|moment| (&moment.review_moment, &moment.position_snapshot))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn intent_enrichment_is_lazy_applicable_ephemeral_and_provider_minimal() {
    let engine = Arc::new(CountingEngine::new());
    let human = Arc::new(CountingHuman::new());
    let processor = Arc::new(
        ReviewSessionProcessor::new_live_with_authors(
            processor_support::CapturedLichess::new(),
            engine.clone(),
            human.clone(),
            Arc::new(processor_support::GroundedAuthor),
        )
        .with_game_import_store(Arc::new(InMemoryGameImportStore::default()))
        .with_review_analysis_cache(Arc::new(InMemoryReviewAnalysisCache::default())),
    );
    let imported = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "lazy-intent-import",
        import_command(),
    )
    .await;
    let baseline = (engine.calls(), human.calls());
    let started = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "lazy-intent-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: imported_game_id(&imported),
        },
    )
    .await;
    let (game_import_id, admitted) = started_readiness(&started);
    let review = match completion(&started) {
        OperationCompletion::ReviewSessionStarted { review, .. } => review.as_ref(),
        result => panic!("expected Review Session start, got {result:?}"),
    };
    assert_eq!(
        (engine.calls(), human.calls()),
        baseline,
        "session start must not call Stockfish or Maia for intent enrichment"
    );

    let selection_for = |kind: fn(&GameReviewMomentClassification) -> bool| {
        let moment = review
            .critical_moments
            .iter()
            .find(|moment| kind(&moment.classification))
            .expect("canonical journey contains the requested Review Moment kind");
        admitted
            .iter()
            .find(|candidate| candidate.review_moment.moment_id == moment.critical_moment_id)
            .expect("started Review Session contains the imported Review Moment")
            .review_moment
            .selection
            .clone()
    };
    let applicable = selection_for(|classification| {
        matches!(
            classification,
            GameReviewMomentClassification::PositiveHighlight { .. }
                | GameReviewMomentClassification::ImprovementOpportunity { .. }
        )
    });
    let applicable_opened = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "lazy-intent-applicable",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id,
            selection: applicable,
            idempotency_key: idempotency_key("lazy-intent-applicable"),
        },
    )
    .await;
    match completion(&applicable_opened) {
        OperationCompletion::ReviewMomentOpened {
            authoring_context: Some(context),
            comment: Some(comment),
            ..
        } => {
            let intent = context
                .intent
                .as_ref()
                .expect("applicable Review Moment receives intent instructions");
            let enrichment = intent
                .enrichment
                .as_ref()
                .expect("recorded providers produce intent enrichment");
            assert_eq!(enrichment.projected_plan_san.len(), 4);
            assert!(!enrichment.objective_counterplay_san.is_empty());
            assert_ne!(
                enrichment.projected_plan_san, enrichment.objective_counterplay_san,
                "Objective Counterplay must be authored independently from the selected Projected Plan"
            );
            assert_eq!(comment.text.matches("My best guess").count(), 1);
            let serialized_intent = serde_json::to_string(intent).unwrap();
            for forbidden in [
                "candidate",
                "probability",
                "score",
                "confidence",
                "provider",
                "trace",
            ] {
                assert!(!serialized_intent.to_ascii_lowercase().contains(forbidden));
            }
        }
        result => panic!("expected an applicable Review Moment opening, got {result:?}"),
    }
    assert!(engine.calls() > baseline.0);
    assert!(human.calls() > baseline.1);
}

#[tokio::test]
async fn player_plan_evaluation_is_grounded_and_leaves_the_session_unchanged() {
    let processor = processor_with_stores(
        Arc::new(InMemoryGameImportStore::default()),
        Arc::new(InMemoryReviewAnalysisCache::default()),
    );
    let imported = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "player-plan-import",
        import_command(),
    )
    .await;
    let started = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "player-plan-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: imported_game_id(&imported),
        },
    )
    .await;
    let (game_import_id, moments) = started_readiness(&started);
    let target = moments
        .first()
        .expect("canonical journey contains a Review Moment")
        .review_moment
        .clone();
    let opened = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "player-plan-open",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection: target.selection.clone(),
            idempotency_key: idempotency_key("player-plan-open"),
        },
    )
    .await;
    assert!(matches!(
        completion(&opened),
        OperationCompletion::ReviewMomentOpened { .. }
    ));
    let target = target.moment_id;
    let before = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "player-plan-before",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;

    let prepared = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "player-plan-prepare",
        ReviewSessionCommand::EvaluatePlayerPlan {
            game_import_id: game_import_id.clone(),
            review_moment_id: target.clone(),
            request: PlayerPlanEvaluationRequest::Prepare,
        },
    )
    .await;
    let context = match completion(&prepared) {
        OperationCompletion::PlayerPlanEvaluationPrepared { context } => context.as_ref(),
        result => panic!("expected Player Plan Evaluation facts, got {result:?}"),
    };
    let counterplay = context
        .facts
        .objective_counterplay_san
        .first()
        .expect("Player Plan Evaluation requires Objective Counterplay");
    let text = format!(
        "{} supports the stated plan, but {counterplay} is the concrete counterplay.",
        context.facts.reviewed_move_san
    );
    let evaluated = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "player-plan-admit",
        ReviewSessionCommand::EvaluatePlayerPlan {
            game_import_id: game_import_id.clone(),
            review_moment_id: target.clone(),
            request: PlayerPlanEvaluationRequest::Admit {
                draft: PlayerPlanEvaluationDraft {
                    facts_ref: context.facts_ref.clone(),
                    text: text.clone(),
                },
            },
        },
    )
    .await;
    assert!(matches!(
        completion(&evaluated),
        OperationCompletion::PlayerPlanEvaluated {
            evaluation: PlayerPlanEvaluation { text: admitted }
        } if admitted == &text
    ));

    let invented = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "player-plan-invented",
        ReviewSessionCommand::EvaluatePlayerPlan {
            game_import_id: game_import_id.clone(),
            review_moment_id: target,
            request: PlayerPlanEvaluationRequest::Admit {
                draft: PlayerPlanEvaluationDraft {
                    facts_ref: context.facts_ref.clone(),
                    text: "The plan works because Qh4 wins immediately.".to_string(),
                },
            },
        },
    )
    .await;
    assert!(matches!(
        invented.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            operation: OperationKind::PlayerPlanEvaluation,
            reason: CommandRejectionReason::InvalidCommand,
            ..
        })
    ));

    let after = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "player-plan-after",
        ReviewSessionCommand::StartReviewSession { game_import_id },
    )
    .await;
    assert_eq!(
        resumed_readiness(&after),
        resumed_readiness(&before),
        "Player Plan Evaluation must not change Review Session state or revision"
    );
}

#[tokio::test]
async fn objective_authoring_preparation_is_scoped_retryable_and_cached() {
    let checkpoints = Arc::new(FailNthReplacementCheckpointStore::new(1));
    let engine = Arc::new(CountingAfterImportEngine::new());
    let processor = live_processor_with_engine_and_stores(
        engine.clone(),
        Arc::new(InMemoryGameImportStore::default()),
        checkpoints.clone(),
    );
    let imported = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "on-demand-import",
        import_command(),
    )
    .await;
    let started = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "on-demand-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: imported_game_id(&imported),
        },
    )
    .await;
    let (game_import_id, admitted) = started_readiness(&started);
    assert!(admitted.len() >= 2);
    let target = admitted[0].review_moment.clone();

    let failed = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "on-demand-failed",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection: target.selection.clone(),
            idempotency_key: idempotency_key("on-demand-failed"),
        },
    )
    .await;
    assert_eq!(
        preparation_stages(&failed, &game_import_id, &target.moment_id),
        vec![
            ReviewMomentPreparationProgressStage::WaitingForCapacity,
            ReviewMomentPreparationProgressStage::PreparingAuthoringContext,
            ReviewMomentPreparationProgressStage::CommittingAuthoringContext,
        ]
    );
    assert!(matches!(
        failed.last().map(|event| &event.event),
        Some(ReviewSessionEvent::ReviewMomentUnavailable {
            game_import_id: failed_session_id,
            review_moment_id,
            reason: ProviderUnavailableReason::Persistence,
            retry: RetryDirective::RetryAllowed,
        }) if failed_session_id == &game_import_id && review_moment_id == &target.moment_id
    ));
    let failed_preparation_calls = engine.preparation_calls.load(Ordering::SeqCst);
    assert_eq!(
        failed_preparation_calls, 0,
        "retained objective evidence must prepare without Intent Enrichment"
    );
    assert_eq!(checkpoints.replace_attempts.load(Ordering::SeqCst), 1);

    let retried = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "on-demand-retry",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection: target.selection.clone(),
            idempotency_key: idempotency_key("on-demand-retry"),
        },
    )
    .await;
    assert_eq!(
        preparation_stages(&retried, &game_import_id, &target.moment_id),
        vec![
            ReviewMomentPreparationProgressStage::WaitingForCapacity,
            ReviewMomentPreparationProgressStage::PreparingAuthoringContext,
            ReviewMomentPreparationProgressStage::CommittingAuthoringContext,
        ]
    );
    let prepared = match completion(&retried) {
        OperationCompletion::ReviewMomentOpened {
            game_import_id: completed_session_id,
            review_moment,
            critical_moment,
            ..
        } => {
            assert_eq!(completed_session_id, &game_import_id);
            assert_eq!(review_moment.review_moment, target);
            assert_eq!(critical_moment.critical_moment_id, target.moment_id);
            completion(&retried).clone()
        }
        completion => panic!("expected Review Moment completion, got {completion:?}"),
    };
    let preparation_calls = engine.preparation_calls.load(Ordering::SeqCst);
    let replacements = checkpoints.replace_attempts.load(Ordering::SeqCst);
    assert!(
        preparation_calls > failed_preparation_calls,
        "Intent Enrichment must run only after objective preparation commits successfully"
    );
    assert_eq!(replacements, 2);

    // The admitted set left the per-moment open payload, so the scope of what
    // one open prepared is read back from the Review Session.
    let (_, live_readiness) = resumed_readiness(
        &surface_events(
            &processor,
            DeliverySurface::CoachApp,
            "on-demand-scope",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await,
    );
    assert_eq!(
        live_readiness
            .iter()
            .map(|moment| (&moment.review_moment, &moment.position_snapshot))
            .collect::<Vec<_>>(),
        admitted
            .iter()
            .map(|moment| (&moment.review_moment, &moment.position_snapshot))
            .collect::<Vec<_>>()
    );
    assert!(live_readiness.iter().all(|moment| {
        if moment.review_moment.moment_id == target.moment_id {
            matches!(
                moment.authoring,
                ReviewMomentAuthoringReadiness::Prepared { .. }
            )
        } else {
            matches!(moment.authoring, ReviewMomentAuthoringReadiness::Pending)
        }
    }));

    let cached = surface_events(
        &processor,
        DeliverySurface::CoachApp,
        "on-demand-cached",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id,
            selection: target.selection,
            idempotency_key: idempotency_key("on-demand-cached"),
        },
    )
    .await;
    assert!(preparation_stages(
        &cached,
        match &prepared {
            OperationCompletion::ReviewMomentOpened { game_import_id, .. } => game_import_id,
            _ => unreachable!(),
        },
        &target.moment_id,
    )
    .is_empty());
    let mut cached_completion = completion(&cached).clone();
    match (&prepared, &mut cached_completion) {
        (
            OperationCompletion::ReviewMomentOpened {
                revision_delta: prepared_delta,
                ..
            },
            OperationCompletion::ReviewMomentOpened {
                session_revision,
                revision_delta,
                ..
            },
        ) => {
            assert_eq!(revision_delta.prior_revision, *session_revision);
            assert_eq!(revision_delta.resulting_revision, *session_revision);
            assert_eq!(
                revision_delta.changed_moment_ids,
                prepared_delta.changed_moment_ids
            );
            *revision_delta = prepared_delta.clone();
        }
        _ => unreachable!(),
    }
    assert_eq!(cached_completion, prepared);
    assert_eq!(
        engine.preparation_calls.load(Ordering::SeqCst),
        preparation_calls,
        "cached objective preparation must not run Intent Enrichment"
    );
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        replacements,
        "cached preparation must not repeat persistence"
    );
}

#[tokio::test]
async fn failed_batch_preparation_preserves_admission_and_prior_prepared_peers() {
    let checkpoints = Arc::new(FailNthReplacementCheckpointStore::new(2));
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let imported = surface_events(
        &first_processor,
        DeliverySurface::Web,
        "partial-batch-import",
        import_command(),
    )
    .await;
    let game_import_id = imported_game_id(&imported);
    let failed = surface_events(
        &first_processor,
        DeliverySurface::Web,
        "partial-batch-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    assert!(matches!(
        failed.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::ReviewSessionStart,
            reason: ProviderUnavailableReason::Persistence,
            retry: RetryDirective::RetryAllowed,
        })
    ));
    assert_eq!(checkpoints.replace_attempts.load(Ordering::SeqCst), 2);
    drop(first_processor);

    let second_processor = processor_with_stores(game_imports, checkpoints);
    let resumed = surface_events(
        &second_processor,
        DeliverySurface::CoachApp,
        "partial-batch-start",
        ReviewSessionCommand::StartReviewSession { game_import_id },
    )
    .await;
    let (_, restored) = started_readiness(&resumed);
    let prepared = restored
        .iter()
        .filter(|moment| {
            matches!(
                moment.authoring,
                ReviewMomentAuthoringReadiness::Prepared { .. }
            )
        })
        .count();

    assert_eq!(prepared, 1);
    assert!(
        restored.len() > prepared,
        "the unprepared peers must remain durably admitted"
    );
}

fn started_readiness(
    events: &[ReviewSessionEventEnvelope],
) -> (GameImportId, Vec<ReviewSessionMoment>) {
    match completion(events) {
        OperationCompletion::ReviewSessionStarted {
            game_import_id,
            review_moments,
            ..
        } => (game_import_id.clone(), review_moments.clone()),
        completion => panic!("expected Review Session start completion, got {completion:?}"),
    }
}

fn resumed_readiness(
    events: &[ReviewSessionEventEnvelope],
) -> (GameImportId, Vec<ReviewSessionMoment>) {
    match completion(events) {
        OperationCompletion::ReviewSessionStarted {
            game_import_id,
            review_moments,
            ..
        } => (game_import_id.clone(), review_moments.clone()),
        completion => panic!("expected Review Session resume completion, got {completion:?}"),
    }
}

fn preparation_stages(
    events: &[ReviewSessionEventEnvelope],
    game_import_id: &GameImportId,
    review_moment_id: &CriticalMomentId,
) -> Vec<ReviewMomentPreparationProgressStage> {
    events
        .iter()
        .filter_map(|event| match &event.event {
            ReviewSessionEvent::Progress {
                stage:
                    OperationProgress::ReviewMomentPreparation {
                        game_import_id: observed_session_id,
                        review_moment_id: observed_review_moment_id,
                        stage,
                    },
            } if observed_session_id == game_import_id
                && observed_review_moment_id == review_moment_id =>
            {
                Some(*stage)
            }
            _ => None,
        })
        .collect()
}

#[derive(Default)]
struct ControlledReferenceGameImportStore {
    inner: InMemoryGameImportStore,
    /// 0 answers normally, 1 answers as a miss, 2 answers as another Player's.
    mode: AtomicUsize,
}

impl GameImportStore for ControlledReferenceGameImportStore {
    fn create<'a>(&'a self, record: GameImportRecord) -> GameImportStoreFuture<'a, ()> {
        self.inner.create(record)
    }

    fn create_with_imported_game_card<'a>(
        &'a self,
        record: GameImportRecord,
        card: ImportedGameCard,
    ) -> GameImportStoreFuture<'a, ()> {
        self.inner.create_with_imported_game_card(record, card)
    }

    fn upsert_imported_game_card<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        card: ImportedGameCard,
    ) -> GameImportStoreFuture<'a, ()> {
        self.inner.upsert_imported_game_card(owner, card)
    }

    fn list_imported_game_cards<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<ImportedGameCard>> {
        self.inner.list_imported_game_cards(owner)
    }

    fn list_game_import_records<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<GameImportRecord>> {
        self.inner.list_game_import_records(owner)
    }

    fn find<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
        game_import_id: &'a GameImportId,
    ) -> GameImportStoreFuture<'a, GameImportLookup> {
        match self.mode.load(Ordering::SeqCst) {
            1 => Box::pin(async { Ok(GameImportLookup::NotFound) }),
            2 => Box::pin(async { Ok(GameImportLookup::OwnerMismatch) }),
            _ => self.inner.find(owner, game_import_id),
        }
    }

    fn retain_for_review_session<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        self.inner.retain_for_review_session(owner, reference)
    }

    fn resolve_review_session_reference<'a>(
        &'a self,
        owner: &'a chen_chess_coach_engine::review_session_processor::ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        self.inner
            .resolve_review_session_reference(owner, reference)
    }
}

/// Answers the cache as unavailable on demand, so the review-build path can be
/// exercised when durable analysis cannot be read.
#[derive(Default)]
struct ControlledReferenceCheckpointStore {
    inner: InMemoryReviewAnalysisCache,
    mode: AtomicUsize,
}

impl ReviewAnalysisCacheStore for ControlledReferenceCheckpointStore {
    fn seed<'a>(&'a self, entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a> {
        self.inner.seed(entries)
    }

    fn load<'a>(
        &'a self,
        game_import_id: &'a GameImportId,
        game: &'a ReviewSessionGame,
        now: chrono::DateTime<Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>> {
        match self.mode.load(Ordering::SeqCst) {
            0 => self.inner.load(game_import_id, game, now),
            _ => Box::pin(async { Err(ReviewAnalysisCacheError::Unavailable) }),
        }
    }

    fn replace_moment<'a>(
        &'a self,
        mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a> {
        self.inner.replace_moment(mutation)
    }
}

struct FailNthReplacementCheckpointStore {
    inner: InMemoryReviewAnalysisCache,
    fail_at: usize,
    replace_attempts: AtomicUsize,
}

impl FailNthReplacementCheckpointStore {
    fn new(fail_at: usize) -> Self {
        Self {
            inner: InMemoryReviewAnalysisCache::default(),
            fail_at,
            replace_attempts: AtomicUsize::new(0),
        }
    }
}

impl ReviewAnalysisCacheStore for FailNthReplacementCheckpointStore {
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
            let attempt = self.replace_attempts.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt == self.fail_at {
                return Err(ReviewAnalysisCacheError::Unavailable);
            }
            self.inner.replace_moment(mutation).await
        })
    }
}
