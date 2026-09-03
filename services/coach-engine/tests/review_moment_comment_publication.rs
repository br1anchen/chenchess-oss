use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

use chen_chess_coach_engine::{
    critical_moment_comment::grounding_ledger_for,
    game_import_store::ReviewSessionGame,
    game_import_store::{GameImportStore, InMemoryGameImportStore},
    review_analysis_cache::{
        InMemoryReviewAnalysisCache, ReviewAnalysisCacheError, ReviewAnalysisCacheFuture,
        ReviewAnalysisCacheStore, ReviewAnalysisEntries, ReviewAnalysisEntry,
        ReviewAnalysisMutation,
    },
    review_annotation_store::{InMemoryReviewAnnotationStore, ReviewAnnotationStore},
    review_session_contract::*,
    review_session_processor::{ProcessorPrincipal, ReviewSessionProcessor},
};
use tokio::sync::watch;

use crate::{processor_support, transport_support};

const PLAYER: &str = "coach-app-player";

fn idempotency_key(label: &str) -> IdempotencyKey {
    IdempotencyKey::try_from(format!("idempotency-key:preparation:{label}")).unwrap()
}

/// A canonical Review Moment Comment is a property of the review, not of the
/// chat that wrote it.
#[tokio::test]
async fn a_published_comment_is_active_when_the_same_game_is_reviewed_in_another_conversation() {
    let game_imports: Arc<dyn GameImportStore> = Arc::new(InMemoryGameImportStore::default());
    let annotations: Arc<dyn ReviewAnnotationStore> =
        Arc::new(InMemoryReviewAnnotationStore::default());

    let first_conversation = processor_sharing_durable_review(&game_imports, &annotations);
    let (game_import_id, review_moment_id, facts, intent_state) =
        import_and_start(&first_conversation, ProcessorPrincipal::Player(player_id())).await;
    let grounded = crate::marker_commentary::commentary(&facts, intent_state.as_ref());
    let published = submit(
        &first_conversation,
        ProcessorPrincipal::Player(player_id()),
        "cross-conversation-publish",
        publication_command(
            game_import_id.clone(),
            review_moment_id.clone(),
            grounded.draft_text.clone(),
            grounding_ledger_for(&facts),
            "cross-conversation",
        ),
    )
    .await;
    assert_eq!(published_comment(&published), &grounded.comment);
    drop(first_conversation);

    // A different conversation over the same durable Game Import: its own
    // process-local Review Session, sharing nothing but the address.
    let second_conversation = processor_sharing_durable_review(&game_imports, &annotations);
    let (second_session_id, admitted) =
        start_review(&second_conversation, "second-conversation-start").await;
    assert_eq!(
        second_session_id, game_import_id,
        "the review address is the same in both conversations"
    );
    let reopened = submit(
        &second_conversation,
        ProcessorPrincipal::Player(player_id()),
        "second-conversation-open",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: second_session_id,
            selection: admitted.review_moment.selection,
            idempotency_key: idempotency_key("second-conversation-open"),
        },
    )
    .await;

    assert!(
        matches!(
            completion(&reopened),
            OperationCompletion::ReviewMomentOpened {
                comment: Some(comment),
                comment_published: true,
                ..
            } if comment.as_ref() == &grounded.comment
        ),
        "a comment published in one conversation is the active comment in the next"
    );
}

