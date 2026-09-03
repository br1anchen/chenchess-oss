use crate::{
    causal_facts::{PlayedMoveEffect, TacticalMechanism},
    domain::{EloProfile, UserLevel},
    engine_analysis::{EngineAnalysis, PositionEvaluation},
    human_move_model::HumanMovePrediction,
    review_session_contract::{
        BoardTerminalOutcome, EloRelativeQualificationReason, EloRelativeStrength,
        EngineEvaluation, GameReviewMomentClassification, ImprovementCorrection,
        ImprovementOutcome, MateOutcome, NeutralReviewReason, ObjectiveExcellenceReason,
        PositiveHighlightQualification, PositiveHighlightQualificationReason,
    },
};

use super::{
    board::{color_from_side, san_for_uci},
    positive_highlights::positive_achievements,
    RuleExtractorError,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn classify_moment(
    game_move: &crate::domain::ImportedMove,
    elo: EloProfile,
    legal_moves: usize,
    engine: &EngineAnalysis,
    played_evaluation: Option<PositionEvaluation>,
    centipawn_loss: Option<u32>,
    played_candidate: Option<&crate::domain::HumanMoveCandidate>,
    human_prediction: &HumanMovePrediction,
    human_evidence_is_legal: bool,
    effects: &[PlayedMoveEffect],
    mechanism: Option<&TacticalMechanism>,
    terminal_outcome: Option<BoardTerminalOutcome>,
) -> Result<GameReviewMomentClassification, RuleExtractorError> {
    let mechanically_forced = legal_moves <= 1;
    let achievements = positive_achievements(&game_move.uci, effects, mechanism, terminal_outcome);
    let sound = objective_soundness(
        elo,
        engine.evaluation,
        played_evaluation,
        centipawn_loss,
        terminal_outcome,
    );
    let objective_reason = objective_reason(
        engine,
        &game_move.uci,
        played_evaluation,
        terminal_outcome,
        !achievements.is_empty(),
    );
    let elo_reasons = elo_relative_reasons(played_candidate, human_prediction);
    let positive = human_evidence_is_legal
        && !mechanically_forced
        && sound
        && !achievements.is_empty()
        && (objective_reason.is_some() || !elo_reasons.is_empty());
    let correction = improvement_correction(
        game_move,
        elo,
        engine,
        played_evaluation,
        centipawn_loss,
        terminal_outcome,
    )?;

    if positive && correction.is_some() {
        return Err(RuleExtractorError::ContradictoryClassification { ply: game_move.ply });
    }
    if positive {
        let mut reasons = objective_reason
            .map(|reason| PositiveHighlightQualificationReason::Objective { reason })
            .into_iter()
            .collect::<Vec<_>>();
        reasons.extend(elo_reasons);
        let qualification = PositiveHighlightQualification {
            reasons,
            achievements,
        };
        let grade = qualification.derived_grade().ok_or(
            RuleExtractorError::InvalidClassificationEvidence {
                ply: game_move.ply,
                reason: "Positive Highlight has no grade-bearing qualification reason",
            },
        )?;
        return Ok(GameReviewMomentClassification::PositiveHighlight {
            qualification,
            grade,
        });
    }
    if let Some(correction) = correction {
        return Ok(GameReviewMomentClassification::ImprovementOpportunity { correction });
    }

    let mut reasons = Vec::new();
    if mechanically_forced {
        reasons.push(NeutralReviewReason::MechanicallyForcedMove);
    }
    if sound && achievements.is_empty() {
        reasons.push(NeutralReviewReason::SoundWithoutConcreteAchievement);
    }
    if terminal_outcome.is_some() && !sound {
        reasons.push(NeutralReviewReason::NonInstructionalTerminalOutcome);
    }
    if reasons.is_empty() {
        reasons.push(NeutralReviewReason::BelowImprovementThreshold);
    }
    Ok(GameReviewMomentClassification::Neutral { reasons })
}

fn objective_soundness(
    elo: EloProfile,
    best: PositionEvaluation,
    played: Option<PositionEvaluation>,
    centipawn_loss: Option<u32>,
    terminal: Option<BoardTerminalOutcome>,
) -> bool {
    matches!(terminal, Some(BoardTerminalOutcome::Checkmate { .. }))
        && matches!(best, PositionEvaluation::MateIn(distance) if distance > 0)
        || matches!(
            (best, played),
            (PositionEvaluation::MateIn(best_distance), Some(PositionEvaluation::MateIn(played_distance)))
                if best_distance > 0 && played_distance > 0
        )
        || centipawn_loss.is_some_and(|loss| loss <= soundness_threshold(elo))
}

fn objective_reason(
    engine: &EngineAnalysis,
    played_move: &str,
    played_evaluation: Option<PositionEvaluation>,
    terminal: Option<BoardTerminalOutcome>,
    has_achievement: bool,
) -> Option<ObjectiveExcellenceReason> {
    if matches!(terminal, Some(BoardTerminalOutcome::Checkmate { .. }))
        && matches!(engine.evaluation, PositionEvaluation::MateIn(distance) if distance > 0)
        && engine.best_move == played_move
    {
        Some(ObjectiveExcellenceReason::CompletedCheckmate)
    } else if matches!(
        (engine.evaluation, played_evaluation),
        (PositionEvaluation::MateIn(best_distance), Some(PositionEvaluation::MateIn(played_distance)))
            if best_distance > 0 && played_distance > 0
    ) {
        Some(ObjectiveExcellenceReason::PreservedForcedMate)
    } else if engine.best_move == played_move && has_achievement {
        Some(ObjectiveExcellenceReason::ExactBestMajorAchievement)
    } else {
        None
    }
}

fn elo_relative_reasons(
    played_candidate: Option<&crate::domain::HumanMoveCandidate>,
    prediction: &HumanMovePrediction,
) -> Vec<PositiveHighlightQualificationReason> {
    let Some(candidate) = played_candidate else {
        return vec![PositiveHighlightQualificationReason::EloRelative {
            reason: EloRelativeQualificationReason::OutsideRecordedCohort,
            strength: EloRelativeStrength::Strong,
        }];
    };
    let mut reasons = Vec::new();
    if candidate.rank >= 3 {
        reasons.push(elo_reason(
            EloRelativeQualificationReason::RarePlayedMoveRank,
            if candidate.rank >= 5 {
                EloRelativeStrength::Strong
            } else {
                EloRelativeStrength::Notable
            },
        ));
    }
    if candidate.probability <= 0.15 {
        reasons.push(elo_reason(
            EloRelativeQualificationReason::LowPlayedMoveProbability,
            if candidate.probability <= 0.05 {
                EloRelativeStrength::Strong
            } else {
                EloRelativeStrength::Notable
            },
        ));
    }
    if let Some(top) = prediction
        .candidates
        .iter()
        .min_by_key(|candidate| candidate.rank)
    {
        let ratio = candidate.probability / top.probability;
        if ratio <= 0.50 {
            reasons.push(elo_reason(
                EloRelativeQualificationReason::LowProbabilityRelativeToTopMove,
                if ratio <= 0.20 {
                    EloRelativeStrength::Strong
                } else {
                    EloRelativeStrength::Notable
                },
            ));
        }
    }
    reasons
}

fn elo_reason(
    reason: EloRelativeQualificationReason,
    strength: EloRelativeStrength,
) -> PositiveHighlightQualificationReason {
    PositiveHighlightQualificationReason::EloRelative { reason, strength }
}

fn improvement_correction(
    game_move: &crate::domain::ImportedMove,
    elo: EloProfile,
    engine: &EngineAnalysis,
    played_evaluation: Option<PositionEvaluation>,
    centipawn_loss: Option<u32>,
    terminal: Option<BoardTerminalOutcome>,
) -> Result<Option<ImprovementCorrection>, RuleExtractorError> {
    if engine.best_move == game_move.uci {
        return Ok(None);
    }
    let better_move_san = san_for_uci(&game_move.position, &engine.best_move, game_move.ply)?;
    if let Some(terminal) = terminal {
        let mover_was_mated = match terminal {
            BoardTerminalOutcome::Checkmate { winner } => color_from_side(game_move.side) != winner,
            BoardTerminalOutcome::Stalemate | BoardTerminalOutcome::InsufficientMaterial => false,
        };
        return Ok(mover_was_mated.then(|| ImprovementCorrection {
            better_move_uci: engine.best_move.clone(),
            better_move_san,
            outcome: ImprovementOutcome::AvoidedTerminal { avoided: terminal },
        }));
    }
    let Some(played) = played_evaluation else {
        return Ok(None);
    };
    let deteriorated_mate = matches!(engine.evaluation, PositionEvaluation::MateIn(best) if best > 0)
        && !matches!(played, PositionEvaluation::MateIn(actual) if actual > 0);
    let better_evaluation = contract_evaluation(engine.evaluation, game_move.side)?;
    Ok(
        (centipawn_loss.is_some_and(|loss| loss >= mistake_threshold(elo)) || deteriorated_mate)
            .then(|| ImprovementCorrection {
                better_move_uci: engine.best_move.clone(),
                better_move_san,
                outcome: ImprovementOutcome::ImprovedAnalyzed { better_evaluation },
            }),
    )
}

fn contract_evaluation(
    evaluation: PositionEvaluation,
    side: crate::domain::MoveSide,
) -> Result<EngineEvaluation, RuleExtractorError> {
    let perspective = color_from_side(side);
    Ok(match evaluation {
        PositionEvaluation::Centipawns(value) => {
            EngineEvaluation::Centipawns { value, perspective }
        }
        PositionEvaluation::MateIn(distance) => EngineEvaluation::Mate {
            outcome: if distance > 0 {
                MateOutcome::Win
            } else {
                MateOutcome::Loss
            },
            distance_plies: u16::try_from(distance.unsigned_abs()).map_err(|_| {
                RuleExtractorError::InvalidClassificationEvidence {
                    ply: 0,
                    reason: "mate distance exceeds the canonical contract limit",
                }
            })?,
            perspective,
        },
    })
}

fn soundness_threshold(elo: EloProfile) -> u32 {
    match UserLevel::from_elo(elo.rating()) {
        UserLevel::Beginner => 50,
        UserLevel::Intermediate => 35,
        UserLevel::Advanced => 25,
    }
}

fn mistake_threshold(elo: EloProfile) -> u32 {
    match UserLevel::from_elo(elo.rating()) {
        UserLevel::Beginner => 150,
        UserLevel::Intermediate => 100,
        UserLevel::Advanced => 70,
    }
}
