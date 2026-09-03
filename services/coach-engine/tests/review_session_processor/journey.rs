use super::*;

#[tokio::test]
async fn manual_web_imports_move_one_imported_game_card_to_the_latest_elo_review() {
    let (_, _, recording) = processor(false);
    let store = Arc::new(InMemoryGameImportStore::default());
    let processor = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(support::RecordingEngine::new(&recording)),
            Arc::new(support::RecordingHuman::new(&recording, false)),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(store.clone()),
    );
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:imported-games".to_string()).unwrap(),
    );
    let first = submit(
        &processor,
        principal.clone(),
        envelope_for(&principal, "imported-games-first", import_command()),
    )
    .await
    .iter()
    .find_map(imported_game)
    .unwrap();
    let reused = submit(
        &processor,
        principal.clone(),
        envelope_for(&principal, "imported-games-reused", import_command()),
    )
    .await
    .iter()
    .find_map(imported_game)
    .unwrap();
    let reused_cards = store.list_imported_game_cards(&principal).await.unwrap();
    assert_eq!(reused, first);
    assert_eq!(reused_cards.len(), 1);
    assert_eq!(reused_cards[0].game_import_id(), &first);
    let stronger = submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "imported-games-stronger",
            ReviewSessionCommand::ImportGame {
                source: GameInputSource::LichessUrl {
                    url: REVIEWED_GAME_URL.to_string(),
                },
                review_side: RequestedReviewSide::FromQualifiedUrl,
                elo_profile: RequestedEloProfile::PlayerProvided {
                    rating: EloRating::try_from(1800).unwrap(),
                },
            },
        ),
    )
    .await
    .iter()
    .find_map(imported_game)
    .unwrap();

    let cards = store.list_imported_game_cards(&principal).await.unwrap();
    assert_ne!(first, stronger);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].game_import_id(), &stronger);
}

#[tokio::test]
async fn local_coach_imports_do_not_create_imported_game_cards() {
    let (_, _, recording) = processor(false);
    let store = Arc::new(InMemoryGameImportStore::default());
    let processor = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(support::RecordingEngine::new(&recording)),
            Arc::new(support::RecordingHuman::new(&recording, false)),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(store.clone()),
    );

    submit(
        &processor,
        ProcessorPrincipal::LocalCoach,
        envelope("local-imported-games", import_command()),
    )
    .await;

    assert!(store
        .list_imported_game_cards(&ProcessorPrincipal::LocalCoach)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn serialized_commands_replay_the_available_automatic_moment_flow() {
    let (processor, _, _) = processor_with_runtime_startup(std::time::Duration::from_millis(17));
    let principal = ProcessorPrincipal::LocalCoach;

    let imported = submit(
        &processor,
        principal.clone(),
        envelope("import", import_command()),
    )
    .await;
    assert_event_stream(&imported, OperationKind::GameImport);
    let timing = imported.iter().find_map(imported_timing).unwrap();
    assert_eq!(timing.runtime_startup_milliseconds, Some(17));
    let expected_provider_calls = u32::try_from(support::canonical_game_plies()).unwrap();
    for (summary, provider) in [
        (&timing.engine_analysis, "Engine Analysis adapter"),
        (&timing.human_move_model, "Human Move Model adapter"),
    ] {
        assert_eq!(summary.provider, provider);
        assert_eq!(summary.call_count, expected_provider_calls);
        assert!(summary.median_milliseconds <= summary.maximum_milliseconds);
        assert!(summary.maximum_milliseconds <= summary.total_milliseconds);
    }
    assert!(
        timing.total_pipeline_milliseconds
            >= timing
                .engine_analysis
                .maximum_milliseconds
                .max(timing.human_move_model.maximum_milliseconds)
    );
    let game_import_id = imported.iter().find_map(imported_game).unwrap();

    let started = submit(
        &processor,
        principal.clone(),
        envelope(
            "start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        ),
    )
    .await;
    assert_event_stream(&started, OperationKind::ReviewSessionStart);
    let (game_import_id, core) = started.iter().find_map(started_session).unwrap();
    assert!(core
        .evidence_packet
        .entries
        .iter()
        .any(|entry| matches!(entry, EvidenceEntry::EngineAnalysis { .. })));

    let inspected = submit(
        &processor,
        principal.clone(),
        envelope(
            "inspect-root",
            ReviewSessionCommand::InspectPosition {
                game_import_id: game_import_id.clone(),
                review_moment_id: core.review_moment.moment_id.clone(),
                target: PositionInspectionTarget::ReviewedMove,
            },
        ),
    )
    .await;
    assert_event_stream(&inspected, OperationKind::PositionInspection);
    let inspection = inspected.iter().find_map(position_inspection).unwrap();
    assert!(inspection.text_board.contains("a b c d e f g h"));
    assert!(!serde_json::to_string(&inspection.context)
        .unwrap()
        .contains("hypothesis"));

    let explored = submit(
        &processor,
        principal.clone(),
        envelope(
            "explore",
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
                idempotency_key: idempotency_key("explore"),
            },
        ),
    )
    .await;
    assert_event_stream(&explored, OperationKind::AlternativeMoveEvaluation);
    let alternative = explored.iter().find_map(explored_move).unwrap();
    let inspected = submit(
        &processor,
        principal.clone(),
        envelope(
            "inspect-alternative",
            ReviewSessionCommand::InspectPosition {
                game_import_id: game_import_id.clone(),
                review_moment_id: core.review_moment.moment_id.clone(),
                target: PositionInspectionTarget::AlternativeMove {
                    alternative_move_id: alternative.alternative_move_id.clone(),
                },
            },
        ),
    )
    .await;
    assert_event_stream(&inspected, OperationKind::PositionInspection);
    let inspection = inspected.iter().find_map(position_inspection).unwrap();
    assert_eq!(inspection.position_snapshot, alternative.resulting_position);
    let coach_turn_id = CoachTurnId::try_from("coach-turn:processor:one".to_string()).unwrap();
    let mut context = inspection.context;
    context.coach_turn_id = coach_turn_id.clone();
    let coached = submit(
        &processor,
        principal.clone(),
        envelope(
            "coach",
            ReviewSessionCommand::StartCoachTurn {
                game_import_id: game_import_id.clone(),
                review_moment_id: core.review_moment.moment_id.clone(),
                coach_turn_id: coach_turn_id.clone(),
                context: Box::new(context),
                message: "How well does this move hold up?".to_string(),
                idempotency_key: idempotency_key("coach"),
                prior_turn: PriorCoachTurn::None,
            },
        ),
    )
    .await;
    assert_event_stream(&coached, OperationKind::CoachTurn);
    let facts = coached.iter().find_map(coach_turn_preparation).unwrap();
    let assessment = alternative_assessment(&facts);

    let published = submit(
        &processor,
        principal,
        envelope(
            "publish",
            ReviewSessionCommand::PublishCoachTurn {
                game_import_id,
                review_moment_id: core.review_moment.moment_id.clone(),
                coach_turn_id,
                assessment: Box::new(assessment),
                idempotency_key: idempotency_key("publish"),
            },
        ),
    )
    .await;
    assert_event_stream(&published, OperationKind::CoachTurn);
}

#[tokio::test]
async fn coach_app_prepares_player_coach_turn_for_the_host_language_layer() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:processor:coach-app".to_string()).unwrap(),
    );
    let (game_import_id, core) = import_and_start(&processor, principal.clone()).await;
    let command = coach_command(
        &processor,
        &principal,
        game_import_id,
        &core,
        "coach-app-prepare",
    )
    .await;

    let coached = submit(&processor, principal, command).await;

    assert_event_stream(&coached, OperationKind::CoachTurn);
    assert!(
        coached.iter().find_map(coach_turn_preparation).is_some(),
        "Coach App must receive immutable facts for its host Language Layer"
    );
}

