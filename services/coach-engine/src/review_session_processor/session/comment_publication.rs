use std::collections::BTreeSet;

use chrono::Utc;

use crate::{
    critical_moment_comment::{
        ground_hosted_comment_text, grounding_ledger_for, safely_rendered_comment,
        validate_hosted_grounding_ledger, CriticalMomentCommentAuthoringProvenance,
    },
    quality_capture::QualityCaptureDraft,
    review_analysis_cache::{
        PreparedReviewSessionMoment, PublishedReviewMomentComment,
        ReviewMomentCommentPublicationCheckpoint, ReviewMomentCommentPublicationOutcome,
    },
    review_annotation_store::{
        ReviewAnnotationLog, ReviewAnnotationStoreError, ReviewMomentAnnotation,
    },
    review_session_contract::{
        CriticalMomentComment, CriticalMomentGroundingLedger, CriticalMomentId, IdempotencyKey,
        ReviewMomentCommentFacts,
    },
};

use super::{ProcessorReviewMoment, ReviewMomentCommentPublication};

pub(in crate::review_session_processor) struct StagedCommentPublication {
    idempotency_key: IdempotencyKey,
    comment_publication: ReviewMomentCommentPublicationCheckpoint,
    idempotency_keys: BTreeSet<IdempotencyKey>,
    checkpoint: PreparedReviewSessionMoment,
    facts: ReviewMomentCommentFacts,
    projected_plan_provenance: Option<crate::projected_plan::ProjectedPlanProvenance>,
    hosted_captures: Vec<QualityCaptureDraft>,
    pub(in crate::review_session_processor) result: ReviewMomentCommentPublication,
}

pub(in crate::review_session_processor) enum CommentPublicationStage {
    Existing(ReviewMomentCommentPublication),
    Mutation(Box<StagedCommentPublication>),
}

pub(in crate::review_session_processor) enum CommentPublicationStageError {
    MissingAuthority,
    InvalidCommand,
}

impl StagedCommentPublication {
    pub(in crate::review_session_processor) fn checkpoint(&self) -> &PreparedReviewSessionMoment {
        &self.checkpoint
    }

    pub(in crate::review_session_processor) fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    pub(in crate::review_session_processor) fn adopt_published_annotation(
        &mut self,
        annotation: ReviewMomentAnnotation,
    ) {
        let published = published_comment(annotation);
        self.comment_publication.record(
            self.idempotency_key.clone(),
            ReviewMomentCommentPublicationOutcome::Published(published.clone()),
        );
        self.comment_publication.adopt_active(published.clone());
        self.checkpoint.comment_publication = self.comment_publication.clone();
        self.result = ReviewMomentCommentPublication::Published {
            comment: published.comment,
            authoring_provenance: Box::new(published.authoring_provenance),
        };
    }

    pub(in crate::review_session_processor) fn adopt_hosted_captures(
        &mut self,
        captures: Vec<QualityCaptureDraft>,
    ) {
        self.hosted_captures = captures;
    }

    pub(in crate::review_session_processor) fn quality_captures(&self) -> Vec<QualityCaptureDraft> {
        if !self.hosted_captures.is_empty() {
            return self.hosted_captures.clone();
        }
        let ReviewMomentCommentPublication::Published {
            comment,
            authoring_provenance,
        } = &self.result
        else {
            return Vec::new();
        };
        match QualityCaptureDraft::coaching_response(
            &self.checkpoint.core,
            self.facts.clone(),
            comment.clone(),
            authoring_provenance.as_ref().clone(),
            self.projected_plan_provenance.clone(),
            Utc::now(),
        ) {
            Ok(capture) => vec![capture],
            Err(error) => {
                tracing::error!(%error, "completed coaching response was not capturable");
                Vec::new()
            }
        }
    }
}

