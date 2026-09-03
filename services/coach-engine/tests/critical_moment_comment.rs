use std::{collections::VecDeque, fs, future::Future, path::Path, pin::Pin, sync::Mutex};

use chen_chess_coach_engine::{
    critical_moment_comment::{
        admit_hosted_review_moment_comment, author_grounded_comment, ground_draft,
        grounding_ledger_for, intent_authoring_context_for, AuthoredComment,
        CriticalMomentCommentAdmissionError, CriticalMomentCommentAuthor,
        CriticalMomentCommentAuthorInput,
    },
    language_layer_prompt::{compile_comment_prompt, CoachingProfileProjection},
    pin_verification::{PinMismatchReport, PinVerificationFailure, PinVerificationJudgement},
    review_session_contract::{
        CriticalMomentCommentDraft, CriticalMomentCommentGenerationContract,
        CriticalMomentCommentGenerationOutcome, CriticalMomentExplainerCandidate,
        CriticalMomentFactualClaim, CriticalMomentGenerationRandomness,
        CriticalMomentGenerationSettings, CriticalMomentGroundingRejection,
        CriticalMomentIntentAuthoringContext, GameReviewLineMove, GameReviewMechanismPayoff,
        GameReviewMomentClassification, GameReviewMomentProvenance, GameReviewPieceRole,
        GameReviewPlayedMoveEffect, GameReviewTacticalMechanism, ImprovementCorrection,
        ImprovementOutcome, IntentEnrichment, NeutralReviewReason, OperationCompletion,
        PositiveHighlightAchievement, ProviderUnavailableReason, ReviewMomentCommentFacts,
        ReviewSessionEvent, ReviewSessionEventEnvelope,
    },
};

/// Commentary in the model's own words, naming every fact through a marker.
///
/// The old fixtures built a "valid" draft by mutating the safe rendering,
/// which only worked because the gate demanded that skeleton. Nothing about
/// these drafts is skeletal: what makes them admissible is that no figure is
/// written and every required claim is marked.
fn positive_comment() -> &'static str {
    "{playedMove} is {grade} here, and it is {playedPopularity}. {achievement}. {difficulty}. My best guess is that the plan may have been Nf6 next, though d4 answers it."
}

fn improvement_comment() -> &'static str {
    "You played {playedMove}, which is {playedPopularity}, leaving the position at {playedEval}. {betterMove} was the stronger try, holding {bestEval}, and {consequence}. My best guess is that the idea may have been to develop, but the bigger point sits on f3. {decisionCue}"
}

fn neutral_comment() -> &'static str {
    "{playedMove} is {playedPopularity}, and it is fine: {reason}. {observation}"
}

fn comment_for(facts: &ReviewMomentCommentFacts) -> &'static str {
    match facts {
        ReviewMomentCommentFacts::Positive { .. } => positive_comment(),
        ReviewMomentCommentFacts::Improvement { .. } => improvement_comment(),
        ReviewMomentCommentFacts::Neutral { .. } => neutral_comment(),
    }
}

fn marker_draft(
    facts: &ReviewMomentCommentFacts,
    text: impl Into<String>,
) -> CriticalMomentCommentDraft {
    CriticalMomentCommentDraft {
        text: text.into(),
        grounding_ledger: grounding_ledger_for(facts),
    }
}

fn grounded_input(
    facts: ReviewMomentCommentFacts,
    intent: Option<CriticalMomentIntentAuthoringContext>,
) -> CriticalMomentCommentAuthorInput {
    CriticalMomentCommentAuthorInput::try_new(facts, intent, generation_contract()).unwrap()
}

#[tokio::test]
async fn each_tagged_kind_is_admitted_and_safe_rendered_with_a_kind_specific_ledger() {
    for facts in [positive_facts(), improvement_facts(), neutral_facts()] {
        let intent = fixture_intent(&facts);
        let grounded = author_grounded_comment(
            &ScriptedAuthor::unavailable(),
            facts.clone(),
            intent.clone(),
            false,
        )
        .await
        .unwrap();
        let text = &grounded.comment.text;
        assert_eq!(text.contains("My best guess"), intent.is_some());
        assert_eq!(text.contains("e4 e5 Nf3"), intent.is_some());
        for forbidden in ["{", "}", "grounded correction", "analyzed 0.0", "maia"] {
            assert!(
                !text.to_ascii_lowercase().contains(forbidden),
                "player-facing fallback leaked {forbidden}: {text}"
            );
        }
        if let ReviewMomentCommentFacts::Improvement { moment } = &facts {
            let GameReviewMomentClassification::ImprovementOpportunity { correction } =
                &moment.classification
            else {
                unreachable!("tagged improvement facts have an improvement classification")
            };
            assert!(text.contains(&correction.better_move_san));
            assert!(!text.contains(&correction.better_move_uci));
        }
        assert_eq!(
            grounded.authoring_provenance.outcome,
            CriticalMomentCommentGenerationOutcome::SafeRendered {
                attempts: 2,
                reason: CriticalMomentGroundingRejection::ProviderUnavailable,
                retried: false,
            }
        );
        assert!(!grounded
            .authoring_provenance
            .grounding_ledger
            .factual_claims
            .is_empty());
        assert!(grounded
            .authoring_provenance
            .is_valid_for(&grounded.comment));
    }
}

