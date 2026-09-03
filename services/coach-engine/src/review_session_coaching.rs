use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};

use crate::{
    evaluation_recording::PINNED_MAIA_CANDIDATE_LIMIT,
    human_move_model::{HumanMoveInput, HumanMoveModel, HumanMoveModelError, HumanMovePrediction},
    operating_limits::{COACH_TURN_DEADLINE_SECONDS, MAIA_DEADLINE_MILLISECONDS},
    review_session_cancellation::ReviewSessionCancellation,
    review_session_contract::*,
    types::EloProfile,
};

use evidence::prepare_evidence;

mod activity;
mod checkpoint;
mod evidence;
mod hosted_author;
mod prose;
mod target;

pub use activity::CoachTurnActivity;
pub(crate) use activity::CoachTurnLease;
pub(crate) use checkpoint::AlternativeMoveCoachingCheckpoint;
use checkpoint::{ActiveCoachTurnCheckpoint, CoachTurnOutcomeCheckpoint};
pub use evidence::prepare_evidence as prepare_recorded_evidence;
pub use evidence::{diagnose_assessment, diagnose_publication, required_evidence_refs};
pub use hosted_author::gate_player_message;
pub(crate) use prose::CoachTurnProseContext;
pub use prose::{CoachTurnDimension, CoachTurnProseRejection, CoachTurnRejection};

const MAIA_DEADLINE: Duration = Duration::from_millis(MAIA_DEADLINE_MILLISECONDS);
const COACH_TURN_DEADLINE: Duration = Duration::from_secs(COACH_TURN_DEADLINE_SECONDS);

pub struct AlternativeMoveCoaching {
    human: Arc<dyn HumanMoveModel>,
    author: Arc<dyn AlternativeMoveAssessmentAuthor>,
    activity: Arc<CoachTurnActivity>,
    admission: Mutex<()>,
    state: Mutex<CoachingState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedCoachTurnTarget {
    context: CoachTurnContext,
    target: AlternativeMoveResult,
    ancestor_branch: Vec<AlternativeMoveResult>,
    source_position: PositionSnapshot,
    evidence_packet: ReviewSessionEvidencePacket,
    elo: EloRating,
}

impl PreparedCoachTurnTarget {
    pub fn context(&self) -> &CoachTurnContext {
        &self.context
    }

    pub fn target(&self) -> &AlternativeMoveResult {
        &self.target
    }

    pub fn ancestor_branch(&self) -> &[AlternativeMoveResult] {
        &self.ancestor_branch
    }

    pub fn source_position(&self) -> &PositionSnapshot {
        &self.source_position
    }

    pub fn evidence_packet(&self) -> &ReviewSessionEvidencePacket {
        &self.evidence_packet
    }