impl ProcessorReviewMoment {
    /// Stages one logical Review Moment Comment publication.
    ///
    /// The Player's idempotency key selects the logical write. Replaying a key
    /// that already published returns that publication's own comment — from
    /// this conversation or from any other, because the annotation store is
    /// shared — so a double-tap or a retried request costs one comment. A key
    /// never seen before is a new logical write and lands beside the others,
    /// whatever was published before it: annotations are append-only and the
    /// newest is what the Player sees.
    pub(in crate::review_session_processor) async fn stage_comment_publication(
        &self,
        idempotency_key: IdempotencyKey,
        text: String,
        grounding_ledger: CriticalMomentGroundingLedger,
    ) -> Result<CommentPublicationStage, CommentPublicationStageError> {
        let current = self.comment_publication.lock().await.clone();
        // Resolving the key once leaves exactly two live cases below: this is a
        // first attempt, or it is the second attempt the Grounding Gate allowed.
        let open_retry = match current.attempt(&idempotency_key) {
            Some(ReviewMomentCommentPublicationOutcome::Published(published)) => {
                return Ok(CommentPublicationStage::Existing(
                    ReviewMomentCommentPublication::Published {
                        comment: published.comment.clone(),
                        authoring_provenance: Box::new(published.authoring_provenance.clone()),
                    },
                ));
            }
            Some(ReviewMomentCommentPublicationOutcome::RetryAllowed {
                generation_contract,
                grounding_ledger,
                ..
            }) => Some((generation_contract.clone(), grounding_ledger.clone())),
            // A key this session has not seen still goes the whole way. It may
            // have published in another conversation, or here before a
            // checkpoint write failed; either way the annotation store dedupes
            // the durable write, and re-running the rest is what carries the
            // Quality Capture and the checkpoint that the first attempt owed.
            None => None,
        };

        let authoring_context = self
            .comment_authoring_context()
            .await
            .ok_or(CommentPublicationStageError::MissingAuthority)?;
        let facts = authoring_context.facts;
        let intent = authoring_context.intent;
        let projected_plan_provenance = self.intent_authoring.provenance().await;
        if !facts.is_well_formed() {
            return Err(CommentPublicationStageError::InvalidCommand);
        }
        let canonical_ledger = grounding_ledger_for(&facts);
        let ledger_validation = validate_hosted_grounding_ledger(&facts, &grounding_ledger);

        let mut idempotency_keys = self.idempotency_keys.lock().await.clone();
        idempotency_keys.insert(idempotency_key.clone());
        let (outcome, result) = match open_retry {
            None => match ledger_validation {
                Err(first_rejection) => (
                    ReviewMomentCommentPublicationOutcome::RetryAllowed {
                        generation_contract:
                            CriticalMomentCommentAuthoringProvenance::hosted_generation_contract(),
                        grounding_ledger: canonical_ledger,
                        first_rejection,
                    },
                    ReviewMomentCommentPublication::RetryRejected,
                ),
                Ok(()) => first_grounded_attempt(text, grounding_ledger, intent.as_ref(), &facts),
            },
            Some((generation_contract, first_ledger)) => {
                if generation_contract
                    != CriticalMomentCommentAuthoringProvenance::hosted_generation_contract()
                    || first_ledger != canonical_ledger
                {
                    return Err(CommentPublicationStageError::InvalidCommand);
                }
                match ledger_validation
                    .and_then(|()| ground_hosted_comment_text(&facts, intent.as_ref(), &text))
                {
                    // The published comment is the substituted one, never the
                    // marker form, and its ledger is the claims those markers
                    // asserted rather than the set the facts merely support.
                    Ok(grounded) => published_comment_state(
                        grounded.comment,
                        CriticalMomentCommentAuthoringProvenance::hosted_authored(
                            grounded.grounding_ledger,
                            2,
                        ),
                    ),
                    Err(reason) => {
                        let comment = safely_rendered_comment(&facts, intent);
                        published_comment_state(
                            comment,
                            /* Coach App admission, not a web retry: this is the
                            surface's first and only rendering of the moment. */
                            CriticalMomentCommentAuthoringProvenance::hosted_safe_rendered(
                                first_ledger,
                                reason,
                                false,
                            ),
                        )
                    }
                }
            }
        };
        let mut comment_publication = current;
        comment_publication.record(idempotency_key.clone(), outcome);
        let checkpoint = PreparedReviewSessionMoment {
            core: self.core_snapshot().await,
            local_decision: self.local_decision.clone(),
            idempotency_keys: idempotency_keys.clone(),
            exploration: self.exploration.checkpoint().await,
            comment_publication: comment_publication.clone(),
        };
        Ok(CommentPublicationStage::Mutation(Box::new(
            StagedCommentPublication {
                idempotency_key,
                comment_publication,
                idempotency_keys,
                checkpoint,
                facts,
                projected_plan_provenance,
                hosted_captures: Vec::new(),
                result,
            },
        )))
    }

