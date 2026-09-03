use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    learning_plan::{
        learning_resources_are_valid, validate_frozen_learning_plan as validate_learning_plan,
    },
    profile_game_feed::ProfileGameWindowEntry,
    review_session_contract::{
        GameImportId, LearningPathRef, LearningPlan, LearningPlanSelectionPolicyVersion,
        LearningResource, LearningResourceCatalogVersion, LearningTrack, LearningTrackKey,
        LearningTrackPurpose, LearningTrackSupport,
    },
};

use super::{CoachingDigestError, FrozenDailyGameReview};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CoachingDigestPriority {
    pub(crate) key: LearningTrackKey,
    pub(crate) resources: Vec<LearningResource>,
    pub(crate) supporting_games: Vec<CoachingDigestPriorityGameSupport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CoachingDigestPriorityGameSupport {
    pub(crate) game_import_id: GameImportId,
    pub(crate) purpose: LearningTrackPurpose,
    pub(crate) learning_path_refs: Vec<LearningPathRef>,
}

pub(super) struct PriorityProjection {
    pub(super) learning_plan_selection_policy_version: LearningPlanSelectionPolicyVersion,
    pub(super) learning_resource_catalog_version: LearningResourceCatalogVersion,
    pub(super) priorities: Vec<CoachingDigestPriority>,
}

struct PrioritySource<'a> {
    game_import_id: &'a GameImportId,
    learning_plan: &'a LearningPlan,
    played_plies: u32,
}

struct PriorityCandidate {
    priority: CoachingDigestPriority,
    improvement_game_count: usize,
}

pub(super) fn project(
    reviewed_games: &[(ProfileGameWindowEntry, FrozenDailyGameReview)],
) -> Result<PriorityProjection, CoachingDigestError> {
    project_sources(reviewed_games.iter().map(|(_, review)| PrioritySource {
        game_import_id: &review.game_import_id,
        learning_plan: &review.learning_plan,
        played_plies: review.played_plies,
    }))
}

fn project_sources<'a>(
    sources: impl IntoIterator<Item = PrioritySource<'a>>,
) -> Result<PriorityProjection, CoachingDigestError> {
    let mut sources = sources.into_iter();
    let first = sources
        .next()
        .ok_or(CoachingDigestError::InvalidAggregate)?;
    let learning_plan_selection_policy_version = first.learning_plan.selection_policy_version;
    let learning_resource_catalog_version = first.learning_plan.resource_catalog_version;
    let mut game_import_ids = BTreeSet::new();
    let mut candidates = BTreeMap::<LearningTrackKey, PriorityCandidate>::new();

    for source in std::iter::once(first).chain(sources) {
        if !game_import_ids.insert(source.game_import_id)
            || source.learning_plan.selection_policy_version
                != learning_plan_selection_policy_version
            || source.learning_plan.resource_catalog_version != learning_resource_catalog_version
        {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        validate_frozen_learning_plan(source.learning_plan, source.played_plies)?;
        for track in &source.learning_plan.tracks {
            let support = priority_game_support(source.game_import_id, track)?;
            let is_improvement = support.purpose == LearningTrackPurpose::Improvement;
            match candidates.get_mut(&track.key) {
                Some(candidate) => {
                    if candidate.priority.resources != track.resources {
                        return Err(CoachingDigestError::InvalidAggregate);
                    }
                    candidate.priority.supporting_games.push(support);
                    candidate.improvement_game_count += usize::from(is_improvement);
                }
                None => {
                    candidates.insert(
                        track.key.clone(),
                        PriorityCandidate {
                            priority: CoachingDigestPriority {
                                key: track.key.clone(),
                                resources: track.resources.clone(),
                                supporting_games: vec![support],
                            },
                            improvement_game_count: usize::from(is_improvement),
                        },
                    );
                }
            }
        }
    }

    let mut ranked = candidates.into_values().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .improvement_game_count
            .cmp(&left.improvement_game_count)
            .then_with(|| {
                right
                    .priority
                    .supporting_games
                    .len()
                    .cmp(&left.priority.supporting_games.len())
            })
            .then_with(|| left.priority.key.cmp(&right.priority.key))
    });

    Ok(PriorityProjection {
        learning_plan_selection_policy_version,
        learning_resource_catalog_version,
        priorities: ranked
            .into_iter()
            .take(2)
            .map(|candidate| candidate.priority)
            .collect(),
    })
}

