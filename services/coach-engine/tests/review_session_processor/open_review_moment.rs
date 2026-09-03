use std::collections::BTreeSet;

use super::*;

#[tokio::test]
async fn reopening_an_automatic_moment_preserves_its_exploration_and_provenance() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let (game_import_id, automatic_moments, _) =
        import_and_start_all(&processor, principal.clone()).await;
    let automatic = automatic_moments
        .first()
        .expect("the canonical fixture has an Automatic Review Moment")
        .clone();

    let alternative = explore_root(&processor, &principal, &game_import_id, &automatic).await;

    let (reopened, _, _) = open_moment(
        &processor,
        principal.clone(),
        &game_import_id,
        "reopen-automatic",
        ReviewMomentSelection::PipelineCriticalMoment {
            critical_moment_id: automatic.review_moment.moment_id.clone(),
        },
    )
    .await;

    assert_eq!(reopened.review_moment, automatic.review_moment);
    assert_eq!(reopened.coach_turn_context, automatic.coach_turn_context);
    assert!(
        reopened.evidence_packet.entries.len() > automatic.evidence_packet.entries.len(),
        "reopening must retain the Alternative Move Exploration evidence"
    );

    let inspection = inspect_alternative(
        &processor,
        principal,
        &game_import_id,
        &automatic.review_moment.moment_id,
        &alternative.alternative_move_id,
        "reopen-automatic-inspect",
    )
    .await;
    assert_eq!(inspection.position_snapshot, alternative.resulting_position);
}

#[tokio::test]
async fn player_selected_moments_are_inserted_once_in_game_order_without_rewriting_automatic_set() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let (game_import_id, automatic_moments, frozen_review) =
        import_and_start_all(&processor, principal.clone()).await;
    let frozen_learning_plan = serde_json::to_vec(&frozen_review.learning_plan).unwrap();
    assert!(
        automatic_moments.len() >= 2,
        "the canonical fixture needs multiple Automatic moments to test between insertion"
    );

    let automatic_plies = automatic_moments
        .iter()
        .map(|core| core.review_moment.ply)
        .collect::<BTreeSet<_>>();
    let game_plies = automatic_moments[0]
        .imported_game
        .game
        .moves
        .iter()
        .map(|game_move| game_move.ply)
        .collect::<Vec<_>>();
    let first_automatic = *automatic_plies.first().unwrap();
    let last_automatic = *automatic_plies.last().unwrap();
    let before = *game_plies
        .iter()
        .rfind(|ply| **ply < first_automatic && !automatic_plies.contains(ply))
        .expect("fixture has a legal ply before the first Automatic moment");
    let between = *game_plies
        .iter()
        .find(|ply| {
            **ply > first_automatic && **ply < last_automatic && !automatic_plies.contains(ply)
        })
        .expect("fixture has a legal ply between Automatic moments");
    let after = *game_plies
        .iter()
        .find(|ply| **ply > last_automatic && !automatic_plies.contains(ply))
        .expect("fixture has a legal ply after the last Automatic moment");

    let mut expected_plies = automatic_plies.clone();
    for (label, ply) in [
        ("open-before", before),
        ("open-between", between),
        ("open-after", after),
    ] {
        let (opened, _, _) = open_moment(
            &processor,
            principal.clone(),
            &game_import_id,
            label,
            ReviewMomentSelection::PlayerSelectedMoment { ply },
        )
        .await;
        assert_eq!(opened.review_moment.ply, ply);
        assert!(matches!(
            opened.review_moment.selection,
            ReviewMomentSelection::PlayerSelectedMoment { ply: selected } if selected == ply
        ));
        expected_plies.insert(ply);
        let (review_moments, review) = session_navigation(
            &processor,
            principal.clone(),
            &game_import_id,
            &format!("{label}-navigation"),
        )
        .await;
        assert_navigation(&review_moments, &expected_plies, &automatic_moments);
        assert_eq!(
            serde_json::to_vec(&review.learning_plan).unwrap(),
            frozen_learning_plan,
            "opening a Player-selected moment must not rewrite the frozen Learning Plan"
        );
    }

    let (first_open, _, _) = open_moment(
        &processor,
        principal.clone(),
        &game_import_id,
        "open-duplicate-first",
        ReviewMomentSelection::PlayerSelectedMoment { ply: between },
    )
    .await;
    let (first_navigation, first_review) = session_navigation(
        &processor,
        principal.clone(),
        &game_import_id,
        "open-duplicate-first-navigation",
    )
    .await;
    let (second_open, _, _) = open_moment(
        &processor,
        principal.clone(),
        &game_import_id,
        "open-duplicate-second",
        ReviewMomentSelection::PlayerSelectedMoment { ply: between },
    )
    .await;
    let (second_navigation, second_review) = session_navigation(
        &processor,
        principal.clone(),
        &game_import_id,
        "open-duplicate-second-navigation",
    )
    .await;
    assert_eq!(second_open, first_open);
    assert_eq!(second_navigation, first_navigation);
    assert_eq!(
        serde_json::to_vec(&first_review.learning_plan).unwrap(),
        frozen_learning_plan
    );
    assert_eq!(
        serde_json::to_vec(&second_review.learning_plan).unwrap(),
        frozen_learning_plan
    );

    let (automatic_open, _, _) = open_moment(
        &processor,
        principal.clone(),
        &game_import_id,
        "open-automatic-by-id",
        ReviewMomentSelection::PipelineCriticalMoment {
            critical_moment_id: automatic_moments[0].review_moment.moment_id.clone(),
        },
    )
    .await;
    let (navigation, review) = session_navigation(
        &processor,
        principal,
        &game_import_id,
        "open-automatic-by-id-navigation",
    )
    .await;
    assert_eq!(automatic_open, automatic_moments[0]);
    assert_navigation(&navigation, &expected_plies, &automatic_moments);
    assert_eq!(
        serde_json::to_vec(&review.learning_plan).unwrap(),
        frozen_learning_plan
    );
}

