use crate::{
    review_session_coaching::AlternativeMoveCoachTurnError, review_session_contract::*,
    review_session_exploration::ExploreAlternativeMoveError,
    review_session_start::ReviewSessionStartError,
};

use super::events::EventEmitter;

pub(super) fn emit_start_error(
    emitter: &EventEmitter,
    operation: OperationKind,
    error: ReviewSessionStartError,
) {
    let reason = match error {
        ReviewSessionStartError::UnknownPipelineCriticalMoment
        | ReviewSessionStartError::PlyOutOfRange { .. } => CommandRejectionReason::UnknownMoment,
        ReviewSessionStartError::InvalidImportedGame(_) => CommandRejectionReason::InvalidCommand,
    };
    emitter.rejected(operation, reason, RejectionRecovery::CorrectInput);
}

pub(super) fn emit_exploration_error(emitter: &EventEmitter, error: ExploreAlternativeMoveError) {
    match error {
        ExploreAlternativeMoveError::Rejected { reason, recovery } => {
            emitter.rejected(OperationKind::AlternativeMoveEvaluation, reason, recovery)
        }
        ExploreAlternativeMoveError::Conflict(reason) => {
            emitter.conflict(OperationKind::AlternativeMoveEvaluation, reason)
        }
        ExploreAlternativeMoveError::Unavailable(reason) => {
            let retry = retry_for(&reason);
            emitter.unavailable(OperationKind::AlternativeMoveEvaluation, reason, retry);
        }
        ExploreAlternativeMoveError::Cancelled => {
            emitter.cancelled(OperationKind::AlternativeMoveEvaluation)
        }
    }
}

pub(super) fn emit_coach_error(emitter: &EventEmitter, error: AlternativeMoveCoachTurnError) {
    match error {
        AlternativeMoveCoachTurnError::Rejected { reason, recovery } => {
            emitter.rejected(OperationKind::CoachTurn, reason, recovery)
        }
        AlternativeMoveCoachTurnError::Conflict(reason) => {
            emitter.conflict(OperationKind::CoachTurn, reason)
        }
        AlternativeMoveCoachTurnError::Unavailable(reason) => {
            let retry = retry_for(&reason);
            emitter.unavailable(OperationKind::CoachTurn, reason, retry)
        }
        AlternativeMoveCoachTurnError::Cancelled => emitter.cancelled(OperationKind::CoachTurn),
    }
}

fn retry_for(reason: &ProviderUnavailableReason) -> RetryDirective {
    match reason {
        ProviderUnavailableReason::RateLimited {
            retry_after_seconds,
        } => RetryDirective::RetryAfter {
            seconds: *retry_after_seconds,
        },
        ProviderUnavailableReason::AdmissionLimit | ProviderUnavailableReason::QueueDeadline => {
            RetryDirective::StartNewOperation
        }
        ProviderUnavailableReason::LichessTransport
        | ProviderUnavailableReason::ChessComTransport
        | ProviderUnavailableReason::StockfishProcess
        | ProviderUnavailableReason::MaiaTransport
        | ProviderUnavailableReason::LanguageLayer
        | ProviderUnavailableReason::Persistence
        | ProviderUnavailableReason::CoachEngineTransport
        | ProviderUnavailableReason::Timeout { .. } => RetryDirective::RetryAllowed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_rejections_require_a_fresh_operation() {
        for reason in [
            ProviderUnavailableReason::AdmissionLimit,
            ProviderUnavailableReason::QueueDeadline,
        ] {
            assert_eq!(retry_for(&reason), RetryDirective::StartNewOperation);
        }
    }
}
