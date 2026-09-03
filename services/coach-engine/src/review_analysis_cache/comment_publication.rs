use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    critical_moment_comment::{
        admissible_grounding_ledger, grounding_ledger_for, validate_emitted_comment,
        CriticalMomentCommentAuthoringProvenance,
    },
    review_session_contract::{
        CriticalMomentComment, CriticalMomentCommentGenerationContract,
        CriticalMomentGroundingLedger, CriticalMomentGroundingRejection, GameReviewCriticalMoment,
        IdempotencyKey, ReviewMomentCommentFacts,
    },
};

/// One published Review Moment Comment together with the provenance proving it
/// passed the Grounding Gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublishedReviewMomentComment {
    pub(crate) comment: CriticalMomentComment,
    pub(crate) authoring_provenance: CriticalMomentCommentAuthoringProvenance,
}

/// Where one logical publication got to.
///
/// A logical publication is addressed by the Player's idempotency key and gets
/// at most two attempts at the Grounding Gate: the first rejection opens a
/// bounded retry, and the second attempt lands either the Player's text or the
/// safely rendered fallback. Both terminal shapes are `Published`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum ReviewMomentCommentPublicationOutcome {
    RetryAllowed {
        generation_contract: CriticalMomentCommentGenerationContract,
        grounding_ledger: CriticalMomentGroundingLedger,
        first_rejection: CriticalMomentGroundingRejection,
    },
    Published(PublishedReviewMomentComment),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReviewMomentCommentPublicationAttempt {
    pub(crate) idempotency_key: IdempotencyKey,
    pub(crate) outcome: ReviewMomentCommentPublicationOutcome,
}

/// What one Review Session knows about a Review Moment's comments.
///
/// It knows two separate things and must not confuse them. Which comment the
/// Player sees is the durable annotation store's answer alone, adopted here as
/// `active`; every conversation reviewing this Game therefore shows the same
/// comment. What this session's own logical writes produced lives in `attempts`
/// and only ever answers replay and the Grounding Gate's bounded retry — never
/// "what is on screen", because an attempt's position in this list is its order
/// in this conversation, not its order in the review.
///
/// Nothing is overwritten, and because the Review Moment is immutable there is
/// nothing for a publication to go stale against.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReviewMomentCommentPublicationCheckpoint {
    /// The comment the durable annotation store holds for this Review Moment,
    /// whichever conversation published it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active: Option<PublishedReviewMomentComment>,
    /// This session's attempts, one per idempotency key, newest last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    attempts: Vec<ReviewMomentCommentPublicationAttempt>,
}

impl ReviewMomentCommentPublicationCheckpoint {
    /// The comment a Player currently sees on this Review Moment.
    pub(crate) fn active_comment(&self) -> Option<&PublishedReviewMomentComment> {
        self.active.as_ref()
    }

    /// Adopts the durable annotation store's answer for this Review Moment.
    ///
    /// Publishing calls this immediately after the write lands, so a
    /// conversation sees its own comment the moment it becomes the review's
    /// newest — and does not see it when a later write elsewhere already
    /// superseded it.
    pub(crate) fn adopt_active(&mut self, published: PublishedReviewMomentComment) {
        self.active = Some(published);
    }

    pub(crate) fn attempt(
        &self,
        idempotency_key: &IdempotencyKey,
    ) -> Option<&ReviewMomentCommentPublicationOutcome> {
        self.attempts
            .iter()
            .find(|attempt| &attempt.idempotency_key == idempotency_key)
            .map(|attempt| &attempt.outcome)
    }

