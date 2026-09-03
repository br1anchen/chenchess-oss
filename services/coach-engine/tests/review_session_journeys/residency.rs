use super::*;

mod checkpoint_store;

use checkpoint_store::*;

/// A review this Player does not own has no session, and the two ways of not
/// owning one are deliberately different answers: an address that names another
/// Player is a mismatch, and an address that names nothing is a miss the model
/// is told to correct.
#[tokio::test]
async fn a_review_the_player_does_not_own_has_no_session() {
    let processor = processor_with_checkpoint_store(Arc::new(TrackingCheckpointStore::default()));
    let mut transport = JourneySurface::jsonl(processor);
    let unknown =
        GameImportId::try_from(format!("game-import:{}:{}", "a".repeat(64), "b".repeat(32)))
            .unwrap();

    let events = transport
        .submit(
            "unowned-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: unknown.clone(),
            },
        )
        .await;

    assert!(
        matches!(
            events.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Rejected {
                operation: OperationKind::ReviewSessionStart,
                reason: CommandRejectionReason::UnknownGameImport,
                recovery: RejectionRecovery::CorrectInput,
            })
        ),
        "{events:#?}"
    );
}

/// The cache is an optimization, never a dependency: a review whose analysis
/// cannot be read still opens, and every Review Moment is simply recomputed.
#[tokio::test]
async fn a_review_opens_when_cached_analysis_cannot_be_read() {
    let checkpoints = Arc::new(UnreadableCheckpointStore::default());
    let processor = processor_with_checkpoint_store(checkpoints.clone());
    let mut transport = JourneySurface::jsonl(processor);
    let imported = transport.submit("cacheless-import", import_command()).await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };

    let started = transport
        .submit(
            "cacheless-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;

    let (_, moments) = started_session(&started);
    assert!(!moments.is_empty());
    assert!(checkpoints.loads.load(Ordering::SeqCst) >= 1);
}