#[tokio::test]
async fn markers_are_substituted_so_every_figure_the_player_reads_was_rendered_here() {
    for facts in [positive_facts(), improvement_facts(), neutral_facts()] {
        let intent = fixture_intent(&facts);
        let draft = marker_draft(&facts, comment_for(&facts));
        let input = grounded_input(facts.clone(), intent.clone());

        let grounded = ground_draft(&input, &draft).expect("marker commentary is admissible");

        assert!(!grounded.comment.text.contains('{'));
        assert!(!grounded.comment.text.contains('}'));
        assert!(grounded.comment.text.contains(&facts.moment().played_san));
        assert!(grounded
            .comment
            .text
            .contains("the most common choice at your rating"));
        // A Positive highlight has no evaluation marker at all: the moment's
        // claim is the achievement, not the score. Where an evaluation *is* a
        // claim, the figure the Player reads came from the display facts.
        if !matches!(facts, ReviewMomentCommentFacts::Positive { .. }) {
            assert!(grounded
                .comment
                .text
                .contains(&facts.moment().display.played_evaluation.score));
        }
    }
}

#[tokio::test]
async fn the_ledger_records_the_claims_the_markers_asserted_not_the_ones_the_facts_support() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);

    let full = ground_draft(&input, &marker_draft(&facts, positive_comment())).unwrap();
    assert!(full
        .grounding_ledger
        .factual_claims
        .contains(&CriticalMomentFactualClaim::PlayedPopularity));

    let without_popularity = ground_draft(
        &input,
        &marker_draft(
            &facts,
            "{playedMove} is {grade}. {achievement}. {difficulty}. My best guess is that the plan may have been e5.",
        ),
    )
    .unwrap();
    assert!(!without_popularity
        .grounding_ledger
        .factual_claims
        .contains(&CriticalMomentFactualClaim::PlayedPopularity));
    assert!(without_popularity
        .grounding_ledger
        .factual_claims
        .contains(&CriticalMomentFactualClaim::PositiveGrade));
    assert_eq!(
        without_popularity.grounding_ledger.facts_ref,
        full.grounding_ledger.facts_ref
    );
}

#[tokio::test]
async fn a_figure_the_model_wrote_itself_is_not_expressible() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);

    for figure in ["+0.9", "-1.2", "#3", "37%"] {
        let draft = marker_draft(
            &facts,
            format!("{} The engine had it at {figure}.", positive_comment()),
        );
        assert_eq!(
            ground_draft(&input, &draft).err(),
            Some(CriticalMomentGroundingRejection::ChangedFact),
            "{figure} should not survive the gate"
        );
    }
}

#[tokio::test]
async fn unknown_repeated_and_missing_markers_are_all_rejected() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);

    assert_eq!(
        ground_draft(
            &input,
            &marker_draft(&facts, format!("{} {{bestEval}}", positive_comment()))
        )
        .err(),
        Some(CriticalMomentGroundingRejection::ChangedFact),
        "a marker this moment kind does not define is unknown"
    );
    assert_eq!(
        ground_draft(
            &input,
            &marker_draft(
                &facts,
                format!("{} And it was {{grade}} again.", positive_comment())
            )
        )
        .err(),
        Some(CriticalMomentGroundingRejection::ChangedFact),
        "a fact rendered twice reads as a stutter"
    );
    assert!(
        ground_draft(
            &input,
            &marker_draft(
                &facts,
                format!("{} And {{playedMove}} still holds.", positive_comment())
            )
        )
        .is_ok(),
        "naming the played move again is reference, not a second claim"
    );
    assert_eq!(
        ground_draft(
            &input,
            &marker_draft(
                &facts,
                "{playedMove} is {grade}. {achievement}. My best guess is that the plan may have been e5."
            )
        )
        .err(),
        Some(CriticalMomentGroundingRejection::MissingFactualClaim),
        "a required claim left unmarked is a missing claim"
    );
}

/// The opponent's resource is offered only when a fact says what the reply
/// does, renders what that is, and grounds a comment that uses it.
#[tokio::test]
async fn commentary_may_say_what_the_opponent_can_answer_with() {
    let quiet = positive_facts();
    assert!(
        !compile_comment_prompt(&quiet, None, &CoachingProfileProjection::cold_start())
            .optional_markers
            .contains(&"opponentResource".to_string()),
        "a reply that takes and attacks nothing offers no resource to name"
    );

    let facts = facts_with_opponent_resource();
    let prompt = compile_comment_prompt(&facts, None, &CoachingProfileProjection::cold_start());
    assert!(prompt
        .optional_markers
        .contains(&"opponentResource".to_string()));
    assert!(
        prompt
            .user
            .contains("Black can answer with Nf6, hitting the knight on f3"),
        "the marker renders the reply and what it does: {}",
        prompt.user
    );
    assert!(
        prompt
            .allowed_literals
            .iter()
            .any(|literal| literal == "f3"),
        "the square the fact names is quotable because the fact names it"
    );

    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    assert!(ground_draft(
        &input,
        &marker_draft(
            &facts,
            "{playedMove} is {grade} and {playedPopularity}. {achievement}. {difficulty}. Watch out though: {opponentResource}. My best guess is that the plan may have been e5.",
        )
    )
    .is_ok());
}