#[tokio::test]
async fn coach_app_can_correct_an_invalid_assessment_without_losing_its_idempotency_key() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::Player(
        PlayerId::try_from("player:processor:coach-app-repair".to_string()).unwrap(),
    );
    let (game_import_id, core) = import_and_start(&processor, principal.clone()).await;
    let command = coach_command(
        &processor,
        &principal,
        game_import_id.clone(),
        &core,
        "coach-app-repair",
    )
    .await;
    let coached = submit(&processor, principal.clone(), command).await;
    let facts = coached.iter().find_map(coach_turn_preparation).unwrap();
    let valid_assessment = alternative_assessment(&facts);
    let mut invalid_assessment = valid_assessment.clone();
    invalid_assessment
        .resilience
        .evidence_refs
        .retain(|evidence_ref| evidence_ref != &facts.evidence.target_branch);
    let key = idempotency_key("coach-app-repair");

    let invalid = submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "coach-app-repair-invalid",
            ReviewSessionCommand::PublishCoachTurn {
                game_import_id: game_import_id.clone(),
                review_moment_id: core.review_moment.moment_id.clone(),
                coach_turn_id: facts.coach_turn_id.clone(),
                assessment: Box::new(invalid_assessment),
                idempotency_key: key.clone(),
            },
        ),
    )
    .await;
    assert!(matches!(
        invalid.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            operation: OperationKind::CoachTurn,
            reason: CommandRejectionReason::InvalidCommand,
            recovery: RejectionRecovery::CorrectInput,
        })
    ));

    let corrected = submit(
        &processor,
        principal.clone(),
        envelope_for(
            &principal,
            "coach-app-repair-corrected",
            ReviewSessionCommand::PublishCoachTurn {
                game_import_id,
                review_moment_id: core.review_moment.moment_id,
                coach_turn_id: facts.coach_turn_id,
                assessment: Box::new(valid_assessment),
                idempotency_key: key,
            },
        ),
    )
    .await;
    assert_event_stream(&corrected, OperationKind::CoachTurn);
}