/// The start path is where a re-review becomes free: a second processor over the
/// same stores reads the Review Moments the first one prepared instead of
/// preparing them again.
#[tokio::test]
async fn a_second_start_over_the_same_cache_prepares_nothing() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let mut first_transport = JourneySurface::jsonl(first_processor);
    let imported = first_transport
        .submit("cache-hit-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = first_transport
        .submit(
            "cache-hit-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    let (_, first_moments) = started_session(&started);
    let seeds_after_first = checkpoints.seeds.load(Ordering::SeqCst);
    assert_eq!(
        seeds_after_first, 1,
        "the first review pays for the analysis"
    );
    drop(first_transport);

    let second_processor = processor_with_stores(game_imports, checkpoints.clone());
    let mut second_transport = JourneySurface::jsonl(second_processor);
    let restarted = second_transport
        .submit(
            "cache-hit-restart",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;

    let (_, second_moments) = started_session(&restarted);
    assert_eq!(second_moments, first_moments);
    assert_eq!(
        checkpoints.seeds.load(Ordering::SeqCst),
        seeds_after_first,
        "a review whose analysis is already cached must not seed it again"
    );
}

#[tokio::test]
async fn acknowledged_objective_mutations_restore_without_transient_authoring_state() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let mut first_transport = JourneySurface::jsonl(first_processor);
    let imported = first_transport
        .submit("durable-mutation-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = first_transport
        .submit(
            "durable-mutation-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (game_import_id, started_moments) = started_session(&started);
    let preparation_replacements = checkpoints.replace_attempts.load(Ordering::SeqCst);
    let automatic = started_moments
        .first()
        .expect("the canonical fixture has an Automatic Review Moment");

    let opened = first_transport
        .submit(
            "durable-player-moment",
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection: ReviewMomentSelection::PlayerSelectedMoment { ply: 49 },
                idempotency_key: idempotency_key("durable-player-moment"),
            },
        )
        .await;
    let opened_local_material = match completion(&opened) {
        OperationCompletion::ReviewMomentOpened {
            review_moment,
            critical_moment,
            ..
        } => {
            assert!(matches!(
                review_moment.review_moment.selection,
                ReviewMomentSelection::PlayerSelectedMoment { ply: 49 }
            ));
            critical_moment.learning_material.clone()
        }
        completion => panic!("expected Review Moment completion, got {completion:?}"),
    };

    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        preparation_replacements + 1
    );
    drop(first_transport);

    let second_processor = processor_with_stores(game_imports, checkpoints.clone());
    let mut second_transport = JourneySurface::jsonl(second_processor);
    let resumed = second_transport
        .submit(
            "durable-mutation-resume",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    let restored_states = match completion(&resumed) {
        OperationCompletion::ReviewSessionStarted { review_moments, .. } => review_moments.clone(),
        completion => panic!("expected started Review Session, got {completion:?}"),
    };
    let (_, restored_moments) = started_session(&resumed);
    assert_eq!(
        restored_moments
            .iter()
            .filter(|moment| matches!(
                moment.review_moment.selection,
                ReviewMomentSelection::PlayerSelectedMoment { ply: 49 }
            ))
            .count(),
        1
    );
    assert_eq!(
        restored_states
            .iter()
            .find(|moment| {
                matches!(
                    moment.review_moment.selection,
                    ReviewMomentSelection::PlayerSelectedMoment { ply: 49 }
                )
            })
            .expect("restoration retains the Player-selected display facts")
            .learning_material,
        opened_local_material
    );

    let reopened = second_transport
        .submit(
            "durable-player-moment-reopen",
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection: ReviewMomentSelection::PlayerSelectedMoment { ply: 49 },
                idempotency_key: idempotency_key("durable-player-moment-reopen"),
            },
        )
        .await;
    let restored_local = restored_states
        .iter()
        .find(|moment| {
            matches!(
                moment.review_moment.selection,
                ReviewMomentSelection::PlayerSelectedMoment { ply: 49 }
            )
        })
        .expect("restoration retains the Player-selected moment");
    match completion(&reopened) {
        OperationCompletion::ReviewMomentOpened {
            review_moment,
            critical_moment,
            ..
        } => {
            assert_eq!(
                review_moment.as_ref(),
                restored_local
                    .prepared_core()
                    .expect("restoration prepares the Player-selected moment"),
                "reopening a restored moment ships the restored core"
            );
            assert_eq!(critical_moment.learning_material, opened_local_material);
        }
        completion => panic!("expected reopened Review Moment, got {completion:?}"),
    }

    let inspected = second_transport
        .submit(
            "durable-objective-inspect",
            ReviewSessionCommand::InspectPosition {
                game_import_id,
                review_moment_id: automatic.review_moment.moment_id.clone(),
                target: PositionInspectionTarget::ReviewedMove,
            },
        )
        .await;
    assert!(matches!(
        completion(&inspected),
        OperationCompletion::PositionInspected { inspection }
            if !serde_json::to_string(&inspection.context)
                .expect("inspection context serializes")
                .contains("intent")
    ));
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        preparation_replacements + 1,
        "resume, reopen, and Position inspection are passive"
    );
}

