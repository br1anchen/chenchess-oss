use std::{future::Future, pin::Pin, sync::Arc};

use chen_chess_coach_engine::learning_path_feedback::{
    LearningPathFeedbackAnalytics, LearningPathFeedbackError, LearningPathFeedbackStore,
    LearningPathFeedbackStoreFuture, LearningPathSample,
};

use super::*;

#[tokio::test]
async fn feedback_is_idempotent_shared_across_surfaces_and_does_not_mutate_the_review() {
    let (processor, _, _) = processor(false);
    let principal = player("feedback-owner");
    let (game_import_id, _) = import_and_start(&processor, principal.clone()).await;
    let (learning_path_ref, frozen_review) =
        selected_learning_path(&processor, &principal, &game_import_id, "feedback-initial").await;

    let first_exposure = feedback_command(
        &processor,
        principal.clone(),
        DeliverySurface::Web,
        "feedback-expose-web",
        ReviewSessionCommand::RecordLearningPathExposure {
            game_import_id: game_import_id.clone(),
            learning_path_ref: learning_path_ref.clone(),
        },
    )
    .await;
    assert_eq!(
        feedback_state(&first_exposure).exposed_surfaces,
        vec![DeliverySurface::Web]
    );

    let repeated_exposure = feedback_command(
        &processor,
        principal.clone(),
        DeliverySurface::Web,
        "feedback-expose-web-retry",
        ReviewSessionCommand::RecordLearningPathExposure {
            game_import_id: game_import_id.clone(),
            learning_path_ref: learning_path_ref.clone(),
        },
    )
    .await;
    assert_eq!(
        feedback_state(&repeated_exposure).exposed_surfaces,
        vec![DeliverySurface::Web]
    );

    feedback_command(
        &processor,
        principal.clone(),
        DeliverySurface::CoachApp,
        "feedback-expose-coach-app",
        ReviewSessionCommand::RecordLearningPathExposure {
            game_import_id: game_import_id.clone(),
            learning_path_ref: learning_path_ref.clone(),
        },
    )
    .await;
    let voted_up = feedback_command(
        &processor,
        principal.clone(),
        DeliverySurface::Web,
        "feedback-vote-up",
        ReviewSessionCommand::UpdateLearningPathVote {
            game_import_id: game_import_id.clone(),
            learning_path_ref: learning_path_ref.clone(),
            vote: Some(LearningPathVote::ThumbsUp),
        },
    )
    .await;
    assert_eq!(
        feedback_state(&voted_up).current_vote,
        Some(LearningPathVote::ThumbsUp)
    );

    let voted_down_elsewhere = feedback_command(
        &processor,
        principal.clone(),
        DeliverySurface::CoachApp,
        "feedback-vote-down",
        ReviewSessionCommand::UpdateLearningPathVote {
            game_import_id: game_import_id.clone(),
            learning_path_ref: learning_path_ref.clone(),
            vote: Some(LearningPathVote::ThumbsDown),
        },
    )
    .await;
    let state = feedback_state(&voted_down_elsewhere);
    assert_eq!(state.current_vote, Some(LearningPathVote::ThumbsDown));
    assert_eq!(
        state.exposed_surfaces,
        vec![DeliverySurface::Web, DeliverySurface::CoachApp]
    );

    let removed = feedback_command(
        &processor,
        principal.clone(),
        DeliverySurface::Web,
        "feedback-remove-vote",
        ReviewSessionCommand::UpdateLearningPathVote {
            game_import_id: game_import_id.clone(),
            learning_path_ref,
            vote: None,
        },
    )
    .await;
    assert_eq!(feedback_state(&removed).current_vote, None);

    let (_, review_after_feedback) =
        selected_learning_path(&processor, &principal, &game_import_id, "feedback-final").await;
    assert_eq!(
        review_after_feedback.learning_plan,
        frozen_review.learning_plan
    );
}

#[tokio::test]
async fn feedback_rejects_unknown_paths_unexposed_surfaces_and_other_players() {
    let (processor, _, _) = processor(false);
    let principal = player("feedback-isolation-owner");
    let (game_import_id, _) = import_and_start(&processor, principal.clone()).await;
    let (learning_path_ref, _) = selected_learning_path(
        &processor,
        &principal,
        &game_import_id,
        "feedback-isolation",
    )
    .await;

    let unknown = feedback_command(
        &processor,
        principal.clone(),
        DeliverySurface::Web,
        "feedback-unknown-path",
        ReviewSessionCommand::RecordLearningPathExposure {
            game_import_id: game_import_id.clone(),
            learning_path_ref: LearningPathRef::try_from("learning-path:unknown".to_string())
                .unwrap(),
        },
    )
    .await;
    assert_rejected(&unknown, CommandRejectionReason::UnknownTarget);

    feedback_command(
        &processor,
        principal.clone(),
        DeliverySurface::Web,
        "feedback-isolation-expose",
        ReviewSessionCommand::RecordLearningPathExposure {
            game_import_id: game_import_id.clone(),
            learning_path_ref: learning_path_ref.clone(),
        },
    )
    .await;
    let unexposed_surface = feedback_command(
        &processor,
        principal.clone(),
        DeliverySurface::CoachApp,
        "feedback-unexposed-surface",
        ReviewSessionCommand::UpdateLearningPathVote {
            game_import_id: game_import_id.clone(),
            learning_path_ref: learning_path_ref.clone(),
            vote: Some(LearningPathVote::ThumbsUp),
        },
    )
    .await;
    assert_rejected(&unexposed_surface, CommandRejectionReason::InvalidCommand);

    let other_player = feedback_command(
        &processor,
        player("feedback-isolation-other"),
        DeliverySurface::Web,
        "feedback-other-player",
        ReviewSessionCommand::RecordLearningPathExposure {
            game_import_id,
            learning_path_ref,
        },
    )
    .await;
    assert_rejected(&other_player, CommandRejectionReason::UnknownGameImport);
}

