use std::collections::BTreeMap;

use crate::{
    decision_explanation::{DecisionExplanationBuild, DecisionExplanationDiagnostic},
    engine_analysis::EngineProvenance,
    learning_plan::{order_learning_tracks, validate_frozen_learning_plan},
    review_session_contract::{
        ArtifactDigest, CriticalMomentId, DecisionEngineProvenance, DecisionExplanation,
        DecisionLearningAbstentionReason, DecisionLearningOutcome, DecisionLearningTrackProjection,
        GameRef, GameReviewCriticalMoment, GameReviewMomentClassification, LearningPathRef,
        LearningPlan, LearningTrack, LearningTrackKey, LearningTrackPurpose, LearningTrackSupport,
        LearningTrackSupportBasis, ReviewMomentLearningMaterial,
        LEARNING_PLAN_SELECTION_POLICY_VERSION, LEARNING_RESOURCE_CATALOG_VERSION,
    },
};

pub(crate) fn decision_provenance(
    provenance: &EngineProvenance,
) -> Option<DecisionEngineProvenance> {
    Some(DecisionEngineProvenance {
        engine: provenance.version.clone(),
        binary_digest: ArtifactDigest::try_from(format!("sha256:{}", provenance.binary_sha256))
            .ok()?,
        depth: provenance.depth,
        threads: provenance.threads,
        hash_mib: provenance.hash_mib,
    })
}

pub(crate) fn learning_purpose(
    classification: &GameReviewMomentClassification,
) -> Option<LearningTrackPurpose> {
    match classification {
        GameReviewMomentClassification::ImprovementOpportunity { .. } => {
            Some(LearningTrackPurpose::Improvement)
        }
        GameReviewMomentClassification::PositiveHighlight { .. } => {
            Some(LearningTrackPurpose::Reinforcement)
        }
        GameReviewMomentClassification::Neutral { .. } => None,
    }
}

pub(crate) fn projected_tracks(
    game_ref: &GameRef,
    moment: &GameReviewCriticalMoment,
    projected: &[DecisionLearningTrackProjection],
) -> Vec<LearningTrack> {
    let Some(purpose) = learning_purpose(&moment.classification) else {
        return Vec::new();
    };
    projected
        .iter()
        .map(|projection| decision_track(game_ref, moment, purpose, projection))
        .collect()
}

pub(crate) enum DecisionLearningBuild {
    TrackSelected {
        explanation: Box<DecisionExplanation>,
        projections: Vec<DecisionLearningTrackProjection>,
    },
    ExplanationUnmapped {
        explanation: Box<DecisionExplanation>,
    },
    Abstained {
        reason: DecisionLearningAbstentionReason,
    },
}

pub(crate) fn decision_learning_build(
    build: DecisionExplanationBuild,
) -> Result<DecisionLearningBuild, &'static str> {
    match build {
        DecisionExplanationBuild::Durable {
            explanation,
            projected_tracks,
            diagnostics,
        } if !projected_tracks.is_empty() && diagnostics.is_empty() => {
            Ok(DecisionLearningBuild::TrackSelected {
                explanation,
                projections: projected_tracks,
            })
        }
        DecisionExplanationBuild::Durable {
            explanation,
            projected_tracks,
            diagnostics,
        } if projected_tracks.is_empty()
            && diagnostics.as_slice()
                == [DecisionExplanationDiagnostic::ResourceMappingUnavailable] =>
        {
            Ok(DecisionLearningBuild::ExplanationUnmapped { explanation })
        }
        DecisionExplanationBuild::Abstained { diagnostics }
            if diagnostics.as_slice()
                == [DecisionExplanationDiagnostic::CandidateEvidenceRejected] =>
        {
            Ok(DecisionLearningBuild::Abstained {
                reason: DecisionLearningAbstentionReason::CandidateEvidenceRejected,
            })
        }
        DecisionExplanationBuild::Abstained { diagnostics }
            if diagnostics.as_slice() == [DecisionExplanationDiagnostic::NoProofValidConcept] =>
        {
            Ok(DecisionLearningBuild::Abstained {
                reason: DecisionLearningAbstentionReason::NoProofValidConcept,
            })
        }
        DecisionExplanationBuild::Durable { .. } | DecisionExplanationBuild::Abstained { .. } => {
            Err("Decision Explanation projections and diagnostics disagree")
        }
    }
}

