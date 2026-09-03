use std::sync::OnceLock;

use serde_json::Value;

use super::{automatic_learning_plan, merge_with_opening, validate_moment_learning_material};
use crate::{
    learning_plan::catalog::resources_for,
    review_session_contract::{
        Color, CriticalMomentId, CurriculumLearningConcept, DecisionExplanation,
        DecisionLearningAbstentionReason, DecisionLearningOutcome, EngineEvaluation,
        GameReviewCriticalMoment, GameReviewMomentClassification, ImprovementCorrection,
        ImprovementOutcome, LearningPathRef, LearningPlan, LearningResourceCatalogVersion,
        LearningResourceMappingId, LearningTrack, LearningTrackKey, LearningTrackPurpose,
        LearningTrackSupport, LearningTrackSupportBasis, OpeningIdentificationProvenance,
        OpeningLearningEvidence, OpeningMetadata, OpeningServiceAttribution,
        OpeningServiceProvider, PositionPhase, PositionPhaseKind, PositionPhasePolicyVersion,
        ReviewMomentLearningMaterial, LEARNING_PLAN_SELECTION_POLICY_VERSION,
        LEARNING_RESOURCE_CATALOG_VERSION,
    },
};

fn fixture_moment() -> GameReviewCriticalMoment {
    static MOMENT: OnceLock<GameReviewCriticalMoment> = OnceLock::new();
    MOMENT
        .get_or_init(|| {
            let events: Value = serde_json::from_str(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../packages/coach-engine-sdk/fixtures/events.json"
            )))
            .expect("the generated events fixture should be valid JSON");
            serde_json::from_value(
                events
                    .pointer("/2/event/result/review/criticalMoments/0")
                    .expect("the fixture should contain a Review Moment")
                    .clone(),
            )
            .expect("the generated Review Moment fixture should match the contract")
        })
        .clone()
}

fn fixture_explanation() -> DecisionExplanation {
    static EXPLANATION: OnceLock<DecisionExplanation> = OnceLock::new();
    EXPLANATION
        .get_or_init(|| {
            let events: Value = serde_json::from_str(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../packages/coach-engine-sdk/fixtures/events.json"
            )))
            .expect("the generated events fixture should be valid JSON");
            serde_json::from_value(
                events
                    .pointer("/11/event/result/explanation")
                    .expect("the fixture should contain a Decision Explanation")
                    .clone(),
            )
            .expect("the generated Decision Explanation fixture should match the contract")
        })
        .clone()
}

fn curriculum_moment(
    ply: u16,
    concept: CurriculumLearningConcept,
    purpose: LearningTrackPurpose,
) -> GameReviewCriticalMoment {
    curriculum_moment_named(ply, &ply.to_string(), concept, purpose)
}

fn curriculum_moment_named(
    ply: u16,
    label: &str,
    concept: CurriculumLearningConcept,
    purpose: LearningTrackPurpose,
) -> GameReviewCriticalMoment {
    let mut moment = fixture_moment();
    moment.critical_moment_id =
        CriticalMomentId::try_from(format!("review-moment:learning-plan:{label}"))
            .expect("the test Review Moment ID should be valid");
    moment.ply = ply;
    moment.move_number = ply.div_ceil(2);
    moment.classification = match purpose {
        LearningTrackPurpose::Improvement => {
            GameReviewMomentClassification::ImprovementOpportunity {
                correction: ImprovementCorrection {
                    better_move_uci: "e2e4".to_string(),
                    better_move_san: "e4".to_string(),
                    outcome: ImprovementOutcome::ImprovedAnalyzed {
                        better_evaluation: EngineEvaluation::Centipawns {
                            value: 100,
                            perspective: Color::White,
                        },
                    },
                },
            }
        }
        LearningTrackPurpose::Reinforcement => moment.classification.clone(),
    };

    let mut explanation = fixture_explanation();
    explanation.critical_moment_id = moment.critical_moment_id.clone();
    let explanation_path_ref = explanation.selected_paths[0].path_ref.clone();
    moment.set_decision_explanation(Some(explanation));

    let key = LearningTrackKey::Curriculum { concept };
    let basis = LearningTrackSupportBasis::DecisionExplanation {
        explanation_path_ref,
    };
    let support = match purpose {
        LearningTrackPurpose::Improvement => LearningTrackSupport::Improvement {
            learning_path_ref: learning_path_ref(ply, label),
            critical_moment_id: moment.critical_moment_id.clone(),
            ply,
            basis,
        },
        LearningTrackPurpose::Reinforcement => LearningTrackSupport::Reinforcement {
            learning_path_ref: learning_path_ref(ply, label),
            critical_moment_id: moment.critical_moment_id.clone(),
            ply,
            basis,
        },
    };
    moment.learning_material = ReviewMomentLearningMaterial {
        selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
        resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
        tracks: vec![LearningTrack {
            resources: resources_for(&key)
                .expect("every curriculum concept in the policy fixture should be mapped"),
            key,
            support: vec![support],
        }],
    };
    moment.decision_learning_outcome = DecisionLearningOutcome::TrackSelected;
    moment
}

