use super::*;
use crate::{
    daily_coaching::digest::DigestedGameCard,
    review_session_contract::{
        CriticalMomentId, CurriculumLearningConcept, ExplanationPathRef, LearningPathRef,
        LearningResource, LearningResourceId, LearningResourceKind, LearningResourceRole,
        LearningTrack, LearningTrackKey, LearningTrackPurpose, LearningTrackSupport,
        LearningTrackSupportBasis, LEARNING_PLAN_SELECTION_POLICY_VERSION,
        LEARNING_RESOURCE_CATALOG_VERSION,
    },
};

#[tokio::test]
async fn publishes_ranked_priorities_from_exact_frozen_learning_plans() {
    let fixture = publish_priority_fixture().await;
    let projected = crate::daily_coaching::dashboard::project_digest(
        fixture.digest.clone(),
        fixture.cards.clone(),
    );

    assert_eq!(fixture.published, 1);
    assert_eq!(fixture.digest.learning_path_count, 5);
    assert_eq!(fixture.digest.priorities.len(), 2);
    assert_eq!(
        fixture
            .digest
            .priorities
            .iter()
            .map(|priority| priority.key.clone())
            .collect::<Vec<_>>(),
        vec![
            curriculum_key(CurriculumLearningConcept::Fork),
            curriculum_key(CurriculumLearningConcept::Pin),
        ]
    );
    assert_eq!(
        fixture.digest.priorities[0].resources,
        learning_resources("fork")
    );
    assert_eq!(fixture.digest.priorities[0].supporting_games.len(), 2);
    assert_eq!(projected.priorities[0].supporting_game_count, 2);
    assert_eq!(
        projected.priorities[0].supporting_game_import_ids,
        fixture.digest.priorities[0]
            .supporting_games
            .iter()
            .map(|support| support.game_import_id.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        projected.priorities[0].purpose,
        LearningTrackPurpose::Improvement
    );
    assert_eq!(
        projected.priorities[0].resources,
        learning_resources("fork")
    );
    assert_eq!(
        projected
            .games
            .iter()
            .map(|game| game.game_import_id.clone())
            .collect::<Vec<_>>(),
        fixture.digest.game_import_ids
    );
    assert_eq!(
        fixture.digest.priorities[0].supporting_games[0].purpose,
        LearningTrackPurpose::Improvement
    );
    assert_eq!(
        fixture.digest.priorities[0].supporting_games[0].learning_path_refs,
        vec![learning_path_ref("fork-a"), learning_path_ref("fork-b")]
    );
    assert_eq!(
        fixture.digest.priorities[0].supporting_games[1].learning_path_refs,
        vec![learning_path_ref("fork-c")]
    );
}

#[tokio::test]
async fn published_run_round_trip_preserves_exact_plans() {
    let fixture = publish_priority_fixture().await;
    let reviewed_games = fixture.run.reviewed_games();

    assert_eq!(
        reviewed_games
            .iter()
            .map(|(_, review)| review.learning_plan.clone())
            .collect::<Vec<_>>(),
        vec![fixture.first_plan, fixture.second_plan]
    );

    let stored_run = serde_json::to_value(&fixture.run).unwrap();
    let round_trip =
        serde_json::from_value::<DailyCoachingRunDocument>(stored_run.clone()).unwrap();
    assert_eq!(round_trip, fixture.run);
}

#[tokio::test]
async fn current_run_rejects_missing_learning_plan() {
    let fixture = publish_priority_fixture().await;
    let mut stored_run = serde_json::to_value(&fixture.run).unwrap();

    stored_run["selection"][0]["progress"]["review"]
        .as_object_mut()
        .unwrap()
        .remove("learningPlan");

    assert!(serde_json::from_value::<DailyCoachingRunDocument>(stored_run).is_err());
}

#[tokio::test]
async fn current_run_rejects_empty_learning_resources() {
    let fixture = publish_priority_fixture().await;
    let mut stored_run = serde_json::to_value(&fixture.run).unwrap();
    stored_run["selection"][0]["progress"]["review"]["learningPlan"]["tracks"][0]["resources"] =
        serde_json::json!([]);

    assert!(serde_json::from_value::<DailyCoachingRunDocument>(stored_run).is_err());
}

#[tokio::test]
async fn non_v1_run_rejects_reviewed_game_with_current_shape() {
    let fixture = publish_priority_fixture().await;
    let mut stored_run = serde_json::to_value(&fixture.run).unwrap();
    stored_run["schemaVersion"] = serde_json::json!(2);

    assert!(serde_json::from_value::<DailyCoachingRunDocument>(stored_run).is_err());
}

#[tokio::test]
async fn non_v1_run_rejects_legacy_reviewed_games_without_exact_plans() {
    let fixture = publish_priority_fixture().await;
    let mut stored_run = serde_json::to_value(&fixture.run).unwrap();
    stored_run["schemaVersion"] = serde_json::json!(2);

    for (selected, learning_path_count) in stored_run["selection"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip([3, 2])
    {
        let review = selected["progress"]["review"].as_object_mut().unwrap();
        review.remove("learningPlan");
        review.insert(
            "learningPathCount".to_string(),
            serde_json::json!(learning_path_count),
        );
        review.insert(
            "learningPlanSelectionPolicyVersion".to_string(),
            serde_json::json!(LEARNING_PLAN_SELECTION_POLICY_VERSION),
        );
        review.insert(
            "learningResourceCatalogVersion".to_string(),
            serde_json::json!(LEARNING_RESOURCE_CATALOG_VERSION),
        );
    }

    assert!(serde_json::from_value::<DailyCoachingRunDocument>(stored_run).is_err());
}

#[tokio::test]
async fn current_construction_rejects_omitted_priority_support() {
    let fixture = publish_priority_fixture().await;
    let reviewed_games = fixture.run.reviewed_games();
    let mut omitted_support = fixture.digest.clone();
    omitted_support.priorities[0].supporting_games.pop();

    assert!(omitted_support
        .validate_new(&fixture.cards, &reviewed_games)
        .is_err());
}

#[tokio::test]
async fn current_construction_rejects_misgrouped_learning_path_ref() {
    let fixture = publish_priority_fixture().await;
    let reviewed_games = fixture.run.reviewed_games();
    let mut misgrouped_support = fixture.digest.clone();
    misgrouped_support.priorities[0].supporting_games[0].learning_path_refs[0] =
        learning_path_ref("pin-a");

    assert!(misgrouped_support
        .validate_new(&fixture.cards, &reviewed_games)
        .is_err());
}

#[tokio::test]
async fn current_construction_rejects_dangling_supporting_game() {
    let fixture = publish_priority_fixture().await;
    let reviewed_games = fixture.run.reviewed_games();
    let mut dangling_support = fixture.digest.clone();
    dangling_support.priorities[0].supporting_games[0].game_import_id =
        GameImportId::try_from("game-import:daily:unknown".to_string()).unwrap();

    assert!(dangling_support
        .validate_new(&fixture.cards, &reviewed_games)
        .is_err());
}

#[tokio::test]
async fn current_construction_rejects_wrong_priority_rank() {
    let fixture = publish_priority_fixture().await;
    let reviewed_games = fixture.run.reviewed_games();
    let mut wrong_rank = fixture.digest.clone();
    wrong_rank.priorities.swap(0, 1);

    assert!(wrong_rank
        .validate_new(&fixture.cards, &reviewed_games)
        .is_err());
}

#[tokio::test]
async fn current_construction_rejects_wrong_priority_cardinality() {
    let fixture = publish_priority_fixture().await;
    let reviewed_games = fixture.run.reviewed_games();
    let mut wrong_cardinality = fixture.digest;
    wrong_cardinality.priorities.pop();

    assert!(wrong_cardinality
        .validate_new(&fixture.cards, &reviewed_games)
        .is_err());
}

#[tokio::test]
async fn archived_priorities_accept_historical_catalog_versions() {
    let fixture = publish_priority_fixture().await;
    let mut archived = serde_json::to_value(&fixture.digest).unwrap();
    archived["priorityPolicyVersion"] =
        serde_json::json!("coaching-digest-priority/test-only-non-current");
    archived["learningResourceCatalogVersion"] = serde_json::json!("learning-resources/2026-07-25");

    let archived = serde_json::from_value::<CoachingDigest>(archived).unwrap();

    assert!(archived.validate(&fixture.cards).is_ok());
}

#[tokio::test]
async fn archived_priorities_reject_removed_selection_policy_versions() {
    let fixture = publish_priority_fixture().await;
    let mut archived = serde_json::to_value(&fixture.digest).unwrap();
    archived["learningPlanSelectionPolicyVersion"] =
        serde_json::json!("learning-plan-selection/v3");

    assert!(serde_json::from_value::<CoachingDigest>(archived).is_err());
}

struct PublishedPriorityFixture {
    published: u64,
    run: DailyCoachingRunDocument,
    digest: CoachingDigest,
    cards: Vec<DigestedGameCard>,
    first_plan: LearningPlan,
    second_plan: LearningPlan,
}

async fn publish_priority_fixture() -> PublishedPriorityFixture {
    let state_store = Arc::new(InMemoryDailyCoachingStore::default());
    let run_store = Arc::new(InMemoryDailyCoachingRunStore::new(state_store.clone()));
    seed_player(&state_store, instant("2026-08-09T12:00:00Z")).await;
    let window = current_window(&state_store).await;
    let client = Arc::new(StaticWindowClient::new(window_body(
        &window,
        &["Synthet1Demo", "Synthet2Demo"],
    )));
    let first_plan = learning_plan(vec![
        curriculum_track(
            CurriculumLearningConcept::Fork,
            "fork",
            vec![
                learning_support(LearningTrackPurpose::Reinforcement, "fork-a", 10),
                learning_support(LearningTrackPurpose::Improvement, "fork-b", 20),
            ],
        ),
        curriculum_track(
            CurriculumLearningConcept::Pin,
            "pin",
            vec![learning_support(
                LearningTrackPurpose::Improvement,
                "pin-a",
                30,
            )],
        ),
    ]);
    let second_plan = learning_plan(vec![
        curriculum_track(
            CurriculumLearningConcept::Fork,
            "fork",
            vec![learning_support(
                LearningTrackPurpose::Improvement,
                "fork-c",
                12,
            )],
        ),
        curriculum_track(
            CurriculumLearningConcept::HangingPiece,
            "hanging-piece",
            vec![learning_support(
                LearningTrackPurpose::Improvement,
                "hanging-a",
                14,
            )],
        ),
    ]);
    let reviewer = Arc::new(ScriptedReviewer::new([
        reviewed_result_with_plan(
            "Synthet1",
            "game-import:daily:first-priority",
            first_plan.clone(),
        ),
        reviewed_result_with_plan(
            "Synthet2",
            "game-import:daily:second-priority",
            second_plan.clone(),
        ),
    ]));
    let lifecycle = lifecycle_with_reviewer(state_store, run_store.clone(), client, reviewer);

    let report = lifecycle.tick(window.due_at).await.unwrap();
    let run = run_store
        .read(&run_address(window.coverage_date))
        .await
        .unwrap()
        .unwrap();
    let (digest, cards) = run_store
        .read_digest(&owner(), &run_address(window.coverage_date).run_id)
        .await
        .unwrap()
        .unwrap();

    PublishedPriorityFixture {
        published: report.published,
        run,
        digest,
        cards,
        first_plan,
        second_plan,
    }
}

fn learning_plan(tracks: Vec<LearningTrack>) -> LearningPlan {
    let tracks = crate::learning_plan::order_learning_tracks(tracks)
        .expect("the publication fixture should use a valid compiled graph");
    LearningPlan {
        selection_policy_version: LEARNING_PLAN_SELECTION_POLICY_VERSION,
        resource_catalog_version: LEARNING_RESOURCE_CATALOG_VERSION,
        tracks,
    }
}

fn curriculum_track(
    concept: CurriculumLearningConcept,
    resource_tag: &str,
    support: Vec<LearningTrackSupport>,
) -> LearningTrack {
    LearningTrack {
        key: curriculum_key(concept),
        support,
        resources: learning_resources(resource_tag),
    }
}

fn curriculum_key(concept: CurriculumLearningConcept) -> LearningTrackKey {
    LearningTrackKey::Curriculum { concept }
}

fn learning_support(purpose: LearningTrackPurpose, tag: &str, ply: u16) -> LearningTrackSupport {
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

fn learning_resources(tag: &str) -> Vec<LearningResource> {
    vec![
        LearningResource {
            resource_id: LearningResourceId::try_from(format!("resource:{tag}:learn")).unwrap(),
            role: LearningResourceRole::Learn,
            kind: LearningResourceKind::PracticeModule,
            title: format!("Learn {tag}"),
            canonical_url: format!("https://lichess.org/practice/{tag}"),
        },
        LearningResource {
            resource_id: LearningResourceId::try_from(format!("resource:{tag}:drill")).unwrap(),
            role: LearningResourceRole::Drill,
            kind: LearningResourceKind::PuzzleStream,
            title: format!("Drill {tag}"),
            canonical_url: format!("https://lichess.org/training/{tag}"),
        },
    ]
}

fn learning_path_ref(tag: &str) -> LearningPathRef {
    LearningPathRef::try_from(format!("learning-path:{tag}")).unwrap()
}