#[tokio::test]
async fn resume_drops_neutral_player_selected_moments() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let mut first_transport = JourneySurface::jsonl(first_processor);
    let imported = first_transport
        .submit("neutral-walk-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    first_transport
        .submit(
            "neutral-walk-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;

    let opened = first_transport
        .submit(
            "neutral-walk-open",
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection: ReviewMomentSelection::PlayerSelectedMoment { ply: 2 },
                idempotency_key: idempotency_key("neutral-walk-open"),
            },
        )
        .await;
    match completion(&opened) {
        OperationCompletion::ReviewMomentOpened {
            critical_moment, ..
        } => {
            assert!(
                matches!(
                    critical_moment.classification,
                    GameReviewMomentClassification::Neutral { .. }
                ),
                "ply 2 must be Neutral so resume can prove the filter"
            );
        }
        completion => panic!("expected Review Moment completion, got {completion:?}"),
    }
    drop(first_transport);

    let second_processor = processor_with_stores(game_imports, checkpoints);
    let mut second_transport = JourneySurface::jsonl(second_processor);
    let resumed = second_transport
        .submit(
            "neutral-walk-resume",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (_, restored_moments) = started_session(&resumed);
    assert_eq!(
        restored_moments
            .iter()
            .filter(|moment| matches!(
                moment.review_moment.selection,
                ReviewMomentSelection::PlayerSelectedMoment { ply: 2 }
            ))
            .count(),
        0
    );
}

#[tokio::test]
async fn resume_keeps_an_instructive_player_selected_moment_with_its_classification() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let mut first_transport = JourneySurface::jsonl(first_processor);
    let imported = first_transport
        .submit("instructive-nominate-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    first_transport
        .submit(
            "instructive-nominate-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;

    let opened = first_transport
        .submit(
            "instructive-nominate-open",
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection: ReviewMomentSelection::PlayerSelectedMoment { ply: 49 },
                idempotency_key: idempotency_key("instructive-nominate-open"),
            },
        )
        .await;
    let opened_kind = match completion(&opened) {
        OperationCompletion::ReviewMomentOpened {
            critical_moment, ..
        } => {
            let kind = ReviewMomentClassificationKind::from(&critical_moment.classification);
            assert_ne!(
                kind,
                ReviewMomentClassificationKind::Neutral,
                "ply 49 must be instructive so resume can prove classification transport"
            );
            kind
        }
        completion => panic!("expected Review Moment completion, got {completion:?}"),
    };
    drop(first_transport);

    let second_processor = processor_with_stores(game_imports, checkpoints);
    let mut second_transport = JourneySurface::jsonl(second_processor);
    let resumed = second_transport
        .submit(
            "instructive-nominate-resume",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let restored = started_review_moments(&resumed)
        .into_iter()
        .find(|moment| {
            matches!(
                moment.review_moment.selection,
                ReviewMomentSelection::PlayerSelectedMoment { ply: 49 }
            )
        })
        .expect("resume keeps an instructive Player-Selected Moment");
    assert_eq!(restored.classification_kind, Some(opened_kind));
}

fn started_review_moments(events: &[ReviewSessionEventEnvelope]) -> Vec<ReviewSessionMoment> {
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewSessionStarted { review_moments, .. } => {
                    Some(review_moments.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("Review Session start should complete: {events:#?}"))
}

#[tokio::test]
async fn evaluated_alternative_move_restores_and_retries_without_a_duplicate_branch() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let first_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let mut first_transport = JourneySurface::jsonl(first_processor);
    let imported = first_transport
        .submit("durable-exploration-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = first_transport
        .submit(
            "durable-exploration-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (game_import_id, moments) = started_session(&started);
    let preparation_replacements = checkpoints.replace_attempts.load(Ordering::SeqCst);
    let moment = moments.first().unwrap();
    let idempotency_key =
        IdempotencyKey::try_from("idempotency-key:journey:durable-exploration".to_string())
            .unwrap();
    let move_uci = first_legal_uci(&moment.position_snapshot.fen);
    let explored = first_transport
        .submit(
            "durable-exploration",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci {
                    uci: move_uci.clone(),
                },
                idempotency_key: idempotency_key.clone(),
            },
        )
        .await;
    let alternative = match completion(&explored) {
        OperationCompletion::AlternativeMoveEvaluated { alternative_move } => {
            alternative_move.as_ref().clone()
        }
        completion => panic!("expected Alternative Move completion, got {completion:?}"),
    };
    let child_uci = first_legal_uci(&alternative.resulting_position.fen);
    let child = first_transport
        .submit(
            "durable-exploration-child",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Move {
                    branch_ref: alternative.branch_ref.clone(),
                },
                source_position_ref: alternative.resulting_position.position_ref.clone(),
                move_input: MoveInput::Uci { uci: child_uci },
                idempotency_key: IdempotencyKey::try_from(
                    "idempotency-key:journey:durable-exploration-child".to_string(),
                )
                .unwrap(),
            },
        )
        .await;
    let child = match completion(&child) {
        OperationCompletion::AlternativeMoveEvaluated { alternative_move } => {
            alternative_move.as_ref().clone()
        }
        completion => panic!("expected child Alternative Move completion, got {completion:?}"),
    };
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        preparation_replacements + 4
    );
    drop(first_transport);

    let second_processor = processor_with_stores(game_imports, checkpoints.clone());
    let mut second_transport = JourneySurface::jsonl(second_processor);
    let resumed = second_transport
        .submit(
            "durable-exploration-resume",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    started_session(&resumed);
    let inspected = second_transport
        .submit(
            "durable-exploration-inspect",
            ReviewSessionCommand::InspectPosition {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment.review_moment.moment_id.clone(),
                target: PositionInspectionTarget::AlternativeMove {
                    alternative_move_id: child.alternative_move_id.clone(),
                },
            },
        )
        .await;
    assert!(matches!(
        completion(&inspected),
        OperationCompletion::PositionInspected { inspection }
            if inspection.position_snapshot == child.resulting_position
    ));
    let repeated = second_transport
        .submit(
            "durable-exploration-repeat",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id,
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci { uci: move_uci },
                idempotency_key,
            },
        )
        .await;
    assert!(matches!(
        completion(&repeated),
        OperationCompletion::AlternativeMoveEvaluated { alternative_move }
            if alternative_move.as_ref() == &alternative
    ));
    assert!(matches!(
        repeated
            .iter()
            .rev()
            .nth(1)
            .map(|envelope| &envelope.event),
        Some(ReviewSessionEvent::Progress {
            stage: OperationProgress::AlternativeMoveAllowance { remaining },
        }) if *remaining == ReviewSessionLimits::V1.max_committed_alternative_moves - 2
    ));
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        preparation_replacements + 4,
        "resume, inspection, and an exact completed retry are passive"
    );
}

