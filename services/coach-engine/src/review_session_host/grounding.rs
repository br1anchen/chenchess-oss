use serde::Deserialize;

use crate::chess_literal_grounding::ChessLiteralGrounding;
use crate::critical_moment_comment::{
    contains_internal_identifier, contains_internal_player_facing_text,
};
use crate::review_session_contract::{HostTurnRefusalReason, HostTurnShowLine};
use crate::shared_assets::GROUNDING_SENTENCES_JSON;

#[derive(Debug, Deserialize)]
struct GroundingSentenceList(Vec<String>);

pub fn shared_grounding_sentences() -> Vec<String> {
    serde_json::from_str::<GroundingSentenceList>(GROUNDING_SENTENCES_JSON)
        .expect("packages/shared-assets/grounding/sentences.json is a JSON string array")
        .0
}

pub fn shared_grounding_block() -> String {
    shared_grounding_sentences().join("\n")
}

pub fn refusal_text(reason: HostTurnRefusalReason) -> &'static str {
    match reason {
        HostTurnRefusalReason::NotAboutThisReview => {
            "I can only talk about this reviewed game and the moments on the board. Ask about a move, a moment, or a line from this review."
        }
        HostTurnRefusalReason::NotAboutChess => "I can only help with this chess review.",
        HostTurnRefusalReason::UnsafeRequest => "I cannot help with that request.",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostTurnGroundingRejection {
    UngroundedChessLiteral,
    Url,
    InternalVocabulary,
    InvalidFocus,
    InvalidShowLine,
}

impl HostTurnGroundingRejection {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::UngroundedChessLiteral => {
                "the answer named a chess move or square that is not in the allowed literals"
            }
            Self::Url => "the answer included a URL",
            Self::InternalVocabulary => {
                "the answer used internal vocabulary, a raw UCI token, or an internal identifier"
            }
            Self::InvalidFocus => {
                "focusMoment does not name a ply pre-loaded or returned this turn"
            }
            Self::InvalidShowLine => {
                "showLine does not name a line pre-loaded or returned this turn"
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct HostTurnAnswerRefs<'a> {
    pub allowed_plies: &'a [u16],
    pub engine_best_allowed: bool,
    pub played_refutation_allowed: bool,
    pub alternative_move_ids: &'a [crate::review_session_contract::AlternativeMoveId],
}

pub(crate) fn ground_host_turn_answer(
    grounding: &ChessLiteralGrounding,
    answer: &str,
    focus_moment: Option<u16>,
    show_line: Option<&HostTurnShowLine>,
    refs: HostTurnAnswerRefs<'_>,
) -> Result<(), HostTurnGroundingRejection> {
    if !grounding.validate(answer) {
        return Err(HostTurnGroundingRejection::UngroundedChessLiteral);
    }
    if contains_url(answer) {
        return Err(HostTurnGroundingRejection::Url);
    }
    if contains_internal_player_facing_text(answer) || contains_internal_identifier(answer) {
        return Err(HostTurnGroundingRejection::InternalVocabulary);
    }
    if let Some(ply) = focus_moment {
        if !refs.allowed_plies.contains(&ply) {
            return Err(HostTurnGroundingRejection::InvalidFocus);
        }
    }
    if let Some(line) = show_line {
        let allowed = match line {
            HostTurnShowLine::EngineBest => refs.engine_best_allowed,
            HostTurnShowLine::PlayedMoveRefutation => refs.played_refutation_allowed,
            HostTurnShowLine::AlternativeMove {
                alternative_move_id,
            } => refs.alternative_move_ids.contains(alternative_move_id),
        };
        if !allowed {
            return Err(HostTurnGroundingRejection::InvalidShowLine);
        }
    }
    Ok(())
}

fn contains_url(text: &str) -> bool {
    let lowercase = text.to_ascii_lowercase();
    ["http://", "https://", "www."]
        .iter()
        .any(|prefix| lowercase.contains(prefix))
}
