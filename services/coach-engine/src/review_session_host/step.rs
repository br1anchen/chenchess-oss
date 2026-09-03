use serde::Deserialize;
use serde_json::json;

use super::capabilities::{EvaluateLineArgs, HostCapabilityCall, MomentReference, OpponentReplies};
use super::digest::digest_canonical_json;
use crate::review_session_contract::{
    AlternativeMoveId, HostTurnRefusalReason, HostTurnShowLine, ReviewMomentReferenceClassification,
};

pub fn host_turn_step_schema() -> serde_json::Value {
    // Vertex nativeSchema still requires every flattened key. The parser
    // defaults dummy keys so an omitted unused field is not a parse error;
    // load-bearing values (`kind`, a non-empty `answer` on an answer step,
    // closed-set `capability` / `refusalReason`) are checked after decode.
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "kind",
            "capability",
            "ply",
            "next",
            "classification",
            "moves",
            "opponentReplies",
            "answer",
            "citations",
            "focusMoment",
            "showLineKind",
            "alternativeMoveId",
            "refusalReason"
        ],
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["call", "answer", "refuse"]
            },
            "capability": {
                "type": "string",
                "enum": ["", "readMoment", "listMoments", "evaluateLine", "learningMaterial"]
            },
            "ply": { "type": "integer" },
            "next": { "type": "boolean" },
            "classification": {
                "type": "string",
                "enum": ["", "improvementOpportunity"]
            },
            "moves": {
                "type": "array",
                "items": { "type": "string" }
            },
            "opponentReplies": {
                "type": "string",
                "enum": ["", "engineBest", "supplied"]
            },
            "answer": {
                "type": "string",
                "maxLength": 2000
            },
            "citations": {
                "type": "array",
                "items": { "type": "string" }
            },
            "focusMoment": { "type": "integer" },
            "showLineKind": {
                "type": "string",
                "enum": ["", "engineBest", "playedMoveRefutation", "alternativeMove"]
            },
            "alternativeMoveId": { "type": "string" },
            "refusalReason": {
                "type": "string",
                "enum": [
                    "none",
                    "notAboutThisReview",
                    "notAboutChess",
                    "unsafeRequest"
                ]
            }
        }
    })
}

pub fn host_turn_step_schema_digest() -> String {
    digest_canonical_json(&host_turn_step_schema())
}

