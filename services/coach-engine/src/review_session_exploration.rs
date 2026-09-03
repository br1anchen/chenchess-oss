use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{
    engine_analysis::{EngineAnalysisError, EngineAnalysisInput, EngineAnalyzer, EngineProvenance},
    operating_limits::ALTERNATIVE_MOVE_DEADLINE_MILLISECONDS,
    review_session_cancellation::ReviewSessionCancellation,
    review_session_contract::*,
    review_session_start::reconstruct_selected_position,
};

use evidence::{
    build_evidence_entries, exact_engine_analysis, initialize_packet, within_cache_limits,
};
pub(crate) use position::normalize_engine_analysis as normalize_live_engine_analysis;
pub(crate) use position::{
    apply_move as apply_move_to_snapshot, compare_evaluations as compare_position_evaluations,
    normalize_child_evaluation as normalize_child_position_evaluation,
};
use position::{
    apply_move, compare_evaluations, normalize_child_evaluation, normalize_engine_analysis,
};

mod checkpoint;
mod evidence;
mod position;

pub(crate) use checkpoint::{
    AlternativeMoveExplorationCheckpoint, AlternativeMoveOperationCheckpoint,
    AlternativeMoveOperationOutcome,
};

const ALTERNATIVE_MOVE_DEADLINE: Duration =
    Duration::from_millis(ALTERNATIVE_MOVE_DEADLINE_MILLISECONDS);
const ALTERNATIVE_MOVE_OPERATION_LIMIT: usize = 96;