#[tokio::test]
async fn feedback_reports_persistence_failures_without_exposing_store_details() {
    let (processor, _, _) = processor(false);
    let processor = Arc::new(
        Arc::try_unwrap(processor)
            .ok()
            .expect("the fixture processor has one owner")
            .with_learning_path_feedback_store(Arc::new(UnavailableFeedbackStore)),
    );
    let principal = player("feedback-unavailable");
    let (game_import_id, _) = import_and_start(&processor, principal.clone()).await;
    let (learning_path_ref, _) = selected_learning_path(
        &processor,
        &principal,
        &game_import_id,
        "feedback-unavailable",
    )
    .await;

    let events = feedback_command(
        &processor,
        principal,
        DeliverySurface::Web,
        "feedback-persistence-failure",
        ReviewSessionCommand::RecordLearningPathExposure {
            game_import_id,
            learning_path_ref,
        },
    )
    .await;

    assert!(matches!(
        events.last().map(|event| &event.event),
        Some(ReviewSessionEvent::Unavailable {
            operation: OperationKind::LearningPathFeedback,
            reason: ProviderUnavailableReason::Persistence,
            retry: RetryDirective::RetryAllowed,
        })
    ));
}

async fn selected_learning_path(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: &ProcessorPrincipal,
    game_import_id: &GameImportId,
    label: &str,
) -> (LearningPathRef, GameReview) {
    let events = submit(
        processor,
        principal.clone(),
        envelope_for(
            principal,
            label,
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
    )
    .await;
    let (review, moments) = events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::ReviewSessionStarted {
                    review,
                    review_moments,
                    ..
                } => Some((review.as_ref().clone(), review_moments)),
                _ => None,
            },
            _ => None,
        })
        .expect("the owned Review Session resumes");
    let learning_path_ref = moments
        .iter()
        .flat_map(|moment| &moment.learning_material.tracks)
        .flat_map(|track| &track.support)
        .map(|support| match support {
            LearningTrackSupport::Improvement {
                learning_path_ref, ..
            }
            | LearningTrackSupport::Reinforcement {
                learning_path_ref, ..
            } => learning_path_ref.clone(),
        })
        .next()
        .expect("the canonical fixture exposes at least one selected Learning Path");
    (learning_path_ref, review)
}

async fn feedback_command(
    processor: &Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    surface: DeliverySurface,
    label: &str,
    command: ReviewSessionCommand,
) -> Vec<ReviewSessionEventEnvelope> {
    submit(
        processor,
        principal,
        ReviewSessionCommandEnvelope {
            request_id: RequestId::try_from(format!("request:processor:{label}")).unwrap(),
            operation_id: OperationId::try_from(format!("operation:processor:{label}")).unwrap(),
            surface,
            command,
        },
    )
    .await
}

fn feedback_state(events: &[ReviewSessionEventEnvelope]) -> &LearningPathFeedbackState {
    assert_event_stream(events, OperationKind::LearningPathFeedback);
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::LearningPathFeedbackRecorded { feedback } => Some(feedback),
                _ => None,
            },
            _ => None,
        })
        .expect("feedback command returns the current state")
}

fn assert_rejected(events: &[ReviewSessionEventEnvelope], reason: CommandRejectionReason) {
    assert!(
        matches!(
            events.last().map(|event| &event.event),
            Some(ReviewSessionEvent::Rejected {
                operation: OperationKind::LearningPathFeedback,
                reason: actual,
                ..
            }) if *actual == reason
        ),
        "{events:#?}"
    );
}

fn player(seed: &str) -> ProcessorPrincipal {
    ProcessorPrincipal::Player(PlayerId::try_from(format!("player:{seed}")).unwrap())
}

struct UnavailableFeedbackStore;

impl LearningPathFeedbackStore for UnavailableFeedbackStore {
    fn record_exposure<'a>(
        &'a self,
        _player_id: &'a PlayerId,
        _sample: LearningPathSample,
        _surface: DeliverySurface,
    ) -> LearningPathFeedbackStoreFuture<'a, LearningPathFeedbackState> {
        unavailable()
    }

    fn update_vote<'a>(
        &'a self,
        _player_id: &'a PlayerId,
        _sample: LearningPathSample,
        _surface: DeliverySurface,
        _vote: Option<LearningPathVote>,
    ) -> LearningPathFeedbackStoreFuture<'a, LearningPathFeedbackState> {
        unavailable()
    }

    fn analytics(&self) -> LearningPathFeedbackStoreFuture<'_, LearningPathFeedbackAnalytics> {
        Box::pin(async { Err(LearningPathFeedbackError::Unavailable) })
    }
}

fn unavailable<'a, T: 'a>(
) -> Pin<Box<dyn Future<Output = Result<T, LearningPathFeedbackError>> + Send + 'a>> {
    Box::pin(async { Err(LearningPathFeedbackError::Unavailable) })
}
