use super::*;

#[tokio::test]
async fn alternative_move_limit_reports_zero_allowance_immediately_before_rejection() {
    use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};

    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let (game_import_id, _) = import_and_start(&processor, principal.clone()).await;
    // The first automatic moment answers a check and has only a handful of
    // legal replies. The limit is about a wide Position, so open one.
    let (core, _, _) = super::open_review_moment::open_moment(
        &processor,
        principal.clone(),
        &game_import_id,
        "alternative-move-limit-open",
        ReviewMomentSelection::PlayerSelectedMoment { ply: 30 },
    )
    .await;
    let chess: Chess = Fen::from_ascii(core.position_snapshot.fen.as_bytes())
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap();
    let moves = chess
        .legal_moves()
        .into_iter()
        .map(|chess_move| UciMove::from_move(&chess_move, CastlingMode::Standard).to_string())
        .collect::<Vec<_>>();
    let limit = usize::from(ReviewSessionLimits::V1.max_committed_alternative_moves);
    assert!(moves.len() > limit);

    for (index, uci) in moves.iter().take(limit).enumerate() {
        let events = submit(
            &processor,
            principal.clone(),
            envelope(
                &format!("alternative-move-limit-{index}"),
                ReviewSessionCommand::ExploreAlternativeMove {
                    game_import_id: game_import_id.clone(),
                    review_moment_id: core.review_moment.moment_id.clone(),
                    parent: BranchParent::Root {
                        position_ref: core.position_snapshot.position_ref.clone(),
                    },
                    source_position_ref: core.position_snapshot.position_ref.clone(),
                    move_input: MoveInput::Uci { uci: uci.clone() },
                    idempotency_key: idempotency_key(&format!("alternative-move-limit-{index}")),
                },
            ),
        )
        .await;
        assert!(matches!(
            events.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Completed {
                result
            }) if matches!(result.as_ref(), OperationCompletion::AlternativeMoveEvaluated { .. })
        ));
    }

    let rejected = submit(
        &processor,
        principal,
        envelope(
            "alternative-move-over-limit",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id,
                review_moment_id: core.review_moment.moment_id,
                parent: BranchParent::Root {
                    position_ref: core.position_snapshot.position_ref.clone(),
                },
                source_position_ref: core.position_snapshot.position_ref,
                move_input: MoveInput::Uci {
                    uci: moves[limit].clone(),
                },
                idempotency_key: idempotency_key("alternative-move-over-limit"),
            },
        ),
    )
    .await;

    assert!(matches!(
        rejected.as_slice(),
        [
            ReviewSessionEventEnvelope {
                sequence: 0,
                event: ReviewSessionEvent::Progress {
                    stage: OperationProgress::AlternativeMoveAllowance { remaining: 0 },
                },
                ..
            },
            ReviewSessionEventEnvelope {
                sequence: 1,
                event: ReviewSessionEvent::Rejected {
                    operation: OperationKind::AlternativeMoveEvaluation,
                    reason: CommandRejectionReason::AlternativeMoveLimit,
                    recovery: RejectionRecovery::StartNewReviewSession,
                },
                ..
            }
        ]
    ));
}

#[tokio::test]
async fn queued_coach_turn_cancels_without_waiting_for_queue_deadline() {
    let (processor, human, _) = processor(false);
    let mut commands = Vec::new();
    for index in 0..5 {
        let label = format!("queued-cancel-{index}");
        let principal = ProcessorPrincipal::Player(
            PlayerId::try_from(format!("player:queued-cancellation-{index}")).unwrap(),
        );
        let (game_import_id, core) =
            import_and_start_labeled(&processor, principal.clone(), &label).await;
        let mut command =
            coach_command(&processor, &principal, game_import_id, &core, &label).await;
        command.surface = DeliverySurface::CoachApp;
        commands.push((principal, command));
    }

    human.begin_holding();
    let mut active = commands[..4]
        .iter()
        .map(|(principal, command)| {
            processor.submit(principal.clone(), &serde_json::to_vec(command).unwrap())
        })
        .collect::<Vec<_>>();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        human.wait_until_started(),
    )
    .await
    .expect("Coach Turn should reach Maia predict");
    let mut queued = processor.submit(
        commands[4].0.clone(),
        &serde_json::to_vec(&commands[4].1).unwrap(),
    );
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;

    let cancelled = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        submit(
            &processor,
            commands[4].0.clone(),
            cancellation("queued-cancel-waiting-request", &commands[4].1),
        ),
    )
    .await
    .expect("queued cancellation should not wait for the queue deadline");
    assert!(matches!(
        cancelled.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Cancelled {
            operation: OperationKind::Cancellation
        })
    ));
    let queued = collect(&mut queued).await;
    assert!(matches!(
        queued.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Cancelled {
            operation: OperationKind::CoachTurn
        })
    ));

    for (index, receiver) in active.iter_mut().enumerate() {
        let cleanup = submit(
            &processor,
            commands[index].0.clone(),
            cancellation(
                &format!("queued-cancel-cleanup-{index}"),
                &commands[index].1,
            ),
        )
        .await;
        assert!(matches!(
            cleanup.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Cancelled { .. })
        ));
        let events = collect(receiver).await;
        assert!(matches!(
            events.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Cancelled { .. })
        ));
    }
}

