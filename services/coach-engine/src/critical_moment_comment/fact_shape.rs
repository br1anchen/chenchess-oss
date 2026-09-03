//! What makes two Review Moments the same authoring test case.
//!
//! The Language Layer is a renderer: every chess fact in a comment is computed
//! before the model is called, and the numbers it would otherwise write are
//! substituted from markers. So two Review Moments are the same *test input*
//! when they hand the model the same authoring problem, whatever chess produced
//! them — and they are different inputs when the problem differs, even if the
//! position is nearly identical.
//!
//! A [`FactShape`] is that problem, named. It is derived from
//! [`CommentFactsPolicy::for_facts`] — the one place a moment's markers are
//! chosen — plus the enum variant that selected each marker's rendering.
//! Nothing here is a hand-kept list of shapes: every derivation below is an
//! exhaustive `match`, so adding a variant to the contract is a compile error
//! in this file rather than a shape that silently collapses into its neighbour.
//!
//! Two rules decide what counts as a discriminant, and both are load-bearing:
//!
//! - **The variant, never a field.** `CapturedPiece { role, square }` and
//!   `WinsMaterialOutright { role }` contribute their variant and drop `role`.
//!   A different role is different *content* in the same sentence frame; a
//!   different variant is a different frame.
//! - **Every enum a rendering matches on**, not a chosen subset. Applying it to
//!   `{achievement}` but not to the equally required `{consequence}` is
//!   hand-curation wearing a derivation's clothes — and it is precisely what
//!   loses `Improvement / mate`, since
//!   [`improvement_correction_marker_text`](super::rendering) renders a mate
//!   and a centipawn correction through the same arm. Only
//!   [`GameReviewResidualClassification`] separates them.
//!
//! Presence of an optional marker needs no discriminant field: `{takeaway}` and
//! `{playedPopularity}` are marker slots, which is the whole point of deriving
//! from the policy.
//!
//! One deliberate coarsening: the *teaching theme* behind `{takeaway}` is not a
//! discriminant. It renders one of four fixed one-liners carrying no
//! moment-specific content, so the authoring variable is whether the model uses
//! the slot at all. That is a hand decision rather than a derived one, and it is
//! recorded as such beside the coverage gaps.

use std::{collections::BTreeSet, fmt, mem};

use serde::{Deserialize, Serialize};

use crate::{
    language_layer_markers::MarkerForm,
    review_session_contract::{
        GameReviewMechanismPayoff, GameReviewMomentClassification,
        GameReviewResidualClassification, ImprovementOutcome, NeutralReviewReason,
        PlayedMoveOutcomeEvidence, PositiveHighlightAchievement, PositiveHighlightGrade,
        PositiveHighlightQualificationReason, ReviewMomentCommentFacts,
    },
};

use super::CommentFactsPolicy;
/// The authoring problem one Review Moment presents to the Language Layer.
///
/// [`FactShape::of`] is the only constructor, so a shape whose marker slots
/// disagree with its discriminants cannot be built.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FactShape {
    markers: Vec<MarkerSlot>,
    discriminants: ShapeDiscriminants,
}

/// One marker the moment offers, without the position-specific text it renders.
///
/// The rendering itself is deliberately absent: `{playedMove}` renders `Nxd4`
/// in one moment and `Bg5` in another, and that difference is the chess, which
/// is not what the Language Layer is measured on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct MarkerSlot {
    pub marker: &'static str,
    pub required: bool,
    pub form: MarkerFormKind,
}

/// Which frame a marker's rendering fits into. Mirrors [`MarkerForm`] without
/// its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerFormKind {
    Literal,
    Anywhere,
    Shaped,
    OwnSentence,
}

impl MarkerFormKind {
    fn of(form: &MarkerForm) -> Self {
        match form {
            MarkerForm::Literal(_) => Self::Literal,
            MarkerForm::Anywhere(_) => Self::Anywhere,
            MarkerForm::Shaped { .. } => Self::Shaped,
            MarkerForm::OwnSentence(_) => Self::OwnSentence,
        }
    }
}

