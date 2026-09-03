use super::*;

#[tokio::test]
async fn an_active_session_no_longer_depends_on_its_game_import_record() {
    use std::sync::atomic::Ordering;

    let (_, _, recording) = processor(false);
    let game_imports = Arc::new(SingleReadGameImportStore::default());
    let processor = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(support::RecordingEngine::new(&recording)),
            Arc::new(support::RecordingHuman::new(&recording, false)),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(game_imports.clone()),
    );
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &processor,
        principal.clone(),
        envelope("active-session-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let start_envelope = envelope(
        "active-session-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    );
    let started = submit(&processor, principal.clone(), start_envelope.clone()).await;
    let (game_import_id, _) = started.iter().find_map(started_session).unwrap();

    let repeated = submit(&processor, principal.clone(), start_envelope).await;
    assert!(matches!(
        repeated.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Completed {
            result
        }) if matches!(result.as_ref(), OperationCompletion::ReviewSessionStarted { .. })
    ));

    let opened = submit(
        &processor,
        principal.clone(),
        envelope(
            "active-session-open-moment",
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id,
                selection: ReviewMomentSelection::PlayerSelectedMoment { ply: 20 },
                idempotency_key: idempotency_key("active-session-open-moment"),
            },
        ),
    )
    .await;
    assert!(matches!(
        opened.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Completed {
            result
        }) if matches!(result.as_ref(), OperationCompletion::ReviewMomentOpened { .. })
    ));
    assert_eq!(
        game_imports.find_calls.load(Ordering::SeqCst),
        2,
        "import resolution and initial session creation are the only record reads"
    );
}

#[derive(Default)]
struct SingleReadGameImportStore {
    record: Mutex<Option<GameImportRecord>>,
    find_calls: std::sync::atomic::AtomicUsize,
}

impl GameImportStore for SingleReadGameImportStore {
    fn create<'a>(&'a self, record: GameImportRecord) -> GameImportStoreFuture<'a, ()> {
        Box::pin(async move {
            *self.record.lock().await = Some(record);
            Ok(())
        })
    }

    fn list_game_import_records<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
    ) -> GameImportStoreFuture<'a, Vec<GameImportRecord>> {
        Box::pin(async move {
            Ok(self
                .record
                .lock()
                .await
                .clone()
                .into_iter()
                .filter(|record| &record.owner == owner)
                .collect())
        })
    }

    fn find<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        game_import_id: &'a GameImportId,
    ) -> GameImportStoreFuture<'a, GameImportLookup> {
        Box::pin(async move {
            use std::sync::atomic::Ordering;

            if self.find_calls.fetch_add(1, Ordering::SeqCst) > 1 {
                return Ok(GameImportLookup::NotFound);
            }
            Ok(match self.record.lock().await.clone() {
                Some(record)
                    if &record.owner == owner && &record.game_import_id == game_import_id =>
                {
                    GameImportLookup::Found(Box::new(record))
                }
                Some(_) => GameImportLookup::OwnerMismatch,
                None => GameImportLookup::NotFound,
            })
        })
    }

    fn retain_for_review_session<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async move {
            let mut record = self.record.lock().await;
            let Some(record) = record.as_mut() else {
                return Ok(GameImportReferenceLookup::NotFound);
            };
            if &record.owner != owner {
                return Ok(GameImportReferenceLookup::OwnerMismatch);
            }
            if record.reference() != *reference {
                return Err(
                    chen_chess_coach_engine::game_import_store::GameImportStoreError::InvalidRecord,
                );
            }
            Ok(GameImportReferenceLookup::Found(Box::new(record.clone())))
        })
    }

    fn resolve_review_session_reference<'a>(
        &'a self,
        owner: &'a ProcessorPrincipal,
        reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async move {
            let record = self.record.lock().await;
            let Some(record) = record.as_ref() else {
                return Ok(GameImportReferenceLookup::NotFound);
            };
            if &record.owner != owner {
                return Ok(GameImportReferenceLookup::OwnerMismatch);
            }
            if record.reference() != *reference {
                return Err(
                    chen_chess_coach_engine::game_import_store::GameImportStoreError::InvalidRecord,
                );
            }
            Ok(GameImportReferenceLookup::Found(Box::new(record.clone())))
        })
    }
}

