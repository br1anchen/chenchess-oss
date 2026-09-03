use super::*;

#[tokio::test]
async fn proof_is_frozen_on_first_open_and_reused_without_engine_analysis() {
    let checkpoints = Arc::new(InMemoryReviewAnalysisCache::default());
    let game_imports = Arc::new(InMemoryGameImportStore::default());
    let engine = Arc::new(CountingAfterImportEngine::new());
    let first_processor = live_processor_with_engine_and_stores(
        engine.clone(),
        game_imports.clone(),
        checkpoints.clone(),
    );
    let imported = surface_events(
        &first_processor,
        DeliverySurface::CoachApp,
        "player-decision-import",
        import_command(),
    )
    .await;
    let started = surface_events(
        &first_processor,
        DeliverySurface::CoachApp,
        "player-decision-start",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: imported_game_id(&imported),
        },
    )
    .await;
    let game_import_id = match completion(&started) {
        OperationCompletion::ReviewSessionStarted { game_import_id, .. } => game_import_id.clone(),
        completion => panic!("expected Review Session start, got {completion:?}"),
    };
    let selection = ReviewMomentSelection::PlayerSelectedMoment { ply: 23 };
    // Import analyses the whole Game; what this measures is that opening a
    // moment adds nothing to that, so the baseline is taken after the start.
    let after_import = engine.preparation_calls.load(Ordering::SeqCst);

    let opened = surface_events(
        &first_processor,
        DeliverySurface::CoachApp,
        "player-decision-open",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: game_import_id.clone(),
            selection: selection.clone(),
            idempotency_key: idempotency_key("player-decision-open"),
        },
    )
    .await;
    let frozen_material = opened_learning_material(completion(&opened)).clone();
    let frozen_explanation_ref = active_decision_ref(completion(&opened));
    assert!(frozen_material.tracks.iter().any(|track| {
        track.support.iter().any(|support| {
            matches!(
                support,
                LearningTrackSupport::Reinforcement {
                    basis: LearningTrackSupportBasis::DecisionExplanation { .. },
                    ..
                }
            )
        })
    }));
    assert_eq!(
        engine.preparation_calls.load(Ordering::SeqCst),
        after_import,
        "first use must project retained Single-PV evidence without another engine request"
    );
    drop(first_processor);

    let second_processor =
        live_processor_with_engine_and_stores(engine.clone(), game_imports, checkpoints);
    let resumed = surface_events(
        &second_processor,
        DeliverySurface::CoachApp,
        "player-decision-resume",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: game_import_id.clone(),
        },
    )
    .await;
    let restored = selected_moment(completion(&resumed), &selection);
    assert_eq!(restored.learning_material, frozen_material);

    let reopened = surface_events(
        &second_processor,
        DeliverySurface::CoachApp,
        "player-decision-reopen",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id,
            selection: selection.clone(),
            idempotency_key: idempotency_key("player-decision-reopen"),
        },
    )
    .await;
    assert_eq!(
        opened_learning_material(completion(&reopened)),
        &frozen_material
    );
    // The reference is a digest of the whole proof, so an equal reference after
    // a restart is the observable form of "the frozen proof was reused".
    assert_eq!(
        active_decision_ref(completion(&reopened)),
        frozen_explanation_ref
    );
    assert_eq!(
        engine.preparation_calls.load(Ordering::SeqCst),
        after_import,
        "restart and reopen must reuse the frozen proof without engine analysis"
    );
}

fn active_decision_ref(completion: &OperationCompletion) -> DecisionExplanationRef {
    match completion {
        OperationCompletion::ReviewMomentOpened {
            decision_explanation_ref: Some(explanation_ref),
            ..
        } => explanation_ref.clone(),
        completion => panic!("expected an opened Decision Explanation, got {completion:?}"),
    }
}

fn opened_learning_material(completion: &OperationCompletion) -> &ReviewMomentLearningMaterial {
    match completion {
        OperationCompletion::ReviewMomentOpened {
            critical_moment, ..
        } => &critical_moment.learning_material,
        completion => panic!("expected an opened Review Moment, got {completion:?}"),
    }
}

fn selected_moment<'a>(
    completion: &'a OperationCompletion,
    selection: &ReviewMomentSelection,
) -> &'a ReviewSessionMoment {
    let review_moments = match completion {
        OperationCompletion::ReviewSessionStarted { review_moments, .. } => review_moments,
        completion => panic!("expected a resumed Review Session, got {completion:?}"),
    };
    review_moments
        .iter()
        .find(|moment| &moment.review_moment.selection == selection)
        .expect("the Player-Selected moment remains in navigation")
}