    /// Stages the first-open hosted comment with the author's pin provenance.
    ///
    /// This is not a Coach App publication: the key is derived, the generation
    /// contract is the pin the author used, and there is no Grounding Gate retry
    /// envelope. `author_grounded_comment` already retried and always returns a
    /// publishable comment.
    pub(in crate::review_session_processor) async fn stage_first_open_comment(
        &self,
        idempotency_key: IdempotencyKey,
        comment: CriticalMomentComment,
        authoring_provenance: CriticalMomentCommentAuthoringProvenance,
    ) -> Result<CommentPublicationStage, CommentPublicationStageError> {
        let current = self.comment_publication.lock().await.clone();
        if let Some(ReviewMomentCommentPublicationOutcome::Published(published)) =
            current.attempt(&idempotency_key)
        {
            return Ok(CommentPublicationStage::Existing(
                ReviewMomentCommentPublication::Published {
                    comment: published.comment.clone(),
                    authoring_provenance: Box::new(published.authoring_provenance.clone()),
                },
            ));
        }
        /* An active comment settles this write unless the open was entitled to
        re-author it -- an edited prompt left it behind, or it is a safe
        rendering with a retry unspent -- in which case the incoming one
        supersedes it. The annotation store is append-only, so the older record
        survives beside the new one and "active" simply moves on. */
        if let Some(published) = current
            .active_comment()
            .filter(|published| published.authoring_provenance.reauthor().is_none())
        {
            return Ok(CommentPublicationStage::Existing(
                ReviewMomentCommentPublication::Published {
                    comment: published.comment.clone(),
                    authoring_provenance: Box::new(published.authoring_provenance.clone()),
                },
            ));
        }

        let authoring_context = self
            .comment_authoring_context()
            .await
            .ok_or(CommentPublicationStageError::MissingAuthority)?;
        let facts = authoring_context.facts;
        if !facts.is_well_formed() {
            return Err(CommentPublicationStageError::InvalidCommand);
        }
        if !authoring_provenance.generation_contract.is_reproducible()
            || !authoring_provenance.is_valid_for(&comment)
        {
            return Err(CommentPublicationStageError::InvalidCommand);
        }

        let projected_plan_provenance = self.intent_authoring.provenance().await;
        let mut idempotency_keys = self.idempotency_keys.lock().await.clone();
        idempotency_keys.insert(idempotency_key.clone());
        let (outcome, result) = published_comment_state(comment, authoring_provenance);
        let mut comment_publication = current;
        comment_publication.record(idempotency_key.clone(), outcome);
        let checkpoint = PreparedReviewSessionMoment {
            core: self.core_snapshot().await,
            local_decision: self.local_decision.clone(),
            idempotency_keys: idempotency_keys.clone(),
            exploration: self.exploration.checkpoint().await,
            comment_publication: comment_publication.clone(),
        };
        Ok(CommentPublicationStage::Mutation(Box::new(
            StagedCommentPublication {
                idempotency_key,
                comment_publication,
                idempotency_keys,
                checkpoint,
                facts,
                projected_plan_provenance,
                hosted_captures: Vec::new(),
                result,
            },
        )))
    }

