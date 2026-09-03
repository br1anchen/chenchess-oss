use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::AlternativeMoveId;

/// One prior HostTurn's Player message and grounded answer.
///
/// Capability results never re-enter this memory. The active branch stays on
/// screen and is read from the Review Session actor, not from this payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostTurnPriorTurn {
    pub message: String,
    pub answer: String,
}

/// Fixed Player-facing HostTurn progress labels (D9).
///
/// These are product language, never capability names. The engine sends the
/// closed kind; Central Host maps it to the display string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum HostTurnStepLabel {
    LookingAtAnotherMoment,
    CheckingThatLine,
    Writing,
}

/// Why the pinned model refused to answer this HostTurn (D6).
///
/// The engine renders the Player-facing sentence; the model never writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum HostTurnRefusalReason {
    NotAboutThisReview,
    NotAboutChess,
    UnsafeRequest,
}

/// A line the web board should show after a completed HostTurn.
///
/// `engineBest` and `playedMoveRefutation` name a pre-loaded objective line
/// kind. `alternativeMove` names a line returned this turn or already on screen.
// Pre-loaded kinds match MoveSequencePresentationKind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum HostTurnShowLine {
    EngineBest,
    PlayedMoveRefutation,
    AlternativeMove {
        alternative_move_id: AlternativeMoveId,
    },
}
