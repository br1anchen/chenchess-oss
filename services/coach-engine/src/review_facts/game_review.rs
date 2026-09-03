use shakmaty::{fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, Move, Position};

use crate::{
    causal_facts::{
        AdvantageStanding, LineMove, MechanismPayoff, PieceRole, PlayedMoveEffect,
        ResidualClassification, ResidualOutcome, TacticalMechanism,
    },
    decision_learning::{apply_decision_learning, automatic_learning_plan},
    learning_plan::build_opening_material,
    moment_display,
    review_session_contract::{
        BoardTerminalOutcome, Color, CriticalMomentId, DecisionLearningOutcome, EloRating,
        EngineEvaluation, GameRef, GameReview, GameReviewAdvantageStanding,
        GameReviewCriticalMoment, GameReviewCriticalMomentCategory, GameReviewEvaluationDisplay,
        GameReviewEvaluationPoint, GameReviewHumanComparison, GameReviewLineMove,
        GameReviewMechanismPayoff, GameReviewMomentDisplay, GameReviewMomentProvenance,
        GameReviewObjectiveComparison, GameReviewObjectiveLines, GameReviewOpeningPrinciple,
        GameReviewPieceRole, GameReviewPlayedMoveEffect, GameReviewPlayerLevel,
        GameReviewPlayerProfile, GameReviewPositionView, GameReviewResidualClassification,
        GameReviewResidualOutcome, GameReviewTacticalMechanism, GameReviewTeachingFacts,
        GameReviewTeachingTheme, GameReviewTeachingVocabularyVersion, MateOutcome, OpeningMetadata,
        PlayedMoveOutcomeEvidence, Probability, ReviewMomentLearningMaterial,
    },
    rule_extractor::{
        CriticalMomentCategory, MomentFact, ObjectiveComparison, OpeningPrinciple,
        TeachingFactVocabularyVersion, TeachingTheme,
    },
    types::{EloProfile, Game, MoveSide, ReviewSide, UserLevel},
};

use super::{ProviderEvidence, ReviewAnalysis, ReviewFactsError};

