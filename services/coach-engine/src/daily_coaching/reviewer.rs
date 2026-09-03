use std::{future::Future, pin::Pin, sync::Arc};

use tokio::sync::mpsc;

use crate::{
    lichess::LichessExportClient,
    profile_game_feed::DailyGameReviewRequest,
    review_session_contract::{
        GameImportId, GameReview, ImportedGame, OperationCompletion, OperationId, PlayerId,
        RequestId, RetryDirective, ReviewSessionEvent, ReviewSessionEventEnvelope,
    },
    review_session_processor::ReviewSessionProcessor,
};
#[cfg(test)]
use crate::{
    profile_game_feed::DailyGameInputSource,
    review_session_contract::{
        CommandRejectionReason, DeliverySurface, OperationKind, RejectionRecovery,
        ReviewSessionCommand, ReviewSessionCommandEnvelope,
    },
    review_session_processor::{ProcessorCommandAdmission, ProcessorPrincipal},
    review_session_transport::ReviewSessionCommandExecutor,
};

pub(crate) type DailyGameReviewFuture<'a> =
    Pin<Box<dyn Future<Output = DailyGameReviewResult> + Send + 'a>>;

pub(crate) trait DailyGameReviewer: Send + Sync {
    fn review<'a>(
        &'a self,
        player_id: &'a crate::review_session_contract::PlayerId,
        request: &'a DailyGameReviewRequest,
    ) -> DailyGameReviewFuture<'a>;
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DailyGameReviewResult {
    Reviewed {
        game_import_id: GameImportId,
        imported_game: Box<ImportedGame>,
        review: Box<GameReview>,
    },
    Retryable {
        retry_after_seconds: Option<u32>,
    },
    Terminal,
}

pub(crate) struct CommandExecutorDailyGameReviewer {
    executor: Arc<dyn DailyGameReviewExecutor>,
}

impl CommandExecutorDailyGameReviewer {
    pub(crate) fn new(executor: Arc<dyn DailyGameReviewExecutor>) -> Self {
        Self { executor }
    }

    #[cfg(test)]
    pub(crate) fn from_command_executor(executor: Arc<dyn ReviewSessionCommandExecutor>) -> Self {
        Self::new(Arc::new(PublicCommandExecutorAdapter { executor }))
    }
}

pub(crate) trait DailyGameReviewExecutor: Send + Sync {
    fn submit(
        self: Arc<Self>,
        player_id: PlayerId,
        request_id: RequestId,
        operation_id: OperationId,
        request: DailyGameReviewRequest,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope>;
}

impl<C> DailyGameReviewExecutor for ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    fn submit(
        self: Arc<Self>,
        player_id: PlayerId,
        request_id: RequestId,
        operation_id: OperationId,
        request: DailyGameReviewRequest,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        self.submit_daily_game(player_id, request_id, operation_id, request)
    }
}

#[cfg(test)]
struct PublicCommandExecutorAdapter {
    executor: Arc<dyn ReviewSessionCommandExecutor>,
}

#[cfg(test)]
impl DailyGameReviewExecutor for PublicCommandExecutorAdapter {
    fn submit(
        self: Arc<Self>,
        player_id: PlayerId,
        request_id: RequestId,
        operation_id: OperationId,
        request: DailyGameReviewRequest,
    ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
        let DailyGameInputSource::LichessUrl { url } = request.source else {
            let (sender, receiver) = mpsc::unbounded_channel();
            sender
                .send(ReviewSessionEventEnvelope {
                    request_id,
                    operation_id,
                    sequence: 0,
                    event: ReviewSessionEvent::Rejected {
                        operation: OperationKind::GameImport,
                        reason: CommandRejectionReason::InvalidCommand,
                        recovery: RejectionRecovery::None,
                    },
                })
                .expect("the test adapter owns the Daily Coaching receiver");
            return receiver;
        };
        let envelope = ReviewSessionCommandEnvelope {
            request_id,
            operation_id,
            surface: DeliverySurface::CoachApp,
            command: ReviewSessionCommand::ImportGame {
                source: crate::review_session_contract::GameInputSource::LichessUrl { url },
                review_side: request.review_side,
                elo_profile: request.elo_profile,
            },
        };
        let bytes = serde_json::to_vec(&envelope)
            .expect("a Daily Coaching command envelope is serializable");
        self.executor.clone().submit(
            ProcessorPrincipal::Player(player_id),
            ProcessorCommandAdmission::parse(&bytes),
        )
    }
}

