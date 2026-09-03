use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlternativeMoveCoachingCheckpoint {
    pub(super) generation: u64,
    pub(super) started_ids: BTreeSet<CoachTurnId>,
    pub(super) active: Option<ActiveCoachTurnCheckpoint>,
    pub(super) admissions: BTreeMap<CoachTurnId, ActiveCoachTurnCheckpoint>,
    pub(super) outcomes: BTreeMap<CoachTurnId, CoachTurnOutcomeCheckpoint>,
    pub(super) prepared: BTreeMap<CoachTurnId, CoachTurnPreparation>,
    pub(super) assessments: Vec<CoachTurnCommit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ActiveCoachTurnCheckpoint {
    pub(super) operation_id: OperationId,
    pub(super) review_moment_id: CriticalMomentId,
    pub(super) coach_turn_id: CoachTurnId,
    pub(super) message: String,
    pub(super) idempotency_key: IdempotencyKey,
    pub(super) target: PreparedCoachTurnTarget,
    pub(super) generation: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum CoachTurnOutcomeCheckpoint {
    Prepared {
        idempotency_key: IdempotencyKey,
        generation: u64,
    },
    Published {
        operation_key: IdempotencyKey,
        idempotency_key: IdempotencyKey,
        generation: u64,
    },
    Unavailable {
        idempotency_key: IdempotencyKey,
        generation: u64,
        reason: ProviderUnavailableReason,
        retry_target: PreparedCoachTurnTarget,
    },
    Cancelled {
        idempotency_key: IdempotencyKey,
        generation: u64,
    },
    Interrupted {
        idempotency_key: IdempotencyKey,
        generation: u64,
        retry_target: PreparedCoachTurnTarget,
    },
}
