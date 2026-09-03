use std::collections::BTreeMap;

use crate::{
    review_session_contract::*, review_session_exploration::AlternativeMoveExplorationState,
};

use super::{evidence, CoachTurnTargetError, PreparedCoachTurnTarget};

impl PreparedCoachTurnTarget {
    pub fn capture(
        core: &ReviewSessionCoreContract,
        exploration: &AlternativeMoveExplorationState,
        alternative_move_id: &AlternativeMoveId,
    ) -> Result<Self, CoachTurnTargetError> {
        let target = exploration
            .committed_moves
            .iter()
            .find(|candidate| &candidate.alternative_move_id == alternative_move_id)
            .cloned()
            .ok_or(CoachTurnTargetError::UnknownTarget)?;
        let source_position = source_position(exploration, &target)?;
        let ancestor_branch = ancestor_branch(exploration, &target)?;
        let evidence_packet = exploration.evidence_packet.objective_branch_evidence();
        let mut context = core.coach_turn_context.objective_context(&evidence_packet);
        for branch in &ancestor_branch {
            push_unique(
                &mut context.required_evidence_refs,
                evidence::branch_evidence_id(&evidence_packet, &branch.branch_ref)
                    .ok_or(CoachTurnTargetError::MissingEvidence)?,
            );
        }
        context.selected_position_ref = source_position.position_ref.clone();
        context.target = CoachTurnTarget::AlternativeMove {
            branch_ref: target.branch_ref.clone(),
            uci: target.move_uci.clone(),
        };

        Ok(Self {
            context,
            target,
            ancestor_branch,
            source_position,
            evidence_packet,
            elo: core.imported_game.elo_profile.rating,
        })
    }

    pub fn reviewed_move(
        core: &ReviewSessionCoreContract,
    ) -> (CoachTurnContext, ReviewSessionEvidencePacket) {
        let evidence_packet = core.evidence_packet.objective_branch_evidence();
        let context = core.coach_turn_context.objective_context(&evidence_packet);
        (context, evidence_packet)
    }
}

fn source_position(
    exploration: &AlternativeMoveExplorationState,
    target: &AlternativeMoveResult,
) -> Result<PositionSnapshot, CoachTurnTargetError> {
    let source = match &target.parent {
        BranchParent::Root { position_ref } if position_ref == &target.source_position_ref => {
            &exploration.root_position
        }
        BranchParent::Move { branch_ref } => exploration
            .committed_moves
            .iter()
            .find(|candidate| &candidate.branch_ref == branch_ref)
            .map(|candidate| &candidate.resulting_position)
            .ok_or(CoachTurnTargetError::InvalidAncestry)?,
        BranchParent::Root { .. } => return Err(CoachTurnTargetError::InvalidAncestry),
    };
    (source.position_ref == target.source_position_ref)
        .then(|| source.clone())
        .ok_or(CoachTurnTargetError::InvalidAncestry)
}

fn ancestor_branch(
    exploration: &AlternativeMoveExplorationState,
    target: &AlternativeMoveResult,
) -> Result<Vec<AlternativeMoveResult>, CoachTurnTargetError> {
    let by_branch = exploration
        .committed_moves
        .iter()
        .map(|node| (&node.branch_ref, node))
        .collect::<BTreeMap<_, _>>();
    let mut branch = vec![target.clone()];
    let mut parent = &target.parent;
    while let BranchParent::Move { branch_ref } = parent {
        let node = by_branch
            .get(branch_ref)
            .ok_or(CoachTurnTargetError::InvalidAncestry)?;
        branch.push((*node).clone());
        parent = &node.parent;
    }
    let BranchParent::Root { position_ref } = parent else {
        unreachable!("branch traversal stops only at the root")
    };
    if position_ref != &exploration.root_position.position_ref {
        return Err(CoachTurnTargetError::InvalidAncestry);
    }
    branch.reverse();
    Ok(branch)
}

fn push_unique(values: &mut Vec<EvidenceId>, value: EvidenceId) {
    if !values.contains(&value) {
        values.push(value);
    }
}