/// The settled material verdict reaches the Player on both tactical paths, in
/// the frame each one leaves open, and nowhere else.
///
/// The two fixtures are the two shapes the ladder actually holds: a credited
/// payoff that never sits first in the achievement list, and a missed line
/// whose mechanism nothing else narrates.
#[tokio::test]
async fn commentary_may_state_what_the_line_settles_at() {
    let outright = positive_facts();
    assert!(
        !compile_comment_prompt(&outright, None, &CoachingProfileProjection::cold_start())
            .optional_markers
            .contains(&"materialVerdict".to_string()),
        "a moment whose payoff settled at the captured piece's value has no verdict to add"
    );

    let facts = facts_with_credited_material_verdict();
    let prompt = compile_comment_prompt(&facts, None, &CoachingProfileProjection::cold_start());
    assert!(prompt
        .optional_markers
        .contains(&"materialVerdict".to_string()));
    assert!(
        prompt.user.contains("the line settles three pawns ahead"),
        "the capture is already named, so only the count is new: {}",
        prompt.user
    );

    let missed = facts_with_missed_material_verdict();
    let missed_prompt =
        compile_comment_prompt(&missed, None, &CoachingProfileProjection::cold_start());
    assert!(
        missed_prompt
            .user
            .contains("the better line wins a rook and settles three pawns ahead"),
        "nothing else says what the missed line wins: {}",
        missed_prompt.user
    );

    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    assert!(ground_draft(
        &input,
        &marker_draft(
            &facts,
            "{playedMove} is {grade} and {playedPopularity}. {achievement}. {difficulty}. Even after the recapture, {materialVerdict}. My best guess is that the plan may have been e5.",
        )
    )
    .is_ok());
    assert!(
        grounding_ledger_for(&facts)
            .factual_claims
            .contains(&CriticalMomentFactualClaim::MaterialVerdict),
        "the facts support the claim the marker asserts"
    );
}

/// A credited payoff that settled for less than the piece it won. The capture
/// is `achievements[0]` and the payoff follows it, which is the only order Rule
/// Extraction produces and the reason this verdict never reached a Player.
fn facts_with_credited_material_verdict() -> ReviewMomentCommentFacts {
    let mut moment = positive_moment();
    let GameReviewMomentClassification::PositiveHighlight { qualification, .. } =
        &mut moment.classification
    else {
        unreachable!("the fixture moment is a positive highlight");
    };
    qualification
        .achievements
        .push(PositiveHighlightAchievement::TacticalPayoff {
            payoff: GameReviewMechanismPayoff::WinsMaterialNet {
                role: GameReviewPieceRole::Rook,
                net_pawn_units: 3,
            },
        });
    ReviewMomentCommentFacts::try_from_moment(moment).unwrap()
}

/// The line the Player did not play, and what it was worth.
fn facts_with_missed_material_verdict() -> ReviewMomentCommentFacts {
    let mut moment = improvement_moment();
    let GameReviewMomentClassification::ImprovementOpportunity { correction } =
        &moment.classification
    else {
        unreachable!("the fixture moment is an improvement opportunity");
    };
    // The mechanism opens on the better move, as Rule Extraction builds it from
    // the principal variation this correction was read off.
    moment.mechanism = Some(GameReviewTacticalMechanism {
        moves: vec![GameReviewLineMove {
            uci: correction.better_move_uci.clone(),
            san: correction.better_move_san.clone(),
        }],
        forcing_index: 0,
        payoff: GameReviewMechanismPayoff::WinsMaterialNet {
            role: GameReviewPieceRole::Rook,
            net_pawn_units: 3,
        },
    });
    ReviewMomentCommentFacts::try_from_moment(moment).unwrap()
}

/// The enemy piece a move takes or newly hits reaches the Player on both
/// tactical paths, in the frame each one leaves open, and nowhere else.
///
/// The fixtures are the two populations the ladder holds: a played move whose
/// capture is already the achievement and whose attack nothing renders, and a
/// better move whose first capture or attack nothing narrates.
#[tokio::test]
async fn commentary_may_name_what_the_move_takes_or_hits() {
    let quiet = positive_facts();
    assert!(
        !compile_comment_prompt(&quiet, None, &CoachingProfileProjection::cold_start())
            .optional_markers
            .contains(&"moveTarget".to_string()),
        "a move that hits nothing has no target to name"
    );

    let facts = facts_with_played_move_target();
    let prompt = compile_comment_prompt(&facts, None, &CoachingProfileProjection::cold_start());
    assert!(prompt.optional_markers.contains(&"moveTarget".to_string()));
    assert!(
        prompt.user.contains("your move also hits the queen on d5"),
        "the capture is already the achievement, so only the attack is new: {}",
        prompt.user
    );

    let better = facts_with_better_move_target();
    let better_prompt =
        compile_comment_prompt(&better, None, &CoachingProfileProjection::cold_start());
    assert!(
        better_prompt
            .user
            .contains("the better move takes the pawn on c4"),
        "nothing else says what the better move does: {}",
        better_prompt.user
    );

    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    assert!(ground_draft(
        &input,
        &marker_draft(
            &facts,
            "{playedMove} is {grade} and {playedPopularity}. {achievement}. {difficulty}. Better still, {moveTarget}. My best guess is that the plan may have been e5.",
        )
    )
    .is_ok());
    assert!(
        grounding_ledger_for(&facts)
            .factual_claims
            .contains(&CriticalMomentFactualClaim::MoveTarget),
        "the facts support the claim the marker asserts"
    );

    // The target's square is admitted through the fact and no other way: c4
    // is on no line this fixture carries and in no move it names.
    let naming_the_square = format!(
        "{} The pawn on c4 was there for the taking.",
        improvement_comment()
    );
    let better_input = grounded_input(better.clone(), fixture_intent(&better));
    assert!(ground_draft(
        &better_input,
        &marker_draft(&better, naming_the_square.clone())
    )
    .is_ok());
    let plain = improvement_facts();
    let plain_input = grounded_input(plain.clone(), fixture_intent(&plain));
    assert_eq!(
        ground_draft(&plain_input, &marker_draft(&plain, naming_the_square)).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact),
        "without the fact, the square is an invention"
    );
}

/// A played move that also hits something. Its capture, when it has one, is
/// already the achievement; the attack is the effect no rendering reads.
fn facts_with_played_move_target() -> ReviewMomentCommentFacts {
    let mut moment = positive_moment();
    moment
        .effects
        .push(GameReviewPlayedMoveEffect::AttackedPiece {
            role: GameReviewPieceRole::Queen,
            square: "d5".to_string(),
        });
    ReviewMomentCommentFacts::try_from_moment(moment).unwrap()
}

