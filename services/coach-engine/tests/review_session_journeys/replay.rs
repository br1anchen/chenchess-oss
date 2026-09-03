use super::*;

#[tokio::test]
async fn completed_exploration_replays_while_another_moment_is_evaluating() {
    let engine = Arc::new(ToggleBlockingEngine::new());
    let processor = processor_with_engine_and_stores(
        engine.clone(),
        Arc::new(InMemoryGameImportStore::default()),
        Arc::new(InMemoryReviewAnalysisCache::default()),
    );
    let mut transport = JourneySurface::jsonl(processor.clone());
    let imported = transport.submit("replay-import", import_command()).await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = transport
        .submit(
            "replay-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (game_import_id, moments) = started_session(&started);
    assert!(moments.len() >= 2);
    let completed_moment = &moments[0];
    let active_moment = &moments[1];
    let completed_key =
        IdempotencyKey::try_from("idempotency-key:journey:replay-completed".to_string()).unwrap();
    let completed_command = ReviewSessionCommand::ExploreAlternativeMove {
        game_import_id: game_import_id.clone(),
        review_moment_id: completed_moment.review_moment.moment_id.clone(),
        parent: BranchParent::Root {
            position_ref: completed_moment.position_snapshot.position_ref.clone(),
        },
        source_position_ref: completed_moment.position_snapshot.position_ref.clone(),
        move_input: MoveInput::Uci {
            uci: first_legal_uci(&completed_moment.position_snapshot.fen),
        },
        idempotency_key: completed_key,
    };
    let completed = transport
        .submit("replay-completed", completed_command.clone())
        .await;
    let expected = match completion(&completed) {
        OperationCompletion::AlternativeMoveEvaluated { alternative_move } => {
            alternative_move.clone()
        }
        completion => panic!("expected Alternative Move completion, got {completion:?}"),
    };

    engine.block();
    let active_envelope = transport_support::envelope(
        DeliverySurface::CoachSkill,
        "replay-active",
        ReviewSessionCommand::ExploreAlternativeMove {
            game_import_id,
            review_moment_id: active_moment.review_moment.moment_id.clone(),
            parent: BranchParent::Root {
                position_ref: active_moment.position_snapshot.position_ref.clone(),
            },
            source_position_ref: active_moment.position_snapshot.position_ref.clone(),
            move_input: MoveInput::Uci {
                uci: first_legal_uci(&active_moment.position_snapshot.fen),
            },
            idempotency_key: IdempotencyKey::try_from(
                "idempotency-key:journey:replay-active".to_string(),
            )
            .unwrap(),
        },
    );
    let mut active_events = ReviewSessionProcessor::submit(
        &processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &serde_json::to_vec(&active_envelope).unwrap(),
    );
    loop {
        let event = active_events
            .recv()
            .await
            .expect("active exploration emits Accepted");
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
    engine.wait_until_started().await;

    let replayed = transport
        .submit("replay-completed", completed_command)
        .await;
    assert!(matches!(
        completion(&replayed),
        OperationCompletion::AlternativeMoveEvaluated { alternative_move }
            if alternative_move == &expected
    ));

    engine.release();
    let remaining = transport_support::collect_receiver(active_events).await;
    assert!(matches!(
        completion(&remaining),
        OperationCompletion::AlternativeMoveEvaluated { .. }
    ));
}

/// A completion the cache refused evicts the runtime, and the operation it was
/// holding is reported as retryable rather than left half-committed.
///
/// The rebuilt runtime closes the operation the lost one was holding, so the
/// Player's next attempt at the same move is admitted rather than refused on
/// behalf of a computation nobody is running.
#[tokio::test]
async fn failed_completion_persistence_evicts_the_active_runtime() {
    let engine = Arc::new(ToggleBlockingEngine::new());
    let checkpoints = Arc::new(FailingReplacementCheckpointStore::default());
    let processor = processor_with_engine_and_stores(
        engine.clone(),
        Arc::new(InMemoryGameImportStore::default()),
        checkpoints.clone(),
    );
    let mut transport = JourneySurface::jsonl(processor.clone());
    let imported = transport
        .submit("failed-completion-import", import_command())
        .await;
    let game_import_id = match completion(&imported) {
        OperationCompletion::GameImported { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Game Import completion, got {completion:?}"),
    };
    let started = transport
        .submit(
            "failed-completion-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        )
        .await;
    let (game_import_id, moments) = started_session(&started);
    let moment = moments.first().unwrap();
    let move_uci = first_legal_uci(&moment.position_snapshot.fen);

    engine.block();
    let active_envelope = transport_support::envelope(
        DeliverySurface::CoachSkill,
        "failed-completion-active",
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
            idempotency_key: IdempotencyKey::try_from(
                "idempotency-key:journey:failed-completion".to_string(),
            )
            .unwrap(),
        },
    );
    let mut active_events = ReviewSessionProcessor::submit(
        &processor,
        chen_chess_coach_engine::review_session_processor::ProcessorPrincipal::LocalCoach,
        &serde_json::to_vec(&active_envelope).unwrap(),
    );
    loop {
        let event = active_events
            .recv()
            .await
            .expect("active exploration emits Accepted");
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
    engine.wait_until_started().await;
    checkpoints.fail_next_replace.store(true, Ordering::SeqCst);
    engine.release();
    let remaining = transport_support::collect_receiver(active_events).await;
    assert!(matches!(
        remaining.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::AlternativeMoveEvaluation,
            reason: ProviderUnavailableReason::Persistence,
            ..
        })
    ));

    let retried = transport
        .submit(
            "failed-completion-retry",
            ReviewSessionCommand::ExploreAlternativeMove {
                game_import_id,
                review_moment_id: moment.review_moment.moment_id.clone(),
                parent: BranchParent::Root {
                    position_ref: moment.position_snapshot.position_ref.clone(),
                },
                source_position_ref: moment.position_snapshot.position_ref.clone(),
                move_input: MoveInput::Uci { uci: move_uci },
                idempotency_key: IdempotencyKey::try_from(
                    "idempotency-key:journey:failed-completion-retry".to_string(),
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

struct ToggleBlockingEngine {
    recording: processor_support::RecordingEngine,
    blocking: AtomicBool,
    started: watch::Sender<bool>,
    released: watch::Sender<bool>,
}

#[derive(Default)]
struct FailingReplacementCheckpointStore {
    inner: InMemoryReviewAnalysisCache,
    fail_next_replace: AtomicBool,
}

impl ReviewAnalysisCacheStore for FailingReplacementCheckpointStore {
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
            if self.fail_next_replace.swap(false, Ordering::SeqCst) {
                return Err(ReviewAnalysisCacheError::Unavailable);
            }
            self.inner.replace_moment(mutation).await
        })
    }
}

impl ToggleBlockingEngine {
    fn new() -> Self {
        let recording = processor_support::provider_recording();
        let (started, _) = watch::channel(false);
        let (released, _) = watch::channel(true);
        Self {
            recording: processor_support::RecordingEngine::new(&recording),
            blocking: AtomicBool::new(false),
            started,
            released,
        }
    }

    fn block(&self) {
        self.started.send_replace(false);
        self.released.send_replace(false);
        self.blocking.store(true, Ordering::SeqCst);
    }

    async fn wait_until_started(&self) {
        let mut started = self.started.subscribe();
        while !*started.borrow_and_update() {
            started.changed().await.unwrap();
        }
    }

    fn release(&self) {
        self.blocking.store(false, Ordering::SeqCst);
        self.released.send_replace(true);
    }
}

impl EngineAnalyzer for ToggleBlockingEngine {
    fn analyze<'a>(
        &'a self,
        input: EngineAnalysisInput<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<EngineAnalysis, EngineAnalysisError>> + Send + 'a>>
    {
        Box::pin(async move {
            if self.blocking.load(Ordering::SeqCst) {
                self.started.send_replace(true);
                let mut released = self.released.subscribe();
                while !*released.borrow_and_update() {
                    released.changed().await.unwrap();
                }
            }
            self.recording.analyze(input).await
        })
    }

    fn provenance(&self) -> Option<EngineProvenance> {
        self.recording.provenance()
    }
}