fn priority_game_support(
    game_import_id: &GameImportId,
    track: &LearningTrack,
) -> Result<CoachingDigestPriorityGameSupport, CoachingDigestError> {
    let mut purpose = LearningTrackPurpose::Reinforcement;
    let mut learning_path_refs = Vec::with_capacity(track.support.len());
    for support in &track.support {
        let (support_purpose, learning_path_ref) = support_parts(support);
        if support_purpose == LearningTrackPurpose::Improvement {
            purpose = LearningTrackPurpose::Improvement;
        }
        learning_path_refs.push(learning_path_ref.clone());
    }
    if learning_path_refs.is_empty() {
        return Err(CoachingDigestError::InvalidAggregate);
    }
    Ok(CoachingDigestPriorityGameSupport {
        game_import_id: game_import_id.clone(),
        purpose,
        learning_path_refs,
    })
}

pub(super) fn validate_frozen_learning_plan(
    plan: &LearningPlan,
    played_plies: u32,
) -> Result<(), CoachingDigestError> {
    validate_learning_plan(plan, played_plies).map_err(|_| CoachingDigestError::InvalidAggregate)
}

pub(super) fn validate_archived(
    priorities: &[CoachingDigestPriority],
    game_import_ids: &[GameImportId],
) -> Result<(), CoachingDigestError> {
    if priorities.len() > 2 {
        return Err(CoachingDigestError::InvalidAggregate);
    }
    let known_games = game_import_ids.iter().collect::<BTreeSet<_>>();
    let mut priority_keys = BTreeSet::new();
    for priority in priorities {
        if !priority_keys.insert(&priority.key)
            || !learning_resources_are_valid(&priority.resources)
            || priority.supporting_games.is_empty()
            || priority.supporting_games.len() > game_import_ids.len()
        {
            return Err(CoachingDigestError::InvalidAggregate);
        }
        let mut supporting_games = BTreeSet::new();
        for support in &priority.supporting_games {
            let mut learning_path_refs = BTreeSet::new();
            if !known_games.contains(&support.game_import_id)
                || !supporting_games.insert(&support.game_import_id)
                || support.learning_path_refs.is_empty()
                || support
                    .learning_path_refs
                    .iter()
                    .any(|path| !learning_path_refs.insert(path))
            {
                return Err(CoachingDigestError::InvalidAggregate);
            }
        }
    }
    Ok(())
}