/// The better move takes something, and nothing else says so.
fn facts_with_better_move_target() -> ReviewMomentCommentFacts {
    let mut moment = improvement_moment();
    let lines = moment
        .objective
        .lines
        .as_mut()
        .expect("the fixture moment carries objective lines");
    lines.best_move_effects = vec![GameReviewPlayedMoveEffect::CapturedPiece {
        role: GameReviewPieceRole::Pawn,
        square: "c4".to_string(),
    }];
    ReviewMomentCommentFacts::try_from_moment(moment).unwrap()
}

/// The fixture moment with an opponent reply that hits something. The fixture's
/// own reply is a quiet pawn move, which is the other half of the test above.
fn facts_with_opponent_resource() -> ReviewMomentCommentFacts {
    let mut moment = positive_moment();
    let lines = moment
        .objective
        .lines
        .as_mut()
        .expect("the fixture moment carries objective lines");
    lines.refutation_effects = vec![GameReviewPlayedMoveEffect::AttackedPiece {
        role: GameReviewPieceRole::Knight,
        square: "f3".to_string(),
    }];
    ReviewMomentCommentFacts::try_from_moment(moment).unwrap()
}

#[tokio::test]
async fn commentary_may_name_a_square_and_quote_the_engine_line() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);

    // Both of these were impossible before the allowlist was projected
    // deliberately: `c2` never appeared as a whitespace-split token, and the
    // engine line was stored as UCI that never parses as SAN.
    let draft = marker_draft(
        &facts,
        "{playedMove} is {grade} and {playedPopularity}: the pawn steps from c2. {achievement}. {difficulty}. The engine keeps going Nf6 d4 d6. My best guess is that the plan may have been exactly that.",
    );
    assert!(ground_draft(&input, &draft).is_ok());

    let invented_square = marker_draft(
        &facts,
        "{playedMove} is {grade} and {playedPopularity}: the pawn steps from h5. {achievement}. {difficulty}. My best guess is that the plan may have been e5.",
    );
    assert_eq!(
        ground_draft(&input, &invented_square).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );
}

#[tokio::test]
async fn naming_the_human_move_model_to_the_player_is_a_rejection_not_a_style_miss() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);

    for leak in ["Maia", "the human model", "the move model", "human-likely"] {
        let draft = marker_draft(&facts, format!("{} {leak} agrees.", positive_comment()));
        assert_eq!(
            ground_draft(&input, &draft).err(),
            Some(CriticalMomentGroundingRejection::ChangedFact),
            "{leak} must not reach a Player"
        );
    }
}

#[tokio::test]
async fn one_failure_retries_byte_identical_tagged_input_then_admits_the_same_grounded_draft() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let draft = marker_draft(&facts, positive_comment());
    let author = ScriptedAuthor::new([
        Err(ProviderUnavailableReason::LanguageLayer),
        Ok(AuthoredComment::without_pin_check(draft)),
    ]);
    let grounded = author_grounded_comment(&author, facts, intent, false)
        .await
        .unwrap();
    assert_eq!(
        grounded.authoring_provenance.outcome,
        CriticalMomentCommentGenerationOutcome::Authored { attempts: 2 }
    );
    assert!(!grounded.comment.text.contains('{'));
    let inputs = author.inputs.lock().unwrap();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0], inputs[1]);
}

#[test]
fn tagged_boundary_rejects_cross_kind_and_malformed_classification_facts() {
    let positive = positive_moment();
    let cross_kind = ReviewMomentCommentFacts::Neutral { moment: positive };
    assert!(
        CriticalMomentCommentAuthorInput::try_new(cross_kind, None, generation_contract()).is_err()
    );

    let mut malformed = positive_moment();
    malformed.classification = GameReviewMomentClassification::Neutral { reasons: vec![] };
    assert_eq!(
        ReviewMomentCommentFacts::try_from_moment(malformed).unwrap_err(),
        chen_chess_coach_engine::review_session_contract::ReviewMomentCommentFactsError::MalformedClassification,
    );
}

#[tokio::test]
async fn invalid_classification_facts_fail_closed_before_the_language_layer() {
    let bad_facts = ReviewMomentCommentFacts::Positive {
        moment: improvement_moment(),
    };
    assert_eq!(
        author_grounded_comment(&ScriptedAuthor::unavailable(), bad_facts, None, false)
            .await
            .unwrap_err(),
        CriticalMomentCommentAdmissionError::InvalidClassificationFacts,
    );
}

#[tokio::test]
async fn grounding_gate_rejects_cross_kind_multi_paragraph_and_authoritative_drafts() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    let mut draft = marker_draft(&facts, positive_comment());

    draft.grounding_ledger.factual_claims.push(
        chen_chess_coach_engine::review_session_contract::CriticalMomentFactualClaim::NeutralReason,
    );
    assert_eq!(
        ground_draft(&input, &draft).err(),
        Some(CriticalMomentGroundingRejection::ChangedReference)
    );
    draft.grounding_ledger.factual_claims.pop();
    draft.text.push('\n');
    assert_eq!(
        ground_draft(&input, &draft).err(),
        Some(CriticalMomentGroundingRejection::MultiParagraph)
    );
    draft.text.pop();
    draft.text = draft.text.replacen(
        "My best guess is that the plan",
        "You definitely planned this",
        1,
    );
    assert_eq!(
        ground_draft(&input, &draft).err(),
        Some(CriticalMomentGroundingRejection::AuthoritativeIntent)
    );
}