#[derive(Debug, Clone, PartialEq)]
pub enum HostTurnStep {
    Call(HostCapabilityCall),
    Answer {
        answer: String,
        citations: Vec<String>,
        focus_moment: Option<u16>,
        show_line: Option<HostTurnShowLine>,
    },
    Refuse {
        reason: HostTurnRefusalReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostTurnStepParseError {
    pub message: String,
}

impl HostTurnStepParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Dummy keys default; `kind` does not. An answer step still rejects an empty
/// `answer` after decode so `{"kind":"answer"}` cannot publish blank prose.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlattenedHostTurnStep {
    kind: String,
    #[serde(default)]
    capability: String,
    #[serde(default)]
    ply: u16,
    #[serde(default)]
    next: bool,
    #[serde(default)]
    classification: String,
    #[serde(default)]
    moves: Vec<String>,
    #[serde(default)]
    opponent_replies: String,
    #[serde(default)]
    answer: String,
    #[serde(default)]
    citations: Vec<String>,
    #[serde(default)]
    focus_moment: u16,
    #[serde(default)]
    show_line_kind: String,
    #[serde(default)]
    alternative_move_id: String,
    #[serde(default)]
    refusal_reason: String,
}

pub fn parse_host_turn_step(
    value: &serde_json::Value,
) -> Result<HostTurnStep, HostTurnStepParseError> {
    let wire: FlattenedHostTurnStep = serde_json::from_value(value.clone())
        .map_err(|error| HostTurnStepParseError::new(error.to_string()))?;
    match wire.kind.as_str() {
        "call" => Ok(HostTurnStep::Call(parse_call(&wire)?)),
        "answer" => {
            if wire.answer.is_empty() {
                return Err(HostTurnStepParseError::new(
                    "answer step requires a non-empty answer",
                ));
            }
            let show_line = parse_show_line(&wire)?;
            Ok(HostTurnStep::Answer {
                answer: wire.answer,
                citations: wire.citations,
                focus_moment: (wire.focus_moment > 0).then_some(wire.focus_moment),
                show_line,
            })
        }
        "refuse" => Ok(HostTurnStep::Refuse {
            reason: parse_refusal(&wire.refusal_reason)?,
        }),
        other => Err(HostTurnStepParseError::new(format!(
            "host turn step kind is not in the closed set: {other}"
        ))),
    }
}

fn parse_call(wire: &FlattenedHostTurnStep) -> Result<HostCapabilityCall, HostTurnStepParseError> {
    match wire.capability.as_str() {
        "readMoment" => Ok(HostCapabilityCall::ReadMoment {
            reference: if wire.next {
                MomentReference::Next {
                    classification: parse_optional_classification(&wire.classification)?,
                }
            } else if wire.ply > 0 {
                MomentReference::Ply { ply: wire.ply }
            } else {
                return Err(HostTurnStepParseError::new(
                    "readMoment requires ply > 0 or next",
                ));
            },
        }),
        "listMoments" => Ok(HostCapabilityCall::ListMoments),
        "evaluateLine" => {
            if wire.moves.is_empty() {
                return Err(HostTurnStepParseError::new(
                    "evaluateLine requires at least one move",
                ));
            }
            Ok(HostCapabilityCall::EvaluateLine(EvaluateLineArgs {
                moves: wire.moves.clone(),
                opponent_replies: match wire.opponent_replies.as_str() {
                    "engineBest" => OpponentReplies::EngineBest,
                    "supplied" => OpponentReplies::Supplied,
                    other => {
                        return Err(HostTurnStepParseError::new(format!(
                            "evaluateLine opponentReplies is not in the closed set: {other}"
                        )))
                    }
                },
            }))
        }
        "learningMaterial" => Ok(HostCapabilityCall::LearningMaterial),
        other => Err(HostTurnStepParseError::new(format!(
            "host capability is not in the closed set: {other}"
        ))),
    }
}

fn parse_optional_classification(
    value: &str,
) -> Result<Option<ReviewMomentReferenceClassification>, HostTurnStepParseError> {
    match value {
        "" => Ok(None),
        "improvementOpportunity" => Ok(Some(
            ReviewMomentReferenceClassification::ImprovementOpportunity,
        )),
        other => Err(HostTurnStepParseError::new(format!(
            "readMoment classification is not in the closed set: {other}"
        ))),
    }
}

fn parse_show_line(
    wire: &FlattenedHostTurnStep,
) -> Result<Option<HostTurnShowLine>, HostTurnStepParseError> {
    match wire.show_line_kind.as_str() {
        "" => Ok(None),
        "engineBest" => Ok(Some(HostTurnShowLine::EngineBest)),
        "playedMoveRefutation" => Ok(Some(HostTurnShowLine::PlayedMoveRefutation)),
        "alternativeMove" => {
            let alternative_move_id = AlternativeMoveId::try_from(wire.alternative_move_id.clone())
                .map_err(|error| HostTurnStepParseError::new(error.to_string()))?;
            Ok(Some(HostTurnShowLine::AlternativeMove {
                alternative_move_id,
            }))
        }
        other => Err(HostTurnStepParseError::new(format!(
            "showLineKind is not in the closed set: {other}"
        ))),
    }
}

fn parse_refusal(value: &str) -> Result<HostTurnRefusalReason, HostTurnStepParseError> {
    match value {
        "notAboutThisReview" => Ok(HostTurnRefusalReason::NotAboutThisReview),
        "notAboutChess" => Ok(HostTurnRefusalReason::NotAboutChess),
        "unsafeRequest" => Ok(HostTurnRefusalReason::UnsafeRequest),
        other => Err(HostTurnStepParseError::new(format!(
            "refusalReason is not in the closed set: {other}"
        ))),
    }
}