#[tokio::test]
async fn a_committed_import_is_available_to_a_new_processor() {
    let (_, _, recording) = processor(false);
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let importing_processor = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(support::RecordingEngine::new(&recording)),
            Arc::new(support::RecordingHuman::new(&recording, false)),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(game_imports.clone()),
    );
    let principal = ProcessorPrincipal::LocalCoach;
    let imported = submit(
        &importing_processor,
        principal.clone(),
        envelope("restored-import", import_command()),
    )
    .await;
    let game_import_id = imported
        .iter()
        .find_map(imported_game)
        .expect("import should return a server-owned import ID");

    let restored_processor = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(support::RecordingEngine::new(&recording)),
            Arc::new(support::RecordingHuman::new(&recording, false)),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(game_imports),
    );
    let events = submit(
        &restored_processor,
        principal,
        envelope(
            "restored-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        ),
    )
    .await;

    assert!(events.iter().any(|event| matches!(
        event.event,
        ReviewSessionEvent::Completed {
            result: ref completion
        } if matches!(completion.as_ref(), OperationCompletion::ReviewSessionStarted { .. })
    )));
}

#[tokio::test]
async fn shared_analysis_spares_a_second_player_the_provider_work_but_not_the_owner_check() {
    let (_, _, recording) = processor(false);
    let engine = Arc::new(support::RecordingEngine::new(&recording));
    let human = Arc::new(support::RecordingHuman::new(&recording, false));
    let processor = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            engine.clone(),
            human.clone(),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap(),
    );
    let first = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-first".to_string()).unwrap(),
    );
    let second = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-second".to_string()).unwrap(),
    );

    let imported = submit(
        &processor,
        first.clone(),
        envelope_for(&first, "shared-findings-first", import_command()),
    )
    .await;
    let first_import_id = imported.iter().find_map(imported_game).unwrap();
    let first_import = imported
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::GameImported {
                    review,
                    imported_game,
                    ..
                } => Some((review.as_ref().clone(), imported_game.as_deref().cloned())),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    let engine_calls = engine.calls();
    let human_calls = human.calls();
    assert!(engine_calls > 0);

    let same_player_reimport = submit(
        &processor,
        first.clone(),
        envelope_for(&first, "shared-findings-first-reimport", import_command()),
    )
    .await;
    let same_player_import_id = same_player_reimport.iter().find_map(imported_game).unwrap();
    let same_player_import = same_player_reimport
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::GameImported {
                    review,
                    imported_game,
                    ..
                } => Some((review.as_ref().clone(), imported_game.as_deref().cloned())),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert_eq!(same_player_import_id, first_import_id);
    assert_eq!(same_player_import, first_import);
    assert_eq!(engine.calls(), engine_calls);
    assert_eq!(human.calls(), human_calls);

    let reused = submit(
        &processor,
        second.clone(),
        envelope_for(&second, "shared-findings-second", import_command()),
    )
    .await;
    let second_import_id = reused.iter().find_map(imported_game).unwrap();

    // Same Game, side, and Elo, so the findings are the same findings: the
    // second Player pays for no provider work at all.
    assert_eq!(engine.calls(), engine_calls);
    assert_eq!(human.calls(), human_calls);
    // Sharing the analysis shares nothing else. The second Player gets their
    // own Game Import and still cannot reach the first Player's.
    assert_ne!(second_import_id, first_import_id);
    let denied = submit(
        &processor,
        second.clone(),
        envelope_for(
            &second,
            "shared-findings-cross",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: first_import_id,
            },
        ),
    )
    .await;
    assert!(matches!(
        denied.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            reason: CommandRejectionReason::UnknownGameImport,
            ..
        })
    ));

    let started = submit(
        &processor,
        second.clone(),
        envelope_for(
            &second,
            "shared-findings-start",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: second_import_id,
            },
        ),
    )
    .await;
    assert!(started.iter().any(|event| matches!(
        event.event,
        ReviewSessionEvent::Completed {
            result: ref completion
        } if matches!(completion.as_ref(), OperationCompletion::ReviewSessionStarted { .. })
    )));
}

