//! Marker-form commentary for tests that need an admissible host draft.
//!
//! A draft and the comment it becomes are two different strings now, so tests
//! that submit prose and then assert what the Player sees need both. Building
//! them here keeps the marker vocabulary in one place instead of spreading the
//! gate's shape across every publication test.

use chen_chess_coach_engine::{
    critical_moment_comment::admit_hosted_review_moment_comment,
    review_session_contract::{
        CriticalMomentComment, CriticalMomentCommentDraft, CriticalMomentGroundingLedger,
        CriticalMomentIntentAuthoringContext, ReviewMomentCommentFacts,
    },
};

pub struct Commentary {
    /// What a Language Layer writes: prose plus markers, no figures.
    pub draft_text: String,
    /// What the Player reads once the runtime has substituted them.
    pub comment: CriticalMomentComment,
}

/// Commentary that names every required claim through a marker and nothing
/// through a figure.
///
/// Only required markers are used. Optional ones depend on facts that vary per
/// moment, and a marker the facts do not carry is an unknown marker.
pub fn marker_text(facts: &ReviewMomentCommentFacts, intent_expected: bool) -> String {
    let mut text = match facts {
        ReviewMomentCommentFacts::Positive { .. } => {
            "{playedMove} is {grade} here. {achievement}. {difficulty}.".to_string()
        }
        ReviewMomentCommentFacts::Improvement { .. } => {
            "You played {playedMove}, which leaves the position at {playedEval}. {betterMove} held {bestEval} instead, and {consequence}. {decisionCue}"
                .to_string()
        }
        ReviewMomentCommentFacts::Neutral { .. } => {
            "{playedMove} is quiet and sound: {reason}. {observation}".to_string()
        }
    };
    if intent_expected {
        text.push_str(" My best guess is that the plan may have been to improve the position.");
    }
    text
}

pub fn commentary(
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
) -> Commentary {
    let draft_text = marker_text(facts, intent.is_some());
    let comment = admit_hosted_review_moment_comment(
        facts,
        intent,
        &CriticalMomentCommentDraft {
            text: draft_text.clone(),
            grounding_ledger: ledger(facts),
        },
    )
    .expect("marker commentary is admissible against its own facts");
    Commentary {
        draft_text,
        comment,
    }
}

pub fn ledger(facts: &ReviewMomentCommentFacts) -> CriticalMomentGroundingLedger {
    chen_chess_coach_engine::critical_moment_comment::grounding_ledger_for(facts)
}
