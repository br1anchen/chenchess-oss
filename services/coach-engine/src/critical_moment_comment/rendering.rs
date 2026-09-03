//! Every sentence a Player reads that the engine wrote itself.
//!
//! These are the Safe Review Moment Rendering and the marker renderings: one
//! deterministic phrase per recorded fact, with no dependency on the grounding
//! gate, the marker vocabulary, or the hosted provider. They are the fallback
//! the Player sees when the Language Layer publishes nothing, and the
//! substituted text of every marker when it publishes something, so the two
//! surfaces cannot drift apart in wording.
//!
//! The Central Host's browser twin re-derives these in TypeScript, in its
//! review-session `reviewMoments` module, because it seeds a thread from the
//! frozen Game Review before any Review Moment opens. The topology test bans
//! naming that path from a service, so it is named by module rather than by
//! path. It is a hand-kept twin, not a generated one, and it is
//! not word-for-word everywhere: the payoff phrases already differ ("secured
//! checkmate" here against "secured a mating payoff" there). What is mirrored
//! deliberately, and must stay so, is `safe_line_opening`/`lineOpening` and
//! `pawn_units_text`/`pawnUnitsText` -- both feed text the gate then checks, so
//! a divergence there is a rejected comment rather than a wording nit. Nothing
//! checks any of this; changing a phrase here means reading that module.

use crate::review_session_contract::{
    BoardTerminalOutcome, Color, CriticalMomentIntentAuthoringContext, GameReviewCriticalMoment,
    GameReviewMechanismPayoff, GameReviewOpeningPrinciple, GameReviewPieceRole,
    GameReviewPlayedMoveEffect, GameReviewResidualClassification, GameReviewTeachingTheme,
    ImprovementCorrection, ImprovementOutcome, MaterialVerdict, MoveTarget, NeutralReviewReason,
    PlayedMoveOutcomeEvidence, PositiveHighlightAchievement, PositiveHighlightGrade,
    PositiveHighlightQualification, PositiveHighlightQualificationReason, ReviewMomentCommentFacts,
};

pub(super) fn positive_grade_marker_text(grade: PositiveHighlightGrade) -> &'static str {
    match grade {
        PositiveHighlightGrade::Good => "a good move",
        PositiveHighlightGrade::Great => "a great move",
    }
}

/// The safe rendering opens its takeaway sentence with a label. Commentary
/// carries the same claim inside a sentence of the model's own, so the label
/// goes.
pub(super) fn takeaway_marker_text(takeaway: &str) -> String {
    takeaway
        .trim_start_matches("Takeaway: ")
        .trim_end_matches('.')
        .to_string()
}

/// The played move's outcome as something that slots anywhere.
///
/// The terminal branch is wrapped rather than passed through:
/// [`terminal_outcome_text`] is a finite clause opening on a capitalised colour
/// ("White delivered checkmate"), so bare it turns "After {playedMove} it's
/// {playedEval}" into nonsense and could not be re-cased to fix it. Its sibling
/// [`improvement_correction_marker_text`] already wraps the same helper, so this
/// is the pair reading the same way rather than a new idea.
pub(super) fn played_outcome_marker_text(moment: &GameReviewCriticalMoment) -> String {
    match &moment.played_move_outcome {
        PlayedMoveOutcomeEvidence::Analyzed { .. } => format!(
            "{}, {}",
            moment.display.played_evaluation.score, moment.display.played_evaluation.label
        ),
        PlayedMoveOutcomeEvidence::Terminal { outcome } => format!(
            "the recorded outcome where {}",
            terminal_outcome_text(*outcome)
        ),
    }
}

pub(super) fn improvement_correction_marker_text(
    moment: &GameReviewCriticalMoment,
    correction: &ImprovementCorrection,
) -> String {
    match &correction.outcome {
        ImprovementOutcome::ImprovedAnalyzed { .. } => format!(
            "{}, {}",
            moment.display.best_evaluation.score, moment.display.best_evaluation.label
        ),
        ImprovementOutcome::AvoidedTerminal { avoided } => format!(
            "clear of the recorded outcome where {}",
            terminal_outcome_text(*avoided)
        ),
    }
}

/// The safe rendering quotes a line's opening, never its transcript: three
/// moves orient the Player, the ellipsis says the line continues. Mirrored in
/// the browser's `lineOpening` (`reviewMoments.ts`), byte for byte.
fn safe_line_opening(san: &[String]) -> String {
    if san.len() <= 3 {
        san.join(" ")
    } else {
        format!("{} …", san[..3].join(" "))
    }
}