#[tokio::test]
async fn every_addressed_operation_hides_whether_an_import_belongs_to_another_player() {
    let (processor, _, _) = processor(false);
    let owner = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-review-owner".to_string()).unwrap(),
    );
    let other = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-review-other".to_string()).unwrap(),
    );
    let imported = submit(
        &processor,
        owner.clone(),
        envelope_for(&owner, "review-owner-import", import_command()),
    )
    .await;
    let owned_game_import_id = imported.iter().find_map(imported_game).unwrap();

    let missing_game_import_id =
        GameImportId::try_from("game-import:missing:review".to_string()).unwrap();
    let foreign_commands = addressed_commands(owned_game_import_id);
    let missing_commands = addressed_commands(missing_game_import_id);

    for ((label, foreign_command), (missing_label, missing_command)) in
        foreign_commands.into_iter().zip(missing_commands)
    {
        assert_eq!(label, missing_label);
        let foreign = submit(
            &processor,
            other.clone(),
            envelope_for(&other, &format!("{label}-foreign"), foreign_command),
        )
        .await;
        let missing = submit(
            &processor,
            other.clone(),
            envelope_for(&other, &format!("{label}-missing"), missing_command),
        )
        .await;

        assert_eq!(
            foreign.last().map(|event| &event.event),
            missing.last().map(|event| &event.event),
            "{label} exposed whether the Game Import exists"
        );
        if label != "cancel-operation" {
            assert!(
                matches!(
                    foreign.last().map(|event| &event.event),
                    Some(ReviewSessionEvent::Rejected {
                        reason: CommandRejectionReason::UnknownGameImport,
                        recovery: RejectionRecovery::CorrectInput,
                        ..
                    })
                ),
                "{label} did not return the common unknown-import outcome: {foreign:#?}"
            );
        }
    }
}

