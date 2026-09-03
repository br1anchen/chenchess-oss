use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    critical_moment_comment::{author_grounded_comment, safely_rendered_comment, Reauthor},
    lichess::LichessExportClient,
    operating_limits::REVIEW_MOMENT_PREPARATION_DEADLINE_SECONDS,
    review_durability::{
        path::hashed_path_segment, review_moment_coach_turn_id, review_moment_request_id,
    },
    review_session_cancellation::ReviewSessionCancellation,
    review_session_contract::*,
    review_session_start::start_review_session,
};

use super::{
    admission::EngineWorkload,
    events::EventEmitter,
    lifecycle::resolve_engine_admission,
    mutation::SessionMutationPersistence,
    prefetch::{log_prefetch_lookup, ReviewMomentPrefetchContext, ReviewMomentPrefetchReuse},
    session::{
        CommentPublicationStage, CommentPublicationStageError, ProcessorReviewMoment,
        ProcessorReviewMomentEntry, ProcessorSession, ReviewMomentCommentPublication,
        SessionBuildError, SupersededComment, WebOpeningComment,
    },
    LiveOperation, ProcessorPrincipal, ReviewSessionProcessor,
};

struct OpenReviewMomentCompletion {
    prior_revision: Option<u64>,
    surface: DeliverySurface,
    generation: u64,
}

/// What opening a Review Moment resolves its comment to: the comment to show,
/// whether it is durably published, and the authoring context to disclose when
/// it is not.
type OpeningComment = (
    Option<Box<CriticalMomentComment>>,
    bool,
    Option<Box<ReviewMomentCommentAuthoringContext>>,
);

/// Prose that is already durably published, so there is no context to disclose.
fn served(comment: Box<CriticalMomentComment>) -> Option<OpeningComment> {
    Some((Some(comment), true, None))
}

pub(super) struct FirstOpenHostedComment<'a> {
    pub(super) session: &'a Arc<ProcessorSession>,
    pub(super) review_moment: &'a ProcessorReviewMoment,
    pub(super) game_import_id: &'a GameImportId,
    pub(super) player_id: &'a PlayerId,
    pub(super) authoring_context: Option<Box<ReviewMomentCommentAuthoringContext>>,
    /// Prose an edited prompt left behind, to serve if re-authoring cannot land.
    pub(super) superseded: Option<Box<SupersededComment>>,
    pub(super) emitter: &'a EventEmitter,
}

pub(super) struct OpenReviewMomentRequest {
    pub(super) principal: ProcessorPrincipal,
    pub(super) operation_id: OperationId,
    pub(super) game_import_id: GameImportId,
    pub(super) selection: ReviewMomentSelection,
    pub(super) idempotency_key: IdempotencyKey,
    pub(super) surface: DeliverySurface,
}