fn support_parts(support: &LearningTrackSupport) -> (LearningTrackPurpose, &LearningPathRef) {
    match support {
        LearningTrackSupport::Improvement {
            learning_path_ref, ..
        } => (LearningTrackPurpose::Improvement, learning_path_ref),
        LearningTrackSupport::Reinforcement {
            learning_path_ref, ..
        } => (LearningTrackPurpose::Reinforcement, learning_path_ref),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review_session_contract::{
        CriticalMomentId, CurriculumLearningConcept, ExplanationPathRef, LearningResourceId,
        LearningResourceKind, LearningResourceMappingId, LearningResourceRole,
        LearningTrackSupportBasis, OpeningIdentificationProvenance, OpeningLearningEvidence,
        OpeningMetadata, OpeningServiceAttribution, OpeningServiceProvider, PositionPhase,
        PositionPhaseKind, LEARNING_PLAN_SELECTION_POLICY_VERSION,
        LEARNING_RESOURCE_CATALOG_VERSION, POSITION_PHASE_POLICY_VERSION,
    };

    #[test]
    fn ranking_uses_improvement_games_then_total_games_then_canonical_key() {
        let first_id = game_import_id("first");
        let second_id = game_import_id("second");
        let third_id = game_import_id("third");
        let first = plan(vec![
            track(
                CurriculumLearningConcept::Fork,
                "fork",
                vec![support(LearningTrackPurpose::Reinforcement, "fork-a", 10)],
            ),
            track(
                CurriculumLearningConcept::Pin,
                "pin",
                vec![support(LearningTrackPurpose::Improvement, "pin-a", 20)],
            ),
            track(
                CurriculumLearningConcept::Skewer,
                "skewer",
                vec![support(LearningTrackPurpose::Reinforcement, "skewer-a", 30)],
            ),
        ]);
        let second = plan(vec![
            track(
                CurriculumLearningConcept::Fork,
                "fork",
                vec![support(LearningTrackPurpose::Improvement, "fork-b", 12)],
            ),
            track(
                CurriculumLearningConcept::Skewer,
                "skewer",
                vec![support(LearningTrackPurpose::Reinforcement, "skewer-b", 22)],
            ),
        ]);
        let third = plan(vec![
            track(
                CurriculumLearningConcept::HangingPiece,
                "hanging-piece",
                vec![support(LearningTrackPurpose::Improvement, "hanging-a", 14)],
            ),
            track(
                CurriculumLearningConcept::Skewer,
                "skewer",
                vec![support(LearningTrackPurpose::Reinforcement, "skewer-c", 24)],
            ),
        ]);

        let projection = project_sources([
            source(&first_id, &first),
            source(&second_id, &second),
            source(&third_id, &third),
        ])
        .unwrap();

        assert_eq!(
            projection
                .priorities
                .iter()
                .map(|priority| priority.key.clone())
                .collect::<Vec<_>>(),
            vec![
                curriculum_key(CurriculumLearningConcept::Fork),
                curriculum_key(CurriculumLearningConcept::Pin),
            ]
        );
    }

    #[test]
    fn one_game_contributes_once_and_improvement_wins_without_losing_path_refs() {
        let game_import_id = game_import_id("mixed");
        let plan = plan(vec![track(
            CurriculumLearningConcept::Fork,
            "fork",
            vec![
                support(LearningTrackPurpose::Reinforcement, "fork-a", 10),
                support(LearningTrackPurpose::Improvement, "fork-b", 20),
            ],
        )]);

        let projection = project_sources([source(&game_import_id, &plan)]).unwrap();
        let [priority] = projection.priorities.as_slice() else {
            panic!("a one-Game key must produce one priority")
        };
        let [game] = priority.supporting_games.as_slice() else {
            panic!("one Game contributes one support aggregate")
        };

        assert_eq!(game.purpose, LearningTrackPurpose::Improvement);
        assert_eq!(
            game.learning_path_refs,
            vec![learning_path_ref("fork-a"), learning_path_ref("fork-b")]
        );
    }

    #[test]
    fn empty_frozen_plans_produce_zero_priorities() {
        let game_import_id = game_import_id("empty");
        let plan = plan(Vec::new());

        let projection = project_sources([source(&game_import_id, &plan)]).unwrap();

        assert!(projection.priorities.is_empty());
    }

    #[test]
    fn opening_tracks_are_eligible_for_digest_priority_projection() {
        let game_import_id = game_import_id("opening");
        let resource_mapping_id = LearningResourceMappingId::try_from(
            "lichess:opening:sicilian-defense-najdorf-variation".to_string(),
        )
        .unwrap();
        let key = LearningTrackKey::Opening {
            resource_mapping_id: resource_mapping_id.clone(),
        };
        let basis = LearningTrackSupportBasis::Opening {
            evidence: OpeningLearningEvidence {
                position_phase: PositionPhase {
                    policy_version: POSITION_PHASE_POLICY_VERSION,
                    phase: PositionPhaseKind::Opening,
                },
                opening_identification: OpeningMetadata::Present {
                    eco: "B90".to_string(),
                    name: "Sicilian Defense: Najdorf Variation".to_string(),
                    provenance: OpeningIdentificationProvenance::Service {
                        provider: OpeningServiceProvider::ChessCom,
                        attribution: OpeningServiceAttribution::DirectImport,
                    },
                },
                resource_mapping_id,
            },
        };
        let plan = plan(vec![LearningTrack {
            resources: crate::learning_plan::catalog::resources_for(&key).unwrap(),
            key: key.clone(),
            support: vec![LearningTrackSupport::Improvement {
                learning_path_ref: learning_path_ref("opening"),
                critical_moment_id: CriticalMomentId::try_from("moment:opening".to_string())
                    .unwrap(),
                ply: 10,
                basis,
            }],
        }]);

        let projection = project_sources([source(&game_import_id, &plan)]).unwrap();

        assert_eq!(projection.priorities.len(), 1);
        assert_eq!(projection.priorities[0].key, key);
    }

    #[test]
    fn matching_keys_with_conflicting_exact_resource_sets_fail_closed() {
        let first_id = game_import_id("first");
        let second_id = game_import_id("second");
        let first = plan(vec![track(
            CurriculumLearningConcept::Fork,
            "fork-one",
            vec![support(LearningTrackPurpose::Improvement, "fork-a", 10)],
        )]);
        let second = plan(vec![track(
            CurriculumLearningConcept::Fork,
            "fork-two",
            vec![support(LearningTrackPurpose::Improvement, "fork-b", 12)],
        )]);

        let result = project_sources([source(&first_id, &first), source(&second_id, &second)]);

        assert_eq!(result.err(), Some(CoachingDigestError::InvalidAggregate));
    }

    #[test]
    fn non_v1_learning_plan_policy_fails_at_deserialization() {
        let mut stored = serde_json::to_value(plan(Vec::new())).unwrap();
        stored["selectionPolicyVersion"] = serde_json::json!("learning-plan-selection/v4");

        assert!(serde_json::from_value::<LearningPlan>(stored).is_err());
    }

    #[test]
    fn mismatched_learning_resource_catalog_versions_fail_closed() {
        let first_id = game_import_id("first");
        let second_id = game_import_id("second");
        let first = plan(Vec::new());
        let mut second = plan(Vec::new());
        second.resource_catalog_version = LearningResourceCatalogVersion::V2026_07_25;

        let result = project_sources([source(&first_id, &first), source(&second_id, &second)]);

        assert_eq!(result.err(), Some(CoachingDigestError::InvalidAggregate));
    }

    #[test]
    fn frozen_plan_rejects_duplicate_track_keys() {
        let duplicate = track(
            CurriculumLearningConcept::Fork,
            "fork",
            vec![support(LearningTrackPurpose::Improvement, "fork-a", 10)],
        );
        let plan = LearningPlan {
            selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
            resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
            tracks: vec![duplicate.clone(), duplicate],
        };

        assert_eq!(
            validate_frozen_learning_plan(&plan, 100),
            Err(CoachingDigestError::InvalidAggregate)
        );
    }

    #[test]
    fn frozen_plan_rejects_unique_tracks_out_of_recommendation_order() {
        let lower_support = track(
            CurriculumLearningConcept::Pin,
            "pin",
            vec![support(LearningTrackPurpose::Reinforcement, "pin-a", 10)],
        );
        let higher_support = track(
            CurriculumLearningConcept::Fork,
            "fork",
            vec![
                support(LearningTrackPurpose::Reinforcement, "fork-a", 20),
                support(LearningTrackPurpose::Reinforcement, "fork-b", 30),
            ],
        );
        let plan = LearningPlan {
            selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
            resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
            tracks: vec![lower_support, higher_support],
        };

        assert_eq!(
            validate_frozen_learning_plan(&plan, 100),
            Err(CoachingDigestError::InvalidAggregate)
        );
    }

    #[test]
    fn frozen_plan_rejects_support_grouped_under_the_wrong_key_kind() {
        let mut track = track(
            CurriculumLearningConcept::Fork,
            "fork",
            vec![support(LearningTrackPurpose::Improvement, "fork-a", 10)],
        );
        let mapping_id = crate::review_session_contract::LearningResourceMappingId::try_from(
            "mapping:opening".to_string(),
        )
        .unwrap();
        track.key = LearningTrackKey::Opening {
            resource_mapping_id: mapping_id,
        };
        let plan = plan(vec![track]);

        assert_eq!(
            validate_frozen_learning_plan(&plan, 100),
            Err(CoachingDigestError::InvalidAggregate)
        );
    }

    #[test]
    fn frozen_plan_rejects_support_outside_the_reviewed_game() {
        let plan = plan(vec![track(
            CurriculumLearningConcept::Fork,
            "fork",
            vec![support(LearningTrackPurpose::Improvement, "fork-a", 101)],
        )]);

        assert_eq!(
            validate_frozen_learning_plan(&plan, 100),
            Err(CoachingDigestError::InvalidAggregate)
        );
    }

    #[test]
    fn archived_priority_validation_rejects_a_dangling_supporting_game() {
        let known_game = game_import_id("known");
        let unknown_game = game_import_id("unknown");
        let priority = CoachingDigestPriority {
            key: curriculum_key(CurriculumLearningConcept::Fork),
            resources: vec![resource("fork")],
            supporting_games: vec![CoachingDigestPriorityGameSupport {
                game_import_id: unknown_game,
                purpose: LearningTrackPurpose::Improvement,
                learning_path_refs: vec![learning_path_ref("fork-a")],
            }],
        };

        assert_eq!(
            validate_archived(&[priority], &[known_game]),
            Err(CoachingDigestError::InvalidAggregate)
        );
    }

    #[test]
    fn archived_priority_allows_the_same_path_reference_in_distinct_games() {
        let first = game_import_id("first");
        let second = game_import_id("second");
        let shared_path = learning_path_ref("shared");
        let priority = CoachingDigestPriority {
            key: curriculum_key(CurriculumLearningConcept::Fork),
            resources: vec![resource("fork")],
            supporting_games: vec![
                CoachingDigestPriorityGameSupport {
                    game_import_id: first.clone(),
                    purpose: LearningTrackPurpose::Improvement,
                    learning_path_refs: vec![shared_path.clone()],
                },
                CoachingDigestPriorityGameSupport {
                    game_import_id: second.clone(),
                    purpose: LearningTrackPurpose::Reinforcement,
                    learning_path_refs: vec![shared_path],
                },
            ],
        };

        assert_eq!(validate_archived(&[priority], &[first, second]), Ok(()));
    }

    fn source<'a>(game_import_id: &'a GameImportId, plan: &'a LearningPlan) -> PrioritySource<'a> {
        PrioritySource {
            game_import_id,
            learning_plan: plan,
            played_plies: 100,
        }
    }

    fn plan(tracks: Vec<LearningTrack>) -> LearningPlan {
        LearningPlan {
            selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
            resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
            tracks: crate::learning_plan::order_learning_tracks(tracks).unwrap(),
        }
    }

    fn track(
        concept: CurriculumLearningConcept,
        resource_tag: &str,
        support: Vec<LearningTrackSupport>,
    ) -> LearningTrack {
        LearningTrack {
            key: curriculum_key(concept),
            support,
            resources: vec![resource(resource_tag)],
        }
    }

    fn curriculum_key(concept: CurriculumLearningConcept) -> LearningTrackKey {
        LearningTrackKey::Curriculum { concept }
    }

    fn support(purpose: LearningTrackPurpose, tag: &str, ply: u16) -> LearningTrackSupport {
        let learning_path_ref = learning_path_ref(tag);
        let critical_moment_id = CriticalMomentId::try_from(format!("moment:{tag}")).unwrap();
        let basis = LearningTrackSupportBasis::DecisionExplanation {
            explanation_path_ref: ExplanationPathRef::from_content(&tag),
        };
        match purpose {
            LearningTrackPurpose::Improvement => LearningTrackSupport::Improvement {
                learning_path_ref,
                critical_moment_id,
                ply,
                basis,
            },
            LearningTrackPurpose::Reinforcement => LearningTrackSupport::Reinforcement {
                learning_path_ref,
                critical_moment_id,
                ply,
                basis,
            },
        }
    }

    fn resource(tag: &str) -> LearningResource {
        LearningResource {
            resource_id: LearningResourceId::try_from(format!("resource:{tag}")).unwrap(),
            role: LearningResourceRole::Drill,
            kind: LearningResourceKind::PuzzleStream,
            title: format!("Resource {tag}"),
            canonical_url: format!("https://lichess.org/training/{tag}"),
        }
    }

    fn game_import_id(tag: &str) -> GameImportId {
        GameImportId::try_from(format!("game-import:{tag}")).unwrap()
    }

    fn learning_path_ref(tag: &str) -> LearningPathRef {
        LearningPathRef::try_from(format!("learning-path:{tag}")).unwrap()
    }
}