    /// Records the outcome of the logical publication addressed by
    /// `idempotency_key`, replacing that key's own earlier outcome in place.
    ///
    /// Replacing in place keeps a retried Grounding Gate attempt one logical
    /// write rather than two. Recording an outcome does not decide what the
    /// Player sees: [`Self::adopt_active`] does.
    pub(crate) fn record(
        &mut self,
        idempotency_key: IdempotencyKey,
        outcome: ReviewMomentCommentPublicationOutcome,
    ) {
        match self
            .attempts
            .iter_mut()
            .find(|attempt| attempt.idempotency_key == idempotency_key)
        {
            Some(attempt) => attempt.outcome = outcome,
            None => self.attempts.push(ReviewMomentCommentPublicationAttempt {
                idempotency_key,
                outcome,
            }),
        }
    }

    pub(super) fn validate(
        &self,
        facts: &GameReviewCriticalMoment,
        idempotency_keys: &BTreeSet<IdempotencyKey>,
    ) -> bool {
        let Ok(facts) = ReviewMomentCommentFacts::try_from_presented_moment(facts.clone()) else {
            return false;
        };
        let canonical_ledger = grounding_ledger_for(&facts);
        self.active
            .as_ref()
            .is_none_or(|active| valid_published(&facts, active))
            && self.attempts.iter().all(|attempt| {
                idempotency_keys.contains(&attempt.idempotency_key)
                    && valid_outcome(&facts, &attempt.outcome, &canonical_ledger)
            })
    }
}

fn valid_outcome(
    facts: &ReviewMomentCommentFacts,
    outcome: &ReviewMomentCommentPublicationOutcome,
    canonical_ledger: &CriticalMomentGroundingLedger,
) -> bool {
    match outcome {
        ReviewMomentCommentPublicationOutcome::RetryAllowed {
            generation_contract,
            grounding_ledger,
            first_rejection,
        } => {
            generation_contract
                == &CriticalMomentCommentAuthoringProvenance::hosted_generation_contract()
                && grounding_ledger == canonical_ledger
                && !matches!(
                    first_rejection,
                    CriticalMomentGroundingRejection::InvalidClassificationFacts
                        | CriticalMomentGroundingRejection::InvalidGenerationContract
                        | CriticalMomentGroundingRejection::AlreadyAuthoredFacts
                )
        }
        ReviewMomentCommentPublicationOutcome::Published(published) => {
            valid_published(facts, published)
        }
    }
}

