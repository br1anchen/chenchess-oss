use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlternativeMoveExplorationCheckpoint {
    pub(crate) committed_moves: Vec<AlternativeMoveResult>,
    pub(crate) operations: Vec<AlternativeMoveOperationCheckpoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlternativeMoveOperationCheckpoint {
    pub(crate) operation_id: OperationId,
    pub(crate) request: ExploreAlternativeMoveRequest,
    pub(crate) normalized_move_uci: String,
    pub(crate) outcome: AlternativeMoveOperationOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AlternativeMoveOperationOutcome {
    Active,
    Completed,
    Cancelled,
    Interrupted,
}

impl AlternativeMoveExplorationCheckpoint {
    /// Closes any operation that was in flight, reporting whether one was.
    ///
    /// Alternative Move Exploration is process-local, so an operation that was
    /// active when a Review Session was last touched has no worker behind it any
    /// more. Rebuilding a session inherits the record but never the work, and
    /// leaving it Active would refuse the Player's retry on behalf of a
    /// computation nobody is running.
    pub(crate) fn interrupt_active(&mut self) -> bool {
        let mut changed = false;
        for operation in &mut self.operations {
            if operation.outcome == AlternativeMoveOperationOutcome::Active {
                operation.outcome = AlternativeMoveOperationOutcome::Interrupted;
                changed = true;
            }
        }
        changed
    }

    /// Names the first way this checkpoint is malformed, or nothing.
    ///
    /// The caller reports the reason to an operator, so which of these fired is
    /// the whole value: they are six unrelated faults, and a checkpoint that
    /// fails one of them fails every later write for its Game Import until the
    /// stored shape is understood.
    pub(crate) fn validate_shape(
        &self,
        idempotency_keys: &BTreeSet<IdempotencyKey>,
    ) -> Result<(), &'static str> {
        if self.committed_moves.len()
            > usize::from(ReviewSessionLimits::V1.max_committed_alternative_moves)
        {
            return Err("more committed Alternative Moves than the session allows");
        }
        if self.operations.len() > ALTERNATIVE_MOVE_OPERATION_LIMIT {
            return Err("more Alternative Move operations than the session allows");
        }
        if self
            .operations
            .iter()
            .filter(|operation| operation.outcome == AlternativeMoveOperationOutcome::Active)
            .count()
            > 1
        {
            return Err("more than one Alternative Move operation is active");
        }
        let operation_ids = self
            .operations
            .iter()
            .map(|operation| &operation.operation_id)
            .collect::<BTreeSet<_>>();
        if operation_ids.len() != self.operations.len() {
            return Err("two Alternative Move operations share one operation id");
        }
        let operation_keys = self
            .operations
            .iter()
            .map(|operation| &operation.request.idempotency_key)
            .collect::<BTreeSet<_>>();
        if operation_keys.len() != self.operations.len() {
            return Err("two Alternative Move operations share one idempotency key");
        }
        if !operation_keys
            .iter()
            .all(|key| idempotency_keys.contains(*key))
        {
            return Err(
                "an Alternative Move operation's idempotency key is absent from the Review Moment",
            );
        }
        Ok(())
    }
}

impl AlternativeMoveExploration {
    pub(crate) async fn checkpoint(&self) -> AlternativeMoveExplorationCheckpoint {
        self.state.lock().await.checkpoint()
    }

    pub(crate) async fn restore_checkpoint(
        &self,
        checkpoint: &AlternativeMoveExplorationCheckpoint,
    ) -> Result<(), AlternativeMoveExplorationStartError> {
        let mut current = self.state.lock().await;
        let state = self.rebuild_state(current.clone(), checkpoint)?;
        *current = state;
        Ok(())
    }

    fn rebuild_state(
        &self,
        mut state: ExplorationState,
        checkpoint: &AlternativeMoveExplorationCheckpoint,
    ) -> Result<ExplorationState, AlternativeMoveExplorationStartError> {
        state.nodes.clear();
        state.completed_by_move.clear();
        state.operations.clear();
        state.active_operation = None;

        for result in &checkpoint.committed_moves {
            state.restore_move(result, &self.engine_provenance)?;
        }
        for operation in &checkpoint.operations {
            state.restore_operation(operation)?;
        }
        Ok(state)
    }
}

impl ExplorationState {
    pub(super) fn checkpoint(&self) -> AlternativeMoveExplorationCheckpoint {
        let mut committed_moves = self
            .nodes
            .values()
            .map(|node| (node.depth, node.result.clone()))
            .collect::<Vec<_>>();
        committed_moves.sort_by(|(left_depth, left), (right_depth, right)| {
            left_depth
                .cmp(right_depth)
                .then_with(|| left.branch_ref.cmp(&right.branch_ref))
        });
        AlternativeMoveExplorationCheckpoint {
            committed_moves: committed_moves
                .into_iter()
                .map(|(_, result)| result)
                .collect(),
            operations: self.operations.values().cloned().collect(),
        }
    }

    fn restore_move(
        &mut self,
        result: &AlternativeMoveResult,
        engine_provenance: &EvidenceProvenance,
    ) -> Result<(), AlternativeMoveExplorationStartError> {
        let request = ExploreAlternativeMoveRequest {
            parent: result.parent.clone(),
            source_position_ref: result.source_position_ref.clone(),
            move_input: MoveInput::Uci {
                uci: result.move_uci.clone(),
            },
            idempotency_key: IdempotencyKey::try_from(
                "idempotency-key:checkpoint:validation".to_string(),
            )
            .expect("static checkpoint validation key is valid"),
        };
        let (source_position, source_history, depth) = self
            .source(&request)
            .map_err(|_| invalid_checkpoint("Alternative Move ancestry is invalid"))?;
        let applied = apply_move(&source_position, &source_history, &request.move_input)
            .map_err(|_| invalid_checkpoint("Alternative Move is no longer legal"))?;
        if applied.uci != result.move_uci || applied.resulting_position != result.resulting_position
        {
            return Err(invalid_checkpoint(
                "Alternative Move resulting Position is invalid",
            ));
        }
        let packet = ReviewSessionEvidencePacket {
            entries: self.evidence_entries.clone(),
        };
        let source_analysis =
            exact_engine_analysis(&packet, &source_position, engine_provenance)
                .ok_or_else(|| invalid_checkpoint("Alternative Move source evidence is missing"))?;
        let child_analysis =
            exact_engine_analysis(&packet, &applied.resulting_position, engine_provenance)
                .ok_or_else(|| {
                    invalid_checkpoint("Alternative Move resulting evidence is missing")
                })?;
        let prepared = PreparedMove {
            operation_id: OperationId::try_from("operation:checkpoint:validation".to_string())
                .expect("static checkpoint validation operation is valid"),
            key: request.idempotency_key,
            move_key: move_key(&result.parent, &result.move_uci),
            parent: result.parent.clone(),
            source_position,
            source_analysis,
            move_uci: result.move_uci.clone(),
            resulting_position: applied.resulting_position,
            resulting_history: applied.resulting_history.clone(),
            depth,
            base_packet: packet,
            cached_child_analysis: Some(child_analysis.clone()),
        };
        let draft = build_commit(&prepared, child_analysis, engine_provenance)
            .map_err(|_| invalid_checkpoint("Alternative Move evaluation is invalid"))?;
        if draft.alternative_move != *result
            || !has_branch_evidence(&self.evidence_entries, result)
            || self.nodes.contains_key(&result.branch_ref)
        {
            return Err(invalid_checkpoint(
                "Alternative Move result or evidence is inconsistent",
            ));
        }
        let commit = AlternativeMoveCommit {
            alternative_move: result.clone(),
        };
        self.nodes.insert(
            result.branch_ref.clone(),
            CommittedNode {
                result: result.clone(),
                depth,
                history: applied.resulting_history,
            },
        );
        self.completed_by_move.insert(prepared.move_key, commit);
        Ok(())
    }

    fn restore_operation(
        &mut self,
        operation: &AlternativeMoveOperationCheckpoint,
    ) -> Result<(), AlternativeMoveExplorationStartError> {
        if self.operations.contains_key(&operation.operation_id)
            || self.operations.values().any(|existing| {
                existing.request.idempotency_key == operation.request.idempotency_key
            })
        {
            return Err(invalid_checkpoint(
                "Alternative Move operation identity is duplicated",
            ));
        }
        let (source, history, _) = self
            .source(&operation.request)
            .map_err(|_| invalid_checkpoint("Alternative Move operation target is invalid"))?;
        let applied = apply_move(&source, &history, &operation.request.move_input)
            .map_err(|_| invalid_checkpoint("Alternative Move operation is not legal"))?;
        if applied.uci != operation.normalized_move_uci {
            return Err(invalid_checkpoint(
                "Alternative Move operation normalization is invalid",
            ));
        }
        if operation.outcome == AlternativeMoveOperationOutcome::Completed
            && !self
                .completed_by_move
                .contains_key(&move_key(&operation.request.parent, &applied.uci))
        {
            return Err(invalid_checkpoint(
                "completed Alternative Move operation has no committed result",
            ));
        }
        if operation.outcome == AlternativeMoveOperationOutcome::Active {
            if self.active_operation.is_some() {
                return Err(invalid_checkpoint(
                    "multiple Alternative Move operations are active",
                ));
            }
            self.active_operation = Some(operation.operation_id.clone());
        }
        self.operations
            .insert(operation.operation_id.clone(), operation.clone());
        Ok(())
    }
}

fn has_branch_evidence(entries: &[EvidenceEntry], result: &AlternativeMoveResult) -> bool {
    entries.iter().any(|entry| {
        matches!(
            entry,
            EvidenceEntry::Branch { branch, .. }
                if branch.branch_ref == result.branch_ref
                    && branch.parent == result.parent
                    && branch.source_position_ref == result.source_position_ref
                    && branch.move_uci == result.move_uci
                    && branch.resulting_position_ref == result.resulting_position.position_ref
        )
    })
}

fn invalid_checkpoint(message: &'static str) -> AlternativeMoveExplorationStartError {
    AlternativeMoveExplorationStartError::InvalidCore(message)
}
