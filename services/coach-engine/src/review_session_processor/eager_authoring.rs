use std::sync::Arc;

use tracing::Instrument;

use crate::{
    lichess::LichessExportClient,
    review_session_contract::{GameImportId, OperationKind},
};

use super::{
    events::EventEmitter, readiness::FirstOpenHostedComment, session::WebOpeningComment,
    ProcessorPrincipal, ReviewSessionProcessor,
};

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    /// Authors every Review Moment's web coaching comment when a Review
    /// Session is first routed onto the web surface, so its moments open on
    /// stored artifacts instead of authoring one by one.
    ///
    /// The web routing is deliberately the lazy trigger: an import spends no
    /// Language Layer budget and adds nothing to the importing surface's
    /// call, and a Game nobody reviews on the web never authors at all.
    /// Fire-and-forget: a moment the hosted Language Layer refuses or fails
    /// settles exactly as it would have on a live first open. Without a bound
    /// hosted Language Layer there is nothing durable to author, so the task
    /// is not spawned at all.
    pub(super) fn schedule_web_artifact_authoring(
        self: &Arc<Self>,
        principal: ProcessorPrincipal,
        game_import_id: GameImportId,
        emitter: &EventEmitter,
    ) {
        if !self.eager_web_artifacts || self.hosted_comment.is_none() {
            return;
        }
        if !matches!(principal, ProcessorPrincipal::Player(_)) {
            return;
        }
        /* The import operation's ids carry into the background telemetry, and
        the receiver is dropped because no surface is listening. */
        let (emitter, receiver) = EventEmitter::new(
            emitter.request_id().clone(),
            emitter.operation_id().clone(),
            OperationKind::ReviewSessionStart,
            None,
            0.0,
        );
        drop(receiver);
        let processor = self.clone();
        let span = tracing::info_span!(
            "web_artifact_authoring",
            game_import_id = game_import_id.as_str(),
        );
        tokio::spawn(
            async move {
                processor
                    .author_web_artifacts(principal, game_import_id, emitter)
                    .await;
            }
            .instrument(span),
        );
    }

    async fn author_web_artifacts(
        &self,
        principal: ProcessorPrincipal,
        game_import_id: GameImportId,
        emitter: Arc<EventEmitter>,
    ) {
        let ProcessorPrincipal::Player(player_id) = principal.clone() else {
            return;
        };
        let session = match self.load_session(&principal, &game_import_id).await {
            Ok(session) => session,
            Err(_) => {
                tracing::info!(
                    event = "coach_web_artifact_authoring_completion",
                    game_import_id = game_import_id.as_str(),
                    status = "session_unavailable",
                    "web artifact authoring found no session to author"
                );
                return;
            }
        };
        if !self
            .prepare_complete_authoring_batch(
                &principal,
                &session,
                &emitter,
                OperationKind::ReviewSessionStart,
            )
            .await
        {
            tracing::warn!(
                event = "coach_web_artifact_authoring_completion",
                game_import_id = game_import_id.as_str(),
                status = "preparation_degraded",
                "web artifact authoring could not complete the moment batch"
            );
            return;
        }
        let mut entries = session.review_moment_entries().await;
        entries.sort_by_key(|entry| entry.ply());
        let mut authored = 0_usize;
        let mut settled = 0_usize;
        for entry in entries {
            let Some(review_moment) = entry.prepared_moment().await else {
                continue;
            };
            /* A moment carrying prose from the prompt this build compiles is
            done. One an edited prompt left behind is authored again, exactly
            as if it had never been written. */
            let superseded = match review_moment.web_opening_comment().await {
                WebOpeningComment::Current(_) => {
                    settled += 1;
                    continue;
                }
                WebOpeningComment::Stale(superseded) => Some(Box::new(superseded)),
                WebOpeningComment::Absent => None,
            };
            let authoring_context = review_moment
                .opening_authoring_context()
                .await
                .map(Box::new);
            if authoring_context.is_none() {
                continue;
            }
            let outcome = self
                .author_first_open_hosted_comment(FirstOpenHostedComment {
                    session: &session,
                    review_moment: &review_moment,
                    game_import_id: &game_import_id,
                    player_id: &player_id,
                    authoring_context,
                    superseded,
                    emitter: &emitter,
                })
                .await;
            match outcome {
                Some((_, true, _)) => authored += 1,
                _ => settled += 1,
            }
        }
        tracing::info!(
            event = "coach_web_artifact_authoring_completion",
            game_import_id = game_import_id.as_str(),
            authored,
            settled,
            status = "finished",
            "web coaching artifacts settled after import"
        );
    }
}