/// The enum variants that selected this moment's renderings.
///
/// One variant per comment path, so the path itself needs no separate field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(
    tag = "path",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ShapeDiscriminants {
    Positive {
        grade: GradeKind,
        /// `positive_difficulty_text` branches on `(grade, elo_relative)` for
        /// the required `{difficulty}` marker. A Great highlight always carries
        /// an Elo-relative reason, so this splits Good only.
        elo_relative: bool,
        achievement: AchievementKind,
        /// `Some` exactly when the achievement is a tactical payoff.
        payoff: Option<PayoffKind>,
        played_outcome: PlayedOutcomeKind,
    },
    Improvement {
        outcome: ImprovementOutcomeKind,
        /// The only axis that separates a missed forced mate from a centipawn
        /// correction: `{bestEval}` renders both through `ImprovedAnalyzed`,
        /// while `{consequence}` renders this.
        residual: ResidualKind,
        played_outcome: PlayedOutcomeKind,
    },
    Neutral {
        /// `{reason}` renders the whole set, joined. The reasons accumulate, so
        /// the set is the discriminant rather than any one member.
        reasons: BTreeSet<NeutralReasonKind>,
        played_outcome: PlayedOutcomeKind,
    },
}

/// An axis on which two Fact Shapes differ. What nearest-miss reporting names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeAxis {
    Path,
    MarkerSlots,
    Grade,
    EloRelative,
    Achievement,
    Payoff,
    ImprovementOutcome,
    Residual,
    NeutralReasons,
    PlayedOutcome,
}

/// A Fact Shape rendered as a stable, readable key.
///
/// Equality is on [`FactShape`], never on this: the id is the display and JSON
/// key form. It must stay injective over reachable shapes, which
/// `distinct_shapes_render_distinct_ids` holds it to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FactShapeId(String);

impl fmt::Display for FactShapeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FactShape {
    /// Derives the shape of the authoring problem these facts present.
    pub fn of(facts: &ReviewMomentCommentFacts) -> Self {
        let policy = CommentFactsPolicy::for_facts(facts);
        let mut markers = policy
            .markers
            .entries()
            .map(|(marker, form, required)| MarkerSlot {
                marker,
                required,
                form: MarkerFormKind::of(form),
            })
            .collect::<Vec<_>>();
        markers.sort();
        Self {
            markers,
            discriminants: ShapeDiscriminants::of(facts),
        }
    }

    pub fn markers(&self) -> &[MarkerSlot] {
        &self.markers
    }

    pub fn discriminants(&self) -> &ShapeDiscriminants {
        &self.discriminants
    }

    /// The axes on which two shapes differ. Empty exactly when they are equal,
    /// and a single element is what makes a moment a *nearest miss* for an
    /// unfilled shape.
    pub fn difference(&self, other: &Self) -> Vec<ShapeAxis> {
        let mut axes = self.discriminants.difference(&other.discriminants);
        if self.markers != other.markers {
            axes.push(ShapeAxis::MarkerSlots);
        }
        axes.sort();
        axes
    }

    /// The readable key: the discriminant axes joined by `/`, then each optional
    /// marker the moment offers.
    pub fn id(&self) -> FactShapeId {
        let optional = self
            .markers
            .iter()
            .filter(|slot| !slot.required)
            .map(|slot| format!("+{}", slot.marker))
            .collect::<Vec<_>>();
        let body = self
            .discriminants
            .axes()
            .into_iter()
            .filter_map(|(_, value)| value)
            .collect::<Vec<_>>()
            .join("/");
        FactShapeId(if optional.is_empty() {
            body
        } else {
            format!("{body} {}", optional.join(" "))
        })
    }
}

impl ShapeDiscriminants {
    fn of(facts: &ReviewMomentCommentFacts) -> Self {
        let moment = facts.moment();
        let played_outcome = PlayedOutcomeKind::of(&moment.played_move_outcome);
        // Matching the classification rather than the tagged facts keeps this
        // total: `try_from_moment` already proved the two agree.
        match &moment.classification {
            GameReviewMomentClassification::PositiveHighlight {
                qualification,
                grade,
            } => {
                let achievement = &qualification.achievements[0];
                Self::Positive {
                    grade: GradeKind::of(*grade),
                    elo_relative: qualification.reasons.iter().any(|reason| {
                        matches!(
                            reason,
                            PositiveHighlightQualificationReason::EloRelative { .. }
                        )
                    }),
                    achievement: AchievementKind::of(achievement),
                    payoff: match achievement {
                        PositiveHighlightAchievement::TacticalPayoff { payoff } => {
                            Some(PayoffKind::of(payoff))
                        }
                        PositiveHighlightAchievement::CompletedCheckmate
                        | PositiveHighlightAchievement::CapturedPiece { .. }
                        | PositiveHighlightAchievement::AdvancedPassedPawn { .. } => None,
                    },
                    played_outcome,
                }
            }
            GameReviewMomentClassification::ImprovementOpportunity { correction } => {
                Self::Improvement {
                    outcome: ImprovementOutcomeKind::of(&correction.outcome),
                    residual: ResidualKind::of(moment.residual_outcome.classification),
                    played_outcome,
                }
            }
            GameReviewMomentClassification::Neutral { reasons } => Self::Neutral {
                reasons: reasons.iter().copied().map(NeutralReasonKind::of).collect(),
                played_outcome,
            },
        }
    }