    /// Makes a staged publication durable before its Review Session records it.
    ///
    /// The annotation store is the comment's home, so it is written first: a
    /// checkpoint that fails afterwards costs the conversation its in-memory
    /// record, not the Player their comment. The store is also the authority on
    /// what a replayed idempotency key published, so the staged state adopts
    /// whatever it returns.
    pub(in crate::review_session_processor) async fn publish_staged_annotation(
        &self,
        mut staged: StagedCommentPublication,
    ) -> Result<StagedCommentPublication, ReviewAnnotationStoreError> {
        let ReviewMomentCommentPublication::Published {
            comment,
            authoring_provenance,
        } = &staged.result
        else {
            return Ok(staged);
        };
        let published = published_comment(
            self.annotations
                .publish(ReviewMomentAnnotation {
                    moment_id: self.moment_id().clone(),
                    idempotency_key: staged.idempotency_key.clone(),
                    comment: comment.clone(),
                    authoring_provenance: authoring_provenance.as_ref().clone(),
                    published_at: Utc::now(),
                })
                .await?,
        );
        staged.comment_publication.record(
            staged.idempotency_key.clone(),
            ReviewMomentCommentPublicationOutcome::Published(published.clone()),
        );
        // What the Player now sees is the review's newest annotation, which is
        // this write unless a later one elsewhere already superseded it.
        seed_from_annotations(
            &mut staged.comment_publication,
            &self.annotations,
            self.moment_id(),
        )
        .await;
        staged.checkpoint.comment_publication = staged.comment_publication.clone();
        staged.result = ReviewMomentCommentPublication::Published {
            comment: published.comment,
            authoring_provenance: Box::new(published.authoring_provenance),
        };
        Ok(staged)
    }

    pub(in crate::review_session_processor) async fn apply_staged_comment_publication(
        &self,
        staged: StagedCommentPublication,
    ) -> ReviewMomentCommentPublication {
        *self.idempotency_keys.lock().await = staged.idempotency_keys;
        *self.comment_publication.lock().await = staged.comment_publication;
        staged.result
    }
}

fn first_grounded_attempt(
    text: String,
    grounding_ledger: CriticalMomentGroundingLedger,
    intent: Option<&crate::review_session_contract::CriticalMomentIntentAuthoringContext>,
    facts: &crate::review_session_contract::ReviewMomentCommentFacts,
) -> (
    ReviewMomentCommentPublicationOutcome,
    ReviewMomentCommentPublication,
) {
    // First-open publication resubmits the engine's already-substituted safe
    // render. That paragraph is the canonical comment, so admitting it here
    // keeps the Grounding Gate retry for host-authored prose that actually
    // failed, not for the engine's own rendering.
    let safe_render = safely_rendered_comment(facts, intent.cloned());
    if text == safe_render.text {
        return published_comment_state(
            safe_render,
            CriticalMomentCommentAuthoringProvenance::hosted(grounding_ledger, true),
        );
    }
    match ground_hosted_comment_text(facts, intent, &text) {
        Ok(grounded) => published_comment_state(
            grounded.comment,
            CriticalMomentCommentAuthoringProvenance::hosted_authored(grounded.grounding_ledger, 1),
        ),
        Err(first_rejection) => (
            ReviewMomentCommentPublicationOutcome::RetryAllowed {
                generation_contract:
                    CriticalMomentCommentAuthoringProvenance::hosted_generation_contract(),
                grounding_ledger,
                first_rejection,
            },
            ReviewMomentCommentPublication::RetryRejected,
        ),
    }
}

