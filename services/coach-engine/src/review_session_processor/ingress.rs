use std::time::Instant;

use serde_json::Value;

use crate::review_session_contract::*;

pub(super) enum DecodedCommand {
    Ready(Box<ReviewSessionCommandEnvelope>),
    Rejected {
        request_id: RequestId,
        operation_id: OperationId,
        operation: OperationKind,
        reason: CommandRejectionReason,
    },
}

pub struct ProcessorCommandAdmission {
    decoded: DecodedCommand,
    validation_milliseconds: f64,
}

impl ProcessorCommandAdmission {
    pub fn parse(bytes: &[u8]) -> Self {
        let started_at = Instant::now();
        let decoded = decode(bytes);
        Self {
            decoded,
            validation_milliseconds: started_at.elapsed().as_secs_f64() * 1_000.0,
        }
    }

    pub fn envelope(&self) -> Option<&ReviewSessionCommandEnvelope> {
        match &self.decoded {
            DecodedCommand::Ready(envelope) => Some(envelope.as_ref()),
            DecodedCommand::Rejected { .. } => None,
        }
    }

    pub(super) fn into_decoded(self) -> (DecodedCommand, f64) {
        (self.decoded, self.validation_milliseconds)
    }
}

fn decode(bytes: &[u8]) -> DecodedCommand {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return rejected(CommandRejectionReason::MalformedInput);
    };
    let request_id =
        decode_field::<RequestId>(&value, "requestId").unwrap_or_else(admission_request_id);
    let operation_id =
        decode_field::<OperationId>(&value, "operationId").unwrap_or_else(admission_operation_id);
    let operation = value
        .get("command")
        .and_then(|command| command.get("kind"))
        .and_then(Value::as_str)
        .and_then(operation_kind)
        .unwrap_or(OperationKind::CommandAdmission);
    if operation == OperationKind::CommandAdmission {
        return DecodedCommand::Rejected {
            request_id,
            operation_id,
            operation,
            reason: CommandRejectionReason::UnknownCommand,
        };
    }

    match serde_json::from_value(value) {
        Ok(envelope) => DecodedCommand::Ready(Box::new(envelope)),
        Err(_) => DecodedCommand::Rejected {
            request_id,
            operation_id,
            operation,
            reason: CommandRejectionReason::InvalidCommand,
        },
    }
}

fn rejected(reason: CommandRejectionReason) -> DecodedCommand {
    DecodedCommand::Rejected {
        request_id: admission_request_id(),
        operation_id: admission_operation_id(),
        operation: OperationKind::CommandAdmission,
        reason,
    }
}

fn decode_field<T: serde::de::DeserializeOwned>(value: &Value, field: &str) -> Option<T> {
    serde_json::from_value(value.get(field)?.clone()).ok()
}

fn admission_request_id() -> RequestId {
    RequestId::try_from("request:command-admission".to_string())
        .expect("the stable admission Request ID is valid")
}

fn admission_operation_id() -> OperationId {
    OperationId::try_from("operation:command-admission".to_string())
        .expect("the stable admission Operation ID is valid")
}

fn operation_kind(kind: &str) -> Option<OperationKind> {
    match kind {
        "importGame" => Some(OperationKind::GameImport),
        "deleteGameImport" => Some(OperationKind::GameImportDeletion),
        "openGameReview" | "openGameReviewByIdentity" | "readGameReviewSnapshot" => {
            Some(OperationKind::GameReviewOpen)
        }
        "startReviewSession" => Some(OperationKind::ReviewSessionStart),
        "openReviewMoment"
        | "openAddressedReviewMoment"
        | "readReviewMomentDetail"
        | "readReviewMomentExplanation" => Some(OperationKind::ReviewMomentOpen),
        "inspectPosition" => Some(OperationKind::PositionInspection),
        "evaluatePlayerPlan" => Some(OperationKind::PlayerPlanEvaluation),
        "exploreAlternativeMove" => Some(OperationKind::AlternativeMoveEvaluation),
        "startCoachTurn" | "publishCoachTurn" => Some(OperationKind::CoachTurn),
        "startHostTurn" => Some(OperationKind::HostTurn),
        "publishReviewMomentComment" => Some(OperationKind::ReviewMomentCommentPublication),
        "recordLearningPathExposure" | "updateLearningPathVote" => {
            Some(OperationKind::LearningPathFeedback)
        }
        "cancelOperation" => Some(OperationKind::Cancellation),
        _ => None,
    }
}