    /// This shape's axes in id order, each with the value it renders — `None`
    /// for an axis this path carries but this shape does not fill.
    ///
    /// One projection serves both [`FactShape::id`] and [`Self::difference`],
    /// so an axis cannot appear in the key and be forgotten by the diff. Two
    /// shapes on the same path project the same axis sequence, which is what
    /// lets `difference` zip them positionally.
    ///
    /// Values are compared as rendered text rather than as variants. Each kind
    /// below renders its variants to distinct literals, so the two are the same
    /// comparison.
    fn axes(&self) -> Vec<(ShapeAxis, Option<String>)> {
        let owned = |text: &str| Some(text.to_string());
        match self {
            Self::Positive {
                grade,
                elo_relative,
                achievement,
                payoff,
                played_outcome,
            } => vec![
                (ShapeAxis::Path, owned("Positive")),
                (ShapeAxis::Grade, owned(grade.as_str())),
                (
                    ShapeAxis::EloRelative,
                    owned(if *elo_relative {
                        "eloRelative"
                    } else {
                        "objectiveOnly"
                    }),
                ),
                (ShapeAxis::Achievement, owned(achievement.as_str())),
                (
                    ShapeAxis::Payoff,
                    payoff.map(|payoff| payoff.as_str().to_string()),
                ),
                (ShapeAxis::PlayedOutcome, owned(played_outcome.as_str())),
            ],
            Self::Improvement {
                outcome,
                residual,
                played_outcome,
            } => vec![
                (ShapeAxis::Path, owned("Improvement")),
                (ShapeAxis::ImprovementOutcome, owned(outcome.as_str())),
                (ShapeAxis::Residual, owned(residual.as_str())),
                (ShapeAxis::PlayedOutcome, owned(played_outcome.as_str())),
            ],
            Self::Neutral {
                reasons,
                played_outcome,
            } => vec![
                (ShapeAxis::Path, owned("Neutral")),
                (
                    ShapeAxis::NeutralReasons,
                    owned(
                        &reasons
                            .iter()
                            .map(|reason| reason.as_str())
                            .collect::<Vec<_>>()
                            .join("+"),
                    ),
                ),
                (ShapeAxis::PlayedOutcome, owned(played_outcome.as_str())),
            ],
        }
    }

    fn difference(&self, other: &Self) -> Vec<ShapeAxis> {
        if mem::discriminant(self) != mem::discriminant(other) {
            return vec![ShapeAxis::Path];
        }
        self.axes()
            .into_iter()
            .zip(other.axes())
            .filter(|((_, mine), (_, theirs))| mine != theirs)
            .map(|((axis, _), _)| axis)
            .collect()
    }
}

/* Each kind below mirrors a contract enum, dropping the fields the renderings
treat as content rather than frame — a captured knight and a captured rook are
one shape.

They are mirrors rather than the contract enums themselves because a Fact Shape
is serialized into the recorded resolution and every run record. Embedding the
contract types would let a serde rename in the contract silently rewrite every
stored shape id and invalidate resolutions recorded against the old spelling.
The benchmark owns its own wire format. Each `of` is an exhaustive `match`, so a
new contract variant fails to compile here. */

/// The grade the Positive Highlight carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GradeKind {
    Good,
    Great,
}

impl GradeKind {
    fn of(grade: PositiveHighlightGrade) -> Self {
        match grade {
            PositiveHighlightGrade::Good => Self::Good,
            PositiveHighlightGrade::Great => Self::Great,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Great => "great",
        }
    }
}

/// The first achievement, which is the one `{achievement}` renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AchievementKind {
    CompletedCheckmate,
    CapturedPiece,
    AdvancedPassedPawn,
    TacticalPayoff,
}

impl AchievementKind {
    fn of(achievement: &PositiveHighlightAchievement) -> Self {
        match achievement {
            PositiveHighlightAchievement::CompletedCheckmate => Self::CompletedCheckmate,
            PositiveHighlightAchievement::CapturedPiece { .. } => Self::CapturedPiece,
            PositiveHighlightAchievement::AdvancedPassedPawn { .. } => Self::AdvancedPassedPawn,
            PositiveHighlightAchievement::TacticalPayoff { .. } => Self::TacticalPayoff,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::CompletedCheckmate => "completedCheckmate",
            Self::CapturedPiece => "capturedPiece",
            Self::AdvancedPassedPawn => "advancedPassedPawn",
            Self::TacticalPayoff => "tacticalPayoff",
        }
    }
}

