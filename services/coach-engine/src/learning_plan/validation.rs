use std::collections::BTreeSet;

use crate::review_session_contract::{
    LearningPathRef, LearningPlan, LearningResource, LearningTrackKey, LearningTrackSupport,
    LearningTrackSupportBasis,
};

use super::ordering::order_learning_tracks;

pub(crate) fn validate_frozen_learning_plan(
    plan: &LearningPlan,
    played_plies: u32,
) -> Result<(), &'static str> {
    let mut track_keys = BTreeSet::new();
    for track in &plan.tracks {
        if !track_keys.insert(&track.key) {
            return Err("frozen Learning Plan tracks must use unique keys");
        }
        if track.support.is_empty() || !learning_resources_are_valid(&track.resources) {
            return Err("frozen Learning Tracks require valid support and resources");
        }
        let mut previous_ply = None;
        let mut learning_path_refs = BTreeSet::new();
        for support in &track.support {
            let (learning_path_ref, ply, basis) = support_parts(support);
            if ply == 0
                || u32::from(ply) > played_plies
                || previous_ply.is_some_and(|previous| previous > ply)
                || !learning_path_refs.insert(learning_path_ref)
                || !support_matches_key(&track.key, basis)
            {
                return Err("frozen Learning Track support is malformed");
            }
            previous_ply = Some(ply);
        }
    }
    let ordered = order_learning_tracks(plan.tracks.clone())?;
    if ordered.as_slice() != plan.tracks.as_slice() {
        return Err("frozen Learning Plan tracks must use canonical recommendation order");
    }
    Ok(())
}

pub(crate) fn learning_resources_are_valid(resources: &[LearningResource]) -> bool {
    let mut resource_ids = BTreeSet::new();
    resources.iter().all(|resource| {
        resource_ids.insert(&resource.resource_id)
            && !resource.title.trim().is_empty()
            && resource.canonical_url.starts_with("https://lichess.org/")
    }) && !resources.is_empty()
}

fn support_parts(
    support: &LearningTrackSupport,
) -> (&LearningPathRef, u16, &LearningTrackSupportBasis) {
    match support {
        LearningTrackSupport::Improvement {
            learning_path_ref,
            ply,
            basis,
            ..
        }
        | LearningTrackSupport::Reinforcement {
            learning_path_ref,
            ply,
            basis,
            ..
        } => (learning_path_ref, *ply, basis),
    }
}

fn support_matches_key(key: &LearningTrackKey, basis: &LearningTrackSupportBasis) -> bool {
    match (key, basis) {
        (
            LearningTrackKey::Curriculum { .. },
            LearningTrackSupportBasis::DecisionExplanation { .. },
        ) => true,
        (
            LearningTrackKey::Opening {
                resource_mapping_id,
            },
            LearningTrackSupportBasis::Opening { evidence },
        ) => evidence.resource_mapping_id == *resource_mapping_id,
        _ => false,
    }
}