pub(super) fn safe_intent_sentence(
    facts: &ReviewMomentCommentFacts,
    intent: &CriticalMomentIntentAuthoringContext,
) -> String {
    let played = &facts.moment().played_san;
    let Some(enrichment) = &intent.enrichment else {
        return format!(
            "My best guess is that {played} may have been aiming to improve the position."
        );
    };
    let plan = safe_line_opening(&enrichment.projected_plan_san);
    let counterplay = safe_line_opening(&enrichment.objective_counterplay_san);
    match facts {
        ReviewMomentCommentFacts::Improvement { .. } => format!(
            "My best guess is that {played} may have been aiming for {plan}, but {counterplay} may disrupt that plan."
        ),
        ReviewMomentCommentFacts::Positive { .. } => format!(
            "My best guess is that {played} may have been aiming for {plan}; {counterplay} is the strongest defense, while the move's achievement still stands."
        ),
        ReviewMomentCommentFacts::Neutral { .. } => {
            unreachable!("neutral moments never receive intent authoring")
        }
    }
}

pub(super) fn teaching_takeaway(moment: &GameReviewCriticalMoment) -> Option<String> {
    moment
        .teaching
        .themes
        .first()
        .map(|theme| match theme {
            GameReviewTeachingTheme::ForcedMateConversion => {
                "Takeaway: convert a forced mate with forcing moves.".to_string()
            }
            GameReviewTeachingTheme::PassedPawnPromotion => {
                "Takeaway: advance passed pawns with promotion in mind.".to_string()
            }
            GameReviewTeachingTheme::QueenExchange => {
                "Takeaway: consider a queen exchange when it improves the resulting position."
                    .to_string()
            }
        })
        .or_else(|| {
            moment
                .teaching
                .opening_principles
                .first()
                .map(|principle| match principle {
                    GameReviewOpeningPrinciple::OccupyTheCenter => {
                        "Takeaway: fight for the center early.".to_string()
                    }
                })
        })
}

pub(super) fn played_outcome_sentence(moment: &GameReviewCriticalMoment) -> String {
    match &moment.played_move_outcome {
        PlayedMoveOutcomeEvidence::Analyzed { .. } => format!(
            "After {}, the evaluation is {} — {}.",
            moment.played_san,
            moment.display.played_evaluation.score,
            moment.display.played_evaluation.label
        ),
        PlayedMoveOutcomeEvidence::Terminal { outcome } => format!(
            "After {}, {}.",
            moment.played_san,
            terminal_outcome_text(*outcome)
        ),
    }
}

/// The same observation inside a sentence, so "and {observation} because…" does
/// not open a second sentence mid-clause. Only the leading word and the closing
/// stop differ, but authoring it keeps the rule "nothing is ever downcased"
/// true of the substitution itself.
pub(super) fn played_outcome_clause(moment: &GameReviewCriticalMoment) -> String {
    match &moment.played_move_outcome {
        PlayedMoveOutcomeEvidence::Analyzed { .. } => format!(
            "after {}, the evaluation is {} — {}",
            moment.played_san,
            moment.display.played_evaluation.score,
            moment.display.played_evaluation.label
        ),
        PlayedMoveOutcomeEvidence::Terminal { outcome } => format!(
            "after {}, {}",
            moment.played_san,
            terminal_outcome_text(*outcome)
        ),
    }
}

pub(super) fn positive_grade_text(grade: PositiveHighlightGrade) -> &'static str {
    match grade {
        PositiveHighlightGrade::Good => "Good",
        PositiveHighlightGrade::Great => "Great",
    }
}

pub(super) fn positive_achievement_text(achievement: &PositiveHighlightAchievement) -> String {
    match achievement {
        PositiveHighlightAchievement::CompletedCheckmate => "completed checkmate".to_string(),
        PositiveHighlightAchievement::CapturedPiece { role, square } => {
            format!("captured the {} on {square}", piece_role_text(*role))
        }
        PositiveHighlightAchievement::AdvancedPassedPawn { to_square } => {
            format!("advanced the passed pawn to {to_square}")
        }
        PositiveHighlightAchievement::TacticalPayoff { payoff } => match payoff {
            GameReviewMechanismPayoff::Mate => "secured checkmate".to_string(),
            GameReviewMechanismPayoff::Promotion => "secured promotion".to_string(),
            GameReviewMechanismPayoff::WinsMaterialOutright { role } => {
                format!("won a {}", piece_role_text(*role))
            }
            GameReviewMechanismPayoff::WinsMaterialNet {
                role,
                net_pawn_units,
            } => format!(
                "won a {} and came out {} ahead",
                piece_role_text(*role),
                pawn_units_text(*net_pawn_units)
            ),
            GameReviewMechanismPayoff::QueenExchange => {
                "secured a favorable queen exchange".to_string()
            }
        },
    }
}