#[tokio::test]
async fn coach_app_publication_is_authorized_idempotent_and_grounded_at_the_shared_seam() {
    let processor = processor_support::processor(false).0;
    let (game_import_id, review_moment_id, facts, intent_state) =
        import_and_start(&processor, ProcessorPrincipal::Player(player_id())).await;
    let grounded = crate::marker_commentary::commentary(&facts, intent_state.as_ref());
    let command = publication_command(
        game_import_id.clone(),
        review_moment_id.clone(),
        grounded.draft_text.clone(),
        grounding_ledger_for(&facts),
        "first",
    );

    let published = submit(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        "publish",
        command.clone(),
    )
    .await;
    assert_eq!(published_comment(&published), &grounded.comment);

    let retried = submit(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        "publish-retry",
        command,
    )
    .await;
    assert_eq!(published_comment(&retried), &grounded.comment);

    // A key the Review Moment has not seen is a second logical write, not a
    // collision with the first. Annotations are append-only, so it publishes
    // instead of conflicting, and the first key keeps replaying its own comment.
    let second_write = submit(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        "publish-second",
        publication_command(
            game_import_id.clone(),
            review_moment_id.clone(),
            grounded.draft_text.clone(),
            grounding_ledger_for(&facts),
            "second",
        ),
    )
    .await;
    assert_eq!(published_comment(&second_write), &grounded.comment);

    let grounding_processor = processor_support::processor(false).0;
    let (grounding_session_id, grounding_moment_id, grounding_facts, grounding_intent_state) =
        import_and_start(
            &grounding_processor,
            ProcessorPrincipal::Player(player_id()),
        )
        .await;
    let mut cross_kind = grounding_ledger_for(&grounding_facts);
    cross_kind
        .factual_claims
        .push(CriticalMomentFactualClaim::NeutralReason);
    let cross_kind = submit(
        &grounding_processor,
        ProcessorPrincipal::Player(player_id()),
        "cross-kind",
        publication_command(
            grounding_session_id.clone(),
            grounding_moment_id.clone(),
            grounded.draft_text.clone(),
            cross_kind,
            "cross-kind",
        ),
    )
    .await;
    assert_rejected(&cross_kind, CommandRejectionReason::InvalidCommand);
    let corrected_grounding =
        crate::marker_commentary::commentary(&grounding_facts, grounding_intent_state.as_ref());
    let corrected = submit(
        &grounding_processor,
        ProcessorPrincipal::Player(player_id()),
        "cross-kind-corrected",
        publication_command(
            grounding_session_id,
            grounding_moment_id,
            corrected_grounding.draft_text.clone(),
            grounding_ledger_for(&grounding_facts),
            "cross-kind",
        ),
    )
    .await;
    assert_eq!(published_comment(&corrected), &corrected_grounding.comment);

    let safe_processor = processor_support::processor(false).0;
    let (safe_session_id, safe_moment_id, safe_facts, safe_intent_state) =
        import_and_start(&safe_processor, ProcessorPrincipal::Player(player_id())).await;
    let invalid_command = publication_command(
        safe_session_id,
        safe_moment_id,
        String::new(),
        grounding_ledger_for(&safe_facts),
        "safe-rendering",
    );
    let first_invalid_prose = submit(
        &safe_processor,
        ProcessorPrincipal::Player(player_id()),
        "safe-rendering",
        invalid_command.clone(),
    )
    .await;
    assert_rejected(&first_invalid_prose, CommandRejectionReason::InvalidCommand);
    let second_invalid_prose = submit(
        &safe_processor,
        ProcessorPrincipal::Player(player_id()),
        "safe-rendering-retry",
        invalid_command,
    )
    .await;
    let safe_comment = published_comment(&second_invalid_prose);
    assert!(!safe_comment.text.is_empty());
    assert_eq!(
        safe_comment.text.contains("My best guess"),
        safe_intent_state.is_some()
    );

    let unknown_moment = submit(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        "unknown-moment",
        publication_command(
            game_import_id.clone(),
            CriticalMomentId::try_from("review-moment:missing".to_string()).unwrap(),
            grounded.draft_text.clone(),
            grounding_ledger_for(&facts),
            "unknown-moment",
        ),
    )
    .await;
    assert_rejected(&unknown_moment, CommandRejectionReason::UnknownMoment);

    // A review this Player does not own is unaddressable rather than refused,
    // and the model is told to correct the address it supplied.
    let unowned = submit(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        "unowned-review",
        publication_command(
            GameImportId::try_from(format!("game-import:{}:{}", "a".repeat(64), "b".repeat(32)))
                .unwrap(),
            review_moment_id.clone(),
            grounded.draft_text.clone(),
            grounding_ledger_for(&facts),
            "unowned",
        ),
    )
    .await;
    assert_rejected(&unowned, CommandRejectionReason::UnknownGameImport);
    assert_recovery(&unowned, RejectionRecovery::CorrectInput);

    let unauthorized = submit(
        &processor,
        ProcessorPrincipal::LocalCoach,
        "unauthorized",
        publication_command(
            game_import_id.clone(),
            review_moment_id.clone(),
            grounded.draft_text.clone(),
            grounding_ledger_for(&facts),
            "unauthorized",
        ),
    )
    .await;
    assert_rejected(
        &unauthorized,
        CommandRejectionReason::AuthenticationRequired,
    );

    let wrong_surface = submit_with_surface(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        DeliverySurface::Web,
        "wrong-surface",
        publication_command(
            game_import_id,
            review_moment_id,
            grounded.draft_text.clone(),
            grounding_ledger_for(&facts),
            "wrong-surface",
        ),
    )
    .await;
    assert_rejected(&wrong_surface, CommandRejectionReason::InvalidCommand);
}

