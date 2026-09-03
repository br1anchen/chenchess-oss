//! The single seam between an in-process Review Session event and the bytes a
//! surface receives.
//!
//! Every surface leaves the process through [`encode_delivery_frame`], so what
//! is dropped on the way out cannot be reintroduced by a transport that forgets
//! to project first.

use super::{
    GameReview, GameReviewCriticalMoment, OperationCompletion, ReviewSessionEvent,
    ReviewSessionEventEnvelope,
};

/// Encodes one event as an NDJSON delivery frame.
pub fn encode_delivery_frame(event: ReviewSessionEventEnvelope) -> Vec<u8> {
    let mut frame = serde_json::to_vec(&event.for_delivery())
        .expect("Review Session event envelopes are serializable");
    frame.push(b'\n');
    frame
}

impl ReviewSessionEventEnvelope {
    /// Prepares the envelope for a delivery surface.
    ///
    /// Proof aggregates dominate a review's bytes and no surface renders them,
    /// so they are dropped on the way out while every in-process consumer —
    /// evaluation, certification, and the durable stores — keeps the whole
    /// proof. Each moment retains the reference that addresses it.
    fn for_delivery(mut self) -> Self {
        if let ReviewSessionEvent::Completed { result } = &mut self.event {
            result.drop_decision_proof();
        }
        self
    }
}

impl OperationCompletion {
    fn drop_decision_proof(&mut self) {
        match self {
            Self::GameImported { review, .. }
            | Self::GameReviewOpened { review, .. }
            | Self::ReviewSessionStarted { review, .. }
            | Self::GameReviewSnapshotRead { review, .. } => review.drop_decision_proof(),
            Self::ReviewMomentOpened {
                critical_moment, ..
            } => critical_moment.drop_decision_proof(),
            // The audit address is the one place a proof is asked for by name,
            // so it is the one completion that keeps it. `ReviewMomentDetailRead`
            // never carried a proof to begin with: it carries the resolved
            // projection of one.
            Self::ReviewMomentExplanationRead { .. }
            | Self::GameImportDeleted { .. }
            | Self::GameReviewSnapshotUnchanged { .. }
            | Self::ReviewMomentDetailUnchanged { .. }
            | Self::ReviewMomentDetailRead { .. }
            | Self::AddressedReviewMomentOpened { .. }
            | Self::PositionInspected { .. }
            | Self::PlayerPlanEvaluationPrepared { .. }
            | Self::PlayerPlanEvaluated { .. }
            | Self::AlternativeMoveEvaluated { .. }
            | Self::CoachTurnCompleted { .. }
            | Self::HostTurnCompleted { .. }
            | Self::HostTurnRefused { .. }
            | Self::CoachTurnPrepared { .. }
            | Self::ReviewMomentCommentPublished { .. }
            | Self::LearningPathFeedbackRecorded { .. } => {}
        }
    }
}

impl GameReview {
    /// Drops the audit-only proof aggregates, leaving each moment's reference.
    fn drop_decision_proof(&mut self) {
        for moment in &mut self.critical_moments {
            moment.drop_decision_proof();
        }
    }
}

impl GameReviewCriticalMoment {
    /// Drops the audit-only proof aggregate, leaving the moment's reference.
    fn drop_decision_proof(&mut self) {
        self.decision_explanation = None;
    }
}