#[tokio::test]
/// Losing the process that accepted an exploration loses the exploration.
///
/// Nothing is written to close it — there is no durable operation to close —
/// and the Player simply asks again. What must not happen is the lost worker
/// committing late into the review the next process rebuilt.
#[allow(clippy::too_many_lines)]
async fn process_loss_discards_an_accepted_exploration_and_late_completion_cannot_commit() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let blocking_engine = Arc::new(BlockingAfterImportEngine::new());
    let first_processor = processor_with_engine_and_stores(
        blocking_engine.clone(),
        game_imports.clone(),
        checkpoints.clone(),
    );
    let mut first_transport = JourneySurface::jsonl(first_processor.clone());
    let imported = first_transport
        .submit("interrupted-exploration-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = first_transport
        .submit(
            "interrupted-exploration-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (game_import_id, moments) = started_session(&started);
    let preparation_replacements = checkpoints.replace_attempts.load(Ordering::SeqCst);
    let moment = moments.first().unwrap();
    let move_uci = first_legal_uci(&moment.position_snapshot.fen);
    let interrupted_key =
        IdempotencyKey::try_from("idempotency-key:journey:interrupted-exploration".to_string())
            .unwrap();
    let interrupted_envelope = transport_support::envelope(
        DeliverySurface::CoachSkill,
        "interrupted-exploration",
        ReviewSessionCommand::ExploreAlternativeMove {
            game_import_id: game_import_id.clone(),
            review_moment_id: moment.review_moment.moment_id.clone(),
            parent: BranchParent::Root {
                position_ref: moment.position_snapshot.position_ref.clone(),
            },
            source_position_ref: moment.position_snapshot.position_ref.clone(),
            move_input: MoveInput::Uci {
                uci: move_uci.clone(),
            },
            idempotency_key: interrupted_key.clone(),
        },
    );
    let mut interrupted_events = ReviewSessionProcessor::submit(
        &first_processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &serde_json::to_vec(&interrupted_envelope).unwrap(),
    );
    loop {
        let event = interrupted_events
            .recv()
            .await
            .expect("exploration emits an Accepted event");
        if matches!(
            event.event,
            ReviewSessionEvent::Accepted {
                operation: OperationKind::AlternativeMoveEvaluation,
                ..
            }
        ) {
            break;
        }
    }
    blocking_engine.wait_until_started().await;
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        preparation_replacements + 1
    );
    drop(first_transport);

    let second_processor = processor_with_stores(game_imports.clone(), checkpoints.clone());
    let mut second_transport = JourneySurface::jsonl(second_processor);
    let resumed = second_transport
        .submit(
            "interrupted-exploration-resume",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    started_session(&resumed);
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        preparation_replacements + 1,
        "an interrupted exploration died with its process, so nothing is written to close it"
    );

    let retry_key = IdempotencyKey::try_from(
        "idempotency-key:journey:interrupted-exploration-retry".to_string(),
    )
    .unwrap();
    let retried = second_transport
        .submit(
            "interrupted-exploration-retry",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci { uci: move_uci },
                idempotency_key: retry_key,
            },
        )
        .await;
    let alternative = match completion(&retried) {
        OperationCompletion::AlternativeMoveEvaluated { alternative_move } => {
            alternative_move.as_ref().clone()
        }
        completion => panic!("expected retried Alternative Move, got {completion:?}"),
    };

    blocking_engine.release();
    let late = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        transport_support::collect_receiver(interrupted_events),
    )
    .await
    .expect("late exploration releases within five seconds");
    // The lost worker finishes into the runtime that started it, and that
    // runtime is gone. What matters is that it cannot fork the review: the move
    // is a pure function of the position, so its result is the one the rebuilt
    // session already reached rather than a second branch.
    assert!(
        matches!(
            completion(&late),
            OperationCompletion::AlternativeMoveEvaluated { alternative_move }
                if alternative_move.branch_ref == alternative.branch_ref
        ),
        "late exploration must not fork the review, got {:?}",
        late.last().map(|event| &event.event)
    );

    let third_processor = processor_with_stores(game_imports, checkpoints);
    let mut third_transport = JourneySurface::jsonl(third_processor);
    let final_resumed = third_transport
        .submit(
            "interrupted-exploration-final-resume",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    assert!(matches!(
        completion(&final_resumed),
        OperationCompletion::ReviewSessionStarted { .. }
    ));
    let inspected = third_transport
        .submit(
            "interrupted-exploration-final-inspect",
            ReviewSessionCommand::InspectPosition {
                game_import_id,
                review_moment_id: moment.review_moment.moment_id.clone(),
                target: PositionInspectionTarget::AlternativeMove {
                    alternative_move_id: alternative.alternative_move_id,
                },
            },
        )
        .await;
    assert!(matches!(
        completion(&inspected),
        OperationCompletion::PositionInspected { .. }
    ));
}