fn addressed_commands(game_import_id: GameImportId) -> Vec<(&'static str, ReviewSessionCommand)> {
    let review_moment_id =
        CriticalMomentId::try_from("critical-moment:privacy".to_string()).unwrap();
    let position_ref = PositionRef::try_from(format!("sha256:{}", "0".repeat(64))).unwrap();
    let coach_turn_id = CoachTurnId::try_from("coach-turn:privacy".to_string()).unwrap();
    let alternative_move_id =
        AlternativeMoveId::try_from("alternative-move:privacy".to_string()).unwrap();
    let idempotency_key = IdempotencyKey::try_from("idempotency-key:privacy".to_string()).unwrap();
    let evidence_ref = EvidenceId::try_from(format!("sha256:{}", "1".repeat(64))).unwrap();
    let dimension = AssessmentDimension {
        explanation: "Fixture assessment.".to_string(),
        evidence_refs: vec![evidence_ref.clone()],
    };
    let context = CoachTurnContext {
        coach_turn_id: coach_turn_id.clone(),
        reviewed_move: ReviewedMoveAnchor {
            critical_moment_id: review_moment_id.clone(),
            ply: 1,
            side: Color::White,
            position_ref: position_ref.clone(),
            played_move_uci: "e2e4".to_string(),
        },
        selected_position_ref: position_ref.clone(),
        target: CoachTurnTarget::ImportedGameMove {
            critical_moment_id: review_moment_id.clone(),
            ply: 1,
            uci: "e2e4".to_string(),
        },
        required_evidence_refs: vec![evidence_ref],
    };

    vec![
        (
            "start-review-session",
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
        (
            "open-game-review",
            ReviewSessionCommand::OpenGameReview {
                game_import_id: game_import_id.clone(),
            },
        ),
        (
            "read-game-review-snapshot",
            ReviewSessionCommand::ReadGameReviewSnapshot {
                game_import_id: game_import_id.clone(),
                known_content_digest: None,
            },
        ),
        (
            "read-review-moment-detail",
            ReviewSessionCommand::ReadReviewMomentDetail {
                game_import_id: game_import_id.clone(),
                review_moment_id: review_moment_id.clone(),
                known_content_digest: None,
            },
        ),
        (
            "open-addressed-review-moment",
            ReviewSessionCommand::OpenAddressedReviewMoment {
                game_import_id: game_import_id.clone(),
                reference: ReviewMomentReference::Critical {
                    review_moment_id: review_moment_id.clone(),
                },
            },
        ),
        (
            "read-review-moment-explanation",
            ReviewSessionCommand::ReadReviewMomentExplanation {
                game_import_id: game_import_id.clone(),
                review_moment_id: review_moment_id.clone(),
            },
        ),
        (
            "open-review-moment",
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection: ReviewMomentSelection::PlayerSelectedMoment { ply: 1 },
                idempotency_key: idempotency_key.clone(),
            },
        ),
        (
            "inspect-position",
            ReviewSessionCommand::InspectPosition {
                game_import_id: game_import_id.clone(),
                review_moment_id: review_moment_id.clone(),
                target: PositionInspectionTarget::ReviewedMove,
            },
        ),
        (
            "evaluate-player-plan",
            ReviewSessionCommand::EvaluatePlayerPlan {
                game_import_id: game_import_id.clone(),
                review_moment_id: review_moment_id.clone(),
                request: PlayerPlanEvaluationRequest::Prepare,
            },
        ),
        (
            "explore-alternative-move",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id: game_import_id.clone(),
                review_moment_id: review_moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: position_ref.clone(),
                },
                source_position_ref: position_ref,
                move_input: MoveInput::Uci {
                    uci: "e2e4".to_string(),
                },
                idempotency_key: idempotency_key.clone(),
            },
        ),
        (
            "start-coach-turn",
            ReviewSessionCommand::StartCoachTurn {
                game_import_id: game_import_id.clone(),
                review_moment_id: review_moment_id.clone(),
                coach_turn_id: coach_turn_id.clone(),
                context: Box::new(context),
                message: "Fixture question.".to_string(),
                idempotency_key: idempotency_key.clone(),
                prior_turn: PriorCoachTurn::None,
            },
        ),
        (
            "publish-coach-turn",
            ReviewSessionCommand::PublishCoachTurn {
                game_import_id: game_import_id.clone(),
                review_moment_id: review_moment_id.clone(),
                coach_turn_id: coach_turn_id.clone(),
                assessment: Box::new(AlternativeMoveAssessment {
                    coach_turn_id,
                    alternative_move_id,
                    objective_quality: dimension.clone(),
                    findability: dimension.clone(),
                    resilience: dimension,
                }),
                idempotency_key: idempotency_key.clone(),
            },
        ),
        (
            "publish-review-moment-comment",
            ReviewSessionCommand::PublishReviewMomentComment {
                game_import_id: game_import_id.clone(),
                review_moment_id,
                text: "Fixture comment.".to_string(),
                grounding_ledger: CriticalMomentGroundingLedger {
                    facts_ref: ArtifactDigest::try_from(format!("sha256:{}", "2".repeat(64)))
                        .unwrap(),
                    factual_claims: Vec::new(),
                },
                idempotency_key: idempotency_key.clone(),
            },
        ),
        (
            "record-learning-path-exposure",
            ReviewSessionCommand::RecordLearningPathExposure {
                game_import_id: game_import_id.clone(),
                learning_path_ref: LearningPathRef::try_from("learning-path:privacy".to_string())
                    .unwrap(),
            },
        ),
        (
            "update-learning-path-vote",
            ReviewSessionCommand::UpdateLearningPathVote {
                game_import_id: game_import_id.clone(),
                learning_path_ref: LearningPathRef::try_from("learning-path:privacy".to_string())
                    .unwrap(),
                vote: Some(LearningPathVote::ThumbsUp),
            },
        ),
        (
            "cancel-operation",
            ReviewSessionCommand::CancelOperation {
                game_import_id,
                operation_id: OperationId::try_from("operation:privacy".to_string()).unwrap(),
                idempotency_key,
            },
        ),
    ]
}