/// The difficulty claim in both shapes prose needs it.
///
/// The clause is a rewrite, not a truncation. "This was especially difficult…"
/// has a demonstrative subject that reads as redundant inside a sentence, and
/// "This required precise play." has no subject to keep at all — it becomes a
/// participle. Deriving one form from the other would have to know that.
pub(super) struct DifficultyText {
    pub(super) sentence: String,
    pub(super) clause: String,
}

pub(super) fn positive_difficulty_text(
    qualification: &PositiveHighlightQualification,
    grade: PositiveHighlightGrade,
) -> DifficultyText {
    let elo_relative = qualification.reasons.iter().any(|reason| {
        matches!(
            reason,
            PositiveHighlightQualificationReason::EloRelative { .. }
        )
    });
    let (sentence, clause) = match (grade, elo_relative) {
        (PositiveHighlightGrade::Great, true) => (
            "This was especially difficult to find for players at your rating.",
            "especially difficult to find for players at your rating",
        ),
        (_, true) => (
            "This was a notable find for players at your rating.",
            "a notable find for players at your rating",
        ),
        _ => ("This required precise play.", "requiring precise play"),
    };
    DifficultyText {
        sentence: sentence.to_string(),
        clause: clause.to_string(),
    }
}

/// The achievement as a sentence of its own.
///
/// [`positive_achievement_text`] renders a subjectless verb phrase, which the
/// safe rendering gives a subject by putting the move in front of it ("Good:
/// Nxd4 captured the knight on d4."). Commentary has no such fixed frame, so
/// the marker supplies the Player as the subject and stands alone. Every
/// variant is a past-tense verb phrase, so one prefix is exact for all of them.
pub(super) fn achievement_sentence(achievement: &str) -> String {
    format!("You {achievement}.")
}

/// The decision cue inside a sentence: "…, but before committing here,
/// calculate Nxd4 first."
pub(super) fn decision_cue_clause(better_move_san: &str) -> String {
    format!("before committing here, calculate {better_move_san} first")
}

pub(super) fn improvement_correction_text(
    moment: &GameReviewCriticalMoment,
    correction: &ImprovementCorrection,
) -> String {
    match &correction.outcome {
        ImprovementOutcome::ImprovedAnalyzed { .. } => format!(
            "The better move was {}, leaving the evaluation at {} — {}.",
            correction.better_move_san,
            moment.display.best_evaluation.score,
            moment.display.best_evaluation.label
        ),
        ImprovementOutcome::AvoidedTerminal { avoided } => format!(
            "The better move was {}, avoiding the recorded outcome where {}.",
            correction.better_move_san,
            terminal_outcome_text(*avoided)
        ),
    }
}

pub(super) fn residual_consequence_text(
    classification: GameReviewResidualClassification,
) -> &'static str {
    match classification {
        GameReviewResidualClassification::MissedForcedMate => "the forced mate was missed",
        GameReviewResidualClassification::AdvantageKept => "the advantage was kept",
        GameReviewResidualClassification::StandingKept => "the position's standing was kept",
        GameReviewResidualClassification::AdvantageReduced => "the advantage was reduced",
        GameReviewResidualClassification::AdvantageLost => "the advantage was lost",
        GameReviewResidualClassification::NowWorse => "the position became unfavorable",
    }
}

pub(super) fn neutral_reason_text(reason: NeutralReviewReason) -> &'static str {
    match reason {
        NeutralReviewReason::MechanicallyForcedMove => "it was mechanically forced",
        NeutralReviewReason::SoundWithoutConcreteAchievement => {
            "it was sound without a concrete achievement"
        }
        NeutralReviewReason::BelowImprovementThreshold => {
            "it stayed below the improvement threshold"
        }
        NeutralReviewReason::NonInstructionalTerminalOutcome => {
            "the terminal outcome did not add an instructional point"
        }
    }
}

fn terminal_outcome_text(outcome: BoardTerminalOutcome) -> String {
    match outcome {
        BoardTerminalOutcome::Checkmate { winner } => {
            format!("{} delivered checkmate", color_text(winner))
        }
        BoardTerminalOutcome::Stalemate => "the game ended in stalemate".to_string(),
        BoardTerminalOutcome::InsufficientMaterial => {
            "the game ended because there was insufficient material".to_string()
        }
    }
}