#[tokio::test]
async fn explicit_exploration_cancellation_is_durable_and_releases_work_for_retry() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let blocking_engine = Arc::new(BlockingAfterImportEngine::new());
    let processor = processor_with_engine_and_stores(
        blocking_engine.clone(),
        game_imports.clone(),
        checkpoints.clone(),
    );
    let mut transport = JourneySurface::jsonl(processor.clone());
    let imported = transport
        .submit("cancelled-exploration-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = transport
        .submit(
            "cancelled-exploration-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (game_import_id, moments) = started_session(&started);
    let preparation_replacements = checkpoints.replace_attempts.load(Ordering::SeqCst);
    let moment = moments.first().unwrap();
    let move_uci = first_legal_uci(&moment.position_snapshot.fen);
    let idempotency_key =
        IdempotencyKey::try_from("idempotency-key:journey:cancelled-exploration".to_string())
            .unwrap();
    let envelope = transport_support::envelope(
        DeliverySurface::CoachSkill,
        "cancelled-exploration",
        ReviewSessionCommand::ExploreAlternativeMove {
            game_import_id: game_import_id.clone(),
            review_moment_id: moment.review_moment.moment_id.clone(),
            parent: BranchParent::Root {
                position_ref: moment.position_snapshot.position_ref.clone(),
            },
            source_position_ref: moment.position_snapshot.position_ref.clone(),
            move_input: MoveInput::Uci {
                uci: move_uci.clone(),
            },
            idempotency_key: idempotency_key.clone(),
        },
    );
    let operation_id = envelope.operation_id.clone();
    let mut evaluation_events = ReviewSessionProcessor::submit(
        &processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &serde_json::to_vec(&envelope).unwrap(),
    );
    loop {
        let event = evaluation_events
            .recv()
            .await
            .expect("exploration emits an Accepted event");
        if matches!(
            event.event,
            ReviewSessionEvent::Accepted {
                operation: OperationKind::AlternativeMoveEvaluation,
                ..
            }
        ) {
            break;
        }
    }
    blocking_engine.wait_until_started().await;

    let cancelled = transport
        .submit(
            "cancelled-exploration-cancel",
            ReviewSessionCommand::CancelOperation {
                game_import_id: game_import_id.clone(),
                operation_id,
                idempotency_key: idempotency_key.clone(),
            },
        )
        .await;
    assert!(matches!(
        cancelled.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Cancelled {
            operation: OperationKind::Cancellation,
        })
    ));
    let evaluation = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        transport_support::collect_receiver(evaluation_events),
    )
    .await
    .expect("cancelled Stockfish work releases within five seconds");
    assert!(matches!(
        evaluation.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Cancelled {
            operation: OperationKind::AlternativeMoveEvaluation,
        })
    ));
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        preparation_replacements + 2,
        "admission and explicit cancellation are each authoritative revisions"
    );
    drop(transport);

    let retry_processor = processor_with_stores(game_imports, checkpoints.clone());
    let mut retry_transport = JourneySurface::jsonl(retry_processor);
    retry_transport
        .submit(
            "cancelled-exploration-resume",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    let stale = retry_transport
        .submit(
            "cancelled-exploration-stale",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci {
                    uci: move_uci.clone(),
                },
                idempotency_key,
            },
        )
        .await;
    assert!(matches!(
        stale.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Conflict {
            reason: OperationConflictReason::IdempotencyKeyMismatch,
            ..
        })
    ));
    let retried = retry_transport
        .submit(
            "cancelled-exploration-retry",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id,
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci { uci: move_uci },
                idempotency_key: IdempotencyKey::try_from(
                    "idempotency-key:journey:cancelled-exploration-retry".to_string(),
                )
                .unwrap(),
            },
        )
        .await;
    assert!(matches!(
        completion(&retried),
        OperationCompletion::AlternativeMoveEvaluated { .. }
    ));
}