fn valid_published(
    facts: &ReviewMomentCommentFacts,
    published: &PublishedReviewMomentComment,
) -> bool {
    admissible_grounding_ledger(facts, &published.authoring_provenance.grounding_ledger)
        && published
            .authoring_provenance
            .generation_contract
            .is_reproducible()
        && published
            .authoring_provenance
            .is_valid_for(&published.comment)
        && validate_emitted_comment(&published.comment).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_session_contract::ArtifactDigest;

    fn ledger() -> CriticalMomentGroundingLedger {
        CriticalMomentGroundingLedger {
            facts_ref: ArtifactDigest::try_from(format!("sha256:{}", "a".repeat(64))).unwrap(),
            factual_claims: Vec::new(),
        }
    }

    fn published(text: &str) -> PublishedReviewMomentComment {
        PublishedReviewMomentComment {
            comment: CriticalMomentComment {
                text: text.to_string(),
            },
            authoring_provenance: CriticalMomentCommentAuthoringProvenance::hosted_authored(
                ledger(),
                1,
            ),
        }
    }

    fn key(label: &str) -> IdempotencyKey {
        IdempotencyKey::try_from(format!("idempotency-key:test:{label}")).unwrap()
    }

    #[test]
    fn one_key_addresses_one_logical_publication_however_often_it_is_replayed() {
        let mut checkpoint = ReviewMomentCommentPublicationCheckpoint::default();
        checkpoint.record(
            key("only"),
            ReviewMomentCommentPublicationOutcome::RetryAllowed {
                generation_contract:
                    CriticalMomentCommentAuthoringProvenance::hosted_generation_contract(),
                grounding_ledger: ledger(),
                first_rejection: CriticalMomentGroundingRejection::MissingUncertainty,
            },
        );
        checkpoint.record(
            key("only"),
            ReviewMomentCommentPublicationOutcome::Published(published("second attempt")),
        );

        assert_eq!(checkpoint.attempts.len(), 1);
    }

    #[test]
    fn a_distinct_key_appends_without_displacing_the_one_before_it() {
        let mut checkpoint = ReviewMomentCommentPublicationCheckpoint::default();
        checkpoint.record(
            key("first"),
            ReviewMomentCommentPublicationOutcome::Published(published("first comment")),
        );
        checkpoint.record(
            key("second"),
            ReviewMomentCommentPublicationOutcome::Published(published("second comment")),
        );

        // Nothing was overwritten: each key still replays its own comment.
        assert!(matches!(
            checkpoint.attempt(&key("first")),
            Some(ReviewMomentCommentPublicationOutcome::Published(earlier))
                if earlier.comment.text == "first comment"
        ));
        assert!(matches!(
            checkpoint.attempt(&key("second")),
            Some(ReviewMomentCommentPublicationOutcome::Published(later))
                if later.comment.text == "second comment"
        ));
    }

    #[test]
    fn what_this_session_recorded_never_decides_what_the_player_sees() {
        let mut checkpoint = ReviewMomentCommentPublicationCheckpoint::default();
        // An attempt on its own shows nothing: the annotation store is the only
        // authority on which comment is active, so ordering cannot disagree
        // between two conversations on one review.
        checkpoint.record(
            key("recorded"),
            ReviewMomentCommentPublicationOutcome::Published(published("recorded comment")),
        );
        assert!(checkpoint.active_comment().is_none());

        checkpoint.adopt_active(published("durable comment"));
        assert_eq!(
            checkpoint
                .active_comment()
                .map(|active| &active.comment.text),
            Some(&"durable comment".to_string())
        );

        // A later Grounding Gate rejection is not a publication and leaves it be.
        checkpoint.record(
            key("rejected"),
            ReviewMomentCommentPublicationOutcome::RetryAllowed {
                generation_contract:
                    CriticalMomentCommentAuthoringProvenance::hosted_generation_contract(),
                grounding_ledger: ledger(),
                first_rejection: CriticalMomentGroundingRejection::MissingUncertainty,
            },
        );
        assert_eq!(
            checkpoint
                .active_comment()
                .map(|active| &active.comment.text),
            Some(&"durable comment".to_string())
        );
    }

    #[test]
    fn a_published_comment_keeps_the_projection_that_shaped_it() {
        let shaped = crate::language_layer_prompt::CoachingProfileProjection::populated([
            "deflection".to_string(),
        ]);
        let later = crate::language_layer_prompt::CoachingProfileProjection::populated([
            "clearance".to_string(),
        ]);
        let mut first = published("shaped comment");
        first.authoring_provenance = first
            .authoring_provenance
            .with_coaching_profile(shaped.clone());
        let mut checkpoint = ReviewMomentCommentPublicationCheckpoint::default();
        checkpoint.record(
            key("shaped"),
            ReviewMomentCommentPublicationOutcome::Published(first.clone()),
        );
        checkpoint.adopt_active(first);

        let mut retrofit = published("later comment");
        retrofit.authoring_provenance = retrofit.authoring_provenance.with_coaching_profile(later);
        checkpoint.record(
            key("later"),
            ReviewMomentCommentPublicationOutcome::Published(retrofit),
        );

        assert_eq!(
            checkpoint
                .attempt(&key("shaped"))
                .and_then(|outcome| match outcome {
                    ReviewMomentCommentPublicationOutcome::Published(published) => {
                        Some(&published.authoring_provenance.coaching_profile_projection)
                    }
                    _ => None,
                }),
            Some(&shaped),
            "a later projection must not retrofit a published comment"
        );
        assert_eq!(
            checkpoint
                .active_comment()
                .map(|active| &active.authoring_provenance.coaching_profile_projection),
            Some(&shaped)
        );
    }
}