#[tokio::test]
async fn grounding_gate_rejects_invented_san_without_semantically_judging_the_hypothesis() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);

    assert!(ground_draft(
        &input,
        &marker_draft(
            &facts,
            "{playedMove} is {grade} and {playedPopularity}. {achievement}. {difficulty}. My best guess is that the plan possibly aimed at e5.",
        )
    )
    .is_ok());

    assert_eq!(
        ground_draft(
            &input,
            &marker_draft(
                &facts,
                "{playedMove} is {grade} and {playedPopularity}. {achievement}. {difficulty}. My best guess is that the plan possibly aimed at Nh8.",
            )
        )
        .err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );
}

#[tokio::test]
async fn learning_prose_is_optional_but_changed_resource_literals_and_urls_fail() {
    let facts = fork_learning_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    let mut draft = marker_draft(&facts, positive_comment());

    assert!(ground_draft(&input, &draft).is_ok());
    draft.text.push_str(
        " Learn: The Fork (https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p).",
    );
    assert!(ground_draft(&input, &draft).is_ok());

    let changed_title = CriticalMomentCommentDraft {
        text: draft.text.replace("Learn: The Fork", "Learn: Fork Tactics"),
        grounding_ledger: draft.grounding_ledger.clone(),
    };
    assert_eq!(
        ground_draft(&input, &changed_title).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );

    let changed_title_without_url = CriticalMomentCommentDraft {
        text: draft.text.replace(
            "Learn: The Fork (https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p)",
            "Learn: Fork Tactics",
        ),
        grounding_ledger: draft.grounding_ledger.clone(),
    };
    assert_eq!(
        ground_draft(&input, &changed_title_without_url).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );

    let changed_url = CriticalMomentCommentDraft {
        text: draft.text.replace("Qj281y1p", "wrong-module"),
        grounding_ledger: draft.grounding_ledger,
    };
    assert_eq!(
        ground_draft(&input, &changed_url).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );
}

#[tokio::test]
async fn player_selected_material_uses_the_same_grounding_and_url_boundary() {
    let facts = player_selected_fork_learning_facts();
    assert_eq!(
        facts.moment().provenance,
        GameReviewMomentProvenance::PlayerSelected
    );
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    let mut draft = marker_draft(&facts, positive_comment());
    draft.text.push_str(
        " Learn: The Fork (https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p). Drill: Fork (https://lichess.org/training/fork).",
    );
    assert!(ground_draft(&input, &draft).is_ok());

    draft.text = draft.text.replace("training/fork", "training/discovered");
    assert_eq!(
        ground_draft(&input, &draft).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );
}

#[tokio::test]
async fn neutral_player_selected_facts_reject_generic_learning_advice() {
    let facts = neutral_facts();
    let input = grounded_input(facts.clone(), None);
    let draft = marker_draft(
        &facts,
        format!("{} Learn: Generic tactics.", neutral_comment()),
    );

    assert_eq!(
        ground_draft(&input, &draft).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );
}

#[tokio::test]
async fn drill_only_hanging_piece_material_is_grounded_without_a_learn_resource() {
    let facts = hanging_piece_learning_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    let mut draft = marker_draft(&facts, positive_comment());

    draft
        .text
        .push_str(" Drill: Hanging piece (https://lichess.org/training/hangingPiece).");
    assert!(ground_draft(&input, &draft).is_ok());

    let invented_learn_role = CriticalMomentCommentDraft {
        text: draft
            .text
            .replace("Drill: Hanging piece", "Learn: Hanging piece"),
        grounding_ledger: draft.grounding_ledger,
    };
    assert_eq!(
        ground_draft(&input, &invented_learn_role).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );
}

#[tokio::test]
async fn drill_only_passed_pawn_material_is_grounded_without_a_learn_resource() {
    let facts = passed_pawn_learning_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    let mut draft = marker_draft(&facts, positive_comment());

    draft
        .text
        .push_str(" Drill: Promotion (https://lichess.org/training/promotion).");
    assert!(ground_draft(&input, &draft).is_ok());

    let invented_learn_role = CriticalMomentCommentDraft {
        text: draft.text.replace("Drill: Promotion", "Learn: Promotion"),
        grounding_ledger: draft.grounding_ledger,
    };
    assert_eq!(
        ground_draft(&input, &invented_learn_role).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );
}

#[tokio::test]
async fn opening_material_preserves_exact_roles_titles_and_urls_in_grounding() {
    let facts = opening_learning_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    let mut draft = marker_draft(&facts, improvement_comment());

    draft.text.push_str(
        " Learn: Vienna Game: Anderssen Defense (https://lichess.org/opening/Vienna_Game_Anderssen_Defense). Drill: Vienna Game: Anderssen Defense puzzles (https://lichess.org/training/Vienna_Game_Anderssen_Defense).",
    );
    assert!(ground_draft(&input, &draft).is_ok());

    let changed_title = CriticalMomentCommentDraft {
        text: draft
            .text
            .replace("Learn: Vienna", "Learn: Unverified Vienna"),
        grounding_ledger: draft.grounding_ledger,
    };
    assert_eq!(
        ground_draft(&input, &changed_title).err(),
        Some(CriticalMomentGroundingRejection::ChangedFact)
    );
}

#[test]
fn hosted_admission_rejects_bad_ledgers_and_never_returns_unpublished_prose() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let ledger = grounding_ledger_for(&facts);
    let invalid_prose = "Host prose that has not passed the grounding gate.".to_string();
    let safe = admit_hosted_review_moment_comment(
        &facts,
        intent.as_ref(),
        &CriticalMomentCommentDraft {
            text: invalid_prose.clone(),
            grounding_ledger: ledger.clone(),
        },
    )
    .unwrap();
    let comment = safe;
    assert_ne!(comment.text, invalid_prose);
    assert!(comment.text.starts_with("Good:") || comment.text.starts_with("Great:"));

    let mut cross_kind = ledger;
    cross_kind.factual_claims.push(
        chen_chess_coach_engine::review_session_contract::CriticalMomentFactualClaim::NeutralReason,
    );
    assert_eq!(
        admit_hosted_review_moment_comment(
            &facts,
            intent.as_ref(),
            &CriticalMomentCommentDraft {
                text: comment.text,
                grounding_ledger: cross_kind,
            },
        ),
        Err(CriticalMomentGroundingRejection::ChangedReference)
    );
}