struct ReviewMomentPreparationContext<'a> {
    principal: &'a ProcessorPrincipal,
    operation_id: &'a OperationId,
    game_import_id: &'a GameImportId,
    review_moment_id: &'a CriticalMomentId,
    idempotency_key: &'a IdempotencyKey,
    emitter: &'a EventEmitter,
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    pub(super) async fn prepare_complete_authoring_batch(
        &self,
        principal: &ProcessorPrincipal,
        session: &Arc<ProcessorSession>,
        emitter: &EventEmitter,
        operation: OperationKind,
    ) -> bool {
        let _mutation = session.begin_mutation().await;
        let entries = session.review_moment_entries().await;
        if entries.is_empty() {
            return true;
        }
        let mut has_pending = false;
        for entry in &entries {
            has_pending |= entry.prepared_moment().await.is_none();
        }
        if !has_pending {
            return true;
        }
        emitter.event(ReviewSessionEvent::Progress {
            stage: OperationProgress::ReviewSession {
                stage: ReviewSessionProgressStage::PreparingEvidence,
            },
        });
        let deadline =
            Instant::now() + Duration::from_secs(REVIEW_MOMENT_PREPARATION_DEADLINE_SECONDS);
        let Some(engine_lease) = resolve_engine_admission(
            self.engine_admission
                .acquire_until(EngineWorkload::Batch, principal, deadline)
                .await,
            operation,
            emitter,
        ) else {
            return false;
        };
        for entry in entries {
            if entry.prepared_moment().await.is_some() {
                continue;
            }
            let started_at = Instant::now();
            let base_revision = session.checkpoint_revision().await;
            let prepared = match entry.prefetched_candidate(base_revision).await {
                Some(prepared) => {
                    log_prefetch_lookup(entry.moment_id(), true, base_revision);
                    prepared
                }
                None => {
                    log_prefetch_lookup(entry.moment_id(), false, base_revision);
                    match entry
                        .prepare_candidate(
                            self.recording.as_deref(),
                            self.engine.clone(),
                            self.human.clone(),
                            session.annotations().clone(),
                            deadline,
                        )
                        .await
                    {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            tracing::error!(
                                category = error.diagnostic_category(),
                                review_moment_id = entry.moment_id().as_str(),
                                "Review Moment complete-batch preparation failed"
                            );
                            emitter.rejected(
                                operation,
                                CommandRejectionReason::MissingEvidence,
                                RejectionRecovery::None,
                            );
                            return false;
                        }
                    }
                }
            };
            let Some(checkpoint) = prepared.prepared_checkpoint().await else {
                emitter.unavailable(
                    operation,
                    ProviderUnavailableReason::Persistence,
                    RetryDirective::RetryAllowed,
                );
                return false;
            };
            let Some(successor) = self
                .persist_session_mutation(
                    session,
                    SessionMutationPersistence::business(&checkpoint, operation, None),
                    emitter,
                )
                .await
            else {
                return false;
            };
            entry.install_prepared(prepared).await;
            session.commit_checkpoint_successor(&successor).await;
            tracing::info!(
                event = "coach_review_moment_authoring_completion",
                review_moment_id = entry.moment_id().as_str(),
                wall_milliseconds = started_at.elapsed().as_millis(),
                "review moment complete-batch preparation metrics"
            );
        }
        drop(engine_lease);
        true
    }

    pub(super) async fn open_review_moment(
        &self,
        request: OpenReviewMomentRequest,
        emitter: Arc<EventEmitter>,
    ) {
        let OpenReviewMomentRequest {
            principal,
            operation_id,
            game_import_id,
            selection,
            idempotency_key,
            surface,
        } = request;
        let Some(session) = self
            .session(
                &game_import_id,
                &principal,
                &emitter,
                OperationKind::ReviewMomentOpen,
            )
            .await
        else {
            return;
        };
        let imported = session.game_import();
        let coach_turn_id = review_moment_coach_turn_id(&game_import_id, &selection)
            .expect("a valid Review Session and Moment produce a Coach Turn ID");
        let moment_request_id = review_moment_request_id(&game_import_id, &selection)
            .expect("a valid Review Session and Moment produce a Request ID");
        let core = match start_review_session(
            moment_request_id,
            coach_turn_id,
            imported.imported_game.clone(),
            selection.clone(),
        ) {
            Ok(core) => core,
            Err(error) => {
                super::terminal::emit_start_error(&emitter, OperationKind::ReviewMomentOpen, error);
                return;
            }
        };
        let moment_id = core.review_moment.moment_id.clone();
        session.cancel_prefetches_except(Some(&moment_id)).await;
        let open_generation = session.begin_open_review_moment().await;
        if let Some(entry) = session.review_moment_entry(&moment_id).await {
            let _mutation = session.begin_mutation().await;
            if let Some(existing) = entry.prepared_moment().await {
                drop(_mutation);
                self.complete_open_review_moment(
                    &emitter,
                    &game_import_id,
                    &session,
                    &existing,
                    OpenReviewMomentCompletion {
                        prior_revision: None,
                        surface,
                        generation: open_generation,
                    },
                )
                .await;
                return;
            }
            let base_revision = session.checkpoint_revision().await;
            let prefetch_reuse = if entry.has_prefetched_candidate(base_revision).await {
                self.await_review_moment_prefetch(
                    ReviewMomentPrefetchContext {
                        principal: &principal,
                        operation_id: &operation_id,
                        game_import_id: &game_import_id,
                        review_moment_id: &moment_id,
                        idempotency_key: &idempotency_key,
                        emitter: &emitter,
                    },
                    &entry,
                    base_revision,
                )
                .await
            } else {
                log_prefetch_lookup(entry.moment_id(), false, base_revision);
                ReviewMomentPrefetchReuse::Miss
            };
            let prepared = match prefetch_reuse {
                ReviewMomentPrefetchReuse::Hit(prepared) => prepared,
                ReviewMomentPrefetchReuse::Terminal => return,
                ReviewMomentPrefetchReuse::Miss => {
                    let Some(prepared) = self
                        .run_review_moment_preparation(
                            ReviewMomentPreparationContext {
                                principal: &principal,
                                operation_id: &operation_id,
                                game_import_id: &game_import_id,
                                review_moment_id: &moment_id,
                                idempotency_key: &idempotency_key,
                                emitter: &emitter,
                            },
                            |deadline| {
                                entry.prepare_candidate(
                                    self.recording.as_deref(),
                                    self.engine.clone(),
                                    self.human.clone(),
                                    session.annotations().clone(),
                                    deadline,
                                )
                            },
                        )
                        .await
                    else {
                        return;
                    };
                    prepared
                }
            };
            let Some(checkpoint) = prepared.prepared_checkpoint().await else {
                emit_review_moment_persistence_unavailable(&emitter, &game_import_id, &moment_id);
                return;
            };
            let Some(successor) = self
                .persist_session_mutation(
                    &session,
                    SessionMutationPersistence::business(
                        &checkpoint,
                        OperationKind::ReviewMomentOpen,
                        Some(&moment_id),
                    ),
                    &emitter,
                )
                .await
            else {
                return;
            };
            entry.install_prepared(prepared.clone()).await;
            session.commit_checkpoint_successor(&successor).await;
            drop(_mutation);
            self.complete_open_review_moment(
                &emitter,
                &game_import_id,
                &session,
                &prepared,
                OpenReviewMomentCompletion {
                    prior_revision: Some(successor.expected_revision),
                    surface,
                    generation: open_generation,
                },
            )
            .await;
            return;
        }
        let ReviewMomentSelection::PlayerSelectedMoment { ply } = selection else {
            emitter.rejected(
                OperationKind::ReviewMomentOpen,
                CommandRejectionReason::UnknownMoment,
                RejectionRecovery::CorrectInput,
            );
            return;
        };
        let Some(imported_moment) = imported.player_selected_moment(ply) else {
            emitter.rejected(
                OperationKind::ReviewMomentOpen,
                CommandRejectionReason::MissingEvidence,
                RejectionRecovery::None,
            );
            return;
        };
        let factual_moment = match crate::player_selected_decision::materialize(
            &core.review_moment.game_ref,
            &core.position_snapshot,
            imported_moment,
        ) {
            Ok(moment) => moment,
            Err(error) => {
                tracing::error!(
                    error = %error,
                    ply,
                    "Player-Selected Decision Explanation construction failed"
                );
                emitter.rejected(
                    OperationKind::ReviewMomentOpen,
                    CommandRejectionReason::MissingEvidence,
                    RejectionRecovery::None,
                );
                return;
            }
        };
        let preparation_core = core.clone();
        let preparation_facts = factual_moment.clone();
        let engine = self.engine.clone();
        let human = self.human.clone();
        let annotations = session.annotations().clone();
        let Some(review_moment) = self
            .run_review_moment_preparation(
                ReviewMomentPreparationContext {
                    principal: &principal,
                    operation_id: &operation_id,
                    game_import_id: &game_import_id,
                    review_moment_id: &moment_id,
                    idempotency_key: &idempotency_key,
                    emitter: &emitter,
                },
                move |deadline| async move {
                    let review_moment = Arc::new(
                        ProcessorReviewMoment::new_with_projection_deadline(
                            preparation_core,
                            None,
                            engine,
                            human,
                            Some(&preparation_facts),
                            annotations,
                            deadline,
                        )
                        .await?,
                    );
                    Ok::<_, SessionBuildError>(review_moment)
                },
            )
            .await
        else {
            return;
        };
        let Some(prepared) = review_moment.prepared_checkpoint().await else {
            emit_review_moment_persistence_unavailable(&emitter, &game_import_id, &moment_id);
            return;
        };
        let _mutation = session.begin_mutation().await;
        if let Some(existing) = session.review_moment(&moment_id).await {
            drop(_mutation);
            self.complete_open_review_moment(
                &emitter,
                &game_import_id,
                &session,
                &existing,
                OpenReviewMomentCompletion {
                    prior_revision: None,
                    surface,
                    generation: open_generation,
                },
            )
            .await;
            return;
        }
        let Some(successor) = self
            .persist_session_mutation(
                &session,
                SessionMutationPersistence::business(
                    &prepared,
                    OperationKind::ReviewMomentOpen,
                    Some(&moment_id),
                ),
                &emitter,
            )
            .await
        else {
            return;
        };
        let entry = Arc::new(ProcessorReviewMomentEntry::from_prepared(
            core,
            Some(factual_moment),
            review_moment.clone(),
        ));
        if let Err(SessionBuildError::DuplicateReviewMoment(_)) =
            session.insert_review_moment(entry).await
        {
            session.commit_checkpoint_successor(&successor).await;
            let existing = session
                .review_moment(&moment_id)
                .await
                .expect("a duplicate review moment remains available");
            drop(_mutation);
            self.complete_open_review_moment(
                &emitter,
                &game_import_id,
                &session,
                &existing,
                OpenReviewMomentCompletion {
                    prior_revision: Some(successor.expected_revision),
                    surface,
                    generation: open_generation,
                },
            )
            .await;
            return;
        }
        session.commit_checkpoint_successor(&successor).await;
        drop(_mutation);
        self.complete_open_review_moment(
            &emitter,
            &game_import_id,
            &session,
            &review_moment,
            OpenReviewMomentCompletion {
                prior_revision: Some(successor.expected_revision),
                surface,
                generation: open_generation,
            },
        )
        .await;
    }

    async fn run_review_moment_preparation<T, Prepare, Preparation>(
        &self,
        context: ReviewMomentPreparationContext<'_>,
        preparation: Prepare,
    ) -> Option<T>
    where
        Prepare: FnOnce(Instant) -> Preparation,
        Preparation: Future<Output = Result<T, SessionBuildError>>,
    {
        let ReviewMomentPreparationContext {
            principal,
            operation_id,
            game_import_id,
            review_moment_id,
            idempotency_key,
            emitter,
        } = context;
        let cancellation = ReviewSessionCancellation::default();
        if !self
            .register_live(
                operation_id.clone(),
                LiveOperation::ReviewMomentPreparation {
                    owner: principal.clone(),
                    game_import_id: game_import_id.clone(),
                    idempotency_key: idempotency_key.clone(),
                    cancellation: cancellation.clone(),
                },
            )
            .await
        {
            emitter.rejected(
                OperationKind::ReviewMomentOpen,
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            );
            return None;
        }
        emit_review_moment_preparation_progress(
            emitter,
            game_import_id,
            review_moment_id,
            ReviewMomentPreparationProgressStage::WaitingForCapacity,
        );
        let deadline =
            Instant::now() + Duration::from_secs(REVIEW_MOMENT_PREPARATION_DEADLINE_SECONDS);
        let admission = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                self.live.lock().await.remove(operation_id);
                emitter.cancelled(OperationKind::ReviewMomentOpen);
                return None;
            }
            admission = self.engine_admission.acquire_until(EngineWorkload::Batch, principal, deadline) => admission,
        };
        let engine_lease = match admission {
            Ok(lease) => lease,
            Err(reason) => {
                self.live.lock().await.remove(operation_id);
                emitter.review_moment_unavailable(
                    game_import_id,
                    review_moment_id,
                    reason,
                    RetryDirective::StartNewOperation,
                );
                return None;
            }
        };
        emit_review_moment_preparation_progress(
            emitter,
            game_import_id,
            review_moment_id,
            ReviewMomentPreparationProgressStage::PreparingAuthoringContext,
        );
        let prepared = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                self.live.lock().await.remove(operation_id);
                emitter.cancelled(OperationKind::ReviewMomentOpen);
                return None;
            }
            prepared = preparation(deadline) => prepared,
        };
        drop(engine_lease);
        self.live.lock().await.remove(operation_id);
        if cancellation.is_cancelled() {
            emitter.cancelled(OperationKind::ReviewMomentOpen);
            return None;
        }
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::error!(
                    category = error.diagnostic_category(),
                    review_moment_id = review_moment_id.as_str(),
                    "Review Moment on-demand preparation failed"
                );
                emit_review_moment_preparation_error(
                    emitter,
                    game_import_id,
                    review_moment_id,
                    &error,
                );
                return None;
            }
        };
        emit_review_moment_preparation_progress(
            emitter,
            game_import_id,
            review_moment_id,
            ReviewMomentPreparationProgressStage::CommittingAuthoringContext,
        );
        Some(prepared)
    }
}