fn opening_moment(ply: u16) -> GameReviewCriticalMoment {
    let mut moment = curriculum_moment(
        ply,
        CurriculumLearningConcept::Pin,
        LearningTrackPurpose::Improvement,
    );
    moment.position_phase = PositionPhase {
        policy_version: PositionPhasePolicyVersion::V1,
        phase: PositionPhaseKind::Opening,
    };
    moment.set_decision_explanation(None);
    moment.decision_learning_outcome = DecisionLearningOutcome::Abstained {
        reason: DecisionLearningAbstentionReason::NoProofValidConcept,
    };
    moment.learning_material.tracks = vec![opening_track(&moment)];
    moment
}

fn opening_track(moment: &GameReviewCriticalMoment) -> LearningTrack {
    let resource_mapping_id = LearningResourceMappingId::try_from(
        "lichess:opening:sicilian-defense-najdorf-variation".to_string(),
    )
    .expect("the cataloged opening mapping ID should be valid");
    let key = LearningTrackKey::Opening {
        resource_mapping_id: resource_mapping_id.clone(),
    };
    let basis = LearningTrackSupportBasis::Opening {
        evidence: OpeningLearningEvidence {
            position_phase: moment.position_phase,
            opening_identification: OpeningMetadata::Present {
                eco: "B90".to_string(),
                name: "Sicilian Defense: Najdorf Variation".to_string(),
                provenance: OpeningIdentificationProvenance::Service {
                    provider: OpeningServiceProvider::Lichess,
                    attribution: OpeningServiceAttribution::DirectImport,
                },
            },
            resource_mapping_id,
        },
    };
    LearningTrack {
        resources: resources_for(&key).expect("the cataloged opening should have resources"),
        key,
        support: vec![LearningTrackSupport::Improvement {
            learning_path_ref: learning_path_ref(moment.ply, "opening"),
            critical_moment_id: moment.critical_moment_id.clone(),
            ply: moment.ply,
            basis,
        }],
    }
}

fn learning_path_ref(ply: u16, family: &str) -> LearningPathRef {
    LearningPathRef::try_from(format!("learning-path:test:{family}:{ply}"))
        .expect("the test Learning Path reference should be valid")
}

fn plan(moments: &[GameReviewCriticalMoment]) -> LearningPlan {
    automatic_learning_plan(moments).expect("the policy fixture should produce a valid plan")
}

fn curriculum_key(concept: CurriculumLearningConcept) -> LearningTrackKey {
    LearningTrackKey::Curriculum { concept }
}

fn keys(plan: &LearningPlan) -> Vec<LearningTrackKey> {
    plan.tracks.iter().map(|track| track.key.clone()).collect()
}

#[test]
fn automatic_plan_ranks_higher_support_count_first() {
    let moments = vec![
        curriculum_moment(
            1,
            CurriculumLearningConcept::Pin,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            2,
            CurriculumLearningConcept::Fork,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            3,
            CurriculumLearningConcept::Fork,
            LearningTrackPurpose::Reinforcement,
        ),
    ];

    let actual = keys(&plan(&moments));

    assert_eq!(
        actual,
        vec![
            curriculum_key(CurriculumLearningConcept::Fork),
            curriculum_key(CurriculumLearningConcept::Pin),
        ]
    );
}

#[test]
fn automatic_plan_prefers_improvement_when_support_counts_tie() {
    let moments = vec![
        curriculum_moment(
            1,
            CurriculumLearningConcept::Pin,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            2,
            CurriculumLearningConcept::Fork,
            LearningTrackPurpose::Improvement,
        ),
    ];

    let actual = keys(&plan(&moments));

    assert_eq!(
        actual,
        vec![
            curriculum_key(CurriculumLearningConcept::Fork),
            curriculum_key(CurriculumLearningConcept::Pin),
        ]
    );
}

