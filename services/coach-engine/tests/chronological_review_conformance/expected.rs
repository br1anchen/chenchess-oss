use std::{fs, path::Path};

use chen_chess_coach_engine::review_session_contract::*;
use serde_json::{json, Value};

pub(crate) fn contract_projection(
    review: &GameReview,
    cores: &[ReviewSessionCoreContract],
) -> Value {
    assert_eq!(
        review.critical_moments.len(),
        cores.len(),
        "each admitted surface must prepare exactly one core per Critical Moment"
    );
    json!({
        "reviewMoments": review
            .critical_moments
            .iter()
            .zip(cores)
            .map(|(moment, _core)| json!({
                "criticalMomentId": moment.critical_moment_id,
                "ply": moment.ply,
                "positionPhase": moment.position_phase,
                "classification": moment.classification,
                "playedMoveOutcome": moment.played_move_outcome,
                "category": moment.category,
                "provenance": moment.provenance,
                "decisionExplanationRef": moment
                    .decision_explanation
                    .as_ref()
                    .map(|explanation| &explanation.decision_explanation_ref),
                "learningMaterial": moment.learning_material,
            }))
            .collect::<Vec<_>>(),
        "learningPlan": review.learning_plan,
    })
}

pub(crate) fn assert_canonical_contract(review: &GameReview, cores: &[ReviewSessionCoreContract]) {
    let baseline: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages/shared-assets/fixtures/Synthet1/provider-recordings/full-game.baseline.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let expected = baseline["facts"]["criticalMoments"]
        .as_array()
        .expect("the accepted baseline has ordered Critical Moments");

    assert_eq!(review.critical_moments.len(), 7);
    assert_eq!(review.critical_moments.len(), cores.len());
    assert_eq!(
        review
            .critical_moments
            .iter()
            .map(|moment| moment.ply)
            .collect::<Vec<_>>(),
        vec![24, 54, 62, 66, 72, 80, 88]
    );

    for ((moment, core), accepted) in review.critical_moments.iter().zip(cores).zip(expected) {
        assert_eq!(json!(moment.ply), accepted["ply"]);
        assert_eq!(
            moment.position_phase.policy_version,
            PositionPhasePolicyVersion::V1
        );
        assert_eq!(json!(moment.classification), accepted["classification"]);
        assert_eq!(json!(moment.category), accepted["category"]);
        assert_eq!(
            json!(moment.played_move_outcome),
            expected_outcome(accepted)
        );
        assert_eq!(json!(moment.provenance), json!("automatic"));
        assert_eq!(
            moment.learning_material.selection_policy_version,
            LearningPlanSelectionPolicyVersion::V1
        );
        assert_eq!(
            moment.learning_material.resource_catalog_version,
            LearningResourceCatalogVersion::V2026_08_03
        );
        if moment.ply == 24 {
            assert_anchor_decision_track(moment);
        }
        assert_eq!(
            moment.critical_moment_id,
            CriticalMomentId::for_imported_game(&core.imported_game.game.game_ref, moment.ply,)
        );
        assert_eq!(core.review_moment.moment_id, moment.critical_moment_id);
        assert!(matches!(
            core.review_moment.selection,
            ReviewMomentSelection::PipelineCriticalMoment { .. }
        ));
        let position = review
            .position_views
            .iter()
            .find(|view| view.critical_moment_id == moment.critical_moment_id)
            .expect("every canonical Review Moment has a Position Snapshot");
        assert_eq!(position.position_snapshot, core.position_snapshot);
    }

    assert_eq!(
        review.critical_moments[0].position_phase.phase,
        PositionPhaseKind::Opening
    );
    assert_eq!(
        review
            .critical_moments
            .iter()
            .map(|moment| (
                moment.ply,
                moment
                    .learning_material
                    .tracks
                    .iter()
                    .map(|track| track.key.clone())
                    .collect::<Vec<_>>(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                24,
                vec![curriculum_key(CurriculumLearningConcept::Clearance)]
            ),
            (
                54,
                vec![curriculum_key(CurriculumLearningConcept::Advantage)]
            ),
            (
                62,
                vec![curriculum_key(CurriculumLearningConcept::Advantage)]
            ),
            (
                66,
                vec![curriculum_key(CurriculumLearningConcept::HangingPiece)],
            ),
            (
                72,
                vec![curriculum_key(CurriculumLearningConcept::HangingPiece)],
            ),
            (80, vec![curriculum_key(CurriculumLearningConcept::Pin)]),
            (
                88,
                vec![curriculum_key(CurriculumLearningConcept::ExposedKing)],
            ),
        ]
    );
    assert!(review
        .critical_moments
        .iter()
        .all(|moment| moment.learning_material.tracks.len() <= 2));
    assert!(review
        .learning_plan
        .tracks
        .iter()
        .flat_map(|track| &track.support)
        .any(|support| matches!(support, LearningTrackSupport::Improvement { .. })));
    assert!(review
        .learning_plan
        .tracks
        .iter()
        .flat_map(|track| &track.support)
        .any(|support| matches!(support, LearningTrackSupport::Reinforcement { .. })));
    assert_eq!(
        review
            .critical_moments
            .iter()
            .find(|moment| moment.ply == 54)
            .unwrap()
            .position_phase
            .phase,
        PositionPhaseKind::Middlegame
    );

    assert_eq!(
        review.learning_plan.selection_policy_version,
        LearningPlanSelectionPolicyVersion::V1
    );
    assert_eq!(
        review.learning_plan.resource_catalog_version,
        LearningResourceCatalogVersion::V2026_08_03
    );
    assert_eq!(
        review
            .learning_plan
            .tracks
            .iter()
            .map(|track| (track.key.clone(), track.support.len()))
            .collect::<Vec<_>>(),
        vec![
            (curriculum_key(CurriculumLearningConcept::Advantage), 2),
            (curriculum_key(CurriculumLearningConcept::HangingPiece), 2),
            (curriculum_key(CurriculumLearningConcept::Clearance), 1),
            (curriculum_key(CurriculumLearningConcept::Pin), 1),
            (curriculum_key(CurriculumLearningConcept::ExposedKing), 1),
        ]
    );
}

fn curriculum_key(concept: CurriculumLearningConcept) -> LearningTrackKey {
    LearningTrackKey::Curriculum { concept }
}

fn assert_anchor_decision_track(moment: &GameReviewCriticalMoment) {
    let explanation = moment
        .decision_explanation
        .as_ref()
        .expect("12...Ra7 should retain its Automatic Decision Explanation");
    let [track] = moment.learning_material.tracks.as_slice() else {
        panic!("12...Ra7 should produce exactly one Decision-derived Learning Track");
    };
    assert_eq!(
        track.key,
        curriculum_key(CurriculumLearningConcept::Clearance)
    );
    let [LearningTrackSupport::Improvement {
        ply,
        basis:
            LearningTrackSupportBasis::DecisionExplanation {
                explanation_path_ref,
            },
        ..
    }] = track.support.as_slice()
    else {
        panic!("the track should carry one Decision Explanation support");
    };
    assert_eq!(*ply, 24);
    assert!(explanation
        .selected_paths
        .iter()
        .any(|path| path.path_ref == *explanation_path_ref));
    assert!(!track.resources.is_empty());
}

fn expected_outcome(accepted: &Value) -> Value {
    let evaluation = &accepted["objective"]["playedEvaluation"];
    let played_evaluation = match evaluation["kind"].as_str().unwrap() {
        "centipawns" => json!({
            "kind": "centipawns",
            "value": evaluation["value"],
            "perspective": "black",
        }),
        "mateIn" => json!({
            "kind": "mate",
            "outcome": "win",
            "distancePlies": evaluation["value"],
            "perspective": "black",
        }),
        kind => panic!("unsupported accepted evaluation kind {kind}"),
    };
    json!({
        "kind": "analyzed",
        "playedEvaluation": played_evaluation,
        "centipawnLoss": accepted["objective"]["centipawnLoss"],
        "residualOutcome": accepted["residualOutcome"],
    })
}