pub(super) fn surface_requires_complete_authoring(surface: DeliverySurface) -> bool {
    matches!(surface, DeliverySurface::Web | DeliverySurface::CoachSkill)
}

impl<C> ReviewSessionProcessor<C>
where
    C: LichessExportClient + 'static,
{
    async fn complete_open_review_moment(
        &self,
        emitter: &EventEmitter,
        game_import_id: &GameImportId,
        session: &Arc<ProcessorSession>,
        review_moment: &ProcessorReviewMoment,
        completion: OpenReviewMomentCompletion,
    ) {
        let session_revision = session.checkpoint_revision().await;
        let Some((comment, comment_published, authoring_context)) = self
            .opening_comment_for_surface(
                session,
                review_moment,
                game_import_id,
                completion.surface,
                emitter,
            )
            .await
        else {
            return;
        };
        let decision_explanation_ref = review_moment
            .decision_explanation()
            .map(|explanation| explanation.decision_explanation_ref);
        let critical_moment = review_moment.critical_moment().clone();
        let review_moment = review_moment.core_snapshot().await;
        session
            .record_open_review_moment(review_moment.review_moment.ply, completion.generation)
            .await;
        emitter.completed(OperationCompletion::ReviewMomentOpened {
            game_import_id: game_import_id.clone(),
            session_revision,
            revision_delta: ReviewSessionRevisionDelta {
                prior_revision: completion.prior_revision.unwrap_or(session_revision),
                resulting_revision: session_revision,
                changed_moment_ids: vec![review_moment.review_moment.moment_id.clone()],
                full_refresh_required: false,
            },
            review_moment: Box::new(review_moment),
            critical_moment: Box::new(critical_moment),
            decision_explanation_ref,
            comment,
            comment_published,
            authoring_context,
        });
    }

    async fn opening_comment_for_surface(
        &self,
        session: &Arc<ProcessorSession>,
        review_moment: &ProcessorReviewMoment,
        game_import_id: &GameImportId,
        surface: DeliverySurface,
        emitter: &EventEmitter,
    ) -> Option<(
        Option<Box<CriticalMomentComment>>,
        bool,
        Option<Box<ReviewMomentCommentAuthoringContext>>,
    )> {
        if surface == DeliverySurface::CoachApp {
            /* Engine-hosted web artifacts stay web-only for now: the Coach App
            sees only a comment its own host model published, and otherwise
            keeps authoring exactly as if nothing were published — so a
            web-only artifact must not mute the authoring context either. */
            if let Some(comment) = review_moment.host_submitted_opening_comment().await {
                return served(Box::new(comment));
            }
            let authoring_context = review_moment
                .comment_authoring_context()
                .await
                .map(Box::new);
            let comment = authoring_context.as_ref().map(|context| {
                Box::new(safely_rendered_comment(
                    &context.facts,
                    context.intent.clone(),
                ))
            });
            return Some((comment, false, authoring_context));
        }
        /* Only a web open owned by a Player, with a bound hosted Language
        Layer, can author a comment — whether founding one or replacing prose
        an edited prompt left behind. Every other reader serves what is stored,
        so a surface that cannot author never pays Intent Enrichment for work
        it could not perform. */
        let hosted_author = match (&self.hosted_comment, &session.owner) {
            (Some(_), ProcessorPrincipal::Player(player_id)) if surface == DeliverySurface::Web => {
                Some(player_id)
            }
            _ => None,
        };
        let superseded = match review_moment.web_opening_comment().await {
            WebOpeningComment::Current(comment) => return served(Box::new(comment)),
            WebOpeningComment::Stale(superseded) if hosted_author.is_none() => {
                return served(Box::new(superseded.comment))
            }
            WebOpeningComment::Stale(superseded) => Some(Box::new(superseded)),
            WebOpeningComment::Absent => None,
        };
        let authoring_context = review_moment
            .opening_authoring_context()
            .await
            .map(Box::new);
        if let Some(player_id) = hosted_author {
            return self
                .author_first_open_hosted_comment(FirstOpenHostedComment {
                    session,
                    review_moment,
                    game_import_id,
                    player_id,
                    authoring_context,
                    superseded,
                    emitter,
                })
                .await;
        }
        let (comment, comment_published) = review_moment
            .opening_comment(authoring_context.as_deref())
            .await
            .map_or((None, false), |(comment, published)| {
                (Some(Box::new(comment)), published)
            });
        Some((comment, comment_published, authoring_context))
    }

    pub(super) async fn author_first_open_hosted_comment(
        &self,
        open: FirstOpenHostedComment<'_>,
    ) -> Option<OpeningComment> {
        let FirstOpenHostedComment {
            session,
            review_moment,
            game_import_id,
            player_id,
            authoring_context,
            superseded,
            emitter,
        } = open;
        let hosted = self.hosted_comment.as_ref()?;
        let digest = hosted.fingerprint.digest.as_str().to_string();
        let flight_key = (
            game_import_id.clone(),
            review_moment.moment_id().clone(),
            digest.clone(),
        );
        match self.first_open_flights.register(flight_key) {
            Err(waiter) => {
                waiter.wait().await;
                /* Whatever the leader settled on is what every follower shows.
                A leader that could not land leaves the superseded prose
                active, which still beats a template rendering. */
                match review_moment.web_opening_comment().await {
                    WebOpeningComment::Current(comment) => return served(Box::new(comment)),
                    WebOpeningComment::Stale(superseded) => {
                        return served(Box::new(superseded.comment))
                    }
                    WebOpeningComment::Absent => {}
                }
                let comment = authoring_context.as_ref().map(|context| {
                    Box::new(safely_rendered_comment(
                        &context.facts,
                        context.intent.clone(),
                    ))
                });
                Some((comment, false, authoring_context))
            }
            Ok(_leader) => {
                let Some(context) = authoring_context.as_ref() else {
                    return match superseded {
                        Some(superseded) => served(Box::new(superseded.comment)),
                        None => Some((None, false, None)),
                    };
                };
                let profile = self.current_coaching_profile();
                let author = hosted.author(
                    player_id.clone(),
                    session.spend.clone(),
                    profile.clone(),
                    session.captures.clone(),
                );
                // Hosted authoring is outside any session mutation guard.
                // Callers of complete_open_review_moment must drop begin_mutation()
                // before this future; persist re-acquires below.
                /* Decided before authoring, because a rendering written on
                this open has to carry the mark that stops the next one. */
                let retrying = superseded
                    .as_ref()
                    .is_some_and(|superseded| matches!(superseded.reason, Reauthor::RetryFallback));
                let grounded = match author_grounded_comment(
                    &author,
                    context.facts.clone(),
                    context.intent.clone(),
                    retrying,
                )
                .await
                {
                    Ok(mut grounded) => {
                        grounded.authoring_provenance =
                            grounded.authoring_provenance.with_coaching_profile(profile);
                        grounded
                    }
                    Err(_) => {
                        self.quality_capture
                            .commit_best_effort(
                                &ProcessorPrincipal::Player(player_id.clone()),
                                &session.captures.take(),
                            )
                            .await;
                        /* The rewrite failed, so the prose it was meant to
                        replace stays on screen. Only a moment that never had
                        one drops to the template rendering. */
                        if let Some(superseded) = superseded {
                            return served(Box::new(superseded.comment));
                        }
                        let comment =
                            safely_rendered_comment(&context.facts, context.intent.clone());
                        return Some((Some(Box::new(comment)), false, authoring_context));
                    }
                };
                /* A safe rendering comes back as an `Ok` that nothing
                downstream can tell from authored prose, so it has to be
                caught by its outcome here.

                It may found a Review Moment that has no comment, and it may
                replace an earlier fallback -- that is how a moment which once
                failed converges after the prompt moves. What it must never do
                is replace prose the Language Layer actually authored: an
                outage would cost the Player real coaching permanently,
                because the rendering carries the compiled digests and no
                later open would try again. */
                if superseded.as_ref().is_some_and(|superseded| {
                    matches!(superseded.reason, Reauthor::PromptEdited { authored: true })
                }) && matches!(
                    grounded.authoring_provenance.outcome,
                    CriticalMomentCommentGenerationOutcome::SafeRendered { .. }
                ) {
                    self.quality_capture
                        .commit_best_effort(
                            &ProcessorPrincipal::Player(player_id.clone()),
                            &grounded
                                .quality_captures
                                .into_iter()
                                .chain(session.captures.take())
                                .collect::<Vec<_>>(),
                        )
                        .await;
                    return superseded
                        .map(|superseded| (Some(Box::new(superseded.comment)), true, None));
                }
                let mut hosted_captures = grounded.quality_captures;
                hosted_captures.extend(session.captures.take());
                let key = first_open_idempotency_key(
                    game_import_id,
                    review_moment.moment_id(),
                    &digest,
                    retrying,
                );
                let _persist = session.begin_mutation().await;
                match review_moment
                    .stage_first_open_comment(key, grounded.comment, grounded.authoring_provenance)
                    .await
                {
                    Ok(CommentPublicationStage::Existing(
                        ReviewMomentCommentPublication::Published { comment, .. },
                    )) => served(Box::new(comment)),
                    Ok(CommentPublicationStage::Mutation(mut staged)) => {
                        staged.adopt_hosted_captures(hosted_captures);
                        match self
                            .persist_first_open_comment(session, review_moment, *staged, emitter)
                            .await
                        {
                            Some(ReviewMomentCommentPublication::Published { comment, .. }) => {
                                served(Box::new(comment))
                            }
                            /* The rewrite did not land. Prose that is already
                            published is still published, so this open shows
                            it rather than reporting the moment unavailable. */
                            Some(ReviewMomentCommentPublication::RetryRejected) | None => {
                                superseded.map(|superseded| {
                                    (Some(Box::new(superseded.comment)), true, None)
                                })
                            }
                        }
                    }
                    Ok(CommentPublicationStage::Existing(
                        ReviewMomentCommentPublication::RetryRejected,
                    )) => superseded
                        .map(|superseded| (Some(Box::new(superseded.comment)), true, None)),
                    Err(CommentPublicationStageError::MissingAuthority) => match superseded {
                        Some(superseded) => served(Box::new(superseded.comment)),
                        None => Some((None, false, None)),
                    },
                    Err(CommentPublicationStageError::InvalidCommand) => match superseded {
                        Some(superseded) => served(Box::new(superseded.comment)),
                        None => {
                            let comment =
                                safely_rendered_comment(&context.facts, context.intent.clone());
                            Some((Some(Box::new(comment)), false, authoring_context))
                        }
                    },
                }
            }
        }
    }
}