#[test]
fn automatic_plan_uses_curriculum_enum_order_as_the_final_tie_break() {
    let moments = vec![
        curriculum_moment(
            2,
            CurriculumLearningConcept::Fork,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            1,
            CurriculumLearningConcept::Skewer,
            LearningTrackPurpose::Reinforcement,
        ),
    ];

    let actual = keys(&plan(&moments));

    assert_eq!(
        actual,
        vec![
            curriculum_key(CurriculumLearningConcept::Skewer),
            curriculum_key(CurriculumLearningConcept::Fork),
        ]
    );
}

#[test]
fn automatic_plan_places_prerequisites_first_and_is_stable_across_input_permutations() {
    let moments = vec![
        curriculum_moment(
            1,
            CurriculumLearningConcept::CheckmatePatterns,
            LearningTrackPurpose::Improvement,
        ),
        curriculum_moment(
            2,
            CurriculumLearningConcept::CheckmatePatterns,
            LearningTrackPurpose::Improvement,
        ),
        curriculum_moment(
            3,
            CurriculumLearningConcept::Checkmate,
            LearningTrackPurpose::Reinforcement,
        ),
    ];
    let mut permuted = moments.clone();
    permuted.reverse();

    let ordered = plan(&moments);
    let reordered = plan(&permuted);

    assert_eq!(
        keys(&ordered),
        vec![
            curriculum_key(CurriculumLearningConcept::Checkmate),
            curriculum_key(CurriculumLearningConcept::CheckmatePatterns),
        ]
    );
    assert_eq!(
        serde_json::to_vec(&ordered).expect("the Learning Plan should serialize"),
        serde_json::to_vec(&reordered).expect("the permuted Learning Plan should serialize")
    );
}

#[test]
fn automatic_plan_promotes_a_prerequisite_without_demoting_its_ranked_dependent() {
    let moments = vec![
        curriculum_moment(
            1,
            CurriculumLearningConcept::PassiveRookDefense,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            2,
            CurriculumLearningConcept::PassiveRookDefense,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            3,
            CurriculumLearningConcept::PassiveRookDefense,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            4,
            CurriculumLearningConcept::Pin,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            5,
            CurriculumLearningConcept::Pin,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            6,
            CurriculumLearningConcept::RookEndgame,
            LearningTrackPurpose::Reinforcement,
        ),
    ];

    let actual = keys(&plan(&moments));

    assert_eq!(
        actual,
        vec![
            curriculum_key(CurriculumLearningConcept::RookEndgame),
            curriculum_key(CurriculumLearningConcept::PassiveRookDefense),
            curriculum_key(CurriculumLearningConcept::Pin),
        ]
    );
}