#[tokio::test]
async fn transport_disconnect_does_not_cancel_an_accepted_exploration() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let blocking_engine = Arc::new(BlockingAfterImportEngine::new());
    let processor = processor_with_engine_and_stores(
        blocking_engine.clone(),
        game_imports.clone(),
        checkpoints.clone(),
    );
    let mut transport = JourneySurface::jsonl(processor.clone());
    let imported = transport
        .submit("disconnected-exploration-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = transport
        .submit(
            "disconnected-exploration-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (game_import_id, moments) = started_session(&started);
    let preparation_replacements = checkpoints.replace_attempts.load(Ordering::SeqCst);
    let moment = moments.first().unwrap();
    let move_uci = first_legal_uci(&moment.position_snapshot.fen);
    let idempotency_key =
        IdempotencyKey::try_from("idempotency-key:journey:disconnected-exploration".to_string())
            .unwrap();
    let envelope = transport_support::envelope(
        DeliverySurface::CoachSkill,
        "disconnected-exploration",
        ReviewSessionCommand::ExploreAlternativeMove {
            game_import_id: game_import_id.clone(),
            review_moment_id: moment.review_moment.moment_id.clone(),
            parent: BranchParent::Root {
                position_ref: moment.position_snapshot.position_ref.clone(),
            },
            source_position_ref: moment.position_snapshot.position_ref.clone(),
            move_input: MoveInput::Uci {
                uci: move_uci.clone(),
            },
            idempotency_key: idempotency_key.clone(),
        },
    );
    let mut events = ReviewSessionProcessor::submit(
        &processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &serde_json::to_vec(&envelope).unwrap(),
    );
    loop {
        let event = events
            .recv()
            .await
            .expect("exploration emits an Accepted event");
        if matches!(
            event.event,
            ReviewSessionEvent::Accepted {
                operation: OperationKind::AlternativeMoveEvaluation,
                ..
            }
        ) {
            break;
        }
    }
    blocking_engine.wait_until_started().await;
    drop(events);
    blocking_engine.release();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while checkpoints.replace_attempts.load(Ordering::SeqCst) < preparation_replacements + 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("disconnected exploration commits within five seconds");
    drop(transport);

    let restored_processor = processor_with_stores(game_imports, checkpoints);
    let mut restored_transport = JourneySurface::jsonl(restored_processor);
    restored_transport
        .submit(
            "disconnected-exploration-resume",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    let repeated = restored_transport
        .submit(
            "disconnected-exploration-repeat",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id,
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci { uci: move_uci },
                idempotency_key,
            },
        )
        .await;
    assert!(matches!(
        completion(&repeated),
        OperationCompletion::AlternativeMoveEvaluated { .. }
    ));
}

#[tokio::test(start_paused = true)]
async fn timed_out_exploration_closes_its_idempotency_key_and_retry_commits_cleanly() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let blocking_engine = Arc::new(BlockingAfterImportEngine::new());
    let processor = processor_with_engine_and_stores(
        blocking_engine,
        game_imports.clone(),
        checkpoints.clone(),
    );
    let mut transport = JourneySurface::jsonl(processor);
    let imported = transport
        .submit("timed-exploration-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = transport
        .submit(
            "timed-exploration-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (game_import_id, moments) = started_session(&started);
    let preparation_replacements = checkpoints.replace_attempts.load(Ordering::SeqCst);
    let moment = moments.first().unwrap();
    let move_uci = first_legal_uci(&moment.position_snapshot.fen);
    let idempotency_key =
        IdempotencyKey::try_from("idempotency-key:journey:timed-exploration".to_string()).unwrap();
    let timed = tokio::time::timeout(
        std::time::Duration::from_millis(
            ALTERNATIVE_MOVE_DEADLINE_MILLISECONDS + CANCELLATION_BUDGET_MILLISECONDS,
        ),
        transport.submit(
            "timed-exploration",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci {
                    uci: move_uci.clone(),
                },
                idempotency_key: idempotency_key.clone(),
            },
        ),
    )
    .await
    .expect("Alternative Move timeout remains within the command deadline");
    assert!(matches!(
        timed.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::AlternativeMoveEvaluation,
            reason: ProviderUnavailableReason::Timeout {
                provider: ProviderKind::Stockfish,
            },
            retry: RetryDirective::RetryAllowed,
        })
    ));
    assert_eq!(
        checkpoints.replace_attempts.load(Ordering::SeqCst),
        preparation_replacements + 2
    );
    drop(transport);

    let retry_processor = processor_with_stores(game_imports, checkpoints);
    let mut retry_transport = JourneySurface::jsonl(retry_processor);
    retry_transport
        .submit(
            "timed-exploration-resume",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    let stale = retry_transport
        .submit(
            "timed-exploration-stale",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: game_import_id.clone(),
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci {
                    uci: move_uci.clone(),
                },
                idempotency_key,
            },
        )
        .await;
    assert!(matches!(
        stale.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Conflict {
            reason: OperationConflictReason::IdempotencyKeyMismatch,
            ..
        })
    ));
    let retried = retry_transport
        .submit(
            "timed-exploration-retry",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id,
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci { uci: move_uci },
                idempotency_key: IdempotencyKey::try_from(
                    "idempotency-key:journey:timed-exploration-retry".to_string(),
                )
                .unwrap(),
            },
        )
        .await;
    assert!(matches!(
        completion(&retried),
        OperationCompletion::AlternativeMoveEvaluated { .. }
    ));
}