/// What the opponent can do about the move just played, in the words the
/// Player gets.
///
/// The line in FACTS already named the reply and the model was told not to
/// transcribe it; this says what makes it a reply worth naming. Only the first
/// effect is rendered, the same way `{achievement}` renders only the first
/// achievement: one concrete thing beats a list, and the derivation puts the
/// capture first when there is one.
pub(super) fn opponent_resource_text(moment: &GameReviewCriticalMoment) -> Option<String> {
    let resource = moment.objective.lines.as_ref()?.opponent_resource()?;
    let does = match resource.does {
        GameReviewPlayedMoveEffect::CapturedPiece { role, square } => {
            format!("taking the {} on {square}", piece_role_text(*role))
        }
        GameReviewPlayedMoveEffect::AttackedPiece { role, square } => {
            format!("hitting the {} on {square}", piece_role_text(*role))
        }
        GameReviewPlayedMoveEffect::AdvancedPassedPawn { to_square } => {
            format!("running the passed pawn to {to_square}")
        }
        GameReviewPlayedMoveEffect::AllowsQueenExchange => "offering a queen trade".to_string(),
    };
    Some(format!(
        "{} can answer with {}, {does}",
        color_text(moment.side.opponent()),
        resource.reply.san
    ))
}

/// What the tactical line is worth, once the piece it won has been paid for.
///
/// The verdict knows which line it describes, so this renders it and asks no
/// second question about the moment. "Settles" rather than "comes out":
/// `net_pawn_units` is the line's last word on material, read at the end of the
/// principal variation rather than at the capture, and the payoff's own moves
/// are truncated before it. Wording it as the line's verdict is what keeps the
/// sentence true of a one-move payoff whose recapture arrives eight plies later.
pub(super) fn material_verdict_text(moment: &GameReviewCriticalMoment) -> Option<String> {
    Some(match moment.material_verdict()? {
        MaterialVerdict::Kept { net_pawn_units } => {
            format!("the line settles {} ahead", pawn_units_text(net_pawn_units))
        }
        MaterialVerdict::Missed {
            role,
            net_pawn_units,
        } => format!(
            "the better line wins a {} and settles {} ahead",
            piece_role_text(role),
            pawn_units_text(net_pawn_units)
        ),
    })
}

/// The enemy piece a move takes or newly hits, in the words the Player gets.
///
/// Which move is the fact's to say, not this function's: the played move's
/// capture is already `{achievement}`, so what it hits reads as an addition,
/// while the better move has nothing said about it beyond its notation, so its
/// target stands on its own. Neither rendering opens with notation: an
/// `Anywhere` marker is capitalised at a sentence start, and "E4" is not a
/// move.
pub(super) fn move_target_text(moment: &GameReviewCriticalMoment) -> Option<String> {
    Some(match moment.move_target()? {
        MoveTarget::PlayedHits { role, square } => {
            format!(
                "your move also hits the {} on {square}",
                piece_role_text(role)
            )
        }
        MoveTarget::BetterTakes { role, square, .. } => {
            format!(
                "the better move takes the {} on {square}",
                piece_role_text(role)
            )
        }
        MoveTarget::BetterHits { role, square, .. } => {
            format!(
                "the better move hits the {} on {square}",
                piece_role_text(role)
            )
        }
    })
}

/// The human move model's read on the played move, in Player-facing words.
///
/// Rank and probability have been in the facts all along and have never reached
/// a comment, yet they are what makes commentary sound like a person watching
/// rather than an engine reporting. The rendering is canonical text rather than
/// a figure, which is also how "players at your rating" stays the only
/// phrasing: the model cannot name the model it is quoting.
pub(super) fn played_popularity_text(moment: &GameReviewCriticalMoment) -> Option<String> {
    Some(match moment.human.played_move_rank? {
        1 => "the most common choice at your rating".to_string(),
        2 => "the second most common choice at your rating".to_string(),
        3 => "the third most common choice at your rating".to_string(),
        _ => "an uncommon choice at your rating".to_string(),
    })
}

fn color_text(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}

fn piece_role_text(role: GameReviewPieceRole) -> &'static str {
    match role {
        GameReviewPieceRole::Pawn => "pawn",
        GameReviewPieceRole::Knight => "knight",
        GameReviewPieceRole::Bishop => "bishop",
        GameReviewPieceRole::Rook => "rook",
        GameReviewPieceRole::Queen => "queen",
    }
}

/// Material in words, so the gate never sees a figure the model did not write.
fn pawn_units_text(pawn_units: i32) -> String {
    match pawn_units {
        1 => "a pawn".to_string(),
        2 => "two pawns".to_string(),
        3 => "three pawns".to_string(),
        4 => "four pawns".to_string(),
        5 => "five pawns".to_string(),
        6 => "six pawns".to_string(),
        7 => "seven pawns".to_string(),
        8 => "eight pawns".to_string(),
        _ => "a decisive amount".to_string(),
    }
}
