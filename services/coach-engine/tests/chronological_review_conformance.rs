use chen_chess_coach_engine::{
    critical_moment_comment::{grounding_ledger_for, intent_authoring_context_for},
    review_session_contract::{
        encode_delivery_frame, CurriculumLearningConcept, DeliverySurface, GameImportId,
        IdempotencyKey, LearningResourceRole, LearningTrackKey, OperationCompletion, OperationId,
        RequestId, ReviewMomentCommentFacts, ReviewSessionCommand, ReviewSessionEvent,
        ReviewSessionEventEnvelope,
    },
    review_session_processor::ProcessorPrincipal,
};
#[path = "chronological_review_conformance/mod.rs"]
mod conformance;

use conformance::{
    expected::{assert_canonical_contract, contract_projection},
    support::{completion, player_id, run_journey, submit},
};

#[tokio::test]
async fn canonical_mixed_review_is_exact_across_web_coach_skill_and_coach_app() {
    let web = run_journey(
        DeliverySurface::Web,
        ProcessorPrincipal::Player(player_id("web")),
    )
    .await;
    let coach_skill =
        run_journey(DeliverySurface::CoachSkill, ProcessorPrincipal::LocalCoach).await;
    let coach_app = run_journey(
        DeliverySurface::CoachApp,
        ProcessorPrincipal::Player(player_id("coach-app")),
    )
    .await;

    let web_contract = contract_projection(&web.review, &web.review_moments);
    let coach_skill_contract =
        contract_projection(&coach_skill.review, &coach_skill.review_moments);
    let coach_app_contract = contract_projection(&coach_app.review, &coach_app.review_moments);

    assert_eq!(web_contract, coach_skill_contract);
    assert_eq!(web_contract, coach_app_contract);
    assert_canonical_contract(&web.review, &web.review_moments);

    assert_eq!(
        web.player_selected_material,
        coach_skill.player_selected_material
    );
    assert_eq!(
        web.player_selected_material,
        coach_app.player_selected_material
    );
    assert_eq!(web.player_selected_material.tracks.len(), 1);
    let local_track = &web.player_selected_material.tracks[0];
    assert!(
        matches!(
            &local_track.key,
            LearningTrackKey::Curriculum {
                concept: CurriculumLearningConcept::XRayAttack
            }
        ),
        "expected the local X-ray path, got {:?}",
        local_track.key
    );
    assert_eq!(
        local_track
            .resources
            .iter()
            .map(|resource| {
                (
                    resource.role,
                    resource.title.as_str(),
                    resource.canonical_url.as_str(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                LearningResourceRole::Learn,
                "X-Ray",
                "https://lichess.org/practice/fundamental-tactics/x-ray/lyVYjhPG",
            ),
            (
                LearningResourceRole::Drill,
                "X-ray attack",
                "https://lichess.org/training/xRayAttack",
            ),
        ]
    );
    assert_eq!(web.neutral_material, coach_skill.neutral_material);
    assert_eq!(web.neutral_material, coach_app.neutral_material);
    assert!(web.neutral_material.tracks.is_empty());
    assert_eq!(
        coach_app.player_selected_authoring_material.as_ref(),
        Some(&coach_app.player_selected_material),
        "Coach App host authoring must receive the same grounded active-moment facts"
    );
}

#[tokio::test]
async fn coach_app_admits_every_canonical_mixed_comment_at_the_shared_publication_seam() {
    let principal = ProcessorPrincipal::Player(player_id("publication"));
    let journey = run_journey(DeliverySurface::CoachApp, principal.clone()).await;

    for (index, core) in journey.review_moments.iter().enumerate() {
        let moment = journey
            .review
            .critical_moments
            .iter()
            .find(|moment| moment.critical_moment_id == core.review_moment.moment_id)
            .expect("every prepared Review Moment belongs to the imported Game Review")
            .clone();
        let facts = ReviewMomentCommentFacts::try_from_presented_moment(moment).unwrap();
        let intent = intent_authoring_context_for(&facts, None);
        let commentary = crate::marker_commentary::commentary(&facts, intent.as_ref());
        let events = submit(
            &journey.processor,
            principal.clone(),
            DeliverySurface::CoachApp,
            &format!("publication-{index}"),
            ReviewSessionCommand::PublishReviewMomentComment {
                game_import_id: journey.game_import_id.clone(),
                review_moment_id: core.review_moment.moment_id.clone(),
                text: commentary.draft_text.clone(),
                grounding_ledger: grounding_ledger_for(&facts),
                idempotency_key: IdempotencyKey::try_from(format!(
                    "idempotency-key:conformance:{index}"
                ))
                .unwrap(),
            },
        )
        .await;
        match completion(&events) {
            OperationCompletion::ReviewMomentCommentPublished { comment, .. } => {
                assert_eq!(comment.as_ref(), &commentary.comment);
            }
            result => panic!("expected an admitted Review Moment Comment, got {result:?}"),
        }
    }
}

/// The proof aggregate dominates a review's bytes and no surface renders it,
/// so delivery drops it while every moment keeps the reference that addresses
/// it. The ratio is asserted rather than described so the diet cannot silently
/// regress.
#[tokio::test]
async fn delivery_drops_the_decision_proof_and_keeps_its_reference() {
    let journey = run_journey(
        DeliverySurface::CoachApp,
        ProcessorPrincipal::Player(player_id("delivery")),
    )
    .await;

    let proof_bearing = journey
        .review
        .critical_moments
        .iter()
        .filter(|moment| moment.decision_explanation.is_some())
        .count();
    assert!(
        proof_bearing > 0,
        "the canonical review must exercise Decision Explanation enrichment"
    );
    for moment in &journey.review.critical_moments {
        assert_eq!(
            moment.decision_explanation_ref.as_ref(),
            moment
                .decision_explanation
                .as_ref()
                .map(|explanation| &explanation.decision_explanation_ref),
            "a moment's reference must address the proof it holds"
        );
    }

    let frozen = ReviewSessionEventEnvelope {
        request_id: RequestId::try_from("request:conformance:delivery".to_string()).unwrap(),
        operation_id: OperationId::try_from("operation:conformance:delivery".to_string()).unwrap(),
        sequence: 0,
        event: ReviewSessionEvent::Completed {
            result: Box::new(OperationCompletion::GameReviewOpened {
                game_import_id: GameImportId::try_from("game-import:conformance".to_string())
                    .unwrap(),
                review: Box::new(journey.review.clone()),
            }),
        },
    };
    let frozen_bytes = serde_json::to_vec(&frozen).unwrap().len();
    let delivered = encode_delivery_frame(frozen);
    let delivered_review =
        serde_json::from_slice::<ReviewSessionEventEnvelope>(&delivered).unwrap();
    let ReviewSessionEvent::Completed { result } = delivered_review.event else {
        panic!("delivery must preserve the completion");
    };
    let OperationCompletion::GameReviewOpened { review, .. } = *result else {
        panic!("delivery must preserve the completion kind");
    };

    assert!(
        review
            .critical_moments
            .iter()
            .all(|moment| moment.decision_explanation.is_none()),
        "no delivered Review Moment may carry the proof aggregate"
    );
    assert_eq!(
        review
            .critical_moments
            .iter()
            .filter(|moment| moment.decision_explanation_ref.is_some())
            .count(),
        proof_bearing,
        "every proof-backed Review Moment keeps its reference"
    );
    assert!(
        delivered.len() * 4 <= frozen_bytes,
        "delivery must drop at least three quarters of the frozen review: \
         {} delivered bytes against {frozen_bytes} frozen",
        delivered.len()
    );
}

/// Opening a Review Moment ships that moment, not the Review Session it belongs
/// to. A conversation opens many moments against one session, so the per-moment
/// frame is asserted against the session frame that established the review.
#[tokio::test]
async fn opening_a_review_moment_ships_the_moment_rather_than_the_session() {
    let principal = ProcessorPrincipal::Player(player_id("per-moment"));
    let journey = run_journey(DeliverySurface::CoachApp, principal.clone()).await;
    let target = journey
        .review_moments
        .first()
        .expect("the canonical fixture prepares at least one Review Moment");

    let resumed = submit(
        &journey.processor,
        principal.clone(),
        DeliverySurface::CoachApp,
        "per-moment-resume",
        ReviewSessionCommand::StartReviewSession {
            game_import_id: journey.game_import_id.clone(),
        },
    )
    .await;
    let opened = submit(
        &journey.processor,
        principal,
        DeliverySurface::CoachApp,
        "per-moment-open",
        ReviewSessionCommand::OpenReviewMoment {
            game_import_id: journey.game_import_id.clone(),
            selection: target.review_moment.selection.clone(),
            idempotency_key: IdempotencyKey::try_from(
                "idempotency-key:conformance:per-moment".to_string(),
            )
            .unwrap(),
        },
    )
    .await;

    let session_frame = encode_delivery_frame(completed_envelope(&resumed));
    let moment_frame = encode_delivery_frame(completed_envelope(&opened));

    let delivered = serde_json::from_slice::<serde_json::Value>(&moment_frame).unwrap();
    let result = delivered
        .pointer("/event/result")
        .and_then(serde_json::Value::as_object)
        .expect("a delivered completion carries its result");
    for absent in ["review", "importedGame", "reviewMoments"] {
        assert!(
            !result.contains_key(absent),
            "a delivered Review Moment open must not re-send `{absent}`"
        );
    }
    assert_eq!(
        result["criticalMoment"]["criticalMomentId"],
        serde_json::json!(target.review_moment.moment_id.as_str()),
        "a delivered open carries the Game Review entry of the moment it opened"
    );

    // What remains is the moment's own core, which still embeds the Game it is
    // grounded in; addressing that is the snapshot-resource work, not this diet.
    assert!(
        moment_frame.len() * 3 <= session_frame.len(),
        "opening a Review Moment must cost under a third of the Review Session \
         it belongs to: {} moment bytes against {} session bytes",
        moment_frame.len(),
        session_frame.len()
    );
}

fn completed_envelope(events: &[ReviewSessionEventEnvelope]) -> ReviewSessionEventEnvelope {
    events
        .iter()
        .find(|event| matches!(event.event, ReviewSessionEvent::Completed { .. }))
        .expect("the operation completed")
        .clone()
}