#[tokio::test]
async fn retry_allowance_and_canonical_comment_restore_without_duplicate_publication() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let game_imports: Arc<dyn GameImportStore> = Arc::new(InMemoryGameImportStore::default());
    let recording = processor_support::provider_recording();
    let engine = Arc::new(processor_support::RecordingEngine::new(&recording));
    let human = Arc::new(processor_support::RecordingHuman::new(&recording, false));
    let first_processor = processor_with_stores_and_providers(
        checkpoints.clone(),
        game_imports.clone(),
        engine.clone(),
        human.clone(),
    );
    let (game_import_id, review_moment_id, facts, intent_state) =
        import_and_start(&first_processor, ProcessorPrincipal::Player(player_id())).await;
    let preparation_replacements = checkpoints.replacements.load(Ordering::SeqCst);
    let grounded = crate::marker_commentary::commentary(&facts, intent_state.as_ref());
    let invalid = publication_command(
        game_import_id.clone(),
        review_moment_id.clone(),
        String::new(),
        grounding_ledger_for(&facts),
        "durable-retry",
    );
    let first_failure = submit(
        &first_processor,
        ProcessorPrincipal::Player(player_id()),
        "durable-retry-first",
        invalid,
    )
    .await;
    assert_rejected(&first_failure, CommandRejectionReason::InvalidCommand);
    assert_eq!(
        checkpoints.replacements.load(Ordering::SeqCst),
        preparation_replacements + 1
    );
    drop(first_processor);

    let calls_before_resume = (engine.calls(), human.calls());
    let second_processor = processor_with_stores_and_providers(
        checkpoints.clone(),
        game_imports.clone(),
        engine.clone(),
        human.clone(),
    );
    let resumed = submit(
        &second_processor,
        ProcessorPrincipal::Player(player_id()),
        "durable-retry-resume",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    assert!(matches!(
        completion(&resumed),
        OperationCompletion::ReviewSessionStarted { .. }
    ));
    assert_eq!(
        (engine.calls(), human.calls()),
        calls_before_resume,
        "checkpoint restoration must not replay intent providers"
    );
    let valid_retry = publication_command(
        game_import_id.clone(),
        review_moment_id.clone(),
        grounded.draft_text.clone(),
        grounding_ledger_for(&facts),
        "durable-retry",
    );
    let published = submit(
        &second_processor,
        ProcessorPrincipal::Player(player_id()),
        "durable-retry-second",
        valid_retry.clone(),
    )
    .await;
    assert_eq!(published_comment(&published), &grounded.comment);
    assert_eq!(
        checkpoints.replacements.load(Ordering::SeqCst),
        preparation_replacements + 2
    );
    assert!(
        engine.calls() > calls_before_resume.0 && human.calls() > calls_before_resume.1,
        "an unpublished retry must rebuild ephemeral authoring enrichment"
    );
    drop(second_processor);

    let calls_after_publication = (engine.calls(), human.calls());
    let third_processor = processor_with_stores_and_providers(
        checkpoints.clone(),
        game_imports,
        engine.clone(),
        human.clone(),
    );
    let repeated = submit(
        &third_processor,
        ProcessorPrincipal::Player(player_id()),
        "durable-published-repeat",
        valid_retry,
    )
    .await;
    assert_eq!(published_comment(&repeated), &grounded.comment);
    let reopened = submit(
        &third_processor,
        ProcessorPrincipal::Player(player_id()),
        "durable-published-open",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection: ReviewMomentSelection::PipelineCriticalMoment {
                critical_moment_id: review_moment_id.clone(),
            },
            idempotency_key: idempotency_key("durable-published-open"),
        },
    )
    .await;
    assert!(matches!(
        completion(&reopened),
        OperationCompletion::ReviewMomentOpened {
            comment: Some(comment),
            comment_published: true,
            ..
        } if comment.as_ref() == &grounded.comment
    ));
    assert_eq!(
        checkpoints.replacements.load(Ordering::SeqCst),
        preparation_replacements + 2,
        "resume, replay, and reopen are passive"
    );
    assert_eq!(
        (engine.calls(), human.calls()),
        calls_after_publication,
        "published comment replay and reopen must not invoke intent providers"
    );

    // A restored comment is the active one only until this Review Session
    // publishes again; a fresh key republishes rather than being refused.
    let republished = submit(
        &third_processor,
        ProcessorPrincipal::Player(player_id()),
        "durable-published-second",
        publication_command(
            game_import_id.clone(),
            review_moment_id.clone(),
            grounded.draft_text.clone(),
            grounding_ledger_for(&facts),
            "durable-published-second",
        ),
    )
    .await;
    assert_eq!(published_comment(&republished), &grounded.comment);
    let cross_player = submit(
        &third_processor,
        ProcessorPrincipal::Player(
            PlayerId::try_from("different-coach-app-player".to_string()).unwrap(),
        ),
        "durable-published-cross-player",
        publication_command(
            game_import_id,
            review_moment_id,
            grounded.comment.text,
            grounding_ledger_for(&facts),
            "durable-retry",
        ),
    )
    .await;
    assert_rejected(&cross_player, CommandRejectionReason::UnknownGameImport);
}