/// What a tactical payoff achieved. Sits beneath `AchievementKind::TacticalPayoff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PayoffKind {
    Mate,
    Promotion,
    WinsMaterialOutright,
    WinsMaterialNet,
    QueenExchange,
}

impl PayoffKind {
    fn of(payoff: &GameReviewMechanismPayoff) -> Self {
        match payoff {
            GameReviewMechanismPayoff::Mate => Self::Mate,
            GameReviewMechanismPayoff::Promotion => Self::Promotion,
            GameReviewMechanismPayoff::WinsMaterialOutright { .. } => Self::WinsMaterialOutright,
            GameReviewMechanismPayoff::WinsMaterialNet { .. } => Self::WinsMaterialNet,
            GameReviewMechanismPayoff::QueenExchange => Self::QueenExchange,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Mate => "mate",
            Self::Promotion => "promotion",
            Self::WinsMaterialOutright => "winsMaterialOutright",
            Self::WinsMaterialNet => "winsMaterialNet",
            Self::QueenExchange => "queenExchange",
        }
    }
}

/// Whether the correction improves an evaluation or avoids a terminal outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImprovementOutcomeKind {
    ImprovedAnalyzed,
    AvoidedTerminal,
}

impl ImprovementOutcomeKind {
    fn of(outcome: &ImprovementOutcome) -> Self {
        match outcome {
            ImprovementOutcome::ImprovedAnalyzed { .. } => Self::ImprovedAnalyzed,
            ImprovementOutcome::AvoidedTerminal { .. } => Self::AvoidedTerminal,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ImprovedAnalyzed => "improvedAnalyzed",
            Self::AvoidedTerminal => "avoidedTerminal",
        }
    }
}

/// What the played move did to the standing, which `{consequence}` renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResidualKind {
    MissedForcedMate,
    AdvantageKept,
    StandingKept,
    AdvantageReduced,
    AdvantageLost,
    NowWorse,
}

impl ResidualKind {
    fn of(classification: GameReviewResidualClassification) -> Self {
        match classification {
            GameReviewResidualClassification::MissedForcedMate => Self::MissedForcedMate,
            GameReviewResidualClassification::AdvantageKept => Self::AdvantageKept,
            GameReviewResidualClassification::StandingKept => Self::StandingKept,
            GameReviewResidualClassification::AdvantageReduced => Self::AdvantageReduced,
            GameReviewResidualClassification::AdvantageLost => Self::AdvantageLost,
            GameReviewResidualClassification::NowWorse => Self::NowWorse,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::MissedForcedMate => "missedForcedMate",
            Self::AdvantageKept => "advantageKept",
            Self::StandingKept => "standingKept",
            Self::AdvantageReduced => "advantageReduced",
            Self::AdvantageLost => "advantageLost",
            Self::NowWorse => "nowWorse",
        }
    }
}

/// One reason a moment is Neutral. `{reason}` renders the whole set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NeutralReasonKind {
    MechanicallyForcedMove,
    SoundWithoutConcreteAchievement,
    BelowImprovementThreshold,
    NonInstructionalTerminalOutcome,
}

impl NeutralReasonKind {
    fn of(reason: NeutralReviewReason) -> Self {
        match reason {
            NeutralReviewReason::MechanicallyForcedMove => Self::MechanicallyForcedMove,
            NeutralReviewReason::SoundWithoutConcreteAchievement => {
                Self::SoundWithoutConcreteAchievement
            }
            NeutralReviewReason::BelowImprovementThreshold => Self::BelowImprovementThreshold,
            NeutralReviewReason::NonInstructionalTerminalOutcome => {
                Self::NonInstructionalTerminalOutcome
            }
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::MechanicallyForcedMove => "mechanicallyForcedMove",
            Self::SoundWithoutConcreteAchievement => "soundWithoutConcreteAchievement",
            Self::BelowImprovementThreshold => "belowImprovementThreshold",
            Self::NonInstructionalTerminalOutcome => "nonInstructionalTerminalOutcome",
        }
    }
}

/// Whether the played move has a post-move evaluation or ended the Game. A
/// terminal moment has no score to render, and none to invent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayedOutcomeKind {
    Analyzed,
    Terminal,
}

impl PlayedOutcomeKind {
    fn of(outcome: &PlayedMoveOutcomeEvidence) -> Self {
        match outcome {
            PlayedMoveOutcomeEvidence::Analyzed { .. } => Self::Analyzed,
            PlayedMoveOutcomeEvidence::Terminal { .. } => Self::Terminal,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Analyzed => "analyzed",
            Self::Terminal => "terminal",
        }
    }
}

#[cfg(test)]
#[path = "fact_shape/tests.rs"]
mod tests;