    pub fn elo(&self) -> EloRating {
        self.elo
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoachTurnTargetSelection {
    Explicit(Box<PreparedCoachTurnTarget>),
    Preserve,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartAlternativeMoveCoachTurn {
    pub coach_turn_id: CoachTurnId,
    pub message: String,
    pub idempotency_key: IdempotencyKey,
    pub prior_turn: PriorCoachTurn,
    pub target: CoachTurnTargetSelection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoachTurnAuthorInput {
    pub coach_turn_id: CoachTurnId,
    pub message: String,
    pub target: PreparedCoachTurnTarget,
    pub evidence_packet: ReviewSessionEvidencePacket,
    pub evidence: CoachTurnEvidenceRefs,
    pub prior_turn: PriorCoachTurnContext,
}

/// Owned form of [`crate::language_layer_prompt::PriorCoachTurnText`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriorCoachTurnContext {
    None,
    SameAlternative {
        objective_quality: String,
        findability: String,
        resilience: String,
    },
}

pub type CoachTurnEvidenceRefs = AlternativeMoveAssessmentEvidenceRefs;

pub trait AlternativeMoveAssessmentAuthor: Send + Sync {
    fn assess<'a>(
        &'a self,
        input: CoachTurnAuthorInput,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<AlternativeMoveAssessment, ProviderUnavailableReason>>
                + Send
                + 'a,
        >,
    >;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoachTurnCommit {
    pub assessment: AlternativeMoveAssessment,
    pub evidence_entries: Vec<EvidenceEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoachTurnPreparation {
    pub facts: CoachTurnFacts,
    pub evidence_entries: Vec<EvidenceEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlternativeMoveCoachingState {
    pub started_turns: u8,
    pub active_turn: Option<ActiveCoachTurnState>,
    pub assessments: Vec<CoachTurnCommit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveCoachTurnState {
    pub coach_turn_id: CoachTurnId,
    pub idempotency_key: IdempotencyKey,
    pub alternative_move_id: AlternativeMoveId,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoachTurnTargetError {
    #[error("Alternative Move target is unknown")]
    UnknownTarget,
    #[error("Alternative Move ancestry is inconsistent")]
    InvalidAncestry,
    #[error("Alternative Move target evidence is missing")]
    MissingEvidence,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AlternativeMoveCoachTurnError {
    #[error("Coach Turn was rejected: {reason:?}")]
    Rejected {
        reason: CommandRejectionReason,
        recovery: RejectionRecovery,
    },
    #[error("Coach Turn conflicted with active state: {0:?}")]
    Conflict(OperationConflictReason),
    #[error("Coach Turn is unavailable: {0:?}")]
    Unavailable(ProviderUnavailableReason),
    #[error("Coach Turn was cancelled")]
    Cancelled,
}

struct CoachingState {
    generation: u64,
    started_ids: BTreeSet<CoachTurnId>,
    active: Option<ActiveTurn>,
    admissions: BTreeMap<CoachTurnId, ActiveCoachTurnCheckpoint>,
    outcomes: BTreeMap<CoachTurnId, CoachTurnOutcomeCheckpoint>,
    prepared: BTreeMap<CoachTurnId, CoachTurnPreparation>,
    assessments: Vec<CoachTurnCommit>,
}

struct ActiveTurn {
    operation_id: OperationId,
    review_moment_id: CriticalMomentId,
    coach_turn_id: CoachTurnId,
    message: String,
    idempotency_key: IdempotencyKey,
    target: Arc<PreparedCoachTurnTarget>,
    generation: u64,
    cancellation: ReviewSessionCancellation,
    stopped: watch::Sender<bool>,
    /// Holds the Player's Coach Turn scope on the reviewed Game. Dropping this
    /// turn frees the scope, so a rollback hands the lease on rather than
    /// letting it fall.
    lease: CoachTurnLease,
}

pub(crate) struct AdmittedTurn {
    coach_turn_id: CoachTurnId,
    message: String,
    idempotency_key: IdempotencyKey,
    target: Arc<PreparedCoachTurnTarget>,
    generation: u64,
    cancellation: ReviewSessionCancellation,
    rollback: AlternativeMoveCoachingCheckpoint,
}

enum TurnOutcome {
    Completed(Box<CoachTurnCommit>),
    Prepared(Box<CoachTurnPreparation>),
    Unavailable(ProviderUnavailableReason),
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoachTurnExecution {
    Author,
    PrepareForAuthor,
}

pub(crate) enum CoachTurnResult {
    Completed(Box<CoachTurnCommit>),
    Prepared(Box<CoachTurnPreparation>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CoachTurnReplay {
    Completed(Box<CoachTurnCommit>),
    Prepared(Box<CoachTurnPreparation>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PreparedAssessmentPublication {
    Published,
    Existing,
}

impl AlternativeMoveCoaching {
    pub fn new(
        human: Arc<dyn HumanMoveModel>,
        author: Arc<dyn AlternativeMoveAssessmentAuthor>,
        activity: Arc<CoachTurnActivity>,
    ) -> Self {
        Self {
            human,
            author,
            activity,
            admission: Mutex::new(()),
            state: Mutex::new(CoachingState {
                generation: 0,
                started_ids: BTreeSet::new(),
                active: None,
                admissions: BTreeMap::new(),
                outcomes: BTreeMap::new(),
                prepared: BTreeMap::new(),
                assessments: Vec::new(),
            }),
        }
    }

    pub(crate) async fn lock_admission(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.admission.lock().await
    }

    pub async fn coach(
        &self,
        request: StartAlternativeMoveCoachTurn,
    ) -> Result<CoachTurnCommit, AlternativeMoveCoachTurnError> {
        match self
            .execute_with_admission(
                request,
                CoachTurnExecution::Author,
                std::future::ready(Ok(())),
            )
            .await?
        {
            CoachTurnResult::Completed(commit) => Ok(*commit),
            CoachTurnResult::Prepared(_) => unreachable!("coaching cannot finish as preparation"),
        }
    }

    pub(crate) async fn execute_with_admission<T>(
        &self,
        request: StartAlternativeMoveCoachTurn,
        execution: CoachTurnExecution,
        admission: impl Future<Output = Result<T, ProviderUnavailableReason>>,
    ) -> Result<CoachTurnResult, AlternativeMoveCoachTurnError> {
        validate_message(&request.message)?;
        let admitted = {
            let _admission = self.admission.lock().await;
            self.admit_inner(request, None).await?
        };
        self.execute_admitted(admitted, execution, admission).await
    }

    pub(crate) async fn execute_admitted<T>(
        &self,
        admitted: AdmittedTurn,
        execution: CoachTurnExecution,
        admission: impl Future<Output = Result<T, ProviderUnavailableReason>>,
    ) -> Result<CoachTurnResult, AlternativeMoveCoachTurnError> {
        let admission = tokio::select! {
            biased;
            _ = admitted.cancellation.cancelled() => None,
            result = admission => Some(result),
        };
        let Some(admission) = admission else {
            return match self.finish(admitted, TurnOutcome::Cancelled).await {
                Err(error) => Err(error),
                Ok(_) => unreachable!("cancelled turn cannot complete"),
            };
        };
        let _permit = match admission {
            Ok(permit) => permit,
            Err(reason) => {
                return match self
                    .finish(admitted, TurnOutcome::Unavailable(reason))
                    .await
                {
                    Err(error) => Err(error),
                    Ok(_) => unreachable!("unavailable turn cannot complete"),
                };
            }
        };
        let work = async {
            match execution {
                CoachTurnExecution::Author => self
                    .run(&admitted)
                    .await
                    .map(|commit| TurnOutcome::Completed(Box::new(commit))),
                CoachTurnExecution::PrepareForAuthor => self
                    .prepare(&admitted)
                    .await
                    .map(|preparation| TurnOutcome::Prepared(Box::new(preparation))),
            }
        };
        let outcome = tokio::select! {
            biased;
            _ = admitted.cancellation.cancelled() => TurnOutcome::Cancelled,
            result = tokio::time::timeout(COACH_TURN_DEADLINE, work) => {
                match result {
                    Ok(Ok(outcome)) => outcome,
                    Ok(Err(reason)) => TurnOutcome::Unavailable(reason),
                    Err(_) => TurnOutcome::Unavailable(ProviderUnavailableReason::Timeout {
                        provider: ProviderKind::LanguageLayer,
                    }),
                }
            }
        };
        drop(_permit);
        self.finish(admitted, outcome).await
    }

    pub(crate) async fn admit_for_operation(
        &self,
        request: StartAlternativeMoveCoachTurn,
        operation_id: OperationId,
    ) -> Result<AdmittedTurn, AlternativeMoveCoachTurnError> {
        validate_message(&request.message)?;
        self.admit_inner(request, Some(operation_id)).await
    }

    pub(crate) async fn discard_unpersisted_admission(&self, admitted: &AdmittedTurn) {
        let mut state = self.state.lock().await;
        let is_current = state.active.as_ref().is_some_and(|active| {
            active.coach_turn_id == admitted.coach_turn_id
                && active.idempotency_key == admitted.idempotency_key
        });
        if !is_current {
            return;
        }
        let discarded = state
            .active
            .take()
            .expect("the matching active turn exists");
        discarded.stopped.send_replace(true);
        // The rolled-back turn reoccupies the same Player and Game, so the
        // scope is handed over rather than released and re-acquired.
        let mut lease = discarded.lease;
        let rollback = admitted.rollback.clone();
        state.generation = rollback.generation;
        state.started_ids = rollback.started_ids;
        state.outcomes = rollback.outcomes;
        state.prepared = rollback.prepared;
        state.assessments = rollback.assessments;
        state.admissions = rollback.admissions;
        state.active = rollback.active.map(|active| {
            let cancellation = ReviewSessionCancellation::default();
            let (stopped, _) = watch::channel(true);
            lease.transfer(&active.coach_turn_id);
            ActiveTurn {
                operation_id: active.operation_id,
                review_moment_id: active.review_moment_id,
                coach_turn_id: active.coach_turn_id,
                message: active.message,
                idempotency_key: active.idempotency_key,
                target: Arc::new(active.target),
                generation: active.generation,
                cancellation,
                stopped,
                lease,
            }
        });
    }

    pub async fn cancel(
        &self,
        coach_turn_id: &CoachTurnId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<(), AlternativeMoveCoachTurnError> {
        let mut stopped = {
            let state = self.state.lock().await;
            if matches!(
                state.outcomes.get(coach_turn_id),
                Some(CoachTurnOutcomeCheckpoint::Cancelled {
                    idempotency_key: cancelled_key,
                    ..
                }) if cancelled_key == idempotency_key
            ) {
                return Ok(());
            }
            let active = state
                .active
                .as_ref()
                .ok_or(AlternativeMoveCoachTurnError::Conflict(
                    OperationConflictReason::IdempotencyKeyMismatch,
                ))?;
            if &active.coach_turn_id != coach_turn_id || &active.idempotency_key != idempotency_key
            {
                return Err(AlternativeMoveCoachTurnError::Conflict(
                    OperationConflictReason::IdempotencyKeyMismatch,
                ));
            }
            active.cancellation.cancel();
            active.stopped.subscribe()
        };
        while !*stopped.borrow_and_update() {
            if stopped.changed().await.is_err() {
                break;
            }
        }
        Ok(())
    }

    pub async fn current_state(&self) -> AlternativeMoveCoachingState {
        let state = self.state.lock().await;
        AlternativeMoveCoachingState {
            started_turns: u8::try_from(state.started_ids.len())
                .expect("Review Session policy caps started Coach Turns below u8::MAX"),
            active_turn: state.active.as_ref().map(|active| ActiveCoachTurnState {
                coach_turn_id: active.coach_turn_id.clone(),
                idempotency_key: active.idempotency_key.clone(),
                alternative_move_id: active.target.target.alternative_move_id.clone(),
                generation: active.generation,
            }),
            assessments: state.assessments.clone(),
        }
    }

    pub(crate) async fn prepared_turn(
        &self,
        coach_turn_id: &CoachTurnId,
    ) -> Option<CoachTurnPreparation> {
        self.state.lock().await.prepared.get(coach_turn_id).cloned()
    }

    pub(crate) async fn replay_for_operation(
        &self,
        operation_id: &OperationId,
        request: &StartAlternativeMoveCoachTurn,
    ) -> Result<Option<CoachTurnReplay>, AlternativeMoveCoachTurnError> {
        validate_message(&request.message)?;
        let CoachTurnTargetSelection::Explicit(target) = &request.target else {
            return Ok(None);
        };
        let state = self.state.lock().await;
        let Some(admission) = state.admissions.get(&request.coach_turn_id) else {
            let Some(commit) = state
                .assessments
                .iter()
                .find(|commit| commit.assessment.coach_turn_id == request.coach_turn_id)
            else {
                return Ok(None);
            };
            return if commit.assessment.alternative_move_id == target.target.alternative_move_id {
                Ok(Some(CoachTurnReplay::Completed(Box::new(commit.clone()))))
            } else {
                Err(rejected(CommandRejectionReason::InvalidCommand))
            };
        };
        if admission.operation_id != *operation_id
            || admission.message != request.message
            || admission.idempotency_key != request.idempotency_key
            || admission.target != **target
        {
            return Err(rejected(CommandRejectionReason::InvalidCommand));
        }
        let Some(outcome) = state.outcomes.get(&request.coach_turn_id) else {
            return Err(AlternativeMoveCoachTurnError::Conflict(
                OperationConflictReason::CoachTurnAlreadyActive,
            ));
        };
        match outcome {
            CoachTurnOutcomeCheckpoint::Prepared { .. } => state
                .prepared
                .get(&request.coach_turn_id)
                .cloned()
                .map(|preparation| Some(CoachTurnReplay::Prepared(Box::new(preparation))))
                .ok_or_else(|| rejected(CommandRejectionReason::InvalidCommand)),
            CoachTurnOutcomeCheckpoint::Published { .. } => state
                .assessments
                .iter()
                .find(|commit| commit.assessment.coach_turn_id == request.coach_turn_id)
                .cloned()
                .map(|commit| Some(CoachTurnReplay::Completed(Box::new(commit))))
                .ok_or_else(|| rejected(CommandRejectionReason::InvalidCommand)),
            CoachTurnOutcomeCheckpoint::Unavailable { reason, .. } => {
                Err(AlternativeMoveCoachTurnError::Unavailable(reason.clone()))
            }
            CoachTurnOutcomeCheckpoint::Cancelled { .. } => {
                Err(AlternativeMoveCoachTurnError::Cancelled)
            }
            CoachTurnOutcomeCheckpoint::Interrupted { .. } => Err(
                AlternativeMoveCoachTurnError::Unavailable(ProviderUnavailableReason::Persistence),
            ),
        }
    }

    pub(crate) async fn prepared_operation_key(
        &self,
        coach_turn_id: &CoachTurnId,
    ) -> Option<IdempotencyKey> {
        match self.state.lock().await.outcomes.get(coach_turn_id) {
            Some(CoachTurnOutcomeCheckpoint::Prepared {
                idempotency_key, ..
            }) => Some(idempotency_key.clone()),
            _ => None,
        }
    }

    pub(crate) async fn published_turn(
        &self,
        coach_turn_id: &CoachTurnId,
        idempotency_key: &IdempotencyKey,
    ) -> Option<CoachTurnCommit> {
        let state = self.state.lock().await;
        let has_matching_outcome = matches!(
            state.outcomes.get(coach_turn_id),
            Some(CoachTurnOutcomeCheckpoint::Published {
                idempotency_key: published_key,
                ..
            }) if published_key == idempotency_key
        );
        if !has_matching_outcome
            && (state.outcomes.contains_key(coach_turn_id)
                || !state
                    .assessments
                    .iter()
                    .any(|commit| &commit.assessment.coach_turn_id == coach_turn_id))
        {
            return None;
        }
        state
            .assessments
            .iter()
            .find(|commit| &commit.assessment.coach_turn_id == coach_turn_id)
            .cloned()
    }

    pub(crate) async fn record_prepared_assessment(
        &self,
        idempotency_key: &IdempotencyKey,
        commit: CoachTurnCommit,
    ) -> Result<PreparedAssessmentPublication, AlternativeMoveCoachTurnError> {
        let mut state = self.state.lock().await;
        let coach_turn_id = &commit.assessment.coach_turn_id;
        if let Some(existing) = state
            .assessments
            .iter()
            .find(|existing| &existing.assessment.coach_turn_id == coach_turn_id)
        {
            return if existing == &commit
                && matches!(
                    state.outcomes.get(coach_turn_id),
                    Some(CoachTurnOutcomeCheckpoint::Published {
                        idempotency_key: published_key,
                        ..
                    }) if published_key == idempotency_key
                ) {
                Ok(PreparedAssessmentPublication::Existing)
            } else {
                Err(rejected(CommandRejectionReason::InvalidCommand))
            };
        }
        let (generation, operation_key) = match state.outcomes.get(coach_turn_id) {
            Some(CoachTurnOutcomeCheckpoint::Prepared {
                generation,
                idempotency_key,
            }) => (*generation, idempotency_key.clone()),
            _ => return Err(rejected(CommandRejectionReason::InvalidCommand)),
        };
        state.prepared.remove(coach_turn_id);
        state.outcomes.insert(
            coach_turn_id.clone(),
            CoachTurnOutcomeCheckpoint::Published {
                operation_key,
                idempotency_key: idempotency_key.clone(),
                generation,
            },
        );
        state.assessments.push(commit);
        Ok(PreparedAssessmentPublication::Published)
    }

    async fn admit_inner(
        &self,
        mut request: StartAlternativeMoveCoachTurn,
        operation_id: Option<OperationId>,
    ) -> Result<AdmittedTurn, AlternativeMoveCoachTurnError> {
        let mut steered_from = None;
        let mut rollback = None;
        loop {
            let wait_for_stop = {
                let mut state = self.state.lock().await;
                if rollback.is_none() {
                    rollback = Some(state.checkpoint());
                }
                match state.active.as_ref() {
                    Some(active) => {
                        let PriorCoachTurn::Steers { coach_turn_id } = &request.prior_turn else {
                            return Err(AlternativeMoveCoachTurnError::Conflict(
                                OperationConflictReason::CoachTurnAlreadyActive,
                            ));
                        };
                        if coach_turn_id != &active.coach_turn_id {
                            return Err(AlternativeMoveCoachTurnError::Conflict(
                                OperationConflictReason::IdempotencyKeyMismatch,
                            ));
                        }
                        state.validate_capacity(&request.coach_turn_id)?;
                        if matches!(request.target, CoachTurnTargetSelection::Preserve) {
                            request.target = CoachTurnTargetSelection::Explicit(Box::new(
                                active.target.as_ref().clone(),
                            ));
                        }
                        let prior_id = active.coach_turn_id.clone();
                        let active = state
                            .active
                            .as_mut()
                            .expect("the inspected active turn still exists");
                        active.cancellation.cancel();
                        Some((prior_id, active.stopped.subscribe()))
                    }
                    None => {
                        // Steering reaches here after the turn it cancelled
                        // released the scope, so a Coach Turn started from
                        // another conversation on this Game can win the gap and
                        // refuse the replacement. That is D14's accepted
                        // testing-only edge case: closing it would mean holding
                        // the scope across the wait, which queues one
                        // conversation behind another's steering.
                        let Some(lease) = self.activity.acquire(&request.coach_turn_id) else {
                            return Err(AlternativeMoveCoachTurnError::Conflict(
                                OperationConflictReason::CoachTurnAlreadyActive,
                            ));
                        };
                        return state.start(
                            request,
                            steered_from.as_ref(),
                            operation_id,
                            rollback.expect("admission rollback was captured"),
                            lease,
                        );
                    }
                }
            };
            if let Some((prior_id, mut stopped)) = wait_for_stop {
                while !*stopped.borrow_and_update() {
                    if stopped.changed().await.is_err() {
                        break;
                    }
                }
                steered_from = Some(prior_id);
            }
        }
    }

    async fn run(
        &self,
        admitted: &AdmittedTurn,
    ) -> Result<CoachTurnCommit, ProviderUnavailableReason> {
        let preparation = self.prepare(admitted).await?;
        let mut target = admitted.target.as_ref().clone();
        target.context.coach_turn_id = admitted.coach_turn_id.clone();
        let prior_turn = {
            let state = self.state.lock().await;
            prior_for_same_alternative(
                &state.assessments,
                &admitted.target.target.alternative_move_id,
            )
        };
        let assessment = self
            .author
            .assess(CoachTurnAuthorInput {
                coach_turn_id: admitted.coach_turn_id.clone(),
                message: admitted.message.clone(),
                target,
                evidence_packet: preparation.facts.evidence_packet.clone(),
                evidence: preparation.facts.evidence.clone(),
                prior_turn,
            })
            .await?;
        let assessment = evidence::ground_assessment(
            &admitted.coach_turn_id,
            admitted.target.as_ref(),
            &assessment,
            &preparation.facts.evidence_packet,
            &preparation.facts.evidence,
        )?;
        Ok(CoachTurnCommit {
            assessment,
            evidence_entries: preparation.evidence_entries,
        })
    }

    async fn prepare(
        &self,
        admitted: &AdmittedTurn,
    ) -> Result<CoachTurnPreparation, ProviderUnavailableReason> {
        let elo = EloProfile::try_from(admitted.target.elo.value())
            .expect("EloRating and EloProfile enforce the same range");
        let source_prediction = self
            .predict(
                &admitted.target.source_position,
                elo,
                &admitted.cancellation,
            )
            .await?;
        let resulting_prediction = self
            .predict(
                &admitted.target.target.resulting_position,
                elo,
                &admitted.cancellation,
            )
            .await?;
        let (evidence_entries, evidence_packet, evidence) = prepare_evidence(
            admitted.target.as_ref(),
            source_prediction,
            resulting_prediction,
        )?;
        let mut context = admitted.target.context.clone();
        context.coach_turn_id = admitted.coach_turn_id.clone();
        Ok(CoachTurnPreparation {
            facts: CoachTurnFacts {
                coach_turn_id: admitted.coach_turn_id.clone(),
                message: admitted.message.clone(),
                context,
                alternative_move: admitted.target.target.clone(),
                ancestor_branch: admitted.target.ancestor_branch.clone(),
                source_position: admitted.target.source_position.clone(),
                evidence_packet,
                evidence,
            },
            evidence_entries,
        })
    }

    async fn predict(
        &self,
        position: &PositionSnapshot,
        elo: EloProfile,
        cancellation: &ReviewSessionCancellation,
    ) -> Result<HumanMovePrediction, ProviderUnavailableReason> {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(ProviderUnavailableReason::MaiaTransport),
            result = tokio::time::timeout(MAIA_DEADLINE, self.human.predict(HumanMoveInput {
                position: &position.fen,
                elo,
                limit: PINNED_MAIA_CANDIDATE_LIMIT,
            })) => match result {
                Ok(Ok(prediction)) => Ok(prediction),
                Ok(Err(error)) => Err(map_human_error(error)),
                Err(_) => Err(ProviderUnavailableReason::Timeout {
                    provider: ProviderKind::Maia,
                }),
            }
        }
    }

    async fn finish(
        &self,
        admitted: AdmittedTurn,
        mut outcome: TurnOutcome,
    ) -> Result<CoachTurnResult, AlternativeMoveCoachTurnError> {
        let mut state = self.state.lock().await;
        let is_current = state.active.as_ref().is_some_and(|active| {
            active.coach_turn_id == admitted.coach_turn_id
                && active.idempotency_key == admitted.idempotency_key
        });
        if !is_current {
            return Err(AlternativeMoveCoachTurnError::Conflict(
                OperationConflictReason::IdempotencyKeyMismatch,
            ));
        }
        if admitted.cancellation.is_cancelled() {
            outcome = TurnOutcome::Cancelled;
        }
        let active = state.active.take().expect("current active turn exists");
        let result = match outcome {
            TurnOutcome::Completed(commit) => {
                state.outcomes.insert(
                    admitted.coach_turn_id.clone(),
                    CoachTurnOutcomeCheckpoint::Published {
                        operation_key: admitted.idempotency_key.clone(),
                        idempotency_key: admitted.idempotency_key.clone(),
                        generation: admitted.generation,
                    },
                );
                state.assessments.push(commit.as_ref().clone());
                Ok(CoachTurnResult::Completed(commit))
            }
            TurnOutcome::Prepared(preparation) => {
                state.outcomes.insert(
                    admitted.coach_turn_id.clone(),
                    CoachTurnOutcomeCheckpoint::Prepared {
                        idempotency_key: admitted.idempotency_key.clone(),
                        generation: admitted.generation,
                    },
                );
                state
                    .prepared
                    .insert(admitted.coach_turn_id.clone(), preparation.as_ref().clone());
                Ok(CoachTurnResult::Prepared(preparation))
            }
            TurnOutcome::Unavailable(reason) => {
                state.outcomes.insert(
                    admitted.coach_turn_id.clone(),
                    CoachTurnOutcomeCheckpoint::Unavailable {
                        idempotency_key: admitted.idempotency_key.clone(),
                        generation: admitted.generation,
                        reason: reason.clone(),
                        retry_target: admitted.target.as_ref().clone(),
                    },
                );
                Err(AlternativeMoveCoachTurnError::Unavailable(reason))
            }
            TurnOutcome::Cancelled => {
                state.outcomes.insert(
                    admitted.coach_turn_id.clone(),
                    CoachTurnOutcomeCheckpoint::Cancelled {
                        idempotency_key: admitted.idempotency_key,
                        generation: admitted.generation,
                    },
                );
                Err(AlternativeMoveCoachTurnError::Cancelled)
            }
        };
        active.stopped.send_replace(true);
        result
    }
}

pub(crate) fn ground_coach_turn_publication(
    coach_turn_id: &CoachTurnId,
    target: &PreparedCoachTurnTarget,
    assessment: &AlternativeMoveAssessment,
    packet: &ReviewSessionEvidencePacket,
) -> Result<AlternativeMoveAssessment, ProviderUnavailableReason> {
    evidence::ground_publication(coach_turn_id, target, assessment, packet)
}

impl CoachingState {
    fn checkpoint(&self) -> AlternativeMoveCoachingCheckpoint {
        AlternativeMoveCoachingCheckpoint {
            generation: self.generation,
            started_ids: self.started_ids.clone(),
            active: self
                .active
                .as_ref()
                .map(|active| ActiveCoachTurnCheckpoint {
                    operation_id: active.operation_id.clone(),
                    review_moment_id: active.review_moment_id.clone(),
                    coach_turn_id: active.coach_turn_id.clone(),
                    message: active.message.clone(),
                    idempotency_key: active.idempotency_key.clone(),
                    target: active.target.as_ref().clone(),
                    generation: active.generation,
                }),
            admissions: self.admissions.clone(),
            outcomes: self.outcomes.clone(),
            prepared: self.prepared.clone(),
            assessments: self.assessments.clone(),
        }
    }

    fn retry_target(&self, coach_turn_id: &CoachTurnId) -> Option<PreparedCoachTurnTarget> {
        match self.outcomes.get(coach_turn_id)? {
            CoachTurnOutcomeCheckpoint::Unavailable { retry_target, .. }
            | CoachTurnOutcomeCheckpoint::Interrupted { retry_target, .. } => {
                Some(retry_target.clone())
            }
            CoachTurnOutcomeCheckpoint::Prepared { .. }
            | CoachTurnOutcomeCheckpoint::Published { .. }
            | CoachTurnOutcomeCheckpoint::Cancelled { .. } => None,
        }
    }

    fn start(
        &mut self,
        request: StartAlternativeMoveCoachTurn,
        steered_from: Option<&CoachTurnId>,
        operation_id: Option<OperationId>,
        rollback: AlternativeMoveCoachingCheckpoint,
        lease: CoachTurnLease,
    ) -> Result<AdmittedTurn, AlternativeMoveCoachTurnError> {
        self.validate_capacity(&request.coach_turn_id)?;
        let preserved = match &request.prior_turn {
            PriorCoachTurn::None => None,
            PriorCoachTurn::Steers { coach_turn_id } if steered_from == Some(coach_turn_id) => None,
            PriorCoachTurn::Steers { .. } => {
                return Err(AlternativeMoveCoachTurnError::Conflict(
                    OperationConflictReason::IdempotencyKeyMismatch,
                ));
            }
            PriorCoachTurn::RetriesUnavailable { coach_turn_id } => Some(
                self.retry_target(coach_turn_id)
                    .map(Arc::new)
                    .ok_or_else(|| rejected(CommandRejectionReason::InvalidCommand))?,
            ),
        };
        let target = match request.target {
            CoachTurnTargetSelection::Explicit(target) => Arc::new(*target),
            CoachTurnTargetSelection::Preserve => {
                preserved.ok_or_else(|| rejected(CommandRejectionReason::UnknownTarget))?
            }
        };
        let cancellation = ReviewSessionCancellation::default();
        let (stopped, _) = watch::channel(false);
        self.generation = self
            .generation
            .checked_add(1)
            .expect("Review Session Coach Turn generation cannot overflow");
        let generation = self.generation;
        let operation_id = operation_id.unwrap_or_else(|| {
            OperationId::try_from(format!(
                "coach-turn-operation:{}",
                request.coach_turn_id.as_str()
            ))
            .expect("a Coach Turn ID produces a valid Operation ID")
        });
        let review_moment_id = target.context.reviewed_move.critical_moment_id.clone();
        self.started_ids.insert(request.coach_turn_id.clone());
        self.admissions.insert(
            request.coach_turn_id.clone(),
            ActiveCoachTurnCheckpoint {
                operation_id: operation_id.clone(),
                review_moment_id: review_moment_id.clone(),
                coach_turn_id: request.coach_turn_id.clone(),
                message: request.message.clone(),
                idempotency_key: request.idempotency_key.clone(),
                target: target.as_ref().clone(),
                generation,
            },
        );
        self.active = Some(ActiveTurn {
            operation_id,
            review_moment_id,
            coach_turn_id: request.coach_turn_id.clone(),
            message: request.message.clone(),
            idempotency_key: request.idempotency_key.clone(),
            target: target.clone(),
            generation,
            cancellation: cancellation.clone(),
            stopped,
            lease,
        });
        Ok(AdmittedTurn {
            coach_turn_id: request.coach_turn_id,
            message: request.message,
            idempotency_key: request.idempotency_key,
            target,
            generation,
            cancellation,
            rollback,
        })
    }

    fn validate_capacity(
        &self,
        coach_turn_id: &CoachTurnId,
    ) -> Result<(), AlternativeMoveCoachTurnError> {
        if self.started_ids.contains(coach_turn_id) {
            return Err(rejected(CommandRejectionReason::InvalidCommand));
        }
        if self.started_ids.len() >= usize::from(ReviewSessionLimits::V1.max_started_coach_turns) {
            return Err(AlternativeMoveCoachTurnError::Rejected {
                reason: CommandRejectionReason::CoachTurnLimit,
                recovery: RejectionRecovery::StartNewReviewSession,
            });
        }
        Ok(())
    }
}

fn prior_for_same_alternative(
    assessments: &[CoachTurnCommit],
    alternative_move_id: &AlternativeMoveId,
) -> PriorCoachTurnContext {
    assessments
        .iter()
        .rev()
        .find(|commit| &commit.assessment.alternative_move_id == alternative_move_id)
        .map(|commit| PriorCoachTurnContext::SameAlternative {
            objective_quality: commit.assessment.objective_quality.explanation.clone(),
            findability: commit.assessment.findability.explanation.clone(),
            resilience: commit.assessment.resilience.explanation.clone(),
        })
        .unwrap_or(PriorCoachTurnContext::None)
}

fn validate_message(message: &str) -> Result<(), AlternativeMoveCoachTurnError> {
    if message.trim().is_empty()
        || message.len() > usize::from(ReviewSessionLimits::V1.max_player_message_bytes)
    {
        Err(AlternativeMoveCoachTurnError::Rejected {
            reason: CommandRejectionReason::MessageTooLong,
            recovery: RejectionRecovery::CorrectInput,
        })
    } else {
        Ok(())
    }
}

fn map_human_error(_: HumanMoveModelError) -> ProviderUnavailableReason {
    ProviderUnavailableReason::MaiaTransport
}

fn rejected(reason: CommandRejectionReason) -> AlternativeMoveCoachTurnError {
    AlternativeMoveCoachTurnError::Rejected {
        reason,
        recovery: RejectionRecovery::CorrectInput,
    }
}
