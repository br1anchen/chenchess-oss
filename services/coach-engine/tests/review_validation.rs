use chen_chess_coach_engine::review_session_contract::{
    OperationCompletion, ReviewSessionEvent, ReviewSessionEventEnvelope,
};
use chen_chess_coach_engine::review_validation::{
    validate_game_review, validate_grounded_game_review, DraftCriticalMomentExplanation,
    DraftGameReview, DraftReviewError,
};
#[test]
fn full_game_validation_accepts_exactly_the_extracted_plies() {
    validate_game_review(&game_draft(&[1, 4]), &[1, 4])
        .expect("complete grounded draft should validate");
}

#[test]
fn full_game_validation_rejects_empty_content() {
    let mut draft = game_draft(&[1]);
    draft.verdict = "  ".to_string();

    assert_eq!(
        validate_game_review(&draft, &[1]).unwrap_err(),
        DraftReviewError::EmptyVerdict
    );
}

#[test]
fn draft_contract_rejects_removed_generic_learning_prose() {
    let result = serde_json::from_value::<DraftGameReview>(serde_json::json!({
        "verdict": "Grounded review",
        "criticalMoments": [],
        "lesson": "Generic lesson",
        "trainingPlan": ["Generic advice"],
    }));

    assert!(result.is_err());
}

#[test]
fn full_game_validation_rejects_duplicate_plies() {
    assert_eq!(
        validate_game_review(&game_draft(&[1, 1]), &[1]).unwrap_err(),
        DraftReviewError::DuplicatePly(1)
    );
}

#[test]
fn full_game_validation_rejects_missing_plies() {
    assert_eq!(
        validate_game_review(&game_draft(&[1]), &[1, 4]).unwrap_err(),
        DraftReviewError::MissingPly(4)
    );
}

#[test]
fn full_game_validation_rejects_unknown_plies() {
    assert_eq!(
        validate_game_review(&game_draft(&[1, 9]), &[1]).unwrap_err(),
        DraftReviewError::UnknownPly(9)
    );
}

#[test]
fn full_game_validation_rejects_out_of_order_plies() {
    assert_eq!(
        validate_game_review(&game_draft(&[4, 1]), &[1, 4]).unwrap_err(),
        DraftReviewError::OutOfOrderPly {
            previous: 4,
            ply: 1,
        }
    );
}

#[test]
fn grounded_validation_requires_causal_literals_and_one_uncertain_hypothesis() {
    let review = fixture_review();
    let moment = &review.critical_moments[0];
    let explanation = positive_explanation(moment);
    let mut draft = game_draft(&[usize::from(moment.ply)]);
    draft.critical_moments[0].explanation = explanation.clone();

    validate_grounded_game_review(&draft, &review).unwrap();

    for forbidden in [
        " { payoff: internal }",
        " The grounded correction is e2e4.",
        " Verified outcome: analyzed 0.0.",
        " Choose e2e4 when this same outcome is at stake.",
    ] {
        let mut invalid = game_draft(&[usize::from(moment.ply)]);
        invalid.critical_moments[0].explanation = format!("{explanation}{forbidden}");
        assert_eq!(
            validate_grounded_game_review(&invalid, &review).unwrap_err(),
            DraftReviewError::UngroundedExplanation(usize::from(moment.ply))
        );
    }

    draft.critical_moments[0].explanation = "Generic advice".to_string();
    assert_eq!(
        validate_grounded_game_review(&draft, &review).unwrap_err(),
        DraftReviewError::UngroundedExplanation(usize::from(moment.ply))
    );
}

#[test]
fn five_moment_review_requires_grounded_authoring_for_every_explanation() {
    let mut review = fixture_review();
    let base = review.critical_moments[0].clone();
    let plies = [25_u16, 29, 35, 47, 57];
    review.critical_moments = plies
        .iter()
        .map(|ply| {
            let mut moment = base.clone();
            moment.ply = *ply;
            moment.comment = None;
            moment
        })
        .collect();
    let mut draft = game_draft(&plies.map(usize::from));
    for explanation in &mut draft.critical_moments {
        let moment = review
            .critical_moments
            .iter()
            .find(|moment| usize::from(moment.ply) == explanation.ply)
            .unwrap();
        explanation.explanation = positive_explanation(moment);
    }

    validate_grounded_game_review(&draft, &review).unwrap();

    draft.critical_moments[4].explanation = "Generic Qf3 advice".to_string();
    assert_eq!(
        validate_grounded_game_review(&draft, &review).unwrap_err(),
        DraftReviewError::UngroundedExplanation(57)
    );
}

fn game_draft(plies: &[usize]) -> DraftGameReview {
    DraftGameReview {
        verdict: "Verdict".to_string(),
        critical_moments: plies.iter().copied().map(explanation).collect(),
    }
}

fn explanation(ply: usize) -> DraftCriticalMomentExplanation {
    DraftCriticalMomentExplanation {
        ply,
        explanation: format!("Explanation for ply {ply}"),
    }
}

fn positive_explanation(
    moment: &chen_chess_coach_engine::review_session_contract::GameReviewCriticalMoment,
) -> String {
    let chen_chess_coach_engine::review_session_contract::GameReviewMomentClassification::PositiveHighlight {
        ..
    } = &moment.classification
    else {
        panic!("fixture uses a Positive Highlight")
    };
    // Commentary in marker form. The explanation names no figure of its own:
    // the grade, the achievement and the difficulty are all claims the runtime
    // renders, and that is what makes the prose free.
    let _ = moment;
    "{playedMove} is {grade} here. {achievement}. {difficulty}. My best guess is that the plan may have been to improve the position.".to_string()
}

fn fixture_review() -> chen_chess_coach_engine::review_session_contract::GameReview {
    let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/coach-engine-sdk/fixtures/events.json"
    )))
    .unwrap();
    events
        .iter()
        .find_map(|event| match &event.event {
            ReviewSessionEvent::Completed { result } => match result.as_ref() {
                OperationCompletion::GameImported { review, .. } => Some(review.as_ref().clone()),
                _ => None,
            },
            _ => None,
        })
        .unwrap()
}