impl DailyGameReviewer for CommandExecutorDailyGameReviewer {
    fn review<'a>(
        &'a self,
        player_id: &'a crate::review_session_contract::PlayerId,
        request: &'a DailyGameReviewRequest,
    ) -> DailyGameReviewFuture<'a> {
        Box::pin(async move {
            let nonce = uuid::Uuid::new_v4();
            let request_id = RequestId::try_from(format!("request:daily-coaching:{nonce}"))
                .expect("a Daily Coaching UUID produces a valid request ID");
            let operation_id = OperationId::try_from(format!("operation:daily-coaching:{nonce}"))
                .expect("a Daily Coaching UUID produces a valid operation ID");
            let mut events = self.executor.clone().submit(
                player_id.clone(),
                request_id,
                operation_id,
                request.clone(),
            );
            while let Some(envelope) = events.recv().await {
                match envelope.event {
                    ReviewSessionEvent::Accepted { .. } | ReviewSessionEvent::Progress { .. } => {}
                    ReviewSessionEvent::Completed { result } => {
                        return match *result {
                            OperationCompletion::GameImported {
                                game_import_id,
                                review,
                                imported_game: Some(imported_game),
                                ..
                            } => DailyGameReviewResult::Reviewed {
                                game_import_id,
                                imported_game,
                                review,
                            },
                            _ => DailyGameReviewResult::Terminal,
                        };
                    }
                    ReviewSessionEvent::Unavailable { retry, .. } => {
                        return retry_result(retry);
                    }
                    ReviewSessionEvent::Rejected { .. }
                    | ReviewSessionEvent::Conflict { .. }
                    | ReviewSessionEvent::Cancelled { .. }
                    | ReviewSessionEvent::ReviewMomentUnavailable { .. } => {
                        return DailyGameReviewResult::Terminal;
                    }
                }
            }
            DailyGameReviewResult::Retryable {
                retry_after_seconds: None,
            }
        })
    }
}

fn retry_result(retry: RetryDirective) -> DailyGameReviewResult {
    match retry {
        RetryDirective::RetryAllowed | RetryDirective::StartNewOperation => {
            DailyGameReviewResult::Retryable {
                retry_after_seconds: None,
            }
        }
        RetryDirective::RetryAfter { seconds } => DailyGameReviewResult::Retryable {
            retry_after_seconds: Some(seconds),
        },
        RetryDirective::NotRetryable => DailyGameReviewResult::Terminal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_session_contract::{
        GameInputSource, PlayerId, RequestedEloProfile, RequestedReviewSide,
        ReviewSessionCommandEnvelope, ReviewSessionEventEnvelope, ReviewSide,
    };
    use std::sync::Mutex;
    use tokio::sync::mpsc;

    #[test]
    fn only_retryable_directives_schedule_another_attempt() {
        assert_eq!(
            retry_result(RetryDirective::RetryAllowed),
            DailyGameReviewResult::Retryable {
                retry_after_seconds: None
            }
        );
        assert_eq!(
            retry_result(RetryDirective::RetryAfter { seconds: 17 }),
            DailyGameReviewResult::Retryable {
                retry_after_seconds: Some(17)
            }
        );
        assert_eq!(
            retry_result(RetryDirective::NotRetryable),
            DailyGameReviewResult::Terminal
        );
    }

    #[tokio::test]
    async fn submits_fresh_single_game_import_operations_under_the_player_principal() {
        let executor = Arc::new(RecordingExecutor::default());
        let reviewer = CommandExecutorDailyGameReviewer::from_command_executor(executor.clone());
        let player_id = PlayerId::try_from("firebase-player".to_string()).unwrap();
        let request = DailyGameReviewRequest {
            source: DailyGameInputSource::LichessUrl {
                url: "https://lichess.org/Synthet1".to_string(),
            },
            review_side: RequestedReviewSide::Selected {
                review_side: ReviewSide::Black,
            },
            elo_profile: RequestedEloProfile::FromImportedMetadata,
            ended_at_unix_milliseconds: Some(1_786_291_200_000),
        };

        assert!(matches!(
            reviewer.review(&player_id, &request).await,
            DailyGameReviewResult::Retryable { .. }
        ));
        assert!(matches!(
            reviewer.review(&player_id, &request).await,
            DailyGameReviewResult::Retryable { .. }
        ));
        let submissions = executor
            .submissions
            .lock()
            .expect("recording executor is not poisoned");

        assert_eq!(submissions.len(), 2);
        assert!(submissions.iter().all(|(principal, envelope)| {
            principal == &ProcessorPrincipal::Player(player_id.clone())
                && matches!(
                    &envelope.command,
                    ReviewSessionCommand::ImportGame {
                        source: GameInputSource::LichessUrl { url },
                        review_side: RequestedReviewSide::Selected {
                            review_side: ReviewSide::Black
                        },
                        elo_profile: RequestedEloProfile::FromImportedMetadata,
                    } if url == "https://lichess.org/Synthet1"
                )
        }));
        assert_ne!(
            submissions[0].1.operation_id.as_str(),
            submissions[1].1.operation_id.as_str()
        );
        assert_ne!(
            submissions[0].1.request_id.as_str(),
            submissions[1].1.request_id.as_str()
        );
    }

    #[derive(Default)]
    struct RecordingExecutor {
        submissions: Mutex<Vec<(ProcessorPrincipal, ReviewSessionCommandEnvelope)>>,
    }

    impl ReviewSessionCommandExecutor for RecordingExecutor {
        fn submit(
            self: Arc<Self>,
            principal: ProcessorPrincipal,
            admission: ProcessorCommandAdmission,
        ) -> mpsc::UnboundedReceiver<ReviewSessionEventEnvelope> {
            self.submissions
                .lock()
                .expect("recording executor is not poisoned")
                .push((
                    principal,
                    admission
                        .envelope()
                        .expect("Daily Coaching submits a valid command")
                        .clone(),
                ));
            let (_sender, receiver) = mpsc::unbounded_channel();
            receiver
        }
    }
}
