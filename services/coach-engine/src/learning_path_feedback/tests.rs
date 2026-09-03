use super::*;
use crate::review_session_contract::{
    CurriculumLearningConcept, LearningPlanSelectionPolicyVersion, LearningResourceCatalogVersion,
};

#[tokio::test]
async fn exposure_is_idempotent_and_vote_can_be_replaced_or_removed() {
    let store = InMemoryLearningPathFeedbackStore::default();
    let player = player("player-a");
    let sample = sample("a");

    store
        .record_exposure(&player, sample.clone(), DeliverySurface::Web)
        .await
        .unwrap();
    let repeated = store
        .record_exposure(&player, sample.clone(), DeliverySurface::Web)
        .await
        .unwrap();
    assert_eq!(repeated.exposed_surfaces, vec![DeliverySurface::Web]);

    let up = store
        .update_vote(
            &player,
            sample.clone(),
            DeliverySurface::Web,
            Some(LearningPathVote::ThumbsUp),
        )
        .await
        .unwrap();
    assert_eq!(up.current_vote, Some(LearningPathVote::ThumbsUp));
    let down = store
        .update_vote(
            &player,
            sample.clone(),
            DeliverySurface::Web,
            Some(LearningPathVote::ThumbsDown),
        )
        .await
        .unwrap();
    assert_eq!(down.current_vote, Some(LearningPathVote::ThumbsDown));
    let removed = store
        .update_vote(&player, sample, DeliverySurface::Web, None)
        .await
        .unwrap();
    assert_eq!(removed.current_vote, None);
}

#[tokio::test]
async fn analytics_excludes_nonresponses_from_relevance_rate() {
    let store = InMemoryLearningPathFeedbackStore::default();
    let first = sample("a");
    let second = sample("b");
    store
        .record_exposure(&player("player-a"), first.clone(), DeliverySurface::Web)
        .await
        .unwrap();
    store
        .update_vote(
            &player("player-a"),
            first,
            DeliverySurface::Web,
            Some(LearningPathVote::ThumbsUp),
        )
        .await
        .unwrap();
    store
        .record_exposure(&player("player-b"), second, DeliverySurface::Web)
        .await
        .unwrap();

    let analytics = store.analytics().await.unwrap();
    let slice = &analytics.slices[0];
    assert_eq!(slice.exposure_count, 2);
    assert_eq!(slice.vote_count, 1);
    assert_eq!(slice.thumbs_up_count, 1);
    assert_eq!(slice.thumbs_down_count, 0);
    assert_eq!(slice.relevance_rate, Some(1.0));
    assert_eq!(slice.response_rate, Some(0.5));
}

#[tokio::test]
async fn voting_requires_the_same_player_path_and_exposed_surface() {
    let store = InMemoryLearningPathFeedbackStore::default();
    let sample = sample("a");
    let player = player("player-a");

    assert_eq!(
        store
            .update_vote(
                &player,
                sample.clone(),
                DeliverySurface::Web,
                Some(LearningPathVote::ThumbsUp),
            )
            .await,
        Err(LearningPathFeedbackError::ExposureRequired)
    );
    store
        .record_exposure(&player, sample.clone(), DeliverySurface::Web)
        .await
        .unwrap();
    assert_eq!(
        store
            .update_vote(
                &player,
                sample,
                DeliverySurface::CoachApp,
                Some(LearningPathVote::ThumbsUp),
            )
            .await,
        Err(LearningPathFeedbackError::ExposureRequired)
    );
}

#[tokio::test]
async fn stored_path_identity_rejects_conflicting_server_metadata() {
    let store = InMemoryLearningPathFeedbackStore::default();
    let player = player("player-a");
    let original = sample("a");
    store
        .record_exposure(&player, original.clone(), DeliverySurface::Web)
        .await
        .unwrap();
    let mut conflicting = original;
    conflicting.purpose = LearningTrackPurpose::Reinforcement;

    assert_eq!(
        store
            .record_exposure(&player, conflicting, DeliverySurface::CoachApp)
            .await,
        Err(LearningPathFeedbackError::InvalidSample)
    );
}

#[tokio::test]
async fn changing_vote_surface_moves_one_effective_vote_between_slices() {
    let store = InMemoryLearningPathFeedbackStore::default();
    let player = player("player-a");
    let sample = sample("a");
    for surface in [DeliverySurface::Web, DeliverySurface::CoachApp] {
        store
            .record_exposure(&player, sample.clone(), surface)
            .await
            .unwrap();
    }
    store
        .update_vote(
            &player,
            sample.clone(),
            DeliverySurface::Web,
            Some(LearningPathVote::ThumbsUp),
        )
        .await
        .unwrap();
    store
        .update_vote(
            &player,
            sample,
            DeliverySurface::CoachApp,
            Some(LearningPathVote::ThumbsDown),
        )
        .await
        .unwrap();

    let analytics = store.analytics().await.unwrap();
    assert_eq!(analytics.slices.len(), 2);
    let web = analytics
        .slices
        .iter()
        .find(|slice| slice.key.surface == DeliverySurface::Web)
        .unwrap();
    let coach_app = analytics
        .slices
        .iter()
        .find(|slice| slice.key.surface == DeliverySurface::CoachApp)
        .unwrap();
    assert_eq!((web.exposure_count, web.vote_count), (1, 0));
    assert_eq!(
        (
            coach_app.exposure_count,
            coach_app.vote_count,
            coach_app.thumbs_down_count,
        ),
        (1, 1, 1)
    );
}

#[test]
fn aggregate_vote_deltas_fail_closed_before_underflow_or_overcount() {
    let key = slice_key(&sample("a"), DeliverySurface::Web);
    let path = analytics_path(&key);
    let mut aggregates = vec![(path, StoredAnalyticsSlice::empty(key))];

    assert_eq!(
        apply_vote_delta(
            &mut aggregates,
            Some((LearningPathVote::ThumbsUp, DeliverySurface::Web)),
            None,
        ),
        Err(LearningPathFeedbackError::InvalidSample)
    );
    assert_eq!(
        aggregates[0].1.counts,
        LearningPathFeedbackCounts::default()
    );
}

fn sample(seed: &str) -> LearningPathSample {
    LearningPathSample {
        learning_path_ref: LearningPathRef::try_from(format!("learning-path:{seed}")).unwrap(),
        game_ref: GameRef::try_from(format!("sha256:{}", "1".repeat(64))).unwrap(),
        critical_moment_id: CriticalMomentId::try_from("moment:1".to_string()).unwrap(),
        key: LearningTrackKey::Curriculum {
            concept: CurriculumLearningConcept::Pin,
        },
        purpose: LearningTrackPurpose::Improvement,
        proof_family: LearningProofFamily::DecisionExplanation,
        selection_policy_version: LearningPlanSelectionPolicyVersion::V1,
        resource_catalog_version: LearningResourceCatalogVersion::V2026_08_03,
        resource_ids: Vec::new(),
    }
}

fn player(id: &str) -> PlayerId {
    PlayerId::try_from(id.to_string()).unwrap()
}