#[test]
fn admission_publishes_the_substituted_comment_never_the_marker_form() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);

    let comment = admit_hosted_review_moment_comment(
        &facts,
        intent.as_ref(),
        &marker_draft(&facts, positive_comment()),
    )
    .unwrap();

    assert!(!comment.text.contains("{playedMove}"));
    assert!(comment.text.starts_with("c3 is a good move"));
}

#[tokio::test]
async fn neutral_terminal_outcomes_remain_one_paragraph_without_an_intent_hypothesis() {
    let mut terminal = neutral_moment();
    terminal.played_move_outcome =
        chen_chess_coach_engine::review_session_contract::PlayedMoveOutcomeEvidence::Terminal {
            outcome:
                chen_chess_coach_engine::review_session_contract::BoardTerminalOutcome::Stalemate,
        };
    let grounded = author_grounded_comment(
        &ScriptedAuthor::unavailable(),
        ReviewMomentCommentFacts::Neutral { moment: terminal },
        None,
        false,
    )
    .await
    .unwrap();
    assert!(!grounded.comment.text.contains('\n'));
    assert!(grounded
        .comment
        .text
        .contains("the game ended in stalemate"));
    assert!(!grounded.comment.text.contains("My best guess"));
}

#[tokio::test]
async fn prose_that_never_guesses_is_given_the_guess_rather_than_refused() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let input = grounded_input(facts.clone(), intent);
    // Every fact marked, nothing overclaimed, and no sentence guessing at a
    // plan — the shape the model writes when the required markers crowd the
    // paragraph.
    let draft = marker_draft(
        &facts,
        "{playedMove} is {grade} and {playedPopularity}. {achievement}. {difficulty}.",
    );

    let grounded = ground_draft(&input, &draft).unwrap();

    assert_eq!(grounded.comment.text.matches("My best guess").count(), 1);
    assert!(grounded.comment.text.contains("may have been aiming"));
    // The model's own prose survives ahead of it, which is the whole point:
    // refusing sent the moment to the safe rendering instead.
    assert!(grounded.comment.text.starts_with("c3 is"));
    assert!(!grounded.comment.text.contains('\n'));
}

#[tokio::test]
async fn a_second_guess_and_a_guess_where_none_was_projected_are_both_still_refused() {
    let facts = positive_facts();
    let twice = format!(
        "{} My best guess is that the plan may have been Nf3 instead.",
        positive_comment()
    );
    assert_eq!(
        ground_draft(
            &grounded_input(facts.clone(), fixture_intent(&facts)),
            &marker_draft(&facts, twice)
        )
        .err(),
        Some(CriticalMomentGroundingRejection::MultipleIntentClaims)
    );
    assert_eq!(
        ground_draft(
            &grounded_input(facts.clone(), None),
            &marker_draft(&facts, positive_comment())
        )
        .err(),
        Some(CriticalMomentGroundingRejection::UnexpectedIntentHypothesis)
    );
}

#[tokio::test]
async fn unavailable_enrichment_keeps_grounded_facts_and_one_uncertain_hypothesis() {
    let facts = improvement_facts();
    let intent = intent_authoring_context_for(&facts, None);
    let grounded = author_grounded_comment(&ScriptedAuthor::unavailable(), facts, intent, false)
        .await
        .unwrap();

    assert_eq!(grounded.comment.text.matches("My best guess").count(), 1);
    assert!(grounded.comment.text.contains("may have been aiming"));
    assert!(!grounded.comment.text.contains("projected"));
    assert!(!grounded.comment.text.contains("probability"));
}

#[tokio::test]
async fn a_pin_mismatch_still_publishes_grounded_hosted_prose() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let hosted = marker_draft(&facts, positive_comment());
    let mismatch = PinVerificationJudgement::Mismatched(PinMismatchReport {
        pinned_model: "google/gemini-3.5-flash-lite-20260721".into(),
        pinned_provider_family: "google-vertex".into(),
        observed_permaslug: Some("other/model".into()),
        observed_provider: Some("Amazon Bedrock".into()),
        observed_provider_family: Some("amazon-bedrock".into()),
        served_endpoint: Some("ep-1".into()),
        served_region: Some("global".into()),
        routed_service_tier: None,
    });
    let author = ScriptedAuthor::new([Ok(AuthoredComment::with_pin_judgement(hosted, mismatch))]);
    let grounded = author_grounded_comment(&author, facts, intent, false)
        .await
        .unwrap();
    assert_eq!(
        grounded.authoring_provenance.outcome,
        CriticalMomentCommentGenerationOutcome::Authored { attempts: 1 },
    );
    assert!(!grounded.comment.text.contains('{'));
    assert!(grounded
        .comment
        .text
        .contains("the most common choice at your rating"));
    assert_eq!(author.inputs.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_verify_deadline_miss_still_publishes_grounded_hosted_prose() {
    let facts = positive_facts();
    let intent = fixture_intent(&facts);
    let hosted = marker_draft(&facts, positive_comment());
    let failed = PinVerificationJudgement::Failed(PinVerificationFailure::DeadlineMissed);
    let author = ScriptedAuthor::new([Ok(AuthoredComment::with_pin_judgement(hosted, failed))]);
    let grounded = author_grounded_comment(&author, facts, intent, false)
        .await
        .unwrap();
    assert_eq!(
        grounded.authoring_provenance.outcome,
        CriticalMomentCommentGenerationOutcome::Authored { attempts: 1 },
    );
    assert!(!grounded.comment.text.contains('{'));
    assert!(grounded
        .comment
        .text
        .contains("the most common choice at your rating"));
}

struct ScriptedAuthor {
    responses: Mutex<VecDeque<Result<AuthoredComment, ProviderUnavailableReason>>>,
    inputs: Mutex<Vec<CriticalMomentCommentAuthorInput>>,
}

impl ScriptedAuthor {
    fn new(
        responses: impl IntoIterator<Item = Result<AuthoredComment, ProviderUnavailableReason>>,
    ) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
            inputs: Mutex::new(Vec::new()),
        }
    }
    fn unavailable() -> Self {
        Self::new([
            Err(ProviderUnavailableReason::LanguageLayer),
            Err(ProviderUnavailableReason::LanguageLayer),
        ])
    }
}