pub struct AlternativeMoveExploration {
    engine: Arc<dyn EngineAnalyzer>,
    engine_provenance: EvidenceProvenance,
    state: Mutex<ExplorationState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExploreAlternativeMoveRequest {
    pub parent: BranchParent,
    pub source_position_ref: PositionRef,
    pub move_input: MoveInput,
    pub idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlternativeMoveCommit {
    pub alternative_move: AlternativeMoveResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlternativeMoveExplorationState {
    pub root_position: PositionSnapshot,
    pub imported_move_uci: String,
    pub committed_moves: Vec<AlternativeMoveResult>,
    pub evidence_packet: ReviewSessionEvidencePacket,
    pub active_evaluation: Option<IdempotencyKey>,
}

pub type AlternativeMoveCancellation = ReviewSessionCancellation;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AlternativeMoveExplorationStartError {
    #[error("invalid Review Session core: {0}")]
    InvalidCore(&'static str),
    #[error("invalid root Stockfish evidence: {0}")]
    InvalidRootEvidence(&'static str),
    #[error("Review Session evidence cache exceeds review-session-policy/v1")]
    EvidenceCacheLimit,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExploreAlternativeMoveError {
    #[error("Alternative Move was rejected: {reason:?}")]
    Rejected {
        reason: CommandRejectionReason,
        recovery: RejectionRecovery,
    },
    #[error("Alternative Move conflicted with active state: {0:?}")]
    Conflict(OperationConflictReason),
    #[error("Alternative Move evaluation is unavailable: {0:?}")]
    Unavailable(ProviderUnavailableReason),
    #[error("Alternative Move evaluation was cancelled")]
    Cancelled,
}

#[derive(Clone)]
struct ExplorationState {
    root_position: PositionSnapshot,
    root_history: Vec<String>,
    imported_move_uci: String,
    evidence_entries: Vec<EvidenceEntry>,
    nodes: BTreeMap<BranchRef, CommittedNode>,
    completed_by_move: BTreeMap<String, AlternativeMoveCommit>,
    operations: BTreeMap<OperationId, AlternativeMoveOperationCheckpoint>,
    active_operation: Option<OperationId>,
}

#[derive(Clone)]
struct CommittedNode {
    result: AlternativeMoveResult,
    depth: u8,
    history: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct PreparedMove {
    operation_id: OperationId,
    key: IdempotencyKey,
    move_key: String,
    parent: BranchParent,
    source_position: PositionSnapshot,
    source_analysis: EngineAnalysisEvidence,
    move_uci: String,
    resulting_position: PositionSnapshot,
    resulting_history: Vec<String>,
    depth: u8,
    base_packet: ReviewSessionEvidencePacket,
    cached_child_analysis: Option<EngineAnalysisEvidence>,
}

impl PreparedMove {
    pub(crate) fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    pub(crate) fn idempotency_key(&self) -> &IdempotencyKey {
        &self.key
    }
}

enum Admission {
    Existing(Box<AlternativeMoveCommit>),
    Completed(Box<AlternativeMoveCommit>),
    Prepared(Box<PreparedMove>),
}

pub(crate) struct StagedAlternativeMoveExploration<T> {
    state: ExplorationState,
    checkpoint: AlternativeMoveExplorationCheckpoint,
    pub(crate) result: T,
}

#[derive(Clone)]
pub(crate) enum AlternativeMoveAdmission {
    Existing(AlternativeMoveCommit),
    Completed(AlternativeMoveCommit),
    Started(Box<PreparedMove>),
}

impl<T> StagedAlternativeMoveExploration<T> {
    pub(crate) fn checkpoint(&self) -> &AlternativeMoveExplorationCheckpoint {
        &self.checkpoint
    }

    pub(crate) fn evidence_packet(&self) -> ReviewSessionEvidencePacket {
        ReviewSessionEvidencePacket {
            entries: self.state.evidence_entries.clone(),
        }
    }
}

pub(crate) fn retained_root_engine_evidence(
    core: &ReviewSessionCoreContract,
    facts: &GameReviewCriticalMoment,
    provenance: &EngineProvenance,
) -> Option<EvidenceEntry> {
    let position_evidence_id =
        core.evidence_packet
            .entries
            .iter()
            .find_map(|entry| match entry {
                EvidenceEntry::Position { metadata, position }
                    if position.position_ref == core.position_snapshot.position_ref =>
                {
                    Some(metadata.evidence_id.clone())
                }
                _ => None,
            })?;
    let provenance = crate::provider_provenance::stockfish(provenance.clone())?;
    Some(EvidenceEntry::engine_analysis(
        EvidenceMetadata::pending(vec![position_evidence_id], provenance),
        core.position_snapshot.position_ref.clone(),
        EngineAnalysisEvidence {
            evaluation: facts.objective.best_evaluation.clone(),
            best_move_uci: facts.objective.best_move_uci.clone(),
            principal_variation: facts.objective.principal_variation.clone(),
        },
    ))
}

pub(crate) async fn analyze_root_engine_evidence(
    core: &ReviewSessionCoreContract,
    engine: Arc<dyn EngineAnalyzer>,
) -> Option<EvidenceEntry> {
    let position_evidence_id =
        core.evidence_packet
            .entries
            .iter()
            .find_map(|entry| match entry {
                EvidenceEntry::Position { metadata, position }
                    if position.position_ref == core.position_snapshot.position_ref =>
                {
                    Some(metadata.evidence_id.clone())
                }
                _ => None,
            })?;
    let output = engine
        .analyze_with_provenance(EngineAnalysisInput {
            position: &core.position_snapshot.fen,
        })
        .await
        .ok()?;
    let provenance = output.provenance?;
    let analysis = normalize_live_engine_analysis(&core.position_snapshot, output.analysis).ok()?;
    let provenance = crate::provider_provenance::stockfish(provenance)?;
    Some(EvidenceEntry::engine_analysis(
        EvidenceMetadata::pending(vec![position_evidence_id], provenance),
        core.position_snapshot.position_ref.clone(),
        analysis,
    ))
}

impl AlternativeMoveExploration {
    pub fn new(
        core: ReviewSessionCoreContract,
        root_engine_evidence: &EvidenceEntry,
        engine: Arc<dyn EngineAnalyzer>,
    ) -> Result<Self, AlternativeMoveExplorationStartError> {
        let selected_index = usize::from(core.review_moment.ply.checked_sub(1).ok_or(
            AlternativeMoveExplorationStartError::InvalidCore("reviewed ply must be positive"),
        )?);
        let (reconstructed, root_history) =
            reconstruct_selected_position(&core.imported_game, selected_index).map_err(|_| {
                AlternativeMoveExplorationStartError::InvalidCore(
                    "reviewed Position cannot be reconstructed",
                )
            })?;
        if reconstructed != core.position_snapshot
            || core.coach_turn_context.reviewed_move.position_ref
                != core.position_snapshot.position_ref
        {
            return Err(AlternativeMoveExplorationStartError::InvalidCore(
                "reviewed Position binding does not match the imported Game",
            ));
        }

        let (evidence_packet, engine_provenance) = initialize_packet(
            core.evidence_packet,
            &core.position_snapshot,
            root_engine_evidence,
        )?;

        Ok(Self {
            engine,
            engine_provenance,
            state: Mutex::new(ExplorationState {
                root_position: core.position_snapshot,
                root_history,
                imported_move_uci: core.coach_turn_context.reviewed_move.played_move_uci,
                evidence_entries: evidence_packet.entries,
                nodes: BTreeMap::new(),
                completed_by_move: BTreeMap::new(),
                operations: BTreeMap::new(),
                active_operation: None,
            }),
        })
    }

    pub async fn explore(
        &self,
        request: ExploreAlternativeMoveRequest,
        cancellation: AlternativeMoveCancellation,
    ) -> Result<AlternativeMoveCommit, ExploreAlternativeMoveError> {
        let operation_id = OperationId::try_from(format!(
            "operation:alternative-move:{}",
            digest(&request.idempotency_key)
        ))
        .expect("digest-derived Alternative Move operation identity is valid");
        let admission = self.stage_admission(operation_id.clone(), request).await?;
        let prepared = match admission.result.clone() {
            AlternativeMoveAdmission::Existing(commit) => return Ok(commit),
            AlternativeMoveAdmission::Completed(commit) => {
                self.apply_staged(admission).await;
                return Ok(commit);
            }
            AlternativeMoveAdmission::Started(prepared) => {
                self.apply_staged(admission).await;
                prepared
            }
        };
        let draft = match self.evaluate(&prepared, cancellation.clone()).await {
            Ok(draft) => draft,
            Err(error) => {
                if let Some(staged) = self
                    .stage_terminal(&operation_id, &prepared.key, terminal_outcome(&error))
                    .await
                {
                    self.apply_staged(staged).await;
                }
                return Err(error);
            }
        };
        let staged = self.stage_completion(&prepared, draft).await?;
        let commit = staged.result.clone();
        self.apply_staged(staged).await;
        Ok(commit)
    }

    pub(crate) async fn stage_admission(
        &self,
        operation_id: OperationId,
        request: ExploreAlternativeMoveRequest,
    ) -> Result<
        StagedAlternativeMoveExploration<AlternativeMoveAdmission>,
        ExploreAlternativeMoveError,
    > {
        let mut state = self.state.lock().await.clone();
        let admission = state.admit(operation_id, request, &self.engine_provenance)?;
        let result = match admission {
            Admission::Existing(commit) => AlternativeMoveAdmission::Existing(*commit),
            Admission::Completed(commit) => AlternativeMoveAdmission::Completed(*commit),
            Admission::Prepared(prepared) => AlternativeMoveAdmission::Started(prepared),
        };
        let checkpoint = state.checkpoint();
        Ok(StagedAlternativeMoveExploration {
            state,
            checkpoint,
            result,
        })
    }

    pub(crate) async fn evaluate(
        &self,
        prepared: &PreparedMove,
        cancellation: AlternativeMoveCancellation,
    ) -> Result<CommitDraft, ExploreAlternativeMoveError> {
        if cancellation.is_cancelled() {
            return Err(ExploreAlternativeMoveError::Cancelled);
        }

        let admitted_at = tokio::time::Instant::now();
        let child_analysis = if let Some(cached) = prepared.cached_child_analysis.clone() {
            Ok(cached)
        } else {
            let analysis = tokio::select! {
                biased;
                _ = cancellation.cancelled() => Err(ExploreAlternativeMoveError::Cancelled),
                result = tokio::time::timeout(
                    ALTERNATIVE_MOVE_DEADLINE,
                    self.engine.analyze(EngineAnalysisInput {
                        position: &prepared.resulting_position.fen,
                    }),
                ) => match result {
                    Ok(Ok(analysis)) => normalize_engine_analysis(&prepared.resulting_position, analysis)
                        .map_err(|_| ExploreAlternativeMoveError::Unavailable(
                            ProviderUnavailableReason::StockfishProcess,
                        )),
                    Ok(Err(error)) => Err(map_engine_error(error)),
                    Err(_) => Err(ExploreAlternativeMoveError::Unavailable(
                        ProviderUnavailableReason::Timeout { provider: ProviderKind::Stockfish },
                    )),
                }
            };
            analysis
        };

        let child_analysis = match child_analysis {
            Ok(analysis) => analysis,
            Err(error) => return Err(error),
        };
        if cancellation.is_cancelled() {
            return Err(ExploreAlternativeMoveError::Cancelled);
        }
        if admitted_at.elapsed() >= ALTERNATIVE_MOVE_DEADLINE {
            return Err(ExploreAlternativeMoveError::Unavailable(
                ProviderUnavailableReason::Timeout {
                    provider: ProviderKind::Stockfish,
                },
            ));
        }

        build_commit(prepared, child_analysis, &self.engine_provenance)
    }

    pub(crate) async fn stage_completion(
        &self,
        prepared: &PreparedMove,
        draft: CommitDraft,
    ) -> Result<StagedAlternativeMoveExploration<AlternativeMoveCommit>, ExploreAlternativeMoveError>
    {
        let mut state = self.state.lock().await.clone();
        let commit = state.commit(prepared.clone(), draft)?;
        let checkpoint = state.checkpoint();
        Ok(StagedAlternativeMoveExploration {
            state,
            checkpoint,
            result: commit,
        })
    }

    pub(crate) async fn stage_terminal(
        &self,
        operation_id: &OperationId,
        key: &IdempotencyKey,
        outcome: AlternativeMoveOperationOutcome,
    ) -> Option<StagedAlternativeMoveExploration<()>> {
        let mut state = self.state.lock().await.clone();
        if !state.finish_operation(operation_id, key, outcome) {
            return None;
        }
        let checkpoint = state.checkpoint();
        Some(StagedAlternativeMoveExploration {
            state,
            checkpoint,
            result: (),
        })
    }

    pub(crate) async fn apply_staged<T>(&self, staged: StagedAlternativeMoveExploration<T>) -> T {
        *self.state.lock().await = staged.state;
        staged.result
    }

    pub async fn current_state(&self) -> AlternativeMoveExplorationState {
        let state = self.state.lock().await;
        AlternativeMoveExplorationState {
            root_position: state.root_position.clone(),
            imported_move_uci: state.imported_move_uci.clone(),
            committed_moves: state
                .nodes
                .values()
                .map(|node| node.result.clone())
                .collect(),
            evidence_packet: ReviewSessionEvidencePacket {
                entries: state.evidence_entries.clone(),
            },
            active_evaluation: state.active_key(),
        }
    }

    pub async fn remaining_allowance(&self) -> u8 {
        self.state.lock().await.remaining_allowance()
    }

    pub(crate) fn grounded_evaluation(
        &self,
        packet: &ReviewSessionEvidencePacket,
        position: &PositionSnapshot,
    ) -> Option<EngineEvaluation> {
        exact_engine_analysis(packet, position, &self.engine_provenance)
            .map(|analysis| analysis.evaluation)
    }

    pub(crate) async fn append_evidence_entries(&self, entries: Vec<EvidenceEntry>) {
        let mut state = self.state.lock().await;
        state.evidence_entries.extend(entries);
    }
}

impl ExplorationState {
    fn admit(
        &mut self,
        operation_id: OperationId,
        request: ExploreAlternativeMoveRequest,
        engine_provenance: &EvidenceProvenance,
    ) -> Result<Admission, ExploreAlternativeMoveError> {
        if let Some(operation) = self.operations.get(&operation_id) {
            if operation.request != request {
                return Err(rejected(
                    CommandRejectionReason::InvalidCommand,
                    RejectionRecovery::CorrectInput,
                ));
            }
            return self.operation_admission(operation);
        }
        if let Some(operation) = self
            .operations
            .values()
            .find(|operation| operation.request.idempotency_key == request.idempotency_key)
        {
            return self.operation_admission(operation);
        }
        if self.active_operation.is_some() {
            return Err(ExploreAlternativeMoveError::Conflict(
                OperationConflictReason::AlternativeMoveEvaluationAlreadyActive,
            ));
        }
        if self.operations.len() >= ALTERNATIVE_MOVE_OPERATION_LIMIT {
            return Err(ExploreAlternativeMoveError::Unavailable(
                ProviderUnavailableReason::AdmissionLimit,
            ));
        }

        let (source_position, source_history, depth) = self.source(&request)?;
        if !matches!(source_position.status, PositionStatus::Ongoing { .. }) {
            return Err(rejected(
                CommandRejectionReason::TerminalPosition,
                RejectionRecovery::None,
            ));
        }
        let applied = apply_move(&source_position, &source_history, &request.move_input)?;
        let move_uci = applied.uci;
        let move_key = move_key(&request.parent, &move_uci);
        if let Some(completed) = self.completed_by_move.get(&move_key) {
            self.operations.insert(
                operation_id.clone(),
                AlternativeMoveOperationCheckpoint {
                    operation_id,
                    request,
                    normalized_move_uci: move_uci,
                    outcome: AlternativeMoveOperationOutcome::Completed,
                },
            );
            return Ok(Admission::Completed(Box::new(completed.clone())));
        }
        if self.remaining_allowance() == 0 {
            return Err(rejected(
                CommandRejectionReason::AlternativeMoveLimit,
                RejectionRecovery::StartNewReviewSession,
            ));
        }
        if depth > ReviewSessionLimits::V1.max_branch_depth_plies {
            return Err(rejected(
                CommandRejectionReason::BranchDepthLimit,
                RejectionRecovery::StartNewReviewSession,
            ));
        }
        let source_analysis = exact_engine_analysis(
            &ReviewSessionEvidencePacket {
                entries: self.evidence_entries.clone(),
            },
            &source_position,
            engine_provenance,
        )
        .ok_or_else(|| {
            rejected(
                CommandRejectionReason::MissingEvidence,
                RejectionRecovery::None,
            )
        })?;
        let resulting_position = applied.resulting_position;
        let resulting_history = applied.resulting_history;
        let cached_child_analysis = exact_engine_analysis(
            &ReviewSessionEvidencePacket {
                entries: self.evidence_entries.clone(),
            },
            &resulting_position,
            engine_provenance,
        );
        self.active_operation = Some(operation_id.clone());
        self.operations.insert(
            operation_id.clone(),
            AlternativeMoveOperationCheckpoint {
                operation_id: operation_id.clone(),
                request: request.clone(),
                normalized_move_uci: move_uci.clone(),
                outcome: AlternativeMoveOperationOutcome::Active,
            },
        );
        Ok(Admission::Prepared(Box::new(PreparedMove {
            operation_id,
            key: request.idempotency_key,
            move_key,
            parent: request.parent,
            source_position,
            source_analysis,
            move_uci,
            resulting_position,
            resulting_history,
            depth,
            base_packet: ReviewSessionEvidencePacket {
                entries: self.evidence_entries.clone(),
            },
            cached_child_analysis,
        })))
    }

    fn operation_admission(
        &self,
        operation: &AlternativeMoveOperationCheckpoint,
    ) -> Result<Admission, ExploreAlternativeMoveError> {
        match operation.outcome {
            AlternativeMoveOperationOutcome::Completed => {
                let move_key = move_key(&operation.request.parent, &operation.normalized_move_uci);
                self.completed_by_move
                    .get(&move_key)
                    .cloned()
                    .map(|commit| Admission::Existing(Box::new(commit)))
                    .ok_or_else(|| {
                        rejected(
                            CommandRejectionReason::MissingEvidence,
                            RejectionRecovery::StartNewReviewSession,
                        )
                    })
            }
            AlternativeMoveOperationOutcome::Active => Err(ExploreAlternativeMoveError::Conflict(
                OperationConflictReason::AlternativeMoveEvaluationAlreadyActive,
            )),
            AlternativeMoveOperationOutcome::Cancelled
            | AlternativeMoveOperationOutcome::Interrupted => {
                Err(ExploreAlternativeMoveError::Conflict(
                    OperationConflictReason::IdempotencyKeyMismatch,
                ))
            }
        }
    }

    fn source(
        &self,
        request: &ExploreAlternativeMoveRequest,
    ) -> Result<(PositionSnapshot, Vec<String>, u8), ExploreAlternativeMoveError> {
        match &request.parent {
            BranchParent::Root { position_ref }
                if position_ref == &self.root_position.position_ref
                    && &request.source_position_ref == position_ref =>
            {
                Ok((self.root_position.clone(), self.root_history.clone(), 1))
            }
            BranchParent::Move { branch_ref } => {
                let node = self.nodes.get(branch_ref).ok_or_else(|| {
                    rejected(
                        CommandRejectionReason::UnknownTarget,
                        RejectionRecovery::None,
                    )
                })?;
                if request.source_position_ref != node.result.resulting_position.position_ref {
                    return Err(rejected(
                        CommandRejectionReason::InvalidCommand,
                        RejectionRecovery::CorrectInput,
                    ));
                }
                Ok((
                    node.result.resulting_position.clone(),
                    node.history.clone(),
                    node.depth + 1,
                ))
            }
            BranchParent::Root { .. } => Err(rejected(
                CommandRejectionReason::InvalidCommand,
                RejectionRecovery::CorrectInput,
            )),
        }
    }

    fn commit(
        &mut self,
        prepared: PreparedMove,
        draft: CommitDraft,
    ) -> Result<AlternativeMoveCommit, ExploreAlternativeMoveError> {
        if self.active_operation.as_ref() != Some(&prepared.operation_id)
            || !self
                .operations
                .get(&prepared.operation_id)
                .is_some_and(|operation| {
                    operation.outcome == AlternativeMoveOperationOutcome::Active
                        && operation.request.idempotency_key == prepared.key
                })
        {
            return Err(ExploreAlternativeMoveError::Conflict(
                OperationConflictReason::IdempotencyKeyMismatch,
            ));
        }
        // Evidence is content-addressed, so an id already held is the same
        // evidence: appending it again records no second fact and leaves a
        // duplicate that fails cache assembly for this Game Import from then
        // on, permanently and for every later move.
        //
        // The draft cannot prevent it alone. It deduplicates against the packet
        // as it stood when the move was prepared, which does not carry anything
        // committed or restored since, and its branch entry is not deduplicated
        // at all. Committing is the point that knows the whole set.
        let mut held = self
            .evidence_entries
            .iter()
            .map(|entry| entry.metadata().evidence_id.clone())
            .collect::<BTreeSet<_>>();
        let novel_evidence = draft
            .evidence_entries
            .into_iter()
            .filter(|entry| held.insert(entry.metadata().evidence_id.clone()))
            .collect::<Vec<_>>();
        let mut entries = self.evidence_entries.clone();
        entries.extend(novel_evidence.iter().cloned());
        let packet = ReviewSessionEvidencePacket { entries };
        if !within_cache_limits(&packet) {
            return Err(ExploreAlternativeMoveError::Unavailable(
                ProviderUnavailableReason::AdmissionLimit,
            ));
        }
        let commit = AlternativeMoveCommit {
            alternative_move: draft.alternative_move.clone(),
        };
        self.nodes.insert(
            draft.alternative_move.branch_ref.clone(),
            CommittedNode {
                result: draft.alternative_move,
                depth: prepared.depth,
                history: prepared.resulting_history,
            },
        );
        self.completed_by_move
            .insert(prepared.move_key, commit.clone());
        self.evidence_entries.extend(novel_evidence);
        self.finish_operation(
            &prepared.operation_id,
            &prepared.key,
            AlternativeMoveOperationOutcome::Completed,
        );
        Ok(commit)
    }

    fn finish_operation(
        &mut self,
        operation_id: &OperationId,
        key: &IdempotencyKey,
        outcome: AlternativeMoveOperationOutcome,
    ) -> bool {
        let Some(operation) = self.operations.get_mut(operation_id) else {
            return false;
        };
        if operation.outcome != AlternativeMoveOperationOutcome::Active
            || &operation.request.idempotency_key != key
        {
            return false;
        }
        operation.outcome = outcome;
        if self.active_operation.as_ref() == Some(operation_id) {
            self.active_operation = None;
        }
        true
    }

    fn active_key(&self) -> Option<IdempotencyKey> {
        self.active_operation
            .as_ref()
            .and_then(|operation_id| self.operations.get(operation_id))
            .map(|operation| operation.request.idempotency_key.clone())
    }

    fn remaining_allowance(&self) -> u8 {
        let committed = u8::try_from(self.nodes.len())
            .expect("committed Alternative Move count fits the contracted limit");
        ReviewSessionLimits::V1
            .max_committed_alternative_moves
            .checked_sub(committed)
            .expect("committed Alternative Move count does not exceed the contracted limit")
    }
}

pub(crate) struct CommitDraft {
    alternative_move: AlternativeMoveResult,
    evidence_entries: Vec<EvidenceEntry>,
}

fn build_commit(
    prepared: &PreparedMove,
    child_analysis: EngineAnalysisEvidence,
    engine_provenance: &EvidenceProvenance,
) -> Result<CommitDraft, ExploreAlternativeMoveError> {
    let mover = prepared.source_position.side_to_move;
    let selected_move = normalize_child_evaluation(&child_analysis.evaluation, mover).ok_or(
        ExploreAlternativeMoveError::Unavailable(ProviderUnavailableReason::StockfishProcess),
    )?;
    let best_move = prepared.source_analysis.evaluation.clone();
    let comparison = compare_evaluations(&best_move, &selected_move).ok_or(
        ExploreAlternativeMoveError::Unavailable(ProviderUnavailableReason::StockfishProcess),
    )?;
    let branch_ref = BranchRef::try_from(format!(
        "branch:{}",
        digest(&(
            &prepared.parent,
            &prepared.source_position.position_ref,
            &prepared.move_uci,
            &prepared.resulting_position.position_ref,
        ))
    ))
    .expect("digest-derived branch identity is valid");
    let alternative_move_id =
        AlternativeMoveId::try_from(format!("alternative-move:{}", digest(&branch_ref)))
            .expect("digest-derived Alternative Move identity is valid");
    let strongest_reply = if matches!(
        prepared.resulting_position.status,
        PositionStatus::Ongoing { .. }
    ) {
        StrongestReply::Offered {
            uci: child_analysis.best_move_uci.clone(),
        }
    } else {
        StrongestReply::Terminal
    };
    let alternative_move = AlternativeMoveResult {
        alternative_move_id,
        branch_ref: branch_ref.clone(),
        parent: prepared.parent.clone(),
        source_position_ref: prepared.source_position.position_ref.clone(),
        move_uci: prepared.move_uci.clone(),
        resulting_position: prepared.resulting_position.clone(),
        evaluation: AlternativeMoveEvaluation {
            selected_move,
            best_move_uci: prepared.source_analysis.best_move_uci.clone(),
            best_move,
            comparison,
        },
        strongest_reply,
    };
    let evidence_entries = build_evidence_entries(
        prepared,
        &alternative_move,
        child_analysis,
        engine_provenance,
    )?;
    Ok(CommitDraft {
        alternative_move,
        evidence_entries,
    })
}

fn map_engine_error(error: EngineAnalysisError) -> ExploreAlternativeMoveError {
    match error {
        EngineAnalysisError::Timeout => {
            ExploreAlternativeMoveError::Unavailable(ProviderUnavailableReason::Timeout {
                provider: ProviderKind::Stockfish,
            })
        }
        EngineAnalysisError::Process(_)
        | EngineAnalysisError::Protocol(_)
        | EngineAnalysisError::InvalidInput(_) => {
            ExploreAlternativeMoveError::Unavailable(ProviderUnavailableReason::StockfishProcess)
        }
    }
}

fn rejected(
    reason: CommandRejectionReason,
    recovery: RejectionRecovery,
) -> ExploreAlternativeMoveError {
    ExploreAlternativeMoveError::Rejected { reason, recovery }
}

fn terminal_outcome(error: &ExploreAlternativeMoveError) -> AlternativeMoveOperationOutcome {
    match error {
        ExploreAlternativeMoveError::Cancelled => AlternativeMoveOperationOutcome::Cancelled,
        ExploreAlternativeMoveError::Rejected { .. }
        | ExploreAlternativeMoveError::Conflict(_)
        | ExploreAlternativeMoveError::Unavailable(_) => {
            AlternativeMoveOperationOutcome::Interrupted
        }
    }
}

fn move_key(parent: &BranchParent, move_uci: &str) -> String {
    digest(&(parent, move_uci))
}

fn digest(value: &impl Serialize) -> String {
    let encoded = serde_json::to_vec(value).expect("domain identity is serializable");
    format!("{:x}", Sha256::digest(encoded))
}
