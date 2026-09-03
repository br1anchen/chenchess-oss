use crate::{
    causal_facts::{self, MechanismPayoff, PlayedMoveEffect, ResidualOutcome, TacticalMechanism},
    domain::{EloProfile, Game},
    engine_analysis::PositionEvaluation,
    position_phase::classify_position_phase,
};

use super::{
    board::{
        board_terminal_outcome, human_evidence_is_legal, legal_move_count, validate_legal_move,
    },
    classification::classify_moment,
    AfterMoveEvidence, CriticalMomentCategory, HumanComparison, MomentFact, MoveEvidence,
    ObjectiveComparison, OpeningPrinciple, RuleExtractorError, TeachingFactVocabularyVersion,
    TeachingFacts, TeachingTheme,
};

const HUMAN_LIKELY_PROBABILITY: f64 = 0.20;

pub(super) struct ExtractedMoment {
    pub(super) fact: MomentFact<Option<PositionEvaluation>, Option<ResidualOutcome>>,
}

impl MomentFact<Option<PositionEvaluation>, Option<ResidualOutcome>> {
    pub(super) fn with_analyzed_evaluation(self) -> Option<MomentFact> {
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
            residual_outcome,
            mechanism,
            teaching,
        } = self;
        let ObjectiveComparison {
            best_move,
            played_move,
            best_evaluation,
            played_evaluation,
            centipawn_loss,
            principal_variation,
        } = objective;

        let played_evaluation = played_evaluation?;
        let residual_outcome = residual_outcome?;

        Some(MomentFact {
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
        })
    }
}

pub(super) fn extract_moment_fact(
    game: &Game,
    game_move: &crate::domain::ImportedMove,
    elo: EloProfile,
    evidence: &MoveEvidence<'_>,
) -> Result<ExtractedMoment, RuleExtractorError> {
    let legal_moves = legal_move_count(&game_move.position).ok_or(
        RuleExtractorError::InvalidClassificationEvidence {
            ply: game_move.ply,
            reason: "the recorded Position cannot be reconstructed",
        },
    )?;
    validate_legal_move(&game_move.position, &game_move.uci, game_move.ply)?;
    validate_legal_move(
        &game_move.position,
        &evidence.engine_before.best_move,
        game_move.ply,
    )?;
    let most_likely = evidence
        .human_before
        .candidates
        .iter()
        .min_by_key(|candidate| candidate.rank)
        .ok_or(RuleExtractorError::NoHumanCandidates { ply: game_move.ply })?;
    let played_evaluation = match evidence.after_move {
        AfterMoveEvidence::Analyzed(evaluation) => Some(
            opposite_perspective(evaluation)
                .ok_or(RuleExtractorError::InvalidEvaluation { ply: game_move.ply })?,
        ),
        AfterMoveEvidence::Terminal => None,
    };
    let centipawn_loss = played_evaluation
        .and_then(|played| centipawn_loss(evidence.engine_before.evaluation, played));
    let played_candidate = evidence
        .human_before
        .candidates
        .iter()
        .find(|candidate| candidate.uci == game_move.uci);
    let played_probability = played_candidate.map(|candidate| candidate.probability);
    let human_evidence_is_legal =
        human_evidence_is_legal(&game_move.position, evidence.human_before);
    let category = if forcing_san(&game_move.san)
        || played_evaluation
            .is_some_and(|played| has_mate_score(evidence.engine_before.evaluation, played))
    {
        CriticalMomentCategory::Tactical
    } else {
        CriticalMomentCategory::Positional
    };
    let causal = causal_facts::extract(
        &game_move.position,
        &game_move.uci,
        &evidence.engine_before.principal_variation,
        matches!(evidence.engine_before.evaluation, PositionEvaluation::MateIn(distance) if distance > 0),
    )
    .map_err(|source| RuleExtractorError::InvalidCausalFacts {
        ply: game_move.ply,
        source,
    })?;
    let residual_outcome = played_evaluation
        .map(|played| causal_facts::classify_residual(evidence.engine_before.evaluation, played));
    let terminal_outcome = match evidence.after_move {
        AfterMoveEvidence::Terminal => Some(board_terminal_outcome(game, game_move)?),
        AfterMoveEvidence::Analyzed(_) => None,
    };
    let teaching = teaching_facts(
        game_move,
        evidence,
        &causal.effects,
        causal.mechanism.as_ref(),
    );
    let classification = classify_moment(
        game_move,
        elo,
        legal_moves,
        evidence.engine_before,
        played_evaluation,
        centipawn_loss,
        played_candidate,
        evidence.human_before,
        human_evidence_is_legal,
        &causal.effects,
        causal.mechanism.as_ref(),
        terminal_outcome,
    )?;

    Ok(ExtractedMoment {
        fact: MomentFact {
            ply: game_move.ply,
            move_number: game_move.move_number,
            side: game_move.side,
            played_san: game_move.san.clone(),
            position_phase: classify_position_phase(game_move),
            classification,
            category,
            objective: ObjectiveComparison {
                best_move: evidence.engine_before.best_move.clone(),
                played_move: game_move.uci.clone(),
                best_evaluation: evidence.engine_before.evaluation,
                played_evaluation,
                centipawn_loss,
                principal_variation: evidence.engine_before.principal_variation.clone(),
            },
            human: HumanComparison {
                most_likely_move: most_likely.uci.clone(),
                most_likely_probability: most_likely.probability,
                played_move_probability: played_probability,
                played_move_rank: played_candidate.map(|candidate| candidate.rank),
                played_move_is_human_likely: played_probability
                    .is_some_and(|probability| probability >= HUMAN_LIKELY_PROBABILITY),
            },
            effects: causal.effects,
            residual_outcome,
            mechanism: causal.mechanism,
            teaching,
        },
    })
}