fn published_comment_state(
    comment: CriticalMomentComment,
    authoring_provenance: CriticalMomentCommentAuthoringProvenance,
) -> (
    ReviewMomentCommentPublicationOutcome,
    ReviewMomentCommentPublication,
) {
    (
        ReviewMomentCommentPublicationOutcome::Published(PublishedReviewMomentComment {
            comment: comment.clone(),
            authoring_provenance: authoring_provenance.clone(),
        }),
        ReviewMomentCommentPublication::Published {
            comment,
            authoring_provenance: Box::new(authoring_provenance),
        },
    )
}

/// Seeds a Review Moment's publication record with the comment it already
/// carries, whatever conversation wrote it.
///
/// The seed is always the review's *newest* annotation for this Review Moment,
/// never whichever one a caller happened to look up. This Review Session's own
/// publications still sit in front of it, so a conversation that has published
/// keeps showing what it published.
pub(in crate::review_session_processor) async fn seed_from_annotations(
    publication: &mut ReviewMomentCommentPublicationCheckpoint,
    annotations: &ReviewAnnotationLog,
    moment_id: &CriticalMomentId,
) {
    if let Some(annotation) = annotations.active(moment_id).await {
        publication.adopt_active(published_comment(annotation));
    }
}

fn published_comment(annotation: ReviewMomentAnnotation) -> PublishedReviewMomentComment {
    PublishedReviewMomentComment {
        comment: annotation.comment,
        authoring_provenance: annotation.authoring_provenance,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        review_annotation_store::{
            InMemoryReviewAnnotationStore, ReviewAnnotationAddress, ReviewAnnotationStore,
        },
        review_session_contract::{
            ArtifactDigest, CriticalMomentGroundingLedger, GameImportId, PlayerId,
        },
        review_session_processor::ProcessorPrincipal,
    };

    use super::*;

    fn moment_id() -> CriticalMomentId {
        CriticalMomentId::try_from("moment:1".to_string()).unwrap()
    }

    fn annotation(key: &str, text: &str, published_at: &str) -> ReviewMomentAnnotation {
        ReviewMomentAnnotation {
            moment_id: moment_id(),
            idempotency_key: IdempotencyKey::try_from(format!("idempotency-key:test:{key}"))
                .unwrap(),
            comment: CriticalMomentComment {
                text: text.to_string(),
            },
            authoring_provenance: CriticalMomentCommentAuthoringProvenance::hosted_authored(
                CriticalMomentGroundingLedger {
                    facts_ref: ArtifactDigest::try_from(format!("sha256:{}", "c".repeat(64)))
                        .unwrap(),
                    factual_claims: Vec::new(),
                },
                1,
            ),
            published_at: published_at.parse().unwrap(),
        }
    }

    async fn log_with(annotations: Vec<ReviewMomentAnnotation>) -> ReviewAnnotationLog {
        let store = Arc::new(InMemoryReviewAnnotationStore::default());
        let address = ReviewAnnotationAddress {
            owner: ProcessorPrincipal::Player(
                PlayerId::try_from("firebase-player-seed".to_string()).unwrap(),
            ),
            game_import_id: GameImportId::try_from(format!(
                "game-import:{}:{}",
                "a".repeat(64),
                "b".repeat(32)
            ))
            .unwrap(),
        };
        for annotation in annotations {
            store.append(&address, annotation).await.unwrap();
        }
        ReviewAnnotationLog::load(store, address).await.unwrap()
    }

    #[tokio::test]
    async fn seeding_adopts_the_newest_annotation_whichever_write_asked_for_it() {
        let log = log_with(vec![
            annotation("earlier", "earlier comment", "2026-08-09T10:00:00Z"),
            annotation("later", "later comment", "2026-08-09T12:00:00Z"),
        ])
        .await;
        let mut publication = ReviewMomentCommentPublicationCheckpoint::default();

        seed_from_annotations(&mut publication, &log, &moment_id()).await;

        assert_eq!(
            publication
                .active_comment()
                .map(|active| active.comment.text.as_str()),
            Some("later comment")
        );
    }
}
