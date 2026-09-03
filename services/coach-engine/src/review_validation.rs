use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    critical_moment_comment::{intent_authoring_context_for, validate_review_moment_comment},
    review_session_contract::{GameReview, ReviewMomentCommentFacts},
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftGameReview {
    pub verdict: String,
    pub critical_moments: Vec<DraftCriticalMomentExplanation>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftCriticalMomentExplanation {
    pub ply: usize,
    pub explanation: String,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum DraftReviewError {
    #[error("Draft Game Review verdict is empty")]
    EmptyVerdict,
    #[error("Draft Game Review contains an empty explanation for ply {0}")]
    EmptyExplanation(usize),
    #[error("Draft Game Review contains duplicate ply {0}")]
    DuplicatePly(usize),
    #[error("Draft Game Review is missing extracted ply {0}")]
    MissingPly(usize),
    #[error("Draft Game Review references unknown ply {0}")]
    UnknownPly(usize),
    #[error("Draft Game Review ply {ply} is out of Game order after ply {previous}")]
    OutOfOrderPly { previous: usize, ply: usize },
    #[error("Draft Game Review explanation for ply {0} is not causally grounded")]
    UngroundedExplanation(usize),
    #[error("Draft Game Review contains invalid classification facts for ply {0}")]
    InvalidClassificationFacts(usize),
}

pub fn validate_grounded_game_review(
    draft: &DraftGameReview,
    review: &GameReview,
) -> Result<(), DraftReviewError> {
    let expected_plies = review
        .critical_moments
        .iter()
        .map(|moment| usize::from(moment.ply))
        .collect::<Vec<_>>();
    validate_game_review(draft, &expected_plies)?;
    for moment in &review.critical_moments {
        let explanation = draft
            .critical_moments
            .iter()
            .find(|explanation| explanation.ply == usize::from(moment.ply))
            .expect("structural validation matched every expected ply");
        let facts = ReviewMomentCommentFacts::try_from_presented_moment(moment.clone())
            .map_err(|_| DraftReviewError::InvalidClassificationFacts(usize::from(moment.ply)))?;
        let intent = intent_authoring_context_for(&facts, None);
        validate_review_moment_comment(&explanation.explanation, &facts, intent.as_ref())
            .map_err(|_| DraftReviewError::UngroundedExplanation(usize::from(moment.ply)))?;
    }
    Ok(())
}

pub fn validate_game_review(
    draft: &DraftGameReview,
    expected_plies: &[usize],
) -> Result<(), DraftReviewError> {
    validate_common(&draft.verdict, draft.critical_moments.iter())?;

    let expected = expected_plies.iter().copied().collect::<HashSet<_>>();
    let mut actual = HashSet::with_capacity(draft.critical_moments.len());
    for moment in &draft.critical_moments {
        if !actual.insert(moment.ply) {
            return Err(DraftReviewError::DuplicatePly(moment.ply));
        }
        if !expected.contains(&moment.ply) {
            return Err(DraftReviewError::UnknownPly(moment.ply));
        }
    }
    for pair in draft.critical_moments.windows(2) {
        if pair[0].ply > pair[1].ply {
            return Err(DraftReviewError::OutOfOrderPly {
                previous: pair[0].ply,
                ply: pair[1].ply,
            });
        }
    }
    for ply in expected_plies {
        if !actual.contains(ply) {
            return Err(DraftReviewError::MissingPly(*ply));
        }
    }
    Ok(())
}

fn validate_common<'a>(
    verdict: &str,
    explanations: impl Iterator<Item = &'a DraftCriticalMomentExplanation>,
) -> Result<(), DraftReviewError> {
    if verdict.trim().is_empty() {
        return Err(DraftReviewError::EmptyVerdict);
    }
    for explanation in explanations {
        if explanation.explanation.trim().is_empty() {
            return Err(DraftReviewError::EmptyExplanation(explanation.ply));
        }
    }
    Ok(())
}
