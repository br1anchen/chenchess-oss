//! Stateless opening analysis on an identity-free root (ADR 0057, #493).
//!
//! Given an Opening Line address and an ordered continuation, return the
//! per-ply evaluations Player Line evaluation already returns, rooted at the
//! initial position instead of a Review Moment. No actor, no key, no
//! residency policy, no Player-owned state: the route reads the pinned
//! catalog through the identification reader, walks the continuation with
//! the exploration position rules, and asks the engine per position.
//!
//! The Opening Analysis Cache is the position-content-addressed engine
//! analysis cache (`ExactEngineCache`): entries are keyed by engine identity
//! plus `PositionContentId` and by nothing else, so two move orders reaching
//! one position collapse onto one entry and no entry can carry an owner or a
//! session segment. Off-book analysis is bounded twice — the same twelve
//! plies a Player Line is capped at, and a per-Player rate limit — because
//! there is no Review Session allowance to scope it.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    engine_analysis::{EngineAnalysisInput, EngineAnalyzer},
    opening_identification::{opening_line_reference, resolve_opening_line},
    pgn::parse_pgn_with_metadata,
    review_session_contract::{
        build_position_snapshot, AlternativeMoveEvaluation, Color, EngineAnalysisEvidence,
        MoveInput, PlayerId, PositionSnapshot, RetryDirective,
    },
    review_session_exploration::{
        apply_move_to_snapshot, compare_position_evaluations, normalize_child_position_evaluation,
        normalize_live_engine_analysis,
    },
    review_session_processor::PlayerTrafficPolicy,
};
use coach_engine_pipeline::operating_limits::ALTERNATIVE_MOVE_DEADLINE_MILLISECONDS;

const OPENING_ANALYSIS_DEADLINE: std::time::Duration =
    std::time::Duration::from_millis(ALTERNATIVE_MOVE_DEADLINE_MILLISECONDS);

/// The same cap a Player Line carries: the interesting deviation is early,
/// and unbounded engine compute on caller-supplied lines is a service anyone
/// with Beta Access could farm.
pub const OPENING_ANALYSIS_PLY_CAP: usize = 12;

#[derive(Clone)]
pub struct OpeningAnalysisRuntime {
    pub(crate) analyzer: Option<Arc<dyn EngineAnalyzer>>,
    pub(crate) traffic: Arc<PlayerTrafficPolicy>,
}

impl OpeningAnalysisRuntime {
    pub fn new(analyzer: Option<Arc<dyn EngineAnalyzer>>) -> Self {
        Self {
            analyzer,
            traffic: Arc::new(PlayerTrafficPolicy::v1()),
        }
    }

    pub fn disabled() -> Self {
        Self::new(None)
    }

    #[cfg(test)]
    pub(crate) fn with_traffic(mut self, traffic: Arc<PlayerTrafficPolicy>) -> Self {
        self.traffic = traffic;
        self
    }
}