/// The one-active-turn rule follows the Player and the Game Import, and the
/// Game Import is now the whole session address — so two conversations over one
/// imported Game reach the same session and cannot each hold a turn.
#[tokio::test]
async fn two_starts_on_one_game_import_share_the_single_active_coach_turn() {
    let (processor, human, _) = processor(false);
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:shared-turn-scope".to_string()).unwrap(),
    );
    let (first_session, first_core) =
        import_and_start_labeled(&processor, principal.clone(), "shared-scope-first").await;
    let (second_session, second_core) =
        import_and_start_labeled(&processor, principal.clone(), "shared-scope-second").await;
    assert_eq!(first_session, second_session);

    let mut holding = coach_command(
        &processor,
        &principal,
        first_session,
        &first_core,
        "shared-scope-holding",
    )
    .await;
    holding.surface = DeliverySurface::CoachApp;
    let mut joining = coach_command(
        &processor,
        &principal,
        second_session,
        &second_core,
        "shared-scope-joining",
    )
    .await;
    joining.surface = DeliverySurface::CoachApp;

    human.begin_holding();
    let mut held = processor.submit(principal.clone(), &serde_json::to_vec(&holding).unwrap());
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        human.wait_until_started(),
    )
    .await
    .expect("Coach Turn should reach Maia predict");
    let refused = submit(&processor, principal.clone(), joining).await;

    assert!(matches!(
        refused.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Conflict {
            operation: OperationKind::CoachTurn,
            reason: OperationConflictReason::CoachTurnAlreadyActive,
        })
    ));

    let cleanup = submit(
        &processor,
        principal,
        cancellation("shared-scope-cleanup", &holding),
    )
    .await;
    assert!(matches!(
        cleanup.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Cancelled { .. })
    ));
    assert!(matches!(
        collect(&mut held).await.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Cancelled { .. })
    ));
}

/// Two Game Imports are two scopes, so one Player reviewing two games keeps a
/// turn in flight on each.
#[tokio::test]
async fn two_game_imports_for_one_player_each_admit_a_coach_turn() {
    let (processor, human, _) = processor(false);
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:per-game-turn-scope".to_string()).unwrap(),
    );
    let (black_session, black_core) =
        import_and_start_labeled(&processor, principal.clone(), "per-game-black").await;
    let (white_session, white_core) = import_and_start_game(
        &processor,
        principal.clone(),
        "per-game-white",
        "https://lichess.org/Synthet1Demo/white",
    )
    .await;

    let mut black_turn = coach_command(
        &processor,
        &principal,
        black_session,
        &black_core,
        "per-game-black-turn",
    )
    .await;
    black_turn.surface = DeliverySurface::CoachApp;
    let mut white_turn = coach_command(
        &processor,
        &principal,
        white_session,
        &white_core,
        "per-game-white-turn",
    )
    .await;
    white_turn.surface = DeliverySurface::CoachApp;

    human.begin_holding();
    let mut black_events =
        processor.submit(principal.clone(), &serde_json::to_vec(&black_turn).unwrap());
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        human.wait_until_started(),
    )
    .await
    .expect("Coach Turn should reach Maia predict");
    let mut white_events =
        processor.submit(principal.clone(), &serde_json::to_vec(&white_turn).unwrap());

    // Both turns block in the author, so admission is the observable outcome.
    assert!(
        wait_for_accepted_coach_turn(&mut white_events).await,
        "a Coach Turn on a second Game Import is admitted while the first is active"
    );

    for (label, command, events) in [
        ("per-game-black-cleanup", &black_turn, &mut black_events),
        ("per-game-white-cleanup", &white_turn, &mut white_events),
    ] {
        let cleanup = submit(&processor, principal.clone(), cancellation(label, command)).await;
        assert!(matches!(
            cleanup.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Cancelled { .. })
        ));
        assert!(matches!(
            collect(events).await.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Cancelled { .. })
        ));
    }
}

async fn wait_for_accepted_coach_turn(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<ReviewSessionEventEnvelope>,
) -> bool {
    while let Some(envelope) = events.recv().await {
        match &envelope.event {
            ReviewSessionEvent::Accepted {
                operation: OperationKind::CoachTurn,
                ..
            } => return true,
            ReviewSessionEvent::Progress { .. } => continue,
            _ => return false,
        }
    }
    false
}

fn cancellation(
    label: &str,
    target: &ReviewSessionCommandEnvelope,
) -> ReviewSessionCommandEnvelope {
    let ReviewSessionCommand::StartCoachTurn {
        game_import_id,
        idempotency_key,
        ..
    } = &target.command
    else {
        unreachable!()
    };
    let mut cancellation = envelope(
        label,
        ReviewSessionCommand::CancelOperation {
            game_import_id: game_import_id.clone(),
            operation_id: target.operation_id.clone(),
            idempotency_key: idempotency_key.clone(),
        },
    );
    cancellation.surface = target.surface;
    cancellation
}