impl CriticalMomentCommentAuthor for ScriptedAuthor {
    fn generation_contract(&self) -> CriticalMomentCommentGenerationContract {
        generation_contract()
    }
    fn author<'a>(
        &'a self,
        input: CriticalMomentCommentAuthorInput,
    ) -> Pin<Box<dyn Future<Output = Result<AuthoredComment, ProviderUnavailableReason>> + Send + 'a>>
    {
        self.inputs.lock().unwrap().push(input);
        let response = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("one response per call");
        Box::pin(async move { response })
    }
}

fn positive_facts() -> ReviewMomentCommentFacts {
    ReviewMomentCommentFacts::try_from_moment(positive_moment()).unwrap()
}
fn improvement_facts() -> ReviewMomentCommentFacts {
    ReviewMomentCommentFacts::try_from_moment(improvement_moment()).unwrap()
}
fn neutral_facts() -> ReviewMomentCommentFacts {
    ReviewMomentCommentFacts::try_from_moment(neutral_moment()).unwrap()
}

fn fork_learning_facts() -> ReviewMomentCommentFacts {
    let mut moment = positive_moment();
    let critical_moment_id = moment.critical_moment_id.as_str();
    moment.learning_material = serde_json::from_value(serde_json::json!({
        "selectionPolicyVersion": "learning-plan-selection/v1",
        "resourceCatalogVersion": "learning-resources/2026-08-03",
        "tracks": [{
            "key": { "kind": "curriculum", "concept": "fork" },
            "support": [{
                "purpose": "reinforcement",
                "learningPathRef": "learning-path:fixture-fork",
                "criticalMomentId": critical_moment_id,
                "ply": moment.ply,
                "basis": {
                    "kind": "decisionExplanation",
                    "explanationPathRef": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                }
            }],
            "resources": [
                {
                    "resourceId": "lichess:practice:Qj281y1p",
                    "role": "learn",
                    "kind": "practiceModule",
                    "title": "The Fork",
                    "canonicalUrl": "https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p"
                },
                {
                    "resourceId": "lichess:puzzles:fork",
                    "role": "drill",
                    "kind": "puzzleStream",
                    "title": "Fork",
                    "canonicalUrl": "https://lichess.org/training/fork"
                }
            ]
        }]
    }))
    .expect("Fork learning material fixture should match the public contract");
    assert_eq!(moment.learning_material.tracks.len(), 1);
    ReviewMomentCommentFacts::try_from_moment(moment).unwrap()
}

fn player_selected_fork_learning_facts() -> ReviewMomentCommentFacts {
    let mut facts = fork_learning_facts();
    match &mut facts {
        ReviewMomentCommentFacts::Positive { moment }
        | ReviewMomentCommentFacts::Improvement { moment }
        | ReviewMomentCommentFacts::Neutral { moment } => {
            moment.provenance = GameReviewMomentProvenance::PlayerSelected;
        }
    }
    facts
}

fn hanging_piece_learning_facts() -> ReviewMomentCommentFacts {
    let mut moment = positive_moment();
    let critical_moment_id = moment.critical_moment_id.as_str();
    moment.learning_material = serde_json::from_value(serde_json::json!({
        "selectionPolicyVersion": "learning-plan-selection/v1",
        "resourceCatalogVersion": "learning-resources/2026-08-03",
        "tracks": [{
            "key": { "kind": "curriculum", "concept": "hangingPiece" },
            "support": [{
                "purpose": "reinforcement",
                "learningPathRef": "learning-path:fixture-hanging-piece",
                "criticalMomentId": critical_moment_id,
                "ply": moment.ply,
                "basis": {
                    "kind": "decisionExplanation",
                    "explanationPathRef": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                }
            }],
            "resources": [{
                "resourceId": "lichess:puzzles:hangingPiece",
                "role": "drill",
                "kind": "puzzleStream",
                "title": "Hanging piece",
                "canonicalUrl": "https://lichess.org/training/hangingPiece"
            }]
        }]
    }))
    .expect("Hanging Piece learning material fixture should match the public contract");
    ReviewMomentCommentFacts::try_from_moment(moment).unwrap()
}

fn passed_pawn_learning_facts() -> ReviewMomentCommentFacts {
    let mut moment = positive_moment();
    let critical_moment_id = moment.critical_moment_id.as_str();
    moment.learning_material = serde_json::from_value(serde_json::json!({
        "selectionPolicyVersion": "learning-plan-selection/v1",
        "resourceCatalogVersion": "learning-resources/2026-08-03",
        "tracks": [{
            "key": { "kind": "curriculum", "concept": "promotion" },
            "support": [{
                "purpose": "reinforcement",
                "learningPathRef": "learning-path:fixture-passed-pawn",
                "criticalMomentId": critical_moment_id,
                "ply": moment.ply,
                "basis": {
                    "kind": "decisionExplanation",
                    "explanationPathRef": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                }
            }],
            "resources": [{
                "resourceId": "lichess:puzzles:promotion",
                "role": "drill",
                "kind": "puzzleStream",
                "title": "Promotion",
                "canonicalUrl": "https://lichess.org/training/promotion"
            }]
        }]
    }))
    .expect("Passed Pawn learning material fixture should match the public contract");
    ReviewMomentCommentFacts::try_from_moment(moment).unwrap()
}

