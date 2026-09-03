use crate::{
    causal_facts::ResidualClassification,
    critical_moment_selector::{Candidate, CandidateKind},
    engine_analysis::PositionEvaluation,
    review_session_contract::GameReviewMomentClassification,
};

use super::{facts::ExtractedMoment, CriticalMomentCategory, MomentFact};

pub(super) fn automatic_candidate(extracted: ExtractedMoment) -> Option<Candidate<MomentFact>> {
    let fact = extracted.fact.with_analyzed_evaluation()?;
    let phase = fact.position_phase.phase;
    let kind = match &fact.classification {
        GameReviewMomentClassification::PositiveHighlight { grade, .. } => {
            CandidateKind::from_positive_grade(*grade)
        }
        GameReviewMomentClassification::ImprovementOpportunity { .. } => improvement_kind(&fact),
        GameReviewMomentClassification::Neutral { .. } => return None,
    };
    let evidence_strength = evidence_strength(&fact, kind);
    Some(Candidate {
        ply: fact.ply,
        side: fact.side,
        kind,
        tactical: matches!(fact.category, CriticalMomentCategory::Tactical),
        phase,
        evidence_strength,
        episode: None,
        payload: fact,
    })
}

fn improvement_kind(fact: &MomentFact) -> CandidateKind {
    if matches!(
        (fact.objective.best_evaluation, fact.objective.played_evaluation),
        (PositionEvaluation::MateIn(best), PositionEvaluation::MateIn(played)) if best > 0 && played <= 0
    ) {
        return CandidateKind::ForcedMateDeterioration;
    }
    match fact.residual_outcome.classification {
        ResidualClassification::MissedForcedMate => CandidateKind::ForcedMateDeterioration,
        ResidualClassification::AdvantageLost => CandidateKind::AdvantageLost,
        ResidualClassification::NowWorse => CandidateKind::NowWorse,
        ResidualClassification::AdvantageReduced => CandidateKind::AdvantageReduced,
        ResidualClassification::AdvantageKept | ResidualClassification::StandingKept => {
            CandidateKind::StandingKept
        }
    }
}

fn evidence_strength(fact: &MomentFact, kind: CandidateKind) -> u8 {
    if kind == CandidateKind::ForcedMateDeterioration {
        return 99;
    }
    let objective = fact.objective.centipawn_loss.unwrap_or_default().min(600) / 10;
    let probability_gap = fact
        .human
        .played_move_probability
        .map(|played| (fact.human.most_likely_probability - played).max(0.0))
        .unwrap_or(fact.human.most_likely_probability);
    let human = (probability_gap * 25.0).round() as u32;
    let rank = fact
        .human
        .played_move_rank
        .map(|rank| rank.saturating_sub(1).min(14) as u32)
        .unwrap_or(15);
    let positive = match &fact.classification {
        GameReviewMomentClassification::PositiveHighlight { qualification, .. } => {
            let reasons =
                u32::try_from(qualification.reasons.len()).expect("small fixed evidence list");
            let achievements =
                u32::try_from(qualification.achievements.len()).expect("small fixed evidence list");
            reasons
                .saturating_mul(8)
                .saturating_add(achievements.saturating_mul(6))
        }
        _ => 0,
    };
    u8::try_from(
        objective
            .saturating_add(human)
            .saturating_add(rank)
            .saturating_add(positive)
            .min(99),
    )
    .expect("strength is explicitly bounded")
}