fn first_open_idempotency_key(
    game_import_id: &GameImportId,
    moment_id: &CriticalMomentId,
    digest: &str,
    retrying: bool,
) -> IdempotencyKey {
    let segment = hashed_path_segment(format!(
        "{}:{}:{}:{retrying}",
        game_import_id.as_str(),
        moment_id.as_str(),
        digest
    ));
    IdempotencyKey::try_from(format!("hosted:{segment}"))
        .expect("a derived first-open key is a valid semantic id")
}

fn emit_review_moment_preparation_progress(
    emitter: &EventEmitter,
    game_import_id: &GameImportId,
    review_moment_id: &CriticalMomentId,
    stage: ReviewMomentPreparationProgressStage,
) {
    emitter.event(ReviewSessionEvent::Progress {
        stage: OperationProgress::ReviewMomentPreparation {
            game_import_id: game_import_id.clone(),
            review_moment_id: review_moment_id.clone(),
            stage,
        },
    });
}

fn emit_review_moment_preparation_error(
    emitter: &EventEmitter,
    game_import_id: &GameImportId,
    review_moment_id: &CriticalMomentId,
    error: &SessionBuildError,
) {
    if let Some(reason) = error.unavailable_reason() {
        emitter.review_moment_unavailable(
            game_import_id,
            review_moment_id,
            reason,
            RetryDirective::RetryAllowed,
        );
    } else {
        emitter.rejected(
            OperationKind::ReviewMomentOpen,
            CommandRejectionReason::MissingEvidence,
            RejectionRecovery::None,
        );
    }
}