#[tokio::test]
async fn cross_player_import_ids_and_persistence_failure_are_typed_outcomes() {
    let (base, _, recording) = processor(false);
    let owner = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-owner".to_string()).unwrap(),
    );
    let imported = submit(
        &base,
        owner.clone(),
        envelope_for(&owner, "owned-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let other = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-other".to_string()).unwrap(),
    );
    let denied = submit(
        &base,
        other.clone(),
        envelope_for(
            &other,
            "cross-player-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        ),
    )
    .await;
    assert!(matches!(
        denied.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            reason: CommandRejectionReason::UnknownGameImport,
            ..
        })
    ));

    let unavailable = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(support::RecordingEngine::new(&recording)),
            Arc::new(support::RecordingHuman::new(&recording, false)),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(Arc::new(UnavailableGameImportStore)),
    );
    let events = submit(
        &unavailable,
        ProcessorPrincipal::LocalCoach,
        envelope("unavailable-persistence", import_command()),
    )
    .await;
    assert!(matches!(
        events.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::GameImport,
            reason: ProviderUnavailableReason::Persistence,
            retry: RetryDirective::RetryAllowed,
        })
    ));

    let invalid = Arc::new(
        ReviewSessionProcessor::new(
            CapturedLichess::new(),
            recording.clone(),
            Arc::new(support::RecordingEngine::new(&recording)),
            Arc::new(support::RecordingHuman::new(&recording, false)),
            Arc::new(support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(Arc::new(InvalidGameImportStore)),
    );
    let events = submit(
        &invalid,
        ProcessorPrincipal::LocalCoach,
        envelope("invalid-persistence-record", import_command()),
    )
    .await;
    assert!(matches!(
        events.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::GameImport,
            reason: ProviderUnavailableReason::Persistence,
            retry: RetryDirective::NotRetryable,
        })
    ));
}

struct UnavailableGameImportStore;

impl GameImportStore for UnavailableGameImportStore {
    fn create<'a>(
        &'a self,
        _record: chen_chess_coach_engine::game_import_store::GameImportRecord,
    ) -> chen_chess_coach_engine::game_import_store::GameImportStoreFuture<'a, ()> {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::Unavailable)
        })
    }

    fn list_game_import_records<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
    ) -> chen_chess_coach_engine::game_import_store::GameImportStoreFuture<
        'a,
        Vec<chen_chess_coach_engine::game_import_store::GameImportRecord>,
    > {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::Unavailable)
        })
    }

    fn find<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
        _game_import_id: &'a GameImportId,
    ) -> chen_chess_coach_engine::game_import_store::GameImportStoreFuture<
        'a,
        chen_chess_coach_engine::game_import_store::GameImportLookup,
    > {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::Unavailable)
        })
    }

    fn retain_for_review_session<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
        _reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::Unavailable)
        })
    }

    fn resolve_review_session_reference<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
        _reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::Unavailable)
        })
    }
}

struct InvalidGameImportStore;