/// Atomically applies one complete chess-concept learning result and the
/// independently selected Opening material to their owning moment.
pub(crate) fn apply_decision_learning(
    game_ref: &GameRef,
    moment: &mut GameReviewCriticalMoment,
    build: DecisionLearningBuild,
    opening_material: &ReviewMomentLearningMaterial,
) -> Result<(), &'static str> {
    let (explanation, outcome, projected) = match build {
        DecisionLearningBuild::TrackSelected {
            explanation,
            projections,
        } => (
            Some(*explanation),
            DecisionLearningOutcome::TrackSelected,
            projected_tracks(game_ref, moment, &projections),
        ),
        DecisionLearningBuild::ExplanationUnmapped { explanation } => (
            Some(*explanation),
            DecisionLearningOutcome::ExplanationUnmapped,
            Vec::new(),
        ),
        DecisionLearningBuild::Abstained { reason } => (
            None,
            DecisionLearningOutcome::Abstained { reason },
            Vec::new(),
        ),
    };
    let learning_material = merge_with_opening(projected, opening_material)?;
    moment.set_decision_explanation(explanation);
    moment.decision_learning_outcome = outcome;
    moment.learning_material = learning_material;
    Ok(())
}

pub(crate) fn merge_with_opening(
    mut projected: Vec<LearningTrack>,
    opening_material: &ReviewMomentLearningMaterial,
) -> Result<ReviewMomentLearningMaterial, &'static str> {
    if opening_material.tracks.len() > 2 {
        return Err("persisted Review Moment Learning Material exceeds the two-track contract");
    }
    let opening = opening_material
        .tracks
        .iter()
        .filter(|track| matches!(track.key, LearningTrackKey::Opening { .. }))
        .cloned()
        .collect::<Vec<_>>();
    if !opening.is_empty()
        && (opening_material.selection_policy_version != LEARNING_PLAN_SELECTION_POLICY_VERSION
            || opening_material.resource_catalog_version != LEARNING_RESOURCE_CATALOG_VERSION)
    {
        return Err("persisted opening Learning Tracks must use the active learning contract");
    }
    projected.truncate(2 - opening.len());
    projected.extend(opening);
    Ok(ReviewMomentLearningMaterial {
        selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
        resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
        tracks: projected,
    })
}

pub(crate) fn automatic_learning_plan(
    moments: &[GameReviewCriticalMoment],
) -> Result<LearningPlan, &'static str> {
    let mut tracks = BTreeMap::<LearningTrackKey, LearningTrack>::new();
    for moment in moments {
        validate_moment_learning_material(moment)?;
    }
    for track in moments
        .iter()
        .flat_map(|moment| moment.learning_material.tracks.iter())
    {
        match tracks.get_mut(&track.key) {
            Some(existing) => {
                if existing.resources != track.resources {
                    return Err("automatic Learning Track resources disagree for the same key");
                }
                existing.support.extend(track.support.iter().cloned());
            }
            None => {
                tracks.insert(track.key.clone(), track.clone());
            }
        }
    }
    for track in tracks.values_mut() {
        track
            .support
            .sort_by(|left, right| support_order_key(left).cmp(&support_order_key(right)));
        track.support.dedup();
    }
    let tracks = order_learning_tracks(tracks.into_values().collect())?;
    let plan = LearningPlan {
        selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
        resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
        tracks,
    };
    let played_plies = moments
        .iter()
        .map(|moment| u32::from(moment.ply))
        .max()
        .unwrap_or_default();
    validate_frozen_learning_plan(&plan, played_plies)?;
    Ok(plan)
}