fn emit_review_moment_persistence_unavailable(
    emitter: &EventEmitter,
    game_import_id: &GameImportId,
    review_moment_id: &CriticalMomentId,
) {
    emitter.review_moment_unavailable(
        game_import_id,
        review_moment_id,
        ProviderUnavailableReason::Persistence,
        RetryDirective::RetryAllowed,
    );
}

#[cfg(test)]
mod first_open_key_tests {
    use super::*;

    #[test]
    fn first_open_key_is_hosted_hashed_segment() {
        let game_import_id =
            GameImportId::try_from(format!("game-import:{}:{}", "a".repeat(64), "b".repeat(32)))
                .unwrap();
        let moment_id = CriticalMomentId::try_from("moment:1".to_string()).unwrap();
        let digest = "sha256:".to_string() + &"c".repeat(64);
        let key = first_open_idempotency_key(&game_import_id, &moment_id, &digest, false);
        let expected = format!(
            "hosted:{}",
            hashed_path_segment(format!(
                "{}:{}:{}:false",
                game_import_id.as_str(),
                moment_id.as_str(),
                digest
            ))
        );
        assert_eq!(key.as_str(), expected);
        assert!(!key.as_str().contains("moment:1"));
        assert!(!key.as_str().contains(&digest));
    }
}
