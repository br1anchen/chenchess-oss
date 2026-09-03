use crate::{
    decision_explanation::{
        explain_decision, DecisionExplanationContractError, DecisionExplanationInput,
    },
    decision_learning::{
        apply_decision_learning, decision_learning_build, decision_provenance,
        DecisionLearningBuild,
    },
    game_import_store::ImportedCriticalMoment,
    review_session_contract::{
        CandidateEvidence, DecisionLearningAbstentionReason, EngineCandidateEvidence, GameRef,
        GameReviewMomentClassification, GameReviewMomentProvenance, PlayerMoveEvidence,
        PositionSnapshot, ProofCapability,
    },
};

pub(crate) fn materialize(
    game_ref: &GameRef,
    position_snapshot: &PositionSnapshot,
    mut imported: ImportedCriticalMoment,
) -> Result<ImportedCriticalMoment, PlayerSelectedDecisionError> {
    if imported.moment.provenance != GameReviewMomentProvenance::PlayerSelected {
        return Err(PlayerSelectedDecisionError::MismatchedMoment);
    }
    if matches!(
        imported.moment.classification,
        GameReviewMomentClassification::Neutral { .. }
    ) {
        imported.decision_explanation = None;
        imported.moment.learning_material.tracks.clear();
        return Ok(imported);
    }
    let Some(candidate_evidence) = single_pv_evidence(&imported) else {
        let opening_material = imported.moment.learning_material.clone();
        apply_decision_learning(
            game_ref,
            &mut imported.moment,
            DecisionLearningBuild::Abstained {
                reason: DecisionLearningAbstentionReason::CandidateEvidenceUnavailable,
            },
            &opening_material,
        )
        .map_err(PlayerSelectedDecisionError::LearningMaterial)?;
        return Ok(imported);
    };
    let build = explain_decision(DecisionExplanationInput {
        game_ref: game_ref.clone(),
        critical_moment_id: imported.moment.critical_moment_id.clone(),
        position_snapshot: position_snapshot.clone(),
        classification: imported.moment.classification.clone(),
        provenance: GameReviewMomentProvenance::PlayerSelected,
        player_move_uci: imported.moment.objective.played_move_uci.clone(),
        candidate_evidence,
    })?;
    let build =
        decision_learning_build(build).map_err(PlayerSelectedDecisionError::LearningMaterial)?;
    let opening_material = imported.moment.learning_material.clone();
    apply_decision_learning(game_ref, &mut imported.moment, build, &opening_material)
        .map_err(PlayerSelectedDecisionError::LearningMaterial)?;
    let explanation = imported.moment.decision_explanation.take();
    if explanation
        .as_ref()
        .is_some_and(|explanation| explanation.capability != ProofCapability::ValidationOnly)
    {
        return Err(PlayerSelectedDecisionError::UnexpectedCapability);
    }
    // A Player-Selected proof is materialized beside the moment rather than
    // frozen into it, so the moment carries only the reference that addresses
    // it — the same thing a delivered Automatic moment carries.
    imported.decision_explanation = explanation;
    Ok(imported)
}

fn single_pv_evidence(imported: &ImportedCriticalMoment) -> Option<CandidateEvidence> {
    let moment = &imported.moment;
    let provenance = decision_provenance(imported.engine_provenance.as_ref()?)?;
    let authoritative = EngineCandidateEvidence {
        rank: 1,
        root_move_uci: moment.objective.best_move_uci.clone(),
        evaluation: moment.objective.best_evaluation.clone(),
        variation: moment.objective.principal_variation.clone(),
        provenance: provenance.clone(),
    };
    let mut retained_variation = vec![moment.objective.played_move_uci.clone()];
    if let Some(lines) = &moment.objective.lines {
        retained_variation.extend(lines.refutation.iter().map(|line| line.uci.clone()));
    }
    if moment.objective.best_move_uci == moment.objective.played_move_uci
        && retained_variation.len() == 1
    {
        retained_variation = moment.objective.principal_variation.clone();
    }
    Some(CandidateEvidence::SinglePv {
        authoritative,
        player_move: PlayerMoveEvidence {
            root_move_uci: moment.objective.played_move_uci.clone(),
            evaluation: moment.objective.played_evaluation.clone(),
            retained_variation,
            provenance,
        },
    })
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PlayerSelectedDecisionError {
    #[error("Player-Selected decision input does not match the Review Moment")]
    MismatchedMoment,
    #[error("Player-Selected Decision Explanation did not derive ValidationOnly")]
    UnexpectedCapability,
    #[error("Player-Selected Learning Material is invalid: {0}")]
    LearningMaterial(&'static str),
    #[error(transparent)]
    Decision(#[from] DecisionExplanationContractError),
}