#[derive(Debug, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningAnalysisRequest {
    pub opening_line_ref: String,
    #[serde(default)]
    pub continuation: Vec<MoveInput>,
}

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpeningLineIdentity {
    pub eco: String,
    pub name: String,
    pub path: String,
    pub opening_line_ref: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema, TS)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum ResolveOpeningLineOutcome {
    Resolved { line: OpeningLineIdentity },
    UnknownOpeningLine,
}

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpeningAnalyzedRoot {
    pub fen: String,
    pub evaluation: crate::review_session_contract::EngineEvaluation,
}

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpeningAnalyzedPly {
    pub index: usize,
    pub move_uci: String,
    pub mover: Color,
    pub evaluation: AlternativeMoveEvaluation,
    pub resulting_fen: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OpeningContinuationVerdict {
    Completed,
    IllegalMove { index: usize },
    PlyLimitReached { index: usize },
}

#[derive(Debug, Clone, Serialize, PartialEq, JsonSchema, TS)]
#[serde(
    tag = "outcome",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OpeningAnalysisOutcome {
    Analyzed {
        line: OpeningLineIdentity,
        root: OpeningAnalyzedRoot,
        plies: Vec<OpeningAnalyzedPly>,
        verdict: OpeningContinuationVerdict,
    },
    UnknownOpeningLine,
    RateLimited {
        retry: RetryDirective,
    },
    Unavailable {
        retry: RetryDirective,
    },
}

pub async fn analyze_opening_line(
    runtime: &OpeningAnalysisRuntime,
    player_id: &PlayerId,
    request: &OpeningAnalysisRequest,
) -> OpeningAnalysisOutcome {
    let Some(line) = resolve_opening_line(&request.opening_line_ref) else {
        return OpeningAnalysisOutcome::UnknownOpeningLine;
    };
    let Some(analyzer) = runtime.analyzer.as_deref() else {
        return unavailable();
    };
    // Admission is charged only for a request that will actually reach the
    // engine; refused addresses and a missing engine spend no allowance.
    if let Err(retry_after_seconds) = runtime.traffic.admit_opening_analysis(player_id) {
        return OpeningAnalysisOutcome::RateLimited {
            retry: RetryDirective::RetryAfter {
                seconds: retry_after_seconds,
            },
        };
    }

    let Some((root, mut history)) = opening_line_root(&line.path) else {
        return unavailable();
    };
    let Some(root_evidence) = analyze_position(analyzer, &root).await else {
        return unavailable();
    };

    let mut plies = Vec::new();
    let mut parent = root.clone();
    let mut parent_evidence = root_evidence.clone();
    let mut verdict = OpeningContinuationVerdict::Completed;
    for (index, move_input) in request.continuation.iter().enumerate() {
        if index >= OPENING_ANALYSIS_PLY_CAP {
            verdict = OpeningContinuationVerdict::PlyLimitReached { index };
            break;
        }
        let Ok(applied) = apply_move_to_snapshot(&parent, &history, move_input) else {
            verdict = OpeningContinuationVerdict::IllegalMove { index };
            break;
        };
        let Some(child_evidence) = analyze_position(analyzer, &applied.resulting_position).await
        else {
            return unavailable();
        };
        let mover = parent.side_to_move;
        let Some(selected) = normalize_child_position_evaluation(&child_evidence.evaluation, mover)
        else {
            return unavailable();
        };
        let best = parent_evidence.evaluation.clone();
        let Some(comparison) = compare_position_evaluations(&best, &selected) else {
            return unavailable();
        };
        plies.push(OpeningAnalyzedPly {
            index,
            move_uci: applied.uci.clone(),
            mover,
            evaluation: AlternativeMoveEvaluation {
                selected_move: selected,
                best_move_uci: parent_evidence.best_move_uci.clone(),
                best_move: best,
                comparison,
            },
            resulting_fen: applied.resulting_position.fen.clone(),
        });
        history = applied.resulting_history;
        parent = applied.resulting_position;
        parent_evidence = child_evidence;
    }

    OpeningAnalysisOutcome::Analyzed {
        line: OpeningLineIdentity {
            eco: line.eco.clone(),
            name: line.name.clone(),
            path: line.path.clone(),
            opening_line_ref: opening_line_reference(&line.eco, &line.name, &line.path),
        },
        root: OpeningAnalyzedRoot {
            fen: root.fen,
            evaluation: root_evidence.evaluation,
        },
        plies,
        verdict,
    }
}

/// The line's final position plus the FEN history that grounds repetition
/// state, read through the same PGN reader the catalog uses. The history
/// includes the root's own FEN, matching the convention `apply_move` keeps.
fn opening_line_root(path: &str) -> Option<(PositionSnapshot, Vec<String>)> {
    let representative_game = format!("{path} *");
    let parsed = parse_pgn_with_metadata(&representative_game).ok()?;
    // Each catalog move carries the position it was played FROM; the line's
    // own destination is the parsed game's final position.
    // The reader rejects a move-less PGN, so this always holds two or more.
    let mut fens: Vec<String> = parsed
        .game
        .moves
        .iter()
        .map(|game_move| game_move.position.clone())
        .collect();
    fens.push(parsed.game.final_position.clone());
    let (root_fen, preceding) = fens.split_last()?;
    let preceding_refs = preceding.iter().map(String::as_str).collect::<Vec<_>>();
    let root = build_position_snapshot(root_fen, &preceding_refs).ok()?;
    Some((root, fens.clone()))
}

async fn analyze_position(
    analyzer: &dyn EngineAnalyzer,
    position: &PositionSnapshot,
) -> Option<EngineAnalysisEvidence> {
    let analysis = tokio::time::timeout(
        OPENING_ANALYSIS_DEADLINE,
        analyzer.analyze(EngineAnalysisInput {
            position: &position.fen,
        }),
    )
    .await
    .ok()?
    .ok()?;
    normalize_live_engine_analysis(position, analysis).ok()
}

fn unavailable() -> OpeningAnalysisOutcome {
    OpeningAnalysisOutcome::Unavailable {
        retry: RetryDirective::RetryAllowed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pinned_catalog_line_grounds_a_root() {
        for line in &crate::opening_identification::catalog().lines {
            let (root, history) = opening_line_root(&line.path)
                .unwrap_or_else(|| panic!("{} should ground a root", line.path));
            assert!(!root.fen.is_empty());
            assert!(!history.is_empty());
        }
    }
}