fn opening_learning_facts() -> ReviewMomentCommentFacts {
    let mut moment = improvement_moment();
    let critical_moment_id = moment.critical_moment_id.as_str();
    moment.learning_material = serde_json::from_value(serde_json::json!({
        "selectionPolicyVersion": "learning-plan-selection/v1",
        "resourceCatalogVersion": "learning-resources/2026-08-03",
        "tracks": [{
            "key": {
                "kind": "opening",
                "resourceMappingId": "lichess:opening:vienna-game-anderssen-defense"
            },
            "support": [{
                "purpose": "improvement",
                "learningPathRef": "learning-path:fixture-opening",
                "criticalMomentId": critical_moment_id,
                "ply": moment.ply,
                "basis": {
                    "kind": "opening",
                    "evidence": {
                        "positionPhase": {
                            "policyVersion": "position-phase/v1",
                            "phase": "opening"
                        },
                        "openingIdentification": {
                            "kind": "present",
                            "eco": "C25",
                            "name": "Vienna Game: Giraffe Attack",
                            "provenance": {
                                "kind": "catalog",
                                "catalogVersion": "chess-openings/2026.04.16",
                                "matchedPly": 5
                            }
                        },
                        "resourceMappingId": "lichess:opening:vienna-game-anderssen-defense"
                    }
                }
            }],
            "resources": [
                {
                    "resourceId": "lichess:opening-reference:Vienna_Game_Anderssen_Defense",
                    "role": "learn",
                    "kind": "openingReference",
                    "title": "Vienna Game: Anderssen Defense",
                    "canonicalUrl": "https://lichess.org/opening/Vienna_Game_Anderssen_Defense"
                },
                {
                    "resourceId": "lichess:opening-puzzles:Vienna_Game_Anderssen_Defense",
                    "role": "drill",
                    "kind": "openingPuzzleStream",
                    "title": "Vienna Game: Anderssen Defense puzzles",
                    "canonicalUrl": "https://lichess.org/training/Vienna_Game_Anderssen_Defense"
                }
            ]
        }]
    }))
    .expect("Opening learning material fixture should match the public contract");
    ReviewMomentCommentFacts::try_from_moment(moment).unwrap()
}

fn positive_moment() -> chen_chess_coach_engine::review_session_contract::GameReviewCriticalMoment {
    let mut moment = imported_review().critical_moments[0].clone();
    moment.comment = None;
    moment
}

fn improvement_moment() -> chen_chess_coach_engine::review_session_contract::GameReviewCriticalMoment
{
    let mut moment = positive_moment();
    moment.classification = GameReviewMomentClassification::ImprovementOpportunity {
        correction: ImprovementCorrection {
            better_move_uci: moment.objective.best_move_uci.clone(),
            better_move_san: moment
                .objective
                .lines
                .as_ref()
                .and_then(|lines| lines.best.first())
                .map(|line_move| line_move.san.clone())
                .unwrap_or_else(|| "Nf3".to_string()),
            outcome: ImprovementOutcome::ImprovedAnalyzed {
                better_evaluation: moment.objective.best_evaluation.clone(),
            },
        },
    };
    moment
}

fn neutral_moment() -> chen_chess_coach_engine::review_session_contract::GameReviewCriticalMoment {
    let mut moment = positive_moment();
    moment.classification = GameReviewMomentClassification::Neutral {
        reasons: vec![NeutralReviewReason::SoundWithoutConcreteAchievement],
    };
    moment
}

fn fixture_intent(
    facts: &ReviewMomentCommentFacts,
) -> Option<CriticalMomentIntentAuthoringContext> {
    intent_authoring_context_for(
        facts,
        Some(IntentEnrichment {
            projected_plan_san: vec!["e4".to_string(), "e5".to_string(), "Nf3".to_string()],
            objective_counterplay_san: vec!["c5".to_string(), "Nf3".to_string()],
        }),
    )
}

fn imported_review() -> chen_chess_coach_engine::review_session_contract::GameReview {
    let events: Vec<ReviewSessionEventEnvelope> = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages/coach-engine-sdk/fixtures/events.json"),
        )
        .unwrap(),
    )
    .unwrap();
    events
        .into_iter()
        .find_map(|event| match event.event {
            ReviewSessionEvent::Completed { result } => match *result {
                OperationCompletion::GameImported { review, .. } => Some(*review),
                _ => None,
            },
            _ => None,
        })
        .unwrap()
}

fn generation_contract() -> CriticalMomentCommentGenerationContract {
    CriticalMomentCommentGenerationContract {
        code_revision: "test-code-revision".to_string(),
        candidate: CriticalMomentExplainerCandidate::new(
            "test-provider".to_string(),
            "test-model".to_string(),
            "2026-07-19".to_string(),
            digest('4'),
            digest('5'),
        ),
        settings: CriticalMomentGenerationSettings {
            randomness: CriticalMomentGenerationRandomness::LowestSupported,
            stable_seed: Some(82),
            seed_supported: true,
            max_output_tokens: 512,
        },
    }
}

fn digest(digit: char) -> chen_chess_coach_engine::review_session_contract::ArtifactDigest {
    chen_chess_coach_engine::review_session_contract::ArtifactDigest::try_from(format!(
        "sha256:{}",
        digit.to_string().repeat(64)
    ))
    .unwrap()
}