#[tokio::test]
async fn player_selected_comments_render_their_prepared_classification_facts() {
    let (processor, _, _) = processor(false);
    let principal = ProcessorPrincipal::LocalCoach;
    let (game_import_id, automatic_moments, frozen_review) =
        import_and_start_all(&processor, principal.clone()).await;
    assert!(
        automatic_moments
            .iter()
            .all(|core| !matches!(core.review_moment.ply, 49 | 41)),
        "the regressions must open moves outside the Automatic set"
    );

    let (opened, comment, learning_material) = open_moment(
        &processor,
        principal.clone(),
        &game_import_id,
        "open-player-selected-positive",
        ReviewMomentSelection::PlayerSelectedMoment { ply: 49 },
    )
    .await;

    assert!(matches!(
        opened.review_moment.selection,
        ReviewMomentSelection::PlayerSelectedMoment { ply: 49 }
    ));
    assert_eq!(
        learning_material.selection_policy_version,
        frozen_review.learning_plan.selection_policy_version
    );
    assert_eq!(
        learning_material.resource_catalog_version,
        frozen_review.learning_plan.resource_catalog_version
    );
    let text = comment
        .expect("a Player-Selected Review Moment returns its canonical comment")
        .text;
    assert!(
        text.starts_with("Good: Bxg5 "),
        "the prepared Positive Highlight facts must determine the opening: {text}"
    );
    assert!(!text.starts_with("Neutral:"));

    let (_, comment, _) = open_moment(
        &processor,
        principal,
        &game_import_id,
        "open-player-selected-improvement",
        ReviewMomentSelection::PlayerSelectedMoment { ply: 41 },
    )
    .await;
    let text = comment
        .expect("a Player-Selected Review Moment returns its canonical comment")
        .text;
    assert!(
        text.starts_with("Improvement: "),
        "the prepared Improvement Opportunity facts must determine the opening: {text}"
    );
    assert!(text.contains("The better move was "));
    assert!(!text.starts_with("Neutral:"));
}

