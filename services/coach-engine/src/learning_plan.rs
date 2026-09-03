use crate::{
    review_session_contract::{
        GameRef, OpeningMetadata, ReviewMomentLearningMaterial,
        LEARNING_PLAN_SELECTION_POLICY_VERSION, LEARNING_RESOURCE_CATALOG_VERSION,
    },
    rule_extractor::MomentFact,
    types::Game,
};

pub(crate) mod catalog;
mod opening;
mod ordering;
mod validation;

pub(crate) use ordering::order_learning_tracks;
pub(crate) use validation::{learning_resources_are_valid, validate_frozen_learning_plan};

pub(crate) fn build_opening_material(
    game: &Game,
    moment: &MomentFact,
    game_ref: &GameRef,
    opening_identification: &OpeningMetadata,
) -> ReviewMomentLearningMaterial {
    let track = match opening::track(game, moment, game_ref, opening_identification) {
        Ok(track) => track,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "omitting invalid opening learning material"
            );
            None
        }
    };
    ReviewMomentLearningMaterial {
        selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
        resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
        tracks: track.into_iter().collect(),
    }
}