#[tokio::test]
async fn concurrent_publication_commits_one_canonical_comment_and_provenance() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let processor = processor_with_checkpoint_store(checkpoints.clone());
    let (game_import_id, review_moment_id, facts, intent_state) =
        import_and_start(&processor, ProcessorPrincipal::Player(player_id())).await;
    let preparation_replacements = checkpoints.replacements.load(Ordering::SeqCst);
    let grounded = crate::marker_commentary::commentary(&facts, intent_state.as_ref());
    let command = publication_command(
        game_import_id,
        review_moment_id,
        grounded.draft_text.clone(),
        grounding_ledger_for(&facts),
        "concurrent",
    );

    let (first, second) = tokio::join!(
        submit(
            &processor,
            ProcessorPrincipal::Player(player_id()),
            "concurrent-first",
            command.clone(),
        ),
        submit(
            &processor,
            ProcessorPrincipal::Player(player_id()),
            "concurrent-second",
            command,
        ),
    );

    assert_eq!(published_comment(&first), &grounded.comment);
    assert_eq!(published_comment(&second), &grounded.comment);
    assert_eq!(
        checkpoints.replacements.load(Ordering::SeqCst),
        preparation_replacements + 1,
        "only the winning publication writes canonical comment and provenance"
    );
}

#[tokio::test]
async fn canonical_output_is_not_applied_until_checkpoint_commit_succeeds() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let processor = processor_with_checkpoint_store(checkpoints.clone());
    let (game_import_id, review_moment_id, facts, intent_state) =
        import_and_start(&processor, ProcessorPrincipal::Player(player_id())).await;
    let preparation_replacements = checkpoints.replacements.load(Ordering::SeqCst);
    let grounded = crate::marker_commentary::commentary(&facts, intent_state.as_ref());
    let command = publication_command(
        game_import_id,
        review_moment_id,
        grounded.draft_text.clone(),
        grounding_ledger_for(&facts),
        "persistence-authority",
    );
    checkpoints.fail_next_replace.store(true, Ordering::SeqCst);

    let failed = submit(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        "persistence-authority-failed",
        command.clone(),
    )
    .await;
    assert!(matches!(
        failed.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::ReviewMomentCommentPublication,
            reason: ProviderUnavailableReason::Persistence,
            ..
        })
    ));

    let retried = submit(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        "persistence-authority-retry",
        command,
    )
    .await;
    assert_eq!(published_comment(&retried), &grounded.comment);
    assert_eq!(
        checkpoints.replacements.load(Ordering::SeqCst),
        preparation_replacements + 2,
        "the failed checkpoint write cannot install an in-memory canonical comment"
    );
}

