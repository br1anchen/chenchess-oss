use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    causal_facts::{CausalFactError, PlayedMoveEffect, ResidualOutcome, TacticalMechanism},
    critical_moment_selector::{self, Selection as SelectorSelection},
    domain::{EloProfile, Game, MoveSide, PlayerProfile, ReviewSide},
    engine_analysis::{EngineAnalysis, PositionEvaluation},
    human_move_model::HumanMovePrediction,
    review_session_contract::{GameReviewMomentClassification, PositionPhase},
};

mod board;
mod candidates;
mod classification;
mod facts;
mod positive_highlights;
#[cfg(test)]
mod tests;

pub use board::board_terminal_outcome;

use candidates::automatic_candidate;
use facts::extract_moment_fact;

pub struct MoveEvidence<'a> {
    pub ply: usize,
    pub engine_before: &'a EngineAnalysis,
    pub after_move: AfterMoveEvidence,
    pub human_before: &'a HumanMovePrediction,
}

#[derive(Clone, Copy)]
pub enum AfterMoveEvidence {
    Analyzed(PositionEvaluation),
    Terminal,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleExtraction {
    pub summary: String,
    pub player_profile: PlayerProfile,
    pub critical_moments: Vec<MomentFact>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleExtractionWithTrace {
    pub facts: RuleExtraction,
    pub selector_trace: SelectorTrace,
}

pub use crate::critical_moment_selector::SelectorTrace;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedMomentExtraction {
    pub summary: String,
    pub player_profile: PlayerProfile,
    pub selected_moment: MomentFact<Option<PositionEvaluation>, Option<ResidualOutcome>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentFact<PlayedEvaluation = PositionEvaluation, Outcome = ResidualOutcome> {
    pub ply: usize,
    pub move_number: u32,
    pub side: MoveSide,
    pub played_san: String,
    pub position_phase: PositionPhase,
    pub classification: GameReviewMomentClassification,
    pub category: CriticalMomentCategory,
    pub objective: ObjectiveComparison<PlayedEvaluation>,
    pub human: HumanComparison,
    pub effects: Vec<PlayedMoveEffect>,
    pub residual_outcome: Outcome,
    pub mechanism: Option<TacticalMechanism>,
    pub teaching: TeachingFacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingFacts {
    pub vocabulary_version: TeachingFactVocabularyVersion,
    pub themes: Vec<TeachingTheme>,
    pub opening_principles: Vec<OpeningPrinciple>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeachingFactVocabularyVersion {
    #[serde(rename = "teaching-facts/v1")]
    V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TeachingTheme {
    ForcedMateConversion,
    PassedPawnPromotion,
    QueenExchange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum OpeningPrinciple {
    OccupyTheCenter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CriticalMomentCategory {
    Tactical,
    Positional,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveComparison<PlayedEvaluation = PositionEvaluation> {
    pub best_move: String,
    pub played_move: String,
    pub best_evaluation: PositionEvaluation,
    pub played_evaluation: PlayedEvaluation,
    pub centipawn_loss: Option<u32>,
    pub principal_variation: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanComparison {
    pub most_likely_move: String,
    pub most_likely_probability: f64,
    pub played_move_probability: Option<f64>,
    pub played_move_rank: Option<usize>,
    pub played_move_is_human_likely: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RuleExtractorError {
    #[error("missing provider evidence for ply {ply}")]
    MissingEvidence { ply: usize },
    #[error("duplicate provider evidence for ply {ply}")]
    DuplicateEvidence { ply: usize },
    #[error("provider evidence references unknown ply {ply}")]
    UnknownPly { ply: usize },
    #[error("Human Move Model returned no candidates for ply {ply}")]
    NoHumanCandidates { ply: usize },
    #[error("provider evaluation cannot be normalized for ply {ply}")]
    InvalidEvaluation { ply: usize },
    #[error("classification evidence is incomplete or illegal for ply {ply}: {reason}")]
    InvalidClassificationEvidence { ply: usize, reason: &'static str },
    #[error("terminal metadata is invalid for ply {ply}")]
    InvalidTerminalOutcome { ply: usize },
    #[error("classification is contradictory for ply {ply}")]
    ContradictoryClassification { ply: usize },
    #[error("causal facts could not be derived for ply {ply}: {source}")]
    InvalidCausalFacts {
        ply: usize,
        #[source]
        source: CausalFactError,
    },
}

pub fn extract(
    game: &Game,
    elo: EloProfile,
    review_side: ReviewSide,
    evidence: &[MoveEvidence<'_>],
) -> Result<RuleExtraction, RuleExtractorError> {
    Ok(extract_with_trace(game, elo, review_side, evidence)?.facts)
}

pub fn extract_with_trace(
    game: &Game,
    elo: EloProfile,
    review_side: ReviewSide,
    evidence: &[MoveEvidence<'_>],
) -> Result<RuleExtractionWithTrace, RuleExtractorError> {
    let known_plies = game
        .moves
        .iter()
        .map(|game_move| game_move.ply)
        .collect::<HashSet<_>>();
    let mut evidence_by_ply = HashMap::with_capacity(evidence.len());
    for item in evidence {
        if !known_plies.contains(&item.ply) {
            return Err(RuleExtractorError::UnknownPly { ply: item.ply });
        }
        if evidence_by_ply.insert(item.ply, item).is_some() {
            return Err(RuleExtractorError::DuplicateEvidence { ply: item.ply });
        }
    }

    let mut candidates = Vec::new();
    for game_move in &game.moves {
        let item = evidence_by_ply
            .get(&game_move.ply)
            .copied()
            .ok_or(RuleExtractorError::MissingEvidence { ply: game_move.ply })?;
        if !review_side.includes(game_move.side) {
            continue;
        }
        let extracted = extract_moment_fact(game, game_move, elo, item)?;
        if let Some(candidate) = automatic_candidate(extracted) {
            candidates.push(candidate);
        }
    }
    let SelectorSelection {
        selected: critical_moments,
        trace: selector_trace,
    } = critical_moment_selector::select(game.moves.len(), review_side, candidates)
        .expect("fixed selector policy always stays within its hard maximum");

    Ok(RuleExtractionWithTrace {
        facts: RuleExtraction {
            summary: format!(
                "Analyzed {} plies and selected {} Critical Moments for Elo {}.",
                game.moves.len(),
                critical_moments.len(),
                elo.rating()
            ),
            player_profile: PlayerProfile::from_elo(elo),
            critical_moments,
        },
        selector_trace,
    })
}

pub fn extract_selected_moment(
    game: &Game,
    elo: EloProfile,
    evidence: &MoveEvidence<'_>,
) -> Result<SelectedMomentExtraction, RuleExtractorError> {
    let game_move = game
        .moves
        .iter()
        .find(|game_move| game_move.ply == evidence.ply)
        .ok_or(RuleExtractorError::UnknownPly { ply: evidence.ply })?;
    let fact = extract_moment_fact(game, game_move, elo, evidence)?.fact;

    Ok(SelectedMomentExtraction {
        summary: format!(
            "Analyzed Player-Selected Moment at ply {} for Elo {}.",
            evidence.ply,
            elo.rating()
        ),
        player_profile: PlayerProfile::from_elo(elo),
        selected_moment: fact,
    })
}
