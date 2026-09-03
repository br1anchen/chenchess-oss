use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    canonical_sha256, ArtifactDigest, Color, CriticalMomentComment, CriticalMomentId,
    DecisionExplanation, DecisionExplanationRef, EloRating, EngineEvaluation, LearningPlan,
    PositionPhase, PositionSnapshot, Probability, ReviewMomentLearningMaterial,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReview {
    pub summary: String,
    pub player_profile: GameReviewPlayerProfile,
    pub critical_moments: Vec<GameReviewCriticalMoment>,
    pub position_views: Vec<GameReviewPositionView>,
    pub evaluation_timeline: Vec<GameReviewEvaluationPoint>,
    pub learning_plan: LearningPlan,
}

impl GameReview {
    pub fn content_digest(&self) -> ArtifactDigest {
        ArtifactDigest::try_from(canonical_sha256(self))
            .expect("canonical Game Review content has a valid digest")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewPlayerProfile {
    pub elo: EloRating,
    pub level: GameReviewPlayerLevel,
    pub coaching_focus: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameReviewPlayerLevel {
    Beginner,
    Intermediate,
    Advanced,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewCriticalMoment {
    pub critical_moment_id: CriticalMomentId,
    pub ply: u16,
    pub move_number: u16,
    pub side: Color,
    pub played_san: String,
    pub position_phase: PositionPhase,
    pub classification: GameReviewMomentClassification,
    pub provenance: GameReviewMomentProvenance,
    pub category: GameReviewCriticalMomentCategory,
    pub objective: GameReviewObjectiveComparison,
    pub human: GameReviewHumanComparison,
    pub effects: Vec<GameReviewPlayedMoveEffect>,
    pub residual_outcome: GameReviewResidualOutcome,
    pub played_move_outcome: PlayedMoveOutcomeEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mechanism: Option<GameReviewTacticalMechanism>,
    pub teaching: GameReviewTeachingFacts,
    /// Addresses the proof aggregate without carrying it.
    ///
    /// The aggregate is audit-only and dominates a review's bytes, so the
    /// delivered payload keeps the reference and drops the proof. The
    /// reference stays resolvable against the durable review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_explanation_ref: Option<DecisionExplanationRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_explanation: Option<DecisionExplanation>,
    /// What the chess-concept learning pass established for this moment.
    /// Opening Learning Tracks are selected independently and may coexist.
    pub decision_learning_outcome: DecisionLearningOutcome,
    pub learning_material: ReviewMomentLearningMaterial,
    pub display: GameReviewMomentDisplay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<CriticalMomentComment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DecisionLearningOutcome {
    /// Import-time Player-Selected moments have not run the on-demand pass yet.
    NotAttempted,
    /// A proof-valid concept and its exact Learning Resource mapping were selected.
    TrackSelected,
    /// A proof-valid concept exists, but the catalog has no exact mapping for it.
    ExplanationUnmapped,
    /// No proof-valid concept could be selected from the available evidence.
    Abstained {
        reason: DecisionLearningAbstentionReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum DecisionLearningAbstentionReason {
    CandidateEvidenceUnavailable,
    CandidateEvidenceRejected,
    CandidateComparisonUnavailable,
    CandidateComparisonRejected,
    NoProofValidConcept,
}

impl GameReviewCriticalMoment {
    /// Sets the proof and the reference that addresses it together, so a
    /// delivered reference can never name an explanation the moment lacks.
    pub fn set_decision_explanation(&mut self, explanation: Option<DecisionExplanation>) {
        self.decision_explanation_ref = explanation
            .as_ref()
            .map(|explanation| explanation.decision_explanation_ref.clone());
        self.decision_explanation = explanation;
    }

    /// What the tactical line settles at, when it settles ahead by less than
    /// the piece it won.
    ///
    /// The single door to this fact. `WinsMaterialOutright` is deliberately
    /// not a verdict: it settles at or above the captured piece's value, so
    /// "captured the rook on h8" already is the verdict and repeating it adds
    /// a clause and no fact. `WinsMaterialNet` is the one that says something
    /// the Player cannot read off the capture — the piece cost something, and
    /// `net_pawn_units` is what is left.
    ///
    /// Which line, by path. A Positive highlight reads the *credited*
    /// achievement rather than the mechanism, because crediting a material
    /// payoff to the played move is a rule Rule Extraction already applies and
    /// re-deriving it here would let a comment claim a piece the opponent
    /// still has to walk into. An Improvement opportunity reads the mechanism,
    /// whose first move is the better move, so the verdict belongs to the line
    /// the Player did not play. Neutral has neither.
    pub fn material_verdict(&self) -> Option<MaterialVerdict> {
        match &self.classification {
            GameReviewMomentClassification::PositiveHighlight { qualification, .. } => {
                qualification
                    .achievements
                    .iter()
                    .find_map(|achievement| match achievement {
                        PositiveHighlightAchievement::TacticalPayoff { payoff } => match payoff {
                            GameReviewMechanismPayoff::WinsMaterialNet {
                                net_pawn_units, ..
                            } => Some(MaterialVerdict::Kept {
                                net_pawn_units: *net_pawn_units,
                            }),
                            GameReviewMechanismPayoff::Mate
                            | GameReviewMechanismPayoff::Promotion
                            | GameReviewMechanismPayoff::WinsMaterialOutright { .. }
                            | GameReviewMechanismPayoff::QueenExchange => None,
                        },
                        PositiveHighlightAchievement::CompletedCheckmate
                        | PositiveHighlightAchievement::CapturedPiece { .. }
                        | PositiveHighlightAchievement::AdvancedPassedPawn { .. } => None,
                    })
            }
            GameReviewMomentClassification::ImprovementOpportunity { .. } => {
                match self.mechanism.as_ref()?.payoff {
                    GameReviewMechanismPayoff::WinsMaterialNet {
                        role,
                        net_pawn_units,
                    } => Some(MaterialVerdict::Missed {
                        role,
                        net_pawn_units,
                    }),
                    GameReviewMechanismPayoff::Mate
                    | GameReviewMechanismPayoff::Promotion
                    | GameReviewMechanismPayoff::WinsMaterialOutright { .. }
                    | GameReviewMechanismPayoff::QueenExchange => None,
                }
            }
            GameReviewMomentClassification::Neutral { .. } => None,
        }
    }

    /// The enemy piece a move takes or newly hits: the half of a move a Player
    /// cannot read off its notation.
    ///
    /// Which move, by path. A Positive highlight reads the played move, whose
    /// captures `{achievement}` already names, so only a piece it newly attacks
    /// is a target left unsaid. An Improvement opportunity reads the better
    /// move, which nothing narrates beyond its notation, so its first capture
    /// or attack is the target. Neutral has neither. The first effect speaks,
    /// the way the opponent's resource and the achievement each render one
    /// thing rather than a list.
    pub fn move_target(&self) -> Option<MoveTarget<'_>> {
        match &self.classification {
            GameReviewMomentClassification::PositiveHighlight { .. } => {
                self.effects.iter().find_map(|effect| match effect {
                    GameReviewPlayedMoveEffect::AttackedPiece { role, square } => {
                        Some(MoveTarget::PlayedHits {
                            role: *role,
                            square,
                        })
                    }
                    GameReviewPlayedMoveEffect::CapturedPiece { .. }
                    | GameReviewPlayedMoveEffect::AdvancedPassedPawn { .. }
                    | GameReviewPlayedMoveEffect::AllowsQueenExchange => None,
                })
            }
            GameReviewMomentClassification::ImprovementOpportunity { correction } => {
                let better_move = correction.better_move_san.as_str();
                self.objective
                    .lines
                    .as_ref()?
                    .best_move_effects
                    .iter()
                    .find_map(|effect| match effect {
                        GameReviewPlayedMoveEffect::CapturedPiece { role, square } => {
                            Some(MoveTarget::BetterTakes {
                                better_move,
                                role: *role,
                                square,
                            })
                        }
                        GameReviewPlayedMoveEffect::AttackedPiece { role, square } => {
                            Some(MoveTarget::BetterHits {
                                better_move,
                                role: *role,
                                square,
                            })
                        }
                        GameReviewPlayedMoveEffect::AdvancedPassedPawn { .. }
                        | GameReviewPlayedMoveEffect::AllowsQueenExchange => None,
                    })
            }
            GameReviewMomentClassification::Neutral { .. } => None,
        }
    }
}

/// What a tactical line settles ahead by, and which line it was.
///
/// The variants carry different halves because the two paths leave different
/// halves unsaid, and which half is missing is a property of the fact rather
/// than of its wording. A Positive highlight has already named the captured
/// piece through `{achievement}`, and a credited material payoff always names
/// the same piece as that capture, so only the count is new. Nothing on the
/// Improvement path names the piece the line the Player did not play would
/// have won.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialVerdict {
    /// The Player's own line, whose capture the achievement already named.
    Kept { net_pawn_units: i32 },
    /// The line the Player did not play, which nothing else describes.
    Missed {
        role: GameReviewPieceRole,
        net_pawn_units: i32,
    },
}

/// The enemy piece a move takes or newly hits, and which move it was.
///
/// Three variants rather than a move-and-effect pair because the reachable
/// combinations are three: the played move's capture is already its
/// achievement, so it can only *hit*, while the better move has nothing said
/// about it and may take or hit. A pair would admit a fourth case no path
/// produces and force the rendering to match it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveTarget<'a> {
    /// A piece the played move newly attacks, beyond whatever it captured.
    PlayedHits {
        role: GameReviewPieceRole,
        square: &'a str,
    },
    /// The better move captures a piece.
    BetterTakes {
        better_move: &'a str,
        role: GameReviewPieceRole,
        square: &'a str,
    },
    /// The better move newly attacks a piece.
    BetterHits {
        better_move: &'a str,
        role: GameReviewPieceRole,
        square: &'a str,
    },
}

impl MoveTarget<'_> {
    /// The square the target stands on: the one literal this fact admits.
    pub fn square(&self) -> &str {
        match self {
            Self::PlayedHits { square, .. }
            | Self::BetterTakes { square, .. }
            | Self::BetterHits { square, .. } => square,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameReviewMomentProvenance {
    Automatic,
    PlayerSelected,
}

/// The teaching result for one reviewed move. Neutral is only valid for a
/// Player-selected move; automatic selection operates on the two Critical
/// Moment variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum GameReviewMomentClassification {
    PositiveHighlight {
        qualification: PositiveHighlightQualification,
        grade: PositiveHighlightGrade,
    },
    ImprovementOpportunity {
        correction: ImprovementCorrection,
    },
    Neutral {
        reasons: Vec<NeutralReviewReason>,
    },
}

impl GameReviewMomentClassification {
    pub fn is_well_formed(&self) -> bool {
        match self {
            Self::PositiveHighlight {
                qualification,
                grade,
            } => {
                !qualification.achievements.is_empty()
                    && qualification
                        .derived_grade()
                        .is_some_and(|derived| derived == *grade)
            }
            Self::ImprovementOpportunity { correction } => {
                !correction.better_move_uci.trim().is_empty()
                    && !correction.better_move_san.trim().is_empty()
            }
            Self::Neutral { reasons } => !reasons.is_empty(),
        }
    }
}

impl From<&GameReviewMomentClassification> for super::ReviewMomentClassificationKind {
    fn from(classification: &GameReviewMomentClassification) -> Self {
        match classification {
            GameReviewMomentClassification::PositiveHighlight { .. } => Self::PositiveHighlight,
            GameReviewMomentClassification::ImprovementOpportunity { .. } => {
                Self::ImprovementOpportunity
            }
            GameReviewMomentClassification::Neutral { .. } => Self::Neutral,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PositiveHighlightQualification {
    pub reasons: Vec<PositiveHighlightQualificationReason>,
    pub achievements: Vec<PositiveHighlightAchievement>,
}

impl PositiveHighlightQualification {
    pub fn derived_grade(&self) -> Option<PositiveHighlightGrade> {
        let has_objective = self.reasons.iter().any(|reason| {
            matches!(
                reason,
                PositiveHighlightQualificationReason::Objective { .. }
            )
        });
        let strongest_elo = self
            .reasons
            .iter()
            .filter_map(|reason| match reason {
                PositiveHighlightQualificationReason::EloRelative { strength, .. } => {
                    Some(*strength)
                }
                PositiveHighlightQualificationReason::Objective { .. } => None,
            })
            .max();
        match (has_objective, strongest_elo) {
            (true, Some(EloRelativeStrength::Strong)) => Some(PositiveHighlightGrade::Great),
            (true, _)
            | (false, Some(EloRelativeStrength::Notable | EloRelativeStrength::Strong)) => {
                Some(PositiveHighlightGrade::Good)
            }
            (false, None) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "lane",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PositiveHighlightQualificationReason {
    Objective {
        reason: ObjectiveExcellenceReason,
    },
    EloRelative {
        reason: EloRelativeQualificationReason,
        strength: EloRelativeStrength,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum ObjectiveExcellenceReason {
    ExactBestMajorAchievement,
    PreservedForcedMate,
    CompletedCheckmate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum EloRelativeQualificationReason {
    OutsideRecordedCohort,
    RarePlayedMoveRank,
    LowPlayedMoveProbability,
    LowProbabilityRelativeToTopMove,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum EloRelativeStrength {
    Notable,
    Strong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum PositiveHighlightGrade {
    Good,
    Great,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PositiveHighlightAchievement {
    CompletedCheckmate,
    CapturedPiece {
        role: GameReviewPieceRole,
        square: String,
    },
    AdvancedPassedPawn {
        to_square: String,
    },
    TacticalPayoff {
        payoff: GameReviewMechanismPayoff,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImprovementCorrection {
    pub better_move_uci: String,
    pub better_move_san: String,
    pub outcome: ImprovementOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ImprovementOutcome {
    ImprovedAnalyzed { better_evaluation: EngineEvaluation },
    AvoidedTerminal { avoided: BoardTerminalOutcome },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum NeutralReviewReason {
    MechanicallyForcedMove,
    SoundWithoutConcreteAchievement,
    BelowImprovementThreshold,
    NonInstructionalTerminalOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PlayedMoveOutcomeEvidence {
    Analyzed {
        played_evaluation: EngineEvaluation,
        centipawn_loss: Option<u32>,
        residual_outcome: GameReviewResidualOutcome,
    },
    Terminal {
        outcome: BoardTerminalOutcome,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum BoardTerminalOutcome {
    Checkmate { winner: Color },
    Stalemate,
    InsufficientMaterial,
}

/// Presentation-ready evaluation rendering for a Critical Moment. Scores and
/// labels are expressed from White's perspective; prose quotes them verbatim
/// instead of converting centipawns or flipping perspectives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewMomentDisplay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub played_annotation: Option<String>,
    pub best_evaluation: GameReviewEvaluationDisplay,
    pub played_evaluation: GameReviewEvaluationDisplay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loss_pawns: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewEvaluationDisplay {
    pub score: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameReviewPieceRole {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum GameReviewPlayedMoveEffect {
    CapturedPiece {
        role: GameReviewPieceRole,
        square: String,
    },
    AdvancedPassedPawn {
        to_square: String,
    },
    AttackedPiece {
        role: GameReviewPieceRole,
        square: String,
    },
    AllowsQueenExchange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewResidualOutcome {
    pub standing_before: GameReviewAdvantageStanding,
    pub standing_after: GameReviewAdvantageStanding,
    pub classification: GameReviewResidualClassification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameReviewAdvantageStanding {
    Winning,
    Favorable,
    Balanced,
    Unfavorable,
    Losing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameReviewResidualClassification {
    MissedForcedMate,
    AdvantageKept,
    StandingKept,
    AdvantageReduced,
    AdvantageLost,
    NowWorse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum GameReviewMechanismPayoff {
    Mate,
    Promotion,
    /// The line settles at or above the captured piece's own value.
    // The alias reads Game Reviews frozen before the exchange was settled,
    // whose payoff was written as one `winsMaterial` variant. Every stored
    // review carries that tag, and without the alias each one fails to decode
    // and its address answers as an unknown Game Import. It is a read path and
    // not a name: the schema still publishes one variant, nothing writes the
    // old tag, and the alias goes when no stored `winsMaterial` payoff remains.
    // It widens no trust boundary either — a Game Review is answered by the
    // Engine and never parsed from a caller, so stored documents are the only
    // thing this reads. Those payoffs were read off a prefix of the engine's
    // line, so what they claim can be too generous until the Game is
    // re-analysed.
    #[serde(alias = "winsMaterial")]
    WinsMaterialOutright {
        role: GameReviewPieceRole,
    },
    /// The line settles ahead but below the captured piece's value: the piece
    /// was won for something, and `net_pawn_units` is what the Player is up
    /// once the line ends. "Won a rook" is the wrong sentence for a rook that
    /// cost a pawn, which is why the two are separate variants rather than one
    /// variant a reader has to do arithmetic on.
    WinsMaterialNet {
        role: GameReviewPieceRole,
        net_pawn_units: i32,
    },
    QueenExchange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewTacticalMechanism {
    pub moves: Vec<GameReviewLineMove>,
    pub forcing_index: u16,
    pub payoff: GameReviewMechanismPayoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameReviewCriticalMomentCategory {
    Tactical,
    Positional,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewObjectiveComparison {
    pub best_move_uci: String,
    pub played_move_uci: String,
    pub best_evaluation: EngineEvaluation,
    pub played_evaluation: EngineEvaluation,
    pub centipawn_loss: Option<u32>,
    pub principal_variation: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<GameReviewObjectiveLines>,
}

impl GameReviewObjectiveComparison {
    pub fn has_played_refutation(&self) -> bool {
        self.lines
            .as_ref()
            .is_some_and(GameReviewObjectiveLines::has_refutation)
    }

    pub fn has_engine_best_line(&self) -> bool {
        self.lines
            .as_ref()
            .is_some_and(GameReviewObjectiveLines::has_engine_best)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewObjectiveLines {
    pub best: Vec<GameReviewLineMove>,
    pub refutation: Vec<GameReviewLineMove>,
    /// What the opponent's first reply in the refutation line does, derived by
    /// the same rule as the played move's own effects.
    ///
    /// The line already says which move the opponent has; this says why it is a
    /// resource, which is the half a Player needs and the half prose could only
    /// invent. Empty when that reply is quiet: a move that takes nothing and
    /// attacks nothing supports no claim, and none is offered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refutation_effects: Vec<GameReviewPlayedMoveEffect>,
    /// What the best line's first move does, by the same rule again.
    ///
    /// On an Improvement Opportunity that move is the better move, and this is
    /// the only fact about it beyond its notation: the line says which move,
    /// this says what it takes or hits. On a Positive Highlight the best move
    /// is usually the played move, whose effects already sit on the moment, so
    /// nothing reads this there. Empty when the move is quiet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub best_move_effects: Vec<GameReviewPlayedMoveEffect>,
}

/// The opponent's answer to the played move, and the one thing it does.
///
/// Both halves together or neither: naming the reply without saying what it
/// does is the line the model was already shown and told not to transcribe,
/// and saying what it does without naming it is unattributable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpponentResource<'a> {
    pub reply: &'a GameReviewLineMove,
    pub does: &'a GameReviewPlayedMoveEffect,
}

impl GameReviewObjectiveLines {
    /// What the opponent can answer with, if the refutation line says anything
    /// about it.
    ///
    /// The single door to this fact. Three surfaces ask the question — the
    /// facts projection, the marker rendering, and the literal allowlist — and
    /// they must give the Player, the model, and the gate the same answer, so
    /// none of them re-derives it. The first effect is the one that speaks: a
    /// reply can take and attack at once, and one concrete thing reads better
    /// than a list, the same choice the achievement rendering makes.
    pub fn opponent_resource(&self) -> Option<OpponentResource<'_>> {
        Some(OpponentResource {
            reply: self.refutation.first()?,
            does: self.refutation_effects.first()?,
        })
    }

    pub fn has_refutation(&self) -> bool {
        !self.refutation.is_empty()
    }

    pub fn has_engine_best(&self) -> bool {
        !self.best.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewLineMove {
    pub uci: String,
    pub san: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewHumanComparison {
    pub most_likely_move_uci: String,
    pub most_likely_probability: Probability,
    pub played_move_probability: Option<Probability>,
    pub played_move_rank: Option<u8>,
    pub played_move_is_human_likely: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewTeachingFacts {
    pub vocabulary_version: GameReviewTeachingVocabularyVersion,
    pub themes: Vec<GameReviewTeachingTheme>,
    pub opening_principles: Vec<GameReviewOpeningPrinciple>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub enum GameReviewTeachingVocabularyVersion {
    #[serde(rename = "teaching-facts/v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameReviewTeachingTheme {
    ForcedMateConversion,
    PassedPawnPromotion,
    QueenExchange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameReviewOpeningPrinciple {
    OccupyTheCenter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewPositionView {
    pub critical_moment_id: CriticalMomentId,
    pub ply: u16,
    pub position_snapshot: PositionSnapshot,
    pub text_board: String,
    pub evaluation: EngineEvaluation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameReviewEvaluationPoint {
    pub ply: u16,
    pub evaluation: EngineEvaluation,
}

#[cfg(test)]
mod tests {
    use super::{
        Color, EngineEvaluation, GameReviewLineMove, GameReviewMechanismPayoff,
        GameReviewMomentClassification, GameReviewObjectiveComparison, GameReviewObjectiveLines,
        GameReviewPieceRole, ObjectiveExcellenceReason, PlayedMoveOutcomeEvidence,
        PositiveHighlightAchievement, PositiveHighlightGrade, PositiveHighlightQualification,
        PositiveHighlightQualificationReason,
    };

    fn comparison(lines: Option<GameReviewObjectiveLines>) -> GameReviewObjectiveComparison {
        let evaluation = EngineEvaluation::Centipawns {
            value: 0,
            perspective: Color::White,
        };
        GameReviewObjectiveComparison {
            best_move_uci: "e2e4".to_string(),
            played_move_uci: "e7e5".to_string(),
            best_evaluation: evaluation.clone(),
            played_evaluation: evaluation,
            centipawn_loss: Some(0),
            principal_variation: Vec::new(),
            lines,
        }
    }

    #[test]
    fn rejects_a_supplied_positive_grade_that_disagrees_with_qualification() {
        let classification = GameReviewMomentClassification::PositiveHighlight {
            qualification: PositiveHighlightQualification {
                reasons: vec![PositiveHighlightQualificationReason::Objective {
                    reason: ObjectiveExcellenceReason::ExactBestMajorAchievement,
                }],
                achievements: vec![PositiveHighlightAchievement::AdvancedPassedPawn {
                    to_square: "e8".to_string(),
                }],
            },
            grade: PositiveHighlightGrade::Great,
        };

        assert!(!classification.is_well_formed());
    }

    #[test]
    fn reads_a_payoff_frozen_before_the_exchange_was_settled() {
        let payoff = serde_json::from_value::<GameReviewMechanismPayoff>(serde_json::json!({
            "kind": "winsMaterial",
            "role": "knight"
        }))
        .expect("a stored review's payoff still reads");

        assert_eq!(
            payoff,
            GameReviewMechanismPayoff::WinsMaterialOutright {
                role: GameReviewPieceRole::Knight
            }
        );
    }

    #[test]
    fn writes_a_settled_payoff_under_its_own_name_only() {
        let written = serde_json::to_value(GameReviewMechanismPayoff::WinsMaterialOutright {
            role: GameReviewPieceRole::Knight,
        })
        .expect("a payoff serializes");

        assert_eq!(
            written,
            serde_json::json!({ "kind": "winsMaterialOutright", "role": "knight" })
        );
    }

    #[test]
    fn rejects_terminal_outcome_payloads_that_mix_in_analyzed_fields() {
        let result = serde_json::from_value::<PlayedMoveOutcomeEvidence>(serde_json::json!({
            "kind": "terminal",
            "outcome": { "kind": "stalemate" },
            "centipawnLoss": 42
        }));

        assert!(result.is_err());
    }

    #[test]
    fn objective_line_predicates_require_nonempty_moves() {
        let empty = comparison(Some(GameReviewObjectiveLines {
            best: Vec::new(),
            refutation: Vec::new(),
            refutation_effects: Vec::new(),
            best_move_effects: Vec::new(),
        }));
        assert!(!empty.has_engine_best_line());
        assert!(!empty.has_played_refutation());

        let filled = comparison(Some(GameReviewObjectiveLines {
            best: vec![GameReviewLineMove {
                uci: "e2e4".to_string(),
                san: "e4".to_string(),
            }],
            refutation_effects: Vec::new(),
            best_move_effects: Vec::new(),
            refutation: vec![GameReviewLineMove {
                uci: "e7e5".to_string(),
                san: "e5".to_string(),
            }],
        }));
        assert!(filled.has_engine_best_line());
        assert!(filled.has_played_refutation());
    }
}