#[tokio::test]
async fn open_waits_for_durable_publication_to_reach_local_authority() {
    let checkpoints = Arc::new(TrackingCheckpointStore::default());
    let processor = processor_with_checkpoint_store(checkpoints.clone());
    let (game_import_id, review_moment_id, facts, intent_state) =
        import_and_start(&processor, ProcessorPrincipal::Player(player_id())).await;
    let grounded = crate::marker_commentary::commentary(&facts, intent_state.as_ref());
    checkpoints.block_next_replace();
    let publication = transport_support::envelope(
        DeliverySurface::CoachApp,
        "open-race-publication",
        publication_command(
            game_import_id.clone(),
            review_moment_id.clone(),
            grounded.draft_text.clone(),
            grounding_ledger_for(&facts),
            "open-race",
        ),
    );
    let publication_events = ReviewSessionProcessor::submit(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        &serde_json::to_vec(&publication).unwrap(),
    );
    checkpoints.wait_until_replace_committed().await;

    let open = transport_support::envelope(
        DeliverySurface::CoachApp,
        "open-race-reader",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id,
            selection: ReviewMomentSelection::PipelineCriticalMoment {
                critical_moment_id: review_moment_id,
            },
            idempotency_key: idempotency_key("open-race-reader"),
        },
    );
    let mut open_events = ReviewSessionProcessor::submit(
        &processor,
        ProcessorPrincipal::Player(player_id()),
        &serde_json::to_vec(&open).unwrap(),
    );
    assert!(matches!(
        open_events.recv().await.map(|event| event.event),
        Some(ReviewSessionEvent::Accepted {
            operation: OperationKind::ReviewMomentOpen,
            ..
        })
    ));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), open_events.recv())
            .await
            .is_err(),
        "open must not observe the pre-publication runtime after durable commit"
    );

    checkpoints.release_replace();
    let published = transport_support::collect_receiver(publication_events).await;
    assert_eq!(published_comment(&published), &grounded.comment);
    let opened = transport_support::collect_receiver(open_events).await;
    assert!(matches!(
        completion(&opened),
        OperationCompletion::ReviewMomentOpened {
            comment: Some(comment),
            comment_published: true,
            ..
        } if comment.as_ref() == &grounded.comment
    ));
}

