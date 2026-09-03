//! Unit tests for [`FactShape`](super::FactShape) derivation.

use std::sync::OnceLock;

use serde_json::Value;

use super::*;
use crate::review_session_contract::{
    BoardTerminalOutcome, Color, EloRelativeQualificationReason, EloRelativeStrength,
    GameReviewCriticalMoment, GameReviewPieceRole, GameReviewPlayedMoveEffect,
    GameReviewTeachingTheme, ImprovementCorrection, ObjectiveExcellenceReason,
    PositiveHighlightQualification,
};

/// The generated SDK fixture: a Positive Highlight, good, objective-only
/// qualification, `advancedPassedPawn`, standing kept, analyzed, no teaching
/// theme, and a played move the human model ranked.
fn base_moment() -> GameReviewCriticalMoment {
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

fn shape_of(moment: GameReviewCriticalMoment) -> FactShape {
    FactShape::of(
        &ReviewMomentCommentFacts::try_from_presented_moment(moment)
            .expect("the fixture moment is authorable"),
    )
}

fn objective_only() -> Vec<PositiveHighlightQualificationReason> {
    vec![PositiveHighlightQualificationReason::Objective {
        reason: ObjectiveExcellenceReason::ExactBestMajorAchievement,
    }]
}

fn objective_and_elo(strength: EloRelativeStrength) -> Vec<PositiveHighlightQualificationReason> {
    let mut reasons = objective_only();
    reasons.push(PositiveHighlightQualificationReason::EloRelative {
        reason: EloRelativeQualificationReason::RarePlayedMoveRank,
        strength,
    });
    reasons
}

fn positive(
    grade: PositiveHighlightGrade,
    reasons: Vec<PositiveHighlightQualificationReason>,
    achievement: PositiveHighlightAchievement,
) -> GameReviewCriticalMoment {
    let mut moment = base_moment();
    moment.classification = GameReviewMomentClassification::PositiveHighlight {
        qualification: PositiveHighlightQualification {
            reasons,
            achievements: vec![achievement],
        },
        grade,
    };
    moment
}

fn captured_knight() -> PositiveHighlightAchievement {
    PositiveHighlightAchievement::CapturedPiece {
        role: GameReviewPieceRole::Knight,
        square: "f6".to_string(),
    }
}

fn good_capture() -> GameReviewCriticalMoment {
    positive(
        PositiveHighlightGrade::Good,
        objective_only(),
        captured_knight(),
    )
}

fn improvement(residual: GameReviewResidualClassification) -> GameReviewCriticalMoment {
    let mut moment = base_moment();
    moment.classification = GameReviewMomentClassification::ImprovementOpportunity {
        correction: ImprovementCorrection {
            better_move_uci: "e2e4".to_string(),
            better_move_san: "e4".to_string(),
            outcome: ImprovementOutcome::ImprovedAnalyzed {
                better_evaluation: moment.objective.best_evaluation.clone(),
            },
        },
    };
    moment.residual_outcome.classification = residual;
    moment
}

fn neutral(reasons: Vec<NeutralReviewReason>) -> GameReviewCriticalMoment {
    let mut moment = base_moment();
    moment.classification = GameReviewMomentClassification::Neutral { reasons };
    moment
}

/// The premise the whole design rests on: the model is a renderer, so the
/// chess that produced a moment is not what distinguishes one test input
/// from another. Two captures of different pieces on different squares,
/// with different evaluations and a different played move, are one shape.
#[test]
fn the_chess_a_moment_is_made_of_does_not_change_its_shape() {
    let mut other = good_capture();
    other.played_san = "Bxg5".to_string();
    other.ply = 41;
    other.classification = GameReviewMomentClassification::PositiveHighlight {
        qualification: PositiveHighlightQualification {
            reasons: objective_only(),
            achievements: vec![PositiveHighlightAchievement::CapturedPiece {
                role: GameReviewPieceRole::Rook,
                square: "a1".to_string(),
            }],
        },
        grade: PositiveHighlightGrade::Good,
    };

    assert_eq!(shape_of(good_capture()), shape_of(other));
}

/// Measured over the pinned corpus, `Positive / good / capturedPiece / pop`
/// — the most common shape in production — is nine moments without a
/// takeaway and four with. The taxonomy the frozen set used could not see
/// that split, so one exemplar stood for two authoring problems.
#[test]
fn a_takeaway_is_a_different_authoring_problem() {
    let mut with_takeaway = good_capture();
    with_takeaway.teaching.themes = vec![GameReviewTeachingTheme::PassedPawnPromotion];

    let without = shape_of(good_capture());
    let with = shape_of(with_takeaway);

    assert_ne!(without, with);
    assert_eq!(without.difference(&with), vec![ShapeAxis::MarkerSlots]);
}

/// `{playedPopularity}` is offered only when the human move model ranked the
/// played move. It is the `pop` axis, and it is a marker slot rather than a
/// discriminant.
#[test]
fn an_unranked_played_move_withdraws_the_popularity_slot() {
    let mut unranked = good_capture();
    unranked.human.played_move_rank = None;

    let ranked = shape_of(good_capture());
    let unranked = shape_of(unranked);

    assert!(ranked
        .markers()
        .iter()
        .any(|slot| slot.marker == "playedPopularity" && !slot.required));
    assert!(unranked
        .markers()
        .iter()
        .all(|slot| slot.marker != "playedPopularity"));
    assert_eq!(ranked.difference(&unranked), vec![ShapeAxis::MarkerSlots]);
}

/// The finding that changed the key. `improvement_correction_marker_text`
/// renders a missed forced mate and a centipawn correction through the same
/// `ImprovedAnalyzed` arm, so without the residual outcome the shape cannot
/// tell them apart — and `Improvement / mate` is the combination #534 is
/// named after.
#[test]
fn the_residual_outcome_separates_a_missed_mate_from_a_centipawn_correction() {
    let missed_mate = shape_of(improvement(
        GameReviewResidualClassification::MissedForcedMate,
    ));
    let advantage_lost = shape_of(improvement(GameReviewResidualClassification::AdvantageLost));

    assert_ne!(missed_mate, advantage_lost);
    assert_eq!(
        missed_mate.difference(&advantage_lost),
        vec![ShapeAxis::Residual]
    );
    assert!(missed_mate.id().to_string().contains("missedForcedMate"));
}

#[test]
fn the_grade_moves_the_shape() {
    let good = shape_of(good_capture());
    let great = shape_of(positive(
        PositiveHighlightGrade::Great,
        objective_and_elo(EloRelativeStrength::Strong),
        captured_knight(),
    ));

    assert_eq!(
        good.difference(&great),
        vec![ShapeAxis::Grade, ShapeAxis::EloRelative]
    );
}

/// `positive_difficulty_text` renders "This required precise play." for a
/// Good highlight with no Elo-relative reason and "This was a notable find
/// for players at your rating." when one is present. Same grade, same
/// achievement, different required sentence.
#[test]
fn an_elo_relative_reason_alone_moves_a_good_highlight() {
    let objective = shape_of(good_capture());
    let notable = shape_of(positive(
        PositiveHighlightGrade::Good,
        objective_and_elo(EloRelativeStrength::Notable),
        captured_knight(),
    ));

    assert_eq!(objective.difference(&notable), vec![ShapeAxis::EloRelative]);
}

#[test]
fn the_achievement_variant_moves_the_shape_and_its_fields_do_not() {
    let captured = shape_of(good_capture());
    let advanced = shape_of(positive(
        PositiveHighlightGrade::Good,
        objective_only(),
        PositiveHighlightAchievement::AdvancedPassedPawn {
            to_square: "g6".to_string(),
        },
    ));

    assert_eq!(captured.difference(&advanced), vec![ShapeAxis::Achievement]);
}

/// A tactical payoff carries its own variant beneath the achievement, and
/// the two material payoffs render different sentences — "won a knight"
/// against "won a knight and came out 2 pawns ahead".
#[test]
fn a_tactical_payoff_carries_its_own_variant() {
    let outright = shape_of(positive(
        PositiveHighlightGrade::Good,
        objective_only(),
        PositiveHighlightAchievement::TacticalPayoff {
            payoff: GameReviewMechanismPayoff::WinsMaterialOutright {
                role: GameReviewPieceRole::Knight,
            },
        },
    ));
    let net = shape_of(positive(
        PositiveHighlightGrade::Good,
        objective_only(),
        PositiveHighlightAchievement::TacticalPayoff {
            payoff: GameReviewMechanismPayoff::WinsMaterialNet {
                role: GameReviewPieceRole::Knight,
                net_pawn_units: 2,
            },
        },
    ));

    // Two axes, not one. The payoff variant is the frame `{achievement}`
    // renders, and a net payoff additionally offers `{materialVerdict}` --
    // the settled count is the half an outright payoff has nothing to say
    // about, so the marker slot moves with the variant.
    assert_eq!(
        outright.difference(&net),
        vec![ShapeAxis::MarkerSlots, ShapeAxis::Payoff]
    );
    assert_eq!(
        shape_of(good_capture()).discriminants(),
        &ShapeDiscriminants::Positive {
            grade: GradeKind::Good,
            elo_relative: false,
            achievement: AchievementKind::CapturedPiece,
            payoff: None,
            played_outcome: PlayedOutcomeKind::Analyzed,
        }
    );
}

/// A target is a marker slot and nothing more. Which piece it is, and whether
/// the move takes or hits it, is the chess the moment is made of -- the same
/// call `{opponentResource}` already makes about the reply's effect.
#[test]
fn a_move_target_is_a_marker_slot_not_a_discriminant() {
    let mut hits_the_queen = good_capture();
    hits_the_queen
        .effects
        .push(GameReviewPlayedMoveEffect::AttackedPiece {
            role: GameReviewPieceRole::Queen,
            square: "d5".to_string(),
        });
    let mut hits_a_rook = good_capture();
    hits_a_rook
        .effects
        .push(GameReviewPlayedMoveEffect::AttackedPiece {
            role: GameReviewPieceRole::Rook,
            square: "a8".to_string(),
        });
    assert_eq!(
        shape_of(good_capture()).difference(&shape_of(hits_the_queen.clone())),
        vec![ShapeAxis::MarkerSlots]
    );
    assert_eq!(shape_of(hits_the_queen), shape_of(hits_a_rook));

    let quiet = improvement(GameReviewResidualClassification::AdvantageLost);
    let mut takes = quiet.clone();
    takes
        .objective
        .lines
        .as_mut()
        .expect("the fixture moment carries objective lines")
        .best_move_effects = vec![GameReviewPlayedMoveEffect::CapturedPiece {
        role: GameReviewPieceRole::Pawn,
        square: "c4".to_string(),
    }];
    let mut hits = quiet.clone();
    hits.objective
        .lines
        .as_mut()
        .expect("the fixture moment carries objective lines")
        .best_move_effects = vec![GameReviewPlayedMoveEffect::AttackedPiece {
        role: GameReviewPieceRole::Knight,
        square: "f6".to_string(),
    }];
    assert_eq!(
        shape_of(quiet).difference(&shape_of(takes.clone())),
        vec![ShapeAxis::MarkerSlots]
    );
    assert_eq!(shape_of(takes), shape_of(hits));
}

/// Neutral reasons accumulate, so the reachable inputs are reason *sets*.
/// The two-reason set is the hardest case in the contract: `{reason}`
/// renders both and the length target is still one line.
#[test]
fn a_neutral_reason_set_is_the_discriminant() {
    let forced = shape_of(neutral(vec![NeutralReviewReason::MechanicallyForcedMove]));
    let forced_and_sound = shape_of(neutral(vec![
        NeutralReviewReason::MechanicallyForcedMove,
        NeutralReviewReason::SoundWithoutConcreteAchievement,
    ]));
    let reordered = shape_of(neutral(vec![
        NeutralReviewReason::SoundWithoutConcreteAchievement,
        NeutralReviewReason::MechanicallyForcedMove,
    ]));

    assert_eq!(
        forced.difference(&forced_and_sound),
        vec![ShapeAxis::NeutralReasons]
    );
    assert_eq!(forced_and_sound, reordered, "the set has no order");
}

/// A terminal position has no post-move score to render, so `{playedEval}`
/// renders "the recorded outcome where…" instead of a number. That is the
/// case the model must not invent a score for.
#[test]
fn a_terminal_played_move_is_a_different_shape_from_an_analyzed_one() {
    let mut terminal = good_capture();
    terminal.played_move_outcome = PlayedMoveOutcomeEvidence::Terminal {
        outcome: BoardTerminalOutcome::Checkmate {
            winner: Color::White,
        },
    };

    assert_eq!(
        shape_of(good_capture()).difference(&shape_of(terminal)),
        vec![ShapeAxis::PlayedOutcome]
    );
}

#[test]
fn shapes_on_different_paths_differ_only_by_path() {
    let positive = shape_of(good_capture());
    let neutral = shape_of(neutral(vec![
        NeutralReviewReason::BelowImprovementThreshold,
    ]));

    assert_eq!(
        positive.difference(&neutral),
        vec![ShapeAxis::Path, ShapeAxis::MarkerSlots]
    );
}

/// The id is the key in every record and coverage report, so two distinct
/// shapes may never render the same one.
#[test]
fn distinct_shapes_render_distinct_ids() {
    let mut with_takeaway = good_capture();
    with_takeaway.teaching.themes = vec![GameReviewTeachingTheme::PassedPawnPromotion];
    let mut unranked = good_capture();
    unranked.human.played_move_rank = None;

    let shapes = [
        shape_of(good_capture()),
        shape_of(with_takeaway),
        shape_of(unranked),
        shape_of(positive(
            PositiveHighlightGrade::Great,
            objective_and_elo(EloRelativeStrength::Strong),
            captured_knight(),
        )),
        shape_of(improvement(
            GameReviewResidualClassification::MissedForcedMate,
        )),
        shape_of(improvement(GameReviewResidualClassification::AdvantageLost)),
        shape_of(neutral(vec![NeutralReviewReason::MechanicallyForcedMove])),
    ];

    let ids = shapes.iter().map(FactShape::id).collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), shapes.len(), "the id must be injective");
}