pub(super) fn build(
    output: ReviewAnalysis,
    evidence: Vec<ProviderEvidence>,
    game_ref: &GameRef,
) -> Result<GameReview, ReviewFactsError> {
    let ReviewAnalysis {
        game,
        facts,
        position_views,
        opening_identification,
        mut decision_builds,
    } = output;
    let mut critical_moments = facts
        .critical_moments
        .iter()
        .map(|moment| {
            critical_moment(
                moment,
                game_ref,
                &game,
                &evidence,
                GameReviewMomentProvenance::Automatic,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (source, moment) in facts.critical_moments.iter().zip(&mut critical_moments) {
        let opening_material =
            build_opening_material(&game, source, game_ref, &opening_identification);
        let decision_build = decision_builds.remove(&source.ply).ok_or_else(|| {
            ReviewFactsError::Contract(format!(
                "automatic Review Moment at ply {} has no decision-learning outcome",
                source.ply
            ))
        })?;
        apply_decision_learning(game_ref, moment, decision_build, &opening_material)
            .map_err(|error| ReviewFactsError::Contract(error.to_string()))?;
    }
    if !decision_builds.is_empty() {
        return Err(ReviewFactsError::Contract(
            "decision-learning outcomes do not belong to selected Review Moments".to_string(),
        ));
    }
    let learning_plan = automatic_learning_plan(&critical_moments)
        .map_err(|error| ReviewFactsError::Contract(error.to_string()))?;
    let position_views = position_views
        .into_iter()
        .map(|view| {
            let moment = critical_moments
                .iter()
                .find(|moment| usize::from(moment.ply) == view.ply)
                .expect("every review Position View belongs to a Critical Moment");
            Ok(GameReviewPositionView {
                critical_moment_id: moment.critical_moment_id.clone(),
                ply: moment.ply,
                evaluation: evaluation(view.evaluation, moment.side)?,
                position_snapshot: view.position_snapshot,
                text_board: view.text_board,
            })
        })
        .collect::<Result<Vec<_>, ReviewFactsError>>()?;
    let evaluation_timeline = evidence
        .iter()
        .map(|item| {
            let ply = contract_ply(item.ply)?;
            let side = if ply % 2 == 1 {
                Color::White
            } else {
                Color::Black
            };
            Ok(GameReviewEvaluationPoint {
                ply,
                evaluation: evaluation(item.engine_before.evaluation, side)?,
            })
        })
        .collect::<Result<Vec<_>, ReviewFactsError>>()?;
    Ok(GameReview {
        summary: facts.summary,
        player_profile: GameReviewPlayerProfile {
            elo: EloRating::try_from(facts.player_profile.elo)
                .map_err(|error| ReviewFactsError::Contract(error.to_string()))?,
            level: match facts.player_profile.level {
                UserLevel::Beginner => GameReviewPlayerLevel::Beginner,
                UserLevel::Intermediate => GameReviewPlayerLevel::Intermediate,
                UserLevel::Advanced => GameReviewPlayerLevel::Advanced,
            },
            coaching_focus: facts.player_profile.coaching_focus,
        },
        critical_moments,
        position_views,
        evaluation_timeline,
        learning_plan,
    })
}

pub(super) fn player_selected_moments(
    game: &Game,
    elo: EloProfile,
    evidence: &[ProviderEvidence],
    game_ref: &GameRef,
    opening_identification: &OpeningMetadata,
) -> Result<Vec<GameReviewCriticalMoment>, ReviewFactsError> {
    evidence
        .iter()
        .map(|item| {
            let extracted = crate::rule_extractor::extract_selected_moment(
                game,
                elo,
                &item.as_rule_evidence(),
            )?;
            let (moment, terminal_outcome) =
                concrete_player_selected_moment(extracted.selected_moment, game)?;
            let opening_material =
                build_opening_material(game, &moment, game_ref, opening_identification);
            let mut presented = critical_moment(
                &moment,
                game_ref,
                game,
                evidence,
                GameReviewMomentProvenance::PlayerSelected,
            )?;
            presented.learning_material = opening_material;
            if let Some(outcome) = terminal_outcome {
                presented.played_move_outcome = PlayedMoveOutcomeEvidence::Terminal { outcome };
            }
            Ok(presented)
        })
        .collect()
}

/// The presented Critical Moments of a *recorded* case, with no provider call.
///
/// The corpus freezes the provider evidence precisely so a case can be replayed
/// without Stockfish or the human move model, and Language Layer authoring needs
/// the presented moment rather than the extractor's `MomentFact`. Everything the
/// comment gate reads — classification, evaluations, effects, display, opening
/// learning material — is derivable from that recording.
///
/// What is *not* replayed here is the Decision Explanation and the projected
/// learning tracks it produces, because those come from the MultiPV comparison
/// rather than from the single-PV recording. The learning material is therefore
/// the opening projection alone, which a caller measuring learning-resource
/// grounding must account for.
pub(crate) fn recorded_critical_moments(
    game: &Game,
    elo: EloProfile,
    review_side: ReviewSide,
    evidence: &[ProviderEvidence],
    game_ref: &GameRef,
) -> Result<Vec<GameReviewCriticalMoment>, ReviewFactsError> {
    let rule_evidence = evidence
        .iter()
        .map(ProviderEvidence::as_rule_evidence)
        .collect::<Vec<_>>();
    let facts = crate::rule_extractor::extract(game, elo, review_side, &rule_evidence)?;
    facts
        .critical_moments
        .iter()
        .map(|source| {
            let mut moment = critical_moment(
                source,
                game_ref,
                game,
                evidence,
                GameReviewMomentProvenance::Automatic,
            )?;
            moment.learning_material =
                build_opening_material(game, source, game_ref, &OpeningMetadata::Absent);
            Ok(moment)
        })
        .collect()
}

/// The Player-selected counterpart of [`recorded_critical_moments`], for the
/// corpus cases whose operation is one named ply.
pub(crate) fn recorded_selected_moment(
    game: &Game,
    elo: EloProfile,
    evidence: &[ProviderEvidence],
    game_ref: &GameRef,
) -> Result<Vec<GameReviewCriticalMoment>, ReviewFactsError> {
    player_selected_moments(game, elo, evidence, game_ref, &OpeningMetadata::Absent)
}

fn critical_moment(
    moment: &crate::rule_extractor::MomentFact,
    game_ref: &GameRef,
    game: &Game,
    evidence: &[ProviderEvidence],
    provenance: GameReviewMomentProvenance,
) -> Result<GameReviewCriticalMoment, ReviewFactsError> {
    let ply = contract_ply(moment.ply)?;
    let side = color(moment.side);
    if !moment.classification.is_well_formed() {
        return Err(ReviewFactsError::Contract(
            "Critical Moment classification is malformed".to_string(),
        ));
    }
    Ok(GameReviewCriticalMoment {
        critical_moment_id: CriticalMomentId::for_imported_game(game_ref, ply),
        ply,
        move_number: u16::try_from(moment.move_number)
            .map_err(|_| ReviewFactsError::Contract("move number exceeds v1 limits".to_string()))?,
        side,
        played_san: moment.played_san.clone(),
        position_phase: moment.position_phase,
        classification: moment.classification.clone(),
        provenance,
        category: match moment.category {
            CriticalMomentCategory::Tactical => GameReviewCriticalMomentCategory::Tactical,
            CriticalMomentCategory::Positional => GameReviewCriticalMomentCategory::Positional,
        },
        objective: GameReviewObjectiveComparison {
            best_move_uci: moment.objective.best_move.clone(),
            played_move_uci: moment.objective.played_move.clone(),
            best_evaluation: evaluation(moment.objective.best_evaluation, side)?,
            played_evaluation: evaluation(moment.objective.played_evaluation, side)?,
            centipawn_loss: moment.objective.centipawn_loss,
            principal_variation: moment.objective.principal_variation.clone(),
            lines: objective_lines(moment, game, evidence)?,
        },
        effects: moment.effects.iter().map(played_move_effect).collect(),
        residual_outcome: residual_outcome(moment.residual_outcome),
        played_move_outcome: PlayedMoveOutcomeEvidence::Analyzed {
            played_evaluation: evaluation(moment.objective.played_evaluation, side)?,
            centipawn_loss: moment.objective.centipawn_loss,
            residual_outcome: residual_outcome(moment.residual_outcome),
        },
        mechanism: moment
            .mechanism
            .as_ref()
            .map(tactical_mechanism)
            .transpose()?,
        human: GameReviewHumanComparison {
            most_likely_move_uci: moment.human.most_likely_move.clone(),
            most_likely_probability: probability(moment.human.most_likely_probability)?,
            played_move_probability: moment
                .human
                .played_move_probability
                .map(probability)
                .transpose()?,
            played_move_rank: moment
                .human
                .played_move_rank
                .map(|rank| {
                    u8::try_from(rank).map_err(|_| {
                        ReviewFactsError::Contract("Human Move rank exceeds v1 limits".to_string())
                    })
                })
                .transpose()?,
            played_move_is_human_likely: moment.human.played_move_is_human_likely,
        },
        teaching: GameReviewTeachingFacts {
            vocabulary_version: match moment.teaching.vocabulary_version {
                TeachingFactVocabularyVersion::V1 => GameReviewTeachingVocabularyVersion::V1,
            },
            themes: moment
                .teaching
                .themes
                .iter()
                .map(|theme| teaching_theme(*theme))
                .collect(),
            opening_principles: moment
                .teaching
                .opening_principles
                .iter()
                .map(|principle| match principle {
                    OpeningPrinciple::OccupyTheCenter => {
                        GameReviewOpeningPrinciple::OccupyTheCenter
                    }
                })
                .collect(),
        },
        decision_explanation_ref: None,
        decision_explanation: None,
        decision_learning_outcome: DecisionLearningOutcome::NotAttempted,
        learning_material: ReviewMomentLearningMaterial::empty(),
        display: moment_display(moment),
        comment: None,
    })
}

fn concrete_player_selected_moment(
    moment: MomentFact<Option<crate::engine_analysis::PositionEvaluation>, Option<ResidualOutcome>>,
    game: &Game,
) -> Result<(MomentFact, Option<BoardTerminalOutcome>), ReviewFactsError> {
    let terminal_outcome = match (moment.objective.played_evaluation, moment.residual_outcome) {
        (Some(_), Some(_)) => None,
        (None, None) => {
            let game_move = game
                .moves
                .iter()
                .find(|game_move| game_move.ply == moment.ply)
                .ok_or(ReviewFactsError::UnknownSelectedPly(moment.ply))?;
            Some(crate::rule_extractor::board_terminal_outcome(
                game, game_move,
            )?)
        }
        _ => {
            return Err(ReviewFactsError::Contract(
                "Player-Selected Moment has inconsistent played-move outcome evidence".to_string(),
            ));
        }
    };
    let played_evaluation = moment.objective.played_evaluation.unwrap_or_else(|| {
        terminal_position_evaluation(
            terminal_outcome.expect("a missing played evaluation has a terminal outcome"),
            moment.side,
        )
    });
    let residual_outcome = moment.residual_outcome.unwrap_or_else(|| {
        crate::causal_facts::classify_residual(moment.objective.best_evaluation, played_evaluation)
    });
    let MomentFact {
        ply,
        move_number,
        side,
        played_san,
        position_phase,
        classification,
        category,
        objective,
        human,
        effects,
        residual_outcome: _,
        mechanism,
        teaching,
    } = moment;
    let ObjectiveComparison {
        best_move,
        played_move,
        best_evaluation,
        played_evaluation: _,
        centipawn_loss,
        principal_variation,
    } = objective;
    Ok((
        MomentFact {
            ply,
            move_number,
            side,
            played_san,
            position_phase,
            classification,
            category,
            objective: ObjectiveComparison {
                best_move,
                played_move,
                best_evaluation,
                played_evaluation,
                centipawn_loss,
                principal_variation,
            },
            human,
            effects,
            residual_outcome,
            mechanism,
            teaching,
        },
        terminal_outcome,
    ))
}

/// The v1 presented-moment shape still has mandatory comparison/display slots.
/// For terminal positions these values are deterministic board-result mirrors;
/// `played_move_outcome` remains the canonical terminal fact used by prose.
fn terminal_position_evaluation(
    outcome: BoardTerminalOutcome,
    mover: MoveSide,
) -> crate::engine_analysis::PositionEvaluation {
    match outcome {
        BoardTerminalOutcome::Checkmate { winner } => {
            crate::engine_analysis::PositionEvaluation::MateIn(if winner == color(mover) {
                1
            } else {
                -1
            })
        }
        BoardTerminalOutcome::Stalemate | BoardTerminalOutcome::InsufficientMaterial => {
            crate::engine_analysis::PositionEvaluation::Centipawns(0)
        }
    }
}

fn moment_display(moment: &crate::rule_extractor::MomentFact) -> GameReviewMomentDisplay {
    GameReviewMomentDisplay {
        played_annotation: moment_display::annotation(
            moment.objective.best_move == moment.objective.played_move,
            moment.objective.centipawn_loss,
            moment.objective.best_evaluation,
            Some(moment.objective.played_evaluation),
        ),
        best_evaluation: evaluation_display(moment.objective.best_evaluation, moment.side),
        played_evaluation: evaluation_display(moment.objective.played_evaluation, moment.side),
        loss_pawns: moment
            .objective
            .centipawn_loss
            .map(moment_display::loss_pawns),
    }
}

fn evaluation_display(
    evaluation: crate::engine_analysis::PositionEvaluation,
    side: MoveSide,
) -> GameReviewEvaluationDisplay {
    let display = moment_display::evaluation_display(evaluation, side);
    GameReviewEvaluationDisplay {
        score: display.score,
        label: display.label,
    }
}

fn teaching_theme(theme: TeachingTheme) -> GameReviewTeachingTheme {
    match theme {
        TeachingTheme::ForcedMateConversion => GameReviewTeachingTheme::ForcedMateConversion,
        TeachingTheme::PassedPawnPromotion => GameReviewTeachingTheme::PassedPawnPromotion,
        TeachingTheme::QueenExchange => GameReviewTeachingTheme::QueenExchange,
    }
}

fn piece_role(role: PieceRole) -> GameReviewPieceRole {
    match role {
        PieceRole::Pawn => GameReviewPieceRole::Pawn,
        PieceRole::Knight => GameReviewPieceRole::Knight,
        PieceRole::Bishop => GameReviewPieceRole::Bishop,
        PieceRole::Rook => GameReviewPieceRole::Rook,
        PieceRole::Queen => GameReviewPieceRole::Queen,
    }
}

fn played_move_effect(effect: &PlayedMoveEffect) -> GameReviewPlayedMoveEffect {
    match effect {
        PlayedMoveEffect::CapturedPiece { role, square } => {
            GameReviewPlayedMoveEffect::CapturedPiece {
                role: piece_role(*role),
                square: square.clone(),
            }
        }
        PlayedMoveEffect::AdvancedPassedPawn { to_square } => {
            GameReviewPlayedMoveEffect::AdvancedPassedPawn {
                to_square: to_square.clone(),
            }
        }
        PlayedMoveEffect::AttackedPiece { role, square } => {
            GameReviewPlayedMoveEffect::AttackedPiece {
                role: piece_role(*role),
                square: square.clone(),
            }
        }
        PlayedMoveEffect::AllowsQueenExchange => GameReviewPlayedMoveEffect::AllowsQueenExchange,
    }
}

fn advantage_standing(standing: AdvantageStanding) -> GameReviewAdvantageStanding {
    match standing {
        AdvantageStanding::Winning => GameReviewAdvantageStanding::Winning,
        AdvantageStanding::Favorable => GameReviewAdvantageStanding::Favorable,
        AdvantageStanding::Balanced => GameReviewAdvantageStanding::Balanced,
        AdvantageStanding::Unfavorable => GameReviewAdvantageStanding::Unfavorable,
        AdvantageStanding::Losing => GameReviewAdvantageStanding::Losing,
    }
}

fn residual_outcome(outcome: ResidualOutcome) -> GameReviewResidualOutcome {
    GameReviewResidualOutcome {
        standing_before: advantage_standing(outcome.standing_before),
        standing_after: advantage_standing(outcome.standing_after),
        classification: match outcome.classification {
            ResidualClassification::MissedForcedMate => {
                GameReviewResidualClassification::MissedForcedMate
            }
            ResidualClassification::AdvantageKept => {
                GameReviewResidualClassification::AdvantageKept
            }
            ResidualClassification::StandingKept => GameReviewResidualClassification::StandingKept,
            ResidualClassification::AdvantageReduced => {
                GameReviewResidualClassification::AdvantageReduced
            }
            ResidualClassification::AdvantageLost => {
                GameReviewResidualClassification::AdvantageLost
            }
            ResidualClassification::NowWorse => GameReviewResidualClassification::NowWorse,
        },
    }
}

fn tactical_mechanism(
    mechanism: &TacticalMechanism,
) -> Result<GameReviewTacticalMechanism, ReviewFactsError> {
    Ok(GameReviewTacticalMechanism {
        moves: mechanism.moves.iter().map(line_move).collect(),
        forcing_index: u16::try_from(mechanism.forcing_index).map_err(|_| {
            ReviewFactsError::Contract("mechanism index exceeds v1 limits".to_string())
        })?,
        payoff: match &mechanism.payoff {
            MechanismPayoff::Mate => GameReviewMechanismPayoff::Mate,
            MechanismPayoff::Promotion => GameReviewMechanismPayoff::Promotion,
            MechanismPayoff::WinsMaterialOutright { role } => {
                GameReviewMechanismPayoff::WinsMaterialOutright {
                    role: piece_role(*role),
                }
            }
            MechanismPayoff::WinsMaterialNet {
                role,
                net_pawn_units,
            } => GameReviewMechanismPayoff::WinsMaterialNet {
                role: piece_role(*role),
                net_pawn_units: *net_pawn_units,
            },
            MechanismPayoff::QueenExchange => GameReviewMechanismPayoff::QueenExchange,
        },
    })
}

fn line_move(line_move: &LineMove) -> GameReviewLineMove {
    GameReviewLineMove {
        uci: line_move.uci.clone(),
        san: line_move.san.clone(),
    }
}

fn objective_lines(
    moment: &crate::rule_extractor::MomentFact,
    game: &Game,
    evidence: &[ProviderEvidence],
) -> Result<Option<GameReviewObjectiveLines>, ReviewFactsError> {
    let (index, game_move) = game
        .moves
        .iter()
        .enumerate()
        .find(|(_, game_move)| game_move.ply == moment.ply)
        .ok_or(ReviewFactsError::UnknownSelectedPly(moment.ply))?;
    let post_move_line = evidence
        .iter()
        .find(|item| item.ply == moment.ply)
        .and_then(|item| match &item.after_move {
            super::ProviderAfterMove::Analyzed {
                principal_variation,
                ..
            } => Some(principal_variation.as_slice()),
            super::ProviderAfterMove::Terminal => None,
        })
        .unwrap_or_default();
    if moment.objective.principal_variation.is_empty() || post_move_line.is_empty() {
        return Ok(None);
    }

    let post_move_fen = game
        .moves
        .get(index + 1)
        .map_or(game.final_position.as_str(), |next| next.position.as_str());
    Ok(Some(GameReviewObjectiveLines {
        best: san_line(
            &game_move.position,
            &moment.objective.principal_variation,
            moment.ply,
        )?,
        refutation: san_line(post_move_fen, post_move_line, moment.ply)?,
        refutation_effects: first_move_effects(post_move_fen, post_move_line, moment.ply)?,
        best_move_effects: first_move_effects(
            &game_move.position,
            &moment.objective.principal_variation,
            moment.ply,
        )?,
    }))
}

/// What a line's first move does, by the rule that derives the played move's
/// own effects.
///
/// The first ply is the only one worth a claim. For the refutation line it is
/// the reply the Player's move hands over, and past it the line is both sides'
/// best play, saying nothing about what the opponent can do *to* them. For the
/// best line it is the move the Player did not play, and past it the line is
/// the engine's, not a move anyone chose.
fn first_move_effects(
    fen: &str,
    principal_variation: &[String],
    ply: usize,
) -> Result<Vec<GameReviewPlayedMoveEffect>, ReviewFactsError> {
    let Some(uci) = principal_variation.first() else {
        return Ok(Vec::new());
    };
    let position = position_from(fen, ply)?;
    let reply = parse_move(uci, &position, ply)?;
    Ok(crate::causal_facts::played_move_effects(&position, &reply)
        .iter()
        .map(played_move_effect)
        .collect())
}

fn san_line(
    fen: &str,
    principal_variation: &[String],
    ply: usize,
) -> Result<Vec<GameReviewLineMove>, ReviewFactsError> {
    let mut position = position_from(fen, ply)?;
    principal_variation
        .iter()
        .map(|uci| {
            let chess_move = parse_move(uci, &position, ply)?;
            let san = SanPlus::from_move(position.clone(), &chess_move).to_string();
            position.play_unchecked(&chess_move);
            Ok(GameReviewLineMove {
                uci: uci.clone(),
                san,
            })
        })
        .collect()
}

/// One engine line's starting position. Every caller that walks a line needs
/// it, and an unparseable FEN is the same failure for all of them.
fn position_from(fen: &str, ply: usize) -> Result<Chess, ReviewFactsError> {
    Fen::from_ascii(fen.as_bytes())
        .map_err(|_| ReviewFactsError::InvalidEngineLine(ply))?
        .into_position(CastlingMode::Standard)
        .map_err(|_| ReviewFactsError::InvalidEngineLine(ply))
}

/// One UCI move of a line, against the position it is played in.
fn parse_move(uci: &str, position: &Chess, ply: usize) -> Result<Move, ReviewFactsError> {
    UciMove::from_ascii(uci.as_bytes())
        .map_err(|_| ReviewFactsError::InvalidEngineLine(ply))?
        .to_move(position)
        .map_err(|_| ReviewFactsError::InvalidEngineLine(ply))
}

fn contract_ply(ply: usize) -> Result<u16, ReviewFactsError> {
    u16::try_from(ply)
        .map_err(|_| ReviewFactsError::Contract("ply exceeds review-session limits".to_string()))
}

pub(super) fn color(side: MoveSide) -> Color {
    match side {
        MoveSide::White => Color::White,
        MoveSide::Black => Color::Black,
    }
}

pub(super) fn evaluation(
    evaluation: crate::engine_analysis::PositionEvaluation,
    perspective: Color,
) -> Result<EngineEvaluation, ReviewFactsError> {
    Ok(match evaluation {
        crate::engine_analysis::PositionEvaluation::Centipawns(value) => {
            EngineEvaluation::Centipawns { value, perspective }
        }
        crate::engine_analysis::PositionEvaluation::MateIn(distance) => EngineEvaluation::Mate {
            outcome: if distance > 0 {
                MateOutcome::Win
            } else {
                MateOutcome::Loss
            },
            distance_plies: u16::try_from(distance.unsigned_abs()).map_err(|_| {
                ReviewFactsError::Contract("mate distance exceeds v1 limits".to_string())
            })?,
            perspective,
        },
    })
}

fn probability(value: f64) -> Result<Probability, ReviewFactsError> {
    Probability::try_from(value).map_err(|error| ReviewFactsError::Contract(error.to_string()))
}