#[test]
fn automatic_plan_is_stable_when_distinct_supports_share_a_ply() {
    let moments = vec![
        curriculum_moment_named(
            1,
            "same_ply_a",
            CurriculumLearningConcept::Pin,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment_named(
            1,
            "same_ply_b",
            CurriculumLearningConcept::Pin,
            LearningTrackPurpose::Reinforcement,
        ),
    ];
    let mut permuted = moments.clone();
    permuted.reverse();

    let ordered = plan(&moments);
    let reordered = plan(&permuted);

    assert_eq!(
        serde_json::to_vec(&ordered).expect("the Learning Plan should serialize"),
        serde_json::to_vec(&reordered).expect("the permuted Learning Plan should serialize")
    );
}

#[test]
fn automatic_plan_counts_refining_support_and_clusters_it_after_the_nearest_present_ancestor() {
    let moments = vec![
        curriculum_moment(
            1,
            CurriculumLearningConcept::DiscoveredAttack,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            2,
            CurriculumLearningConcept::DiscoveredCheck,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            3,
            CurriculumLearningConcept::DiscoveredCheck,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            4,
            CurriculumLearningConcept::Pin,
            LearningTrackPurpose::Reinforcement,
        ),
        curriculum_moment(
            5,
            CurriculumLearningConcept::Pin,
            LearningTrackPurpose::Reinforcement,
        ),
    ];

    let actual = keys(&plan(&moments));

    assert_eq!(
        actual,
        vec![
            curriculum_key(CurriculumLearningConcept::DiscoveredAttack),
            curriculum_key(CurriculumLearningConcept::DiscoveredCheck),
            curriculum_key(CurriculumLearningConcept::Pin),
        ]
    );
}

#[test]
fn automatic_plan_reorders_whole_tracks_without_changing_projected_content() {
    let moments = vec![
        curriculum_moment(
            1,
            CurriculumLearningConcept::CheckmatePatterns,
            LearningTrackPurpose::Improvement,
        ),
        curriculum_moment(
            2,
            CurriculumLearningConcept::CheckmatePatterns,
            LearningTrackPurpose::Improvement,
        ),
        curriculum_moment(
            3,
            CurriculumLearningConcept::Checkmate,
            LearningTrackPurpose::Reinforcement,
        ),
    ];
    let original_moments = moments.clone();
    let expected = vec![
        moments[2].learning_material.tracks[0].clone(),
        LearningTrack {
            key: moments[0].learning_material.tracks[0].key.clone(),
            support: vec![
                moments[0].learning_material.tracks[0].support[0].clone(),
                moments[1].learning_material.tracks[0].support[0].clone(),
            ],
            resources: moments[0].learning_material.tracks[0].resources.clone(),
        },
    ];

    let actual = plan(&moments);

    assert_eq!(moments, original_moments);
    assert_eq!(actual.tracks, expected);
}

#[test]
fn automatic_plan_includes_opening_tracks() {
    let moment = opening_moment(1);
    let expected = moment.learning_material.tracks.clone();

    let actual = plan(&[moment]);

    assert_eq!(actual.tracks, expected);
}

#[test]
fn merge_with_opening_reserves_a_slot_when_two_curriculum_tracks_are_projected() {
    let first = curriculum_moment(
        1,
        CurriculumLearningConcept::Pin,
        LearningTrackPurpose::Improvement,
    )
    .learning_material
    .tracks
    .remove(0);
    let second = curriculum_moment(
        1,
        CurriculumLearningConcept::Fork,
        LearningTrackPurpose::Improvement,
    )
    .learning_material
    .tracks
    .remove(0);
    let opening = opening_moment(1).learning_material;
    let expected = vec![first.clone(), opening.tracks[0].clone()];

    let actual = merge_with_opening(vec![first, second], &opening)
        .expect("current opening material should merge")
        .tracks;

    assert_eq!(actual, expected);
}

#[test]
fn merge_with_opening_rejects_a_stale_retained_catalog() {
    let mut opening = opening_moment(1).learning_material;
    opening.resource_catalog_version = LearningResourceCatalogVersion::V2026_07_25;

    let error = merge_with_opening(Vec::new(), &opening)
        .expect_err("retained opening tracks must not be relabeled under the active catalog");

    assert_eq!(
        error,
        "persisted opening Learning Tracks must use the active learning contract"
    );
}

#[test]
fn merge_with_opening_rejects_unbounded_persisted_material() {
    let mut opening = opening_moment(1).learning_material;
    opening.tracks = vec![opening.tracks[0].clone(); 3];

    let error = merge_with_opening(Vec::new(), &opening)
        .expect_err("persisted cardinality must be checked before opening tracks are truncated");

    assert_eq!(
        error,
        "persisted Review Moment Learning Material exceeds the two-track contract"
    );
}

#[test]
fn automatic_moments_cannot_persist_an_unattempted_learning_outcome() {
    let mut moment = fixture_moment();
    moment.decision_learning_outcome = DecisionLearningOutcome::NotAttempted;

    let error = validate_moment_learning_material(&moment)
        .expect_err("automatic learning must finish with an explicit result");

    assert_eq!(
        error,
        "Decision Learning outcome, proof reference, and curriculum tracks disagree"
    );
}

#[test]
fn an_abstained_decision_can_keep_independent_opening_material() {
    let moment = opening_moment(1);

    validate_moment_learning_material(&moment)
        .expect("opening material must not masquerade as a selected chess concept");
}

#[test]
fn a_selected_outcome_requires_a_curriculum_track() {
    let mut moment = curriculum_moment(
        1,
        CurriculumLearningConcept::Pin,
        LearningTrackPurpose::Improvement,
    );
    moment.learning_material.tracks.clear();

    let error = validate_moment_learning_material(&moment)
        .expect_err("a proof reference alone is not a selected Learning Track");

    assert_eq!(
        error,
        "Decision Learning outcome, proof reference, and curriculum tracks disagree"
    );
}