fn teaching_facts(
    game_move: &crate::domain::ImportedMove,
    evidence: &MoveEvidence<'_>,
    effects: &[PlayedMoveEffect],
    mechanism: Option<&TacticalMechanism>,
) -> TeachingFacts {
    let mut themes = Vec::new();
    if matches!(
        evidence.engine_before.evaluation,
        PositionEvaluation::MateIn(distance) if distance > 0
    ) {
        themes.push(TeachingTheme::ForcedMateConversion);
    }
    if matches!(
        mechanism.map(|mechanism| &mechanism.payoff),
        Some(MechanismPayoff::Promotion)
    ) || effects
        .iter()
        .any(|effect| matches!(effect, PlayedMoveEffect::AdvancedPassedPawn { .. }))
    {
        themes.push(TeachingTheme::PassedPawnPromotion);
    }
    if matches!(
        mechanism.map(|mechanism| &mechanism.payoff),
        Some(MechanismPayoff::QueenExchange)
    ) || effects
        .iter()
        .any(|effect| matches!(effect, PlayedMoveEffect::AllowsQueenExchange))
    {
        themes.push(TeachingTheme::QueenExchange);
    }
    let opening_principles = (game_move.move_number == 1
        && is_initial_central_pawn_advance(&evidence.engine_before.best_move)
        && !is_initial_central_pawn_advance(&game_move.uci))
    .then_some(OpeningPrinciple::OccupyTheCenter)
    .into_iter()
    .collect();

    TeachingFacts {
        vocabulary_version: TeachingFactVocabularyVersion::V1,
        themes,
        opening_principles,
    }
}

fn is_initial_central_pawn_advance(uci: &str) -> bool {
    matches!(uci, "d2d4" | "e2e4" | "d7d5" | "e7e5")
}

fn opposite_perspective(evaluation: PositionEvaluation) -> Option<PositionEvaluation> {
    match evaluation {
        PositionEvaluation::Centipawns(value) => {
            value.checked_neg().map(PositionEvaluation::Centipawns)
        }
        PositionEvaluation::MateIn(value) => value.checked_neg().map(PositionEvaluation::MateIn),
    }
}

fn centipawn_loss(best: PositionEvaluation, played: PositionEvaluation) -> Option<u32> {
    match (best, played) {
        (PositionEvaluation::Centipawns(best), PositionEvaluation::Centipawns(played)) => {
            Some(i64::from(best).saturating_sub(i64::from(played)).max(0) as u32)
        }
        _ => None,
    }
}

fn has_mate_score(best: PositionEvaluation, played: PositionEvaluation) -> bool {
    matches!(best, PositionEvaluation::MateIn(_)) || matches!(played, PositionEvaluation::MateIn(_))
}

fn forcing_san(san: &str) -> bool {
    san.contains(['x', '+', '#', '='])
}