pub(crate) fn validate_moment_learning_material(
    moment: &GameReviewCriticalMoment,
) -> Result<(), &'static str> {
    let material = &moment.learning_material;
    if material.selection_policy_version != LEARNING_PLAN_SELECTION_POLICY_VERSION
        || material.resource_catalog_version != LEARNING_RESOURCE_CATALOG_VERSION
        || material.tracks.len() > 2
    {
        return Err("Review Moment Learning Material must use the active bounded contract");
    }
    let curriculum_track_count = material
        .tracks
        .iter()
        .filter(|track| matches!(track.key, LearningTrackKey::Curriculum { .. }))
        .count();
    let explanation_ref = moment
        .decision_explanation
        .as_ref()
        .map(|explanation| &explanation.decision_explanation_ref);
    if explanation_ref.is_some() && explanation_ref != moment.decision_explanation_ref.as_ref() {
        return Err("a Decision Explanation and its reference must identify the same proof");
    }
    match moment.decision_learning_outcome {
        DecisionLearningOutcome::NotAttempted
            if moment.provenance
                == crate::review_session_contract::GameReviewMomentProvenance::PlayerSelected
                && moment.decision_explanation_ref.is_none()
                && curriculum_track_count == 0 => {}
        DecisionLearningOutcome::TrackSelected
            if moment.decision_explanation_ref.is_some() && curriculum_track_count > 0 => {}
        DecisionLearningOutcome::ExplanationUnmapped
            if moment.decision_explanation_ref.is_some() && curriculum_track_count == 0 => {}
        DecisionLearningOutcome::Abstained { .. }
            if moment.decision_explanation_ref.is_none()
                && moment.decision_explanation.is_none()
                && curriculum_track_count == 0 => {}
        DecisionLearningOutcome::NotAttempted
        | DecisionLearningOutcome::TrackSelected
        | DecisionLearningOutcome::ExplanationUnmapped
        | DecisionLearningOutcome::Abstained { .. } => {
            return Err(
                "Decision Learning outcome, proof reference, and curriculum tracks disagree",
            );
        }
    }
    let expected_purpose = learning_purpose(&moment.classification);
    for track in &material.tracks {
        let [support] = track.support.as_slice() else {
            return Err("a moment-local Learning Track must have exactly one support");
        };
        if track.resources.is_empty() {
            return Err("a selected Learning Track must have at least one resource");
        }
        let (purpose, critical_moment_id, ply, basis) = match support {
            LearningTrackSupport::Improvement {
                critical_moment_id,
                ply,
                basis,
                ..
            } => (
                LearningTrackPurpose::Improvement,
                critical_moment_id,
                ply,
                basis,
            ),
            LearningTrackSupport::Reinforcement {
                critical_moment_id,
                ply,
                basis,
                ..
            } => (
                LearningTrackPurpose::Reinforcement,
                critical_moment_id,
                ply,
                basis,
            ),
        };
        if expected_purpose != Some(purpose)
            || critical_moment_id != &moment.critical_moment_id
            || *ply != moment.ply
        {
            return Err("Learning Track support must identify its owning Review Moment");
        }
        match (&track.key, basis) {
            (
                LearningTrackKey::Curriculum { .. },
                LearningTrackSupportBasis::DecisionExplanation {
                    explanation_path_ref,
                },
            ) => {
                let Some(explanation) = &moment.decision_explanation else {
                    return Err(
                        "Curriculum Learning Track support requires a Decision Explanation",
                    );
                };
                if explanation.critical_moment_id != moment.critical_moment_id
                    || !explanation
                        .selected_paths
                        .iter()
                        .any(|path| path.path_ref == *explanation_path_ref)
                {
                    return Err(
                        "Curriculum Learning Track support must reference a selected explanation path",
                    );
                }
            }
            (
                LearningTrackKey::Opening {
                    resource_mapping_id,
                },
                LearningTrackSupportBasis::Opening { evidence },
            ) if evidence.resource_mapping_id == *resource_mapping_id => {}
            _ => return Err("Learning Track support basis must match its track key"),
        }
    }
    Ok(())
}

fn decision_track(
    game_ref: &GameRef,
    moment: &GameReviewCriticalMoment,
    purpose: LearningTrackPurpose,
    projection: &DecisionLearningTrackProjection,
) -> LearningTrack {
    let basis = LearningTrackSupportBasis::DecisionExplanation {
        explanation_path_ref: projection.explanation_path_ref.clone(),
    };
    let path_ref = LearningPathRef::for_selected_support(
        game_ref,
        &moment.critical_moment_id,
        &projection.key,
        purpose,
        &basis,
        LEARNING_PLAN_SELECTION_POLICY_VERSION,
        LEARNING_RESOURCE_CATALOG_VERSION,
    );
    let support = match purpose {
        LearningTrackPurpose::Improvement => LearningTrackSupport::Improvement {
            learning_path_ref: path_ref,
            critical_moment_id: moment.critical_moment_id.clone(),
            ply: moment.ply,
            basis,
        },
        LearningTrackPurpose::Reinforcement => LearningTrackSupport::Reinforcement {
            learning_path_ref: path_ref,
            critical_moment_id: moment.critical_moment_id.clone(),
            ply: moment.ply,
            basis,
        },
    };
    LearningTrack {
        key: projection.key.clone(),
        support: vec![support],
        resources: projection.resources.clone(),
    }
}

fn support_order_key(
    support: &LearningTrackSupport,
) -> (
    u16,
    LearningTrackPurpose,
    &CriticalMomentId,
    &LearningPathRef,
) {
    match support {
        LearningTrackSupport::Improvement {
            ply,
            critical_moment_id,
            learning_path_ref,
            ..
        } => (
            *ply,
            LearningTrackPurpose::Improvement,
            critical_moment_id,
            learning_path_ref,
        ),
        LearningTrackSupport::Reinforcement {
            ply,
            critical_moment_id,
            learning_path_ref,
            ..
        } => (
            *ply,
            LearningTrackPurpose::Reinforcement,
            critical_moment_id,
            learning_path_ref,
        ),
    }
}

#[cfg(test)]
#[path = "decision_learning/tests.rs"]
mod tests;