impl GameImportStore for InvalidGameImportStore {
    fn create<'a>(
        &'a self,
        _record: chen_chess_coach_engine::game_import_store::GameImportRecord,
    ) -> chen_chess_coach_engine::game_import_store::GameImportStoreFuture<'a, ()> {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::InvalidRecord)
        })
    }

    fn list_game_import_records<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
    ) -> chen_chess_coach_engine::game_import_store::GameImportStoreFuture<
        'a,
        Vec<chen_chess_coach_engine::game_import_store::GameImportRecord>,
    > {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::InvalidRecord)
        })
    }

    fn find<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
        _game_import_id: &'a GameImportId,
    ) -> chen_chess_coach_engine::game_import_store::GameImportStoreFuture<
        'a,
        chen_chess_coach_engine::game_import_store::GameImportLookup,
    > {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::InvalidRecord)
        })
    }

    fn retain_for_review_session<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
        _reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::InvalidRecord)
        })
    }

    fn resolve_review_session_reference<'a>(
        &'a self,
        _owner: &'a ProcessorPrincipal,
        _reference: &'a GameImportReference,
    ) -> GameImportStoreFuture<'a, GameImportReferenceLookup> {
        Box::pin(async {
            Err(chen_chess_coach_engine::game_import_store::GameImportStoreError::InvalidRecord)
        })
    }
}

#[tokio::test]
async fn cancellation_is_owner_bound_and_live_handles_are_removed_at_terminal() {
    let (processor, human, _) = processor(true);
    let principal = ProcessorPrincipal::LocalCoach;
    let (game_import_id, core) = import_and_start(&processor, principal.clone()).await;
    let alternative = explore_root(&processor, &principal, &game_import_id, &core).await;
    let inspection = inspect_alternative(
        &processor,
        principal.clone(),
        &game_import_id,
        &core.review_moment.moment_id,
        &alternative.alternative_move_id,
        "cancel-inspect",
    )
    .await;
    let coach_turn_id = CoachTurnId::try_from("coach-turn:processor:cancel".to_string()).unwrap();
    let key = idempotency_key("cancel-target");
    let target_operation_id =
        OperationId::try_from("operation:processor:cancel-target".to_string()).unwrap();
    let mut context = inspection.context;
    context.coach_turn_id = coach_turn_id.clone();
    let coach_envelope = ReviewSessionCommandEnvelope {
        request_id: RequestId::try_from("request:processor:cancel-target".to_string()).unwrap(),
        operation_id: target_operation_id.clone(),
        surface: DeliverySurface::CoachSkill,
        command: ReviewSessionCommand::StartCoachTurn {
            game_import_id: game_import_id.clone(),
            review_moment_id: core.review_moment.moment_id.clone(),
            coach_turn_id,
            context: Box::new(context),
            message: "Please assess this branch.".to_string(),
            idempotency_key: key.clone(),
            prior_turn: PriorCoachTurn::None,
        },
    };
    let mut coach_events = processor.submit(
        principal.clone(),
        &serde_json::to_vec(&coach_envelope).unwrap(),
    );
    human.wait_until_started().await;

    let other = ProcessorPrincipal::Player(
        PlayerId::try_from("firebase-player-cancel-other".to_string()).unwrap(),
    );
    let foreign = submit(
        &processor,
        other.clone(),
        envelope_for(
            &other,
            "cancel-foreign-owner",
            ReviewSessionCommand::CancelOperation {
                game_import_id: game_import_id.clone(),
                operation_id: target_operation_id.clone(),
                idempotency_key: key.clone(),
            },
        ),
    )
    .await;
    let missing = submit(
        &processor,
        other.clone(),
        envelope_for(
            &other,
            "cancel-missing-owner",
            ReviewSessionCommand::CancelOperation {
                game_import_id: GameImportId::try_from(
                    "game-import:missing:cancellation".to_string(),
                )
                .unwrap(),
                operation_id: target_operation_id.clone(),
                idempotency_key: key.clone(),
            },
        ),
    )
    .await;
    assert_eq!(
        foreign.last().map(|event| &event.event),
        missing.last().map(|event| &event.event)
    );
    assert!(matches!(
        foreign.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Rejected {
            operation: OperationKind::Cancellation,
            reason: CommandRejectionReason::UnknownGameImport,
            recovery: RejectionRecovery::CorrectInput,
        })
    ));

    let cancelled = submit(
        &processor,
        principal.clone(),
        envelope(
            "cancel",
            ReviewSessionCommand::CancelOperation {
                game_import_id: game_import_id.clone(),
                operation_id: target_operation_id.clone(),
                idempotency_key: key.clone(),
            },
        ),
    )
    .await;
    assert!(matches!(
        cancelled.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Cancelled {
            operation: OperationKind::Cancellation
        })
    ));
    let coach_events = collect(&mut coach_events).await;
    assert!(matches!(
        coach_events.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Cancelled {
            operation: OperationKind::CoachTurn
        })
    ));

    let stale = submit(
        &processor,
        principal,
        envelope(
            "cancel-stale",
            ReviewSessionCommand::CancelOperation {
                game_import_id,
                operation_id: target_operation_id,
                idempotency_key: key,
            },
        ),
    )
    .await;
    assert!(matches!(
        stale.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Conflict {
            reason: OperationConflictReason::IdempotencyKeyMismatch,
            ..
        })
    ));
}

