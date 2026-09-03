use crate::{
    review_session_contract::{
        CriticalMomentId, GameRef, GameReviewMomentClassification, LearningPathRef, LearningTrack,
        LearningTrackPurpose, LearningTrackSupport, LearningTrackSupportBasis,
        OpeningLearningEvidence, OpeningMetadata, PositionPhaseKind,
        LEARNING_PLAN_SELECTION_POLICY_VERSION, LEARNING_RESOURCE_CATALOG_VERSION,
    },
    rule_extractor::MomentFact,
    types::Game,
};

use super::catalog::{opening_key_for, resources_for, ResourceCatalogError};

pub(super) fn track(
    game: &Game,
    moment: &MomentFact,
    game_ref: &GameRef,
    opening_identification: &OpeningMetadata,
) -> Result<Option<LearningTrack>, ResourceCatalogError> {
    if !matches!(
        moment.classification,
        GameReviewMomentClassification::ImprovementOpportunity { .. }
    ) || moment.position_phase.phase != PositionPhaseKind::Opening
        || matches!(opening_identification, OpeningMetadata::Absent)
    {
        return Ok(None);
    }
    let game_move = game
        .moves
        .iter()
        .find(|game_move| game_move.ply == moment.ply)
        .ok_or(ResourceCatalogError::MissingGameMove)?;
    let Some((key, resource_mapping_id)) =
        opening_key_for(opening_identification, &game_move.position)?
    else {
        return Ok(None);
    };
    let ply = u16::try_from(moment.ply).map_err(|_| ResourceCatalogError::InvalidPly)?;
    let critical_moment_id = CriticalMomentId::for_imported_game(game_ref, ply);
    let basis = LearningTrackSupportBasis::Opening {
        evidence: OpeningLearningEvidence {
            position_phase: moment.position_phase,
            opening_identification: opening_identification.clone(),
            resource_mapping_id,
        },
    };
    let learning_path_ref = LearningPathRef::for_selected_support(
        game_ref,
        &critical_moment_id,
        &key,
        LearningTrackPurpose::Improvement,
        &basis,
        LEARNING_PLAN_SELECTION_POLICY_VERSION,
        LEARNING_RESOURCE_CATALOG_VERSION,
    );
    Ok(Some(LearningTrack {
        resources: resources_for(&key)?,
        key,
        support: vec![LearningTrackSupport::Improvement {
            learning_path_ref,
            critical_moment_id,
            ply,
            basis,
        }],
    }))
}