fn processor_sharing_durable_review(
    game_imports: &Arc<dyn GameImportStore>,
    annotations: &Arc<dyn ReviewAnnotationStore>,
) -> Arc<ReviewSessionProcessor<processor_support::CapturedLichess>> {
    Arc::new(
        ReviewSessionProcessor::new(
            processor_support::CapturedLichess::new(),
            processor_support::provider_recording(),
            Arc::new(processor_support::RecordingEngine::new(
                &processor_support::provider_recording(),
            )),
            Arc::new(processor_support::RecordingHuman::new(
                &processor_support::provider_recording(),
                false,
            )),
            Arc::new(processor_support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(game_imports.clone())
        .with_review_annotation_store(annotations.clone()),
    )
}

/// Imports the fixture Game and starts one Review Session over it.
async fn start_review(
    processor: &Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    label: &str,
) -> (GameImportId, ReviewSessionMoment) {
    let imported = submit(
        processor,
        ProcessorPrincipal::Player(player_id()),
        &format!("{label}-import"),
        ReviewSessionCommand::ImportGame {
            source: GameInputSource::LichessUrl {
                url: "https://lichess.org/Synthet1Demo/black".to_string(),
            },
            review_side: RequestedReviewSide::FromQualifiedUrl,
            elo_profile: RequestedEloProfile::FromImportedMetadata,
        },
    )
    .await;
    let OperationCompletion::GameImported { game_import_id, .. } = completion(&imported) else {
        panic!("expected game import");
    };
    let started = submit(
        processor,
        ProcessorPrincipal::Player(player_id()),
        label,
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    match completion(&started) {
        OperationCompletion::ReviewSessionStarted {
            game_import_id,
            review_moments,
            ..
        } => (game_import_id.clone(), review_moments[0].clone()),
        result => panic!("expected session start, got {result:?}"),
    }
}

async fn import_and_start(
    processor: &Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    principal: ProcessorPrincipal,
) -> (
    GameImportId,
    CriticalMomentId,
    ReviewMomentCommentFacts,
    Option<CriticalMomentIntentAuthoringContext>,
) {
    let imported = submit(
        processor,
        principal.clone(),
        "import",
        ReviewSessionCommand::ImportGame {
            source: GameInputSource::LichessUrl {
                url: "https://lichess.org/Synthet1Demo/black".to_string(),
            },
            review_side: RequestedReviewSide::FromQualifiedUrl,
            elo_profile: RequestedEloProfile::FromImportedMetadata,
        },
    )
    .await;
    let (game_import_id, review) = match completion(&imported) {
        OperationCompletion::GameImported {
            game_import_id,
            review,
            ..
        } => (game_import_id.clone(), review),
        result => panic!("expected game import, got {result:?}"),
    };
    let started = submit(
        processor,
        principal.clone(),
        "start",
        ReviewSessionCommand::StartReviewSession { game_import_id },
    )
    .await;
    let (game_import_id, admitted) = match completion(&started) {
        OperationCompletion::ReviewSessionStarted {
            game_import_id,
            review_moments,
            ..
        } => (game_import_id.clone(), review_moments[0].clone()),
        result => panic!("expected session start, got {result:?}"),
    };
    let opened = submit(
        processor,
        principal,
        "open",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection: admitted.review_moment.selection,
            idempotency_key: idempotency_key("open"),
        },
    )
    .await;
    let (core, intent) = match completion(&opened) {
        OperationCompletion::ReviewMomentOpened {
            comment_published: false,
            review_moment,
            authoring_context,
            ..
        } => (
            review_moment.as_ref().clone(),
            authoring_context
                .as_deref()
                .and_then(|context| context.intent.clone()),
        ),
        result => panic!("expected Review Moment preparation, got {result:?}"),
    };
    let review_moment_id = core.review_moment.moment_id.clone();
    let moment = review
        .critical_moments
        .iter()
        .find(|moment| moment.critical_moment_id == review_moment_id)
        .unwrap()
        .clone();
    (
        game_import_id,
        review_moment_id,
        ReviewMomentCommentFacts::try_from_presented_moment(moment).unwrap(),
        intent,
    )
}

fn publication_command(
    game_import_id: GameImportId,
    review_moment_id: CriticalMomentId,
    text: String,
    grounding_ledger: CriticalMomentGroundingLedger,
    key: &str,
) -> ReviewSessionCommand {
    ReviewSessionCommand::PublishReviewMomentComment {
        game_import_id,
        review_moment_id,
        text,
        grounding_ledger,
        idempotency_key: IdempotencyKey::try_from(format!("idempotency-key:{key}")).unwrap(),
    }
}

async fn submit(
    processor: &Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    principal: ProcessorPrincipal,
    label: &str,
    command: ReviewSessionCommand,
) -> Vec<ReviewSessionEventEnvelope> {
    submit_with_surface(
        processor,
        principal,
        DeliverySurface::CoachApp,
        label,
        command,
    )
    .await
}

async fn submit_with_surface(
    processor: &Arc<ReviewSessionProcessor<processor_support::CapturedLichess>>,
    principal: ProcessorPrincipal,
    surface: DeliverySurface,
    label: &str,
    command: ReviewSessionCommand,
) -> Vec<ReviewSessionEventEnvelope> {
    let envelope = transport_support::envelope(surface, label, command);
    transport_support::collect_receiver(
        processor.submit(principal, &serde_json::to_vec(&envelope).unwrap()),
    )
    .await
}

fn completion(events: &[ReviewSessionEventEnvelope]) -> &OperationCompletion {
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => Some(result.as_ref()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected completion: {events:?}"))
}

fn published_comment(events: &[ReviewSessionEventEnvelope]) -> &CriticalMomentComment {
    match completion(events) {
        OperationCompletion::ReviewMomentCommentPublished { comment, .. } => comment,
        result => panic!("expected comment publication, got {result:?}"),
    }
}

fn assert_rejected(events: &[ReviewSessionEventEnvelope], expected: CommandRejectionReason) {
    assert!(
        events.iter().any(|event| matches!(
            event.event,
            ReviewSessionEvent::Rejected { reason, .. } if reason == expected
        )),
        "expected {expected:?}: {events:?}"
    );
}

fn assert_recovery(events: &[ReviewSessionEventEnvelope], expected: RejectionRecovery) {
    assert!(
        events.iter().any(|event| matches!(
            &event.event,
            ReviewSessionEvent::Rejected { recovery, .. } if recovery == &expected
        )),
        "expected {expected:?}: {events:?}"
    );
}

fn player_id() -> PlayerId {
    PlayerId::try_from(PLAYER.to_string()).unwrap()
}

fn processor_with_checkpoint_store(
    checkpoints: Arc<dyn ReviewAnalysisCacheStore>,
) -> Arc<ReviewSessionProcessor<processor_support::CapturedLichess>> {
    processor_with_stores(checkpoints, Arc::new(InMemoryGameImportStore::default()))
}

fn processor_with_stores(
    checkpoints: Arc<dyn ReviewAnalysisCacheStore>,
    game_imports: Arc<dyn GameImportStore>,
) -> Arc<ReviewSessionProcessor<processor_support::CapturedLichess>> {
    let recording = processor_support::provider_recording();
    processor_with_stores_and_providers(
        checkpoints,
        game_imports,
        Arc::new(processor_support::RecordingEngine::new(&recording)),
        Arc::new(processor_support::RecordingHuman::new(&recording, false)),
    )
}

fn processor_with_stores_and_providers(
    checkpoints: Arc<dyn ReviewAnalysisCacheStore>,
    game_imports: Arc<dyn GameImportStore>,
    engine: Arc<dyn chen_chess_coach_engine::engine_analysis::EngineAnalyzer>,
    human: Arc<dyn chen_chess_coach_engine::human_move_model::HumanMoveModel>,
) -> Arc<ReviewSessionProcessor<processor_support::CapturedLichess>> {
    Arc::new(
        ReviewSessionProcessor::new(
            processor_support::CapturedLichess::new(),
            processor_support::provider_recording(),
            engine,
            human,
            Arc::new(processor_support::GroundedAuthor),
        )
        .unwrap()
        .with_game_import_store(game_imports)
        .with_review_analysis_cache(checkpoints),
    )
}

struct TrackingCheckpointStore {
    inner: InMemoryReviewAnalysisCache,
    replacements: AtomicUsize,
    fail_next_replace: AtomicBool,
    block_next_replace: AtomicBool,
    replace_committed: watch::Sender<bool>,
    replace_released: watch::Sender<bool>,
}

impl Default for TrackingCheckpointStore {
    fn default() -> Self {
        let (replace_committed, _) = watch::channel(false);
        let (replace_released, _) = watch::channel(true);
        Self {
            inner: InMemoryReviewAnalysisCache::default(),
            replacements: AtomicUsize::new(0),
            fail_next_replace: AtomicBool::new(false),
            block_next_replace: AtomicBool::new(false),
            replace_committed,
            replace_released,
        }
    }
}

impl TrackingCheckpointStore {
    fn block_next_replace(&self) {
        self.replace_committed.send_replace(false);
        self.replace_released.send_replace(false);
        self.block_next_replace.store(true, Ordering::SeqCst);
    }

    async fn wait_until_replace_committed(&self) {
        let mut committed = self.replace_committed.subscribe();
        while !*committed.borrow_and_update() {
            committed.changed().await.unwrap();
        }
    }

    fn release_replace(&self) {
        self.replace_released.send_replace(true);
    }
}

impl ReviewAnalysisCacheStore for TrackingCheckpointStore {
    fn seed<'a>(&'a self, entries: ReviewAnalysisEntries) -> ReviewAnalysisCacheFuture<'a> {
        self.inner.seed(entries)
    }

    fn load<'a>(
        &'a self,
        game_import_id: &'a GameImportId,
        game: &'a ReviewSessionGame,
        now: chrono::DateTime<chrono::Utc>,
    ) -> ReviewAnalysisCacheFuture<'a, Vec<ReviewAnalysisEntry>> {
        self.inner.load(game_import_id, game, now)
    }

    fn replace_moment<'a>(
        &'a self,
        mutation: ReviewAnalysisMutation,
    ) -> ReviewAnalysisCacheFuture<'a> {
        Box::pin(async move {
            self.replacements.fetch_add(1, Ordering::SeqCst);
            if self.fail_next_replace.swap(false, Ordering::SeqCst) {
                return Err(ReviewAnalysisCacheError::Unavailable);
            }
            let result = self.inner.replace_moment(mutation).await;
            if result.is_ok() && self.block_next_replace.swap(false, Ordering::SeqCst) {
                self.replace_committed.send_replace(true);
                let mut released = self.replace_released.subscribe();
                while !*released.borrow_and_update() {
                    released.changed().await.unwrap();
                }
            }
            result
        })
    }
}