#[tokio::test]
async fn malformed_contract_and_authentication_fail_before_admission() {
    let (processor, _, _) = processor(false);
    for (bytes, reason) in [
        (
            b"not-json".as_slice(),
            CommandRejectionReason::MalformedInput,
        ),
        (
            br#"{"requestId":"request:unknown","operationId":"operation:unknown","surface":"coachSkill","command":{"kind":"unknown"}}"#
                .as_slice(),
            CommandRejectionReason::UnknownCommand,
        ),
    ] {
        let mut events = processor.submit(ProcessorPrincipal::LocalCoach, bytes);
        let events = collect(&mut events).await;
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0].event,
            ReviewSessionEvent::Rejected {
                operation: OperationKind::CommandAdmission,
                reason: actual,
                ..
            } if actual == &reason
        ));
    }

    let valid = envelope("boundary", import_command());
    let mut unexpected_field = serde_json::to_value(&valid).unwrap();
    unexpected_field["unexpected"] = serde_json::json!(true);
    let mut events = processor.submit(
        ProcessorPrincipal::LocalCoach,
        &serde_json::to_vec(&unexpected_field).unwrap(),
    );
    let events = collect(&mut events).await;
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0].event,
        ReviewSessionEvent::Rejected {
            reason: CommandRejectionReason::InvalidCommand,
            ..
        }
    ));

    let events = submit(
        &processor,
        ProcessorPrincipal::Player(PlayerId::try_from("player:wrong-surface".to_string()).unwrap()),
        valid,
    )
    .await;
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0].event,
        ReviewSessionEvent::Rejected {
            reason: CommandRejectionReason::AuthenticationRequired,
            ..
        }
    ));
}

#[tokio::test]
async fn unavailable_provider_commits_no_branch_state() {
    let processor = processor_with_failing_engine();
    let principal = ProcessorPrincipal::LocalCoach;
    let (game_import_id, core) = import_and_start(&processor, principal.clone()).await;
    for label in ["failure-provider-one", "failure-provider-two"] {
        let unavailable = submit(
            &processor,
            principal.clone(),
            envelope(
                label,
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
                    idempotency_key: idempotency_key(label),
                },
            ),
        )
        .await;
        assert!(matches!(
            unavailable.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Unavailable {
                operation: OperationKind::AlternativeMoveEvaluation,
                reason: ProviderUnavailableReason::StockfishProcess,
                retry: RetryDirective::RetryAllowed,
            })
        ));
    }
}