async fn import_and_start_all(
    processor: &std::sync::Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
) -> (GameImportId, Vec<ReviewSessionCoreContract>, GameReview) {
    let imported = submit(
        processor,
        principal.clone(),
        envelope_for(&principal, "open-moment-import", import_command()),
    )
    .await;
    let game_import_id = imported.iter().find_map(imported_game).unwrap();
    let started = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            "open-moment-start",
            ReviewSessionCommand::StartReviewSession { game_import_id },
        ),
    )
    .await;
    assert_event_stream(&started, OperationKind::ReviewSessionStart);
    match &started.last().unwrap().event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewSessionStarted {
                game_import_id,
                review,
                review_moments,
                ..
            } => (
                game_import_id.clone(),
                review_moments
                    .iter()
                    .map(|moment| {
                        moment
                            .prepared_core()
                            .expect("Coach Skill starts return a complete prepared batch")
                            .clone()
                    })
                    .collect(),
                review.as_ref().clone(),
            ),
            completion => panic!("expected a started Review Session, got {completion:?}"),
        },
        event => panic!("expected a completed Review Session event, got {event:?}"),
    }
}

pub(super) async fn open_moment(
    processor: &std::sync::Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    game_import_id: &GameImportId,
    label: &str,
    selection: ReviewMomentSelection,
) -> (
    ReviewSessionCoreContract,
    Option<CriticalMomentComment>,
    ReviewMomentLearningMaterial,
) {
    let events = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            label,
            ReviewSessionCommand::OpenReviewMoment {
                game_import_id: game_import_id.clone(),
                selection,
                idempotency_key: idempotency_key(label),
            },
        ),
    )
    .await;
    assert_event_stream(&events, OperationKind::ReviewMomentOpen);
    match &events.last().unwrap().event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewMomentOpened {
                review_moment,
                critical_moment,
                comment,
                authoring_context,
                ..
            } => {
                assert_eq!(
                    critical_moment.critical_moment_id, review_moment.review_moment.moment_id,
                    "an open ships the Game Review entry of the moment it opened"
                );
                if let Some(context) = authoring_context {
                    assert_eq!(
                        context.facts.moment().learning_material,
                        critical_moment.learning_material,
                        "Coach App authoring must receive the same active-moment material"
                    );
                }
                (
                    review_moment.as_ref().clone(),
                    comment.as_deref().cloned(),
                    critical_moment.learning_material.clone(),
                )
            }
            completion => panic!("expected an opened Review Moment, got {completion:?}"),
        },
        event => panic!("expected a completed Review Moment event, got {event:?}"),
    }
}

/// Navigation and the frozen review left the per-moment open payload, so the
/// session-scoped facts are read back from the session itself.
async fn session_navigation(
    processor: &std::sync::Arc<ReviewSessionProcessor<CapturedLichess>>,
    principal: ProcessorPrincipal,
    game_import_id: &GameImportId,
    label: &str,
) -> (Vec<ReviewSessionCoreContract>, GameReview) {
    let events = submit(
        processor,
        principal.clone(),
        envelope_for(
            &principal,
            label,
            ReviewSessionCommand::StartReviewSession {
                game_import_id: game_import_id.clone(),
            },
        ),
    )
    .await;
    match &events.last().unwrap().event {
        ReviewSessionEvent::Completed { result } => match result.as_ref() {
            OperationCompletion::ReviewSessionStarted {
                review,
                review_moments,
                ..
            } => (
                review_moments
                    .iter()
                    .map(|moment| {
                        moment
                            .prepared_core()
                            .expect("Coach Skill resumes return a complete prepared batch")
                            .clone()
                    })
                    .collect(),
                review.as_ref().clone(),
            ),
            completion => panic!("expected a resumed Review Session, got {completion:?}"),
        },
        event => panic!("expected a completed Review Session event, got {event:?}"),
    }
}

fn assert_navigation(
    review_moments: &[ReviewSessionCoreContract],
    expected_plies: &BTreeSet<u16>,
    automatic_moments: &[ReviewSessionCoreContract],
) {
    let actual_plies = review_moments
        .iter()
        .map(|core| core.review_moment.ply)
        .collect::<Vec<_>>();
    assert_eq!(
        actual_plies,
        expected_plies.iter().copied().collect::<Vec<_>>(),
        "navigation must remain strictly ordered by Game ply"
    );

    let automatic = review_moments
        .iter()
        .filter(|core| {
            matches!(
                core.review_moment.selection,
                ReviewMomentSelection::PipelineCriticalMoment { .. }
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(automatic, automatic_moments);
}