#[tokio::test]
async fn opening_by_original_identity_returns_the_players_durable_game_review() {
    let checkpoints = Arc::new(InMemoryReviewAnalysisCache::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let processor = processor_with_stores(game_imports, checkpoints);
    let mut transport = JourneySurface::jsonl(processor);
    let imported = transport.submit("latest-import", import_command()).await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = transport
        .submit(
            "latest-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    let (game_import_id, _) = started_session(&started);
    // Side and rating are whatever the import resolved, which is exactly what
    // the review summary shows the Player.
    let (review_side, elo_rating) = match completion(&started) {
        OperationCompletion::ReviewSessionStarted { imported_game, .. } => {
            (imported_game.review_side, imported_game.elo_profile.rating)
        }
        completion => panic!("expected a started Review Session, got {completion:?}"),
    };
    let next = transport
        .submit(
            "latest-start-next-incarnation",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    let (latest_session_id, _) = started_session(&next);
    assert_eq!(
        latest_session_id, game_import_id,
        "starting again names the same review, not a new incarnation"
    );

    let opened = transport
        .submit(
            "durable-review-open",
            ReviewSessionCommand::OpenGameReviewByIdentity {
                // The URL the Player typed is still in the conversation even
                // though the handle is not.
                source: GameInputSource::LichessUrl {
                    url: "https://lichess.org/Synthet1Demo/black".to_string(),
                },
                review_side,
                elo_rating,
            },
        )
        .await;

    match completion(&opened) {
        OperationCompletion::GameReviewOpened {
            game_import_id: opened_id,
            ..
        } => assert_eq!(opened_id, &game_import_id),
        completion => panic!("expected a durable Game Review, got {completion:?}"),
    }

    // Exact live-session continuation remains separately addressable.
    let historical = transport
        .submit(
            "latest-resume-historical",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        )
        .await;
    match completion(&historical) {
        OperationCompletion::ReviewSessionStarted {
            game_import_id: resumed_id,
            ..
        } => assert_eq!(resumed_id, &game_import_id),
        completion => panic!("expected a historical Review Session, got {completion:?}"),
    }
}

#[tokio::test]
async fn opening_by_identity_does_not_return_a_different_game_review() {
    let checkpoints = Arc::new(InMemoryReviewAnalysisCache::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let processor = processor_with_stores(game_imports, checkpoints);
    let mut transport = JourneySurface::jsonl(processor);
    let imported = transport.submit("other-import", import_command()).await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    transport
        .submit(
            "other-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;

    // A durable review exists, but it is of a different Game, so naming this
    // one must not hand back another review the Player happens to own.
    let events = transport
        .submit(
            "other-review-open",
            ReviewSessionCommand::OpenGameReviewByIdentity {
                source: GameInputSource::ChessComUrl {
                    url: "https://www.chess.com/game/live/999999".to_string(),
                },
                review_side: ReviewSide::White,
                elo_rating: EloRating::try_from(1300).unwrap(),
            },
        )
        .await;

    assert!(matches!(
        events.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            operation: OperationKind::GameReviewOpen,
            reason: CommandRejectionReason::UnknownGameImport,
            recovery: RejectionRecovery::CorrectInput,
        })
    ));
}

#[tokio::test]
async fn opening_by_identity_reports_no_game_review_when_the_player_has_none() {
    let checkpoints = Arc::new(InMemoryReviewAnalysisCache::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let processor = processor_with_stores(game_imports, checkpoints);
    let mut transport = JourneySurface::jsonl(processor);

    let events = transport
        .submit(
            "durable-review-empty",
            ReviewSessionCommand::OpenGameReviewByIdentity {
                source: GameInputSource::LichessUrl {
                    url: "https://lichess.org/Synthet1Demo/black".to_string(),
                },
                review_side: ReviewSide::Black,
                elo_rating: EloRating::try_from(1450).unwrap(),
            },
        )
        .await;

    assert!(matches!(
        events.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            operation: OperationKind::GameReviewOpen,
            reason: CommandRejectionReason::UnknownGameImport,
            recovery: RejectionRecovery::CorrectInput,
        })
    ));
}
