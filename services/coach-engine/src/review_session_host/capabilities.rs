use std::sync::Arc;

use serde::Serialize;
use serde_json::json;
use shakmaty::{fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess};

use super::digest::digest_canonical_json;
use crate::chess_literal_grounding::ChessLiteralGrounding;
use crate::critical_moment_comment::chess_literal_grounding_for;
use crate::language_layer_prompt::project_comment_facts;
use crate::review_session_contract::{
    AlternativeMoveResult, BranchParent, IdempotencyKey, MoveInput, PositionRef,
    ReviewMomentCommentFacts, ReviewMomentLearningMaterial, ReviewMomentReferenceClassification,
    ReviewSessionEvidencePacket, StrongestReply,
};
use crate::review_session_exploration::{
    AlternativeMoveCancellation, AlternativeMoveExploration, ExploreAlternativeMoveError,
    ExploreAlternativeMoveRequest,
};

pub fn host_capability_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["capability", "ply", "next", "classification", "moves", "opponentReplies"],
        "properties": {
            "capability": {
                "type": "string",
                "enum": ["readMoment", "listMoments", "evaluateLine", "learningMaterial"]
            },
            "ply": { "type": "integer" },
            "next": { "type": "boolean" },
            "classification": {
                "type": "string",
                "enum": ["", "improvementOpportunity"]
            },
            "moves": {
                "type": "array",
                "items": { "type": "string" }
            },
            "opponentReplies": {
                "type": "string",
                "enum": ["engineBest", "supplied"]
            }
        }
    })
}

pub fn host_capability_schema_digest() -> String {
    digest_canonical_json(&host_capability_schema())
}

/// User-template placeholders that constitute pre-loaded HostTurn evidence.
///
/// Keys stay in lockstep with `WEB_HOST_USER_TEMPLATE`. A golden digest pins
/// this list. Every `{{...}}` in the template must appear here or in
/// `USER_TEMPLATE_NON_EVIDENCE_PLACEHOLDERS`.
const PRELOADED_EVIDENCE_PLACEHOLDERS: &[(&str, &str)] = &[
    ("elo", "{{elo}}"),
    (
        "coachingProfileProjection",
        "{{coaching_profile_projection}}",
    ),
    ("openMomentPacket", "{{open_moment_packet}}"),
    ("activeBranch", "{{active_branch}}"),
    ("priorTurns", "{{prior_turns}}"),
    ("allowedChessLiterals", "{{allowed_chess_literals}}"),
];

/// Placeholders in `WEB_HOST_USER_TEMPLATE` that are not pre-loaded evidence.
///
/// `player_message_pointer` is a pointer to the next chat turn, not evidence
/// already in the user template.
#[cfg(test)]
pub(super) const USER_TEMPLATE_NON_EVIDENCE_PLACEHOLDERS: &[&str] = &["{{player_message_pointer}}"];

pub fn preloaded_evidence_schema() -> serde_json::Value {
    json!({
        "kind": "hostTurnPreloadedEvidence",
        "version": "v1",
        "keys": PRELOADED_EVIDENCE_PLACEHOLDERS
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
    })
}

pub fn preloaded_evidence_placeholders() -> &'static [(&'static str, &'static str)] {
    PRELOADED_EVIDENCE_PLACEHOLDERS
}

pub fn preloaded_evidence_schema_digest() -> String {
    digest_canonical_json(&preloaded_evidence_schema())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCapabilityCall {
    ReadMoment { reference: MomentReference },
    ListMoments,
    EvaluateLine(EvaluateLineArgs),
    LearningMaterial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MomentReference {
    Ply {
        ply: u16,
    },
    Next {
        classification: Option<ReviewMomentReferenceClassification>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluateLineArgs {
    pub moves: Vec<String>,
    pub opponent_replies: OpponentReplies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpponentReplies {
    EngineBest,
    Supplied,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostCapabilityError {
    pub message: String,
}

impl HostCapabilityError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<ExploreAlternativeMoveError> for HostCapabilityError {
    fn from(error: ExploreAlternativeMoveError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct StoredHostMoment {
    facts: ReviewMomentCommentFacts,
    packet: ReviewSessionEvidencePacket,
    material: Option<ReviewMomentLearningMaterial>,
    exploration: Option<Arc<AlternativeMoveExploration>>,
}

impl StoredHostMoment {
    pub fn from_facts(
        facts: ReviewMomentCommentFacts,
        packet: ReviewSessionEvidencePacket,
        material: Option<ReviewMomentLearningMaterial>,
    ) -> Self {
        Self {
            facts,
            packet,
            material,
            exploration: None,
        }
    }

    pub fn with_exploration(self, exploration: AlternativeMoveExploration) -> Self {
        self.with_shared_exploration(Arc::new(exploration))
    }

    pub fn with_shared_exploration(mut self, exploration: Arc<AlternativeMoveExploration>) -> Self {
        self.exploration = Some(exploration);
        self
    }

    pub fn ply(&self) -> u16 {
        self.facts.moment().ply
    }

    pub fn facts(&self) -> &ReviewMomentCommentFacts {
        &self.facts
    }

    pub fn packet(&self) -> &ReviewSessionEvidencePacket {
        &self.packet
    }

    pub fn material(&self) -> Option<&ReviewMomentLearningMaterial> {
        self.material.as_ref()
    }

    pub fn exploration(&self) -> Option<&AlternativeMoveExploration> {
        self.exploration.as_deref()
    }
}

#[derive(Clone, Default)]
pub struct HostCapabilityStore {
    moments: Vec<StoredHostMoment>,
}

impl HostCapabilityStore {
    pub fn new(moments: Vec<StoredHostMoment>) -> Self {
        Self { moments }
    }

    pub fn moments(&self) -> &[StoredHostMoment] {
        &self.moments
    }

    pub fn moment_at(&self, ply: u16) -> Option<&StoredHostMoment> {
        self.moments.iter().find(|moment| moment.ply() == ply)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCapabilityDispatch {
    pub call_id: String,
    pub evidence: HostCapabilityEvidence,
    pub projection: serde_json::Value,
    pub allowed_chess_literals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HostCapabilityEvidence {
    #[serde(rename_all = "camelCase")]
    Moment {
        ply: u16,
        packet: Box<ReviewSessionEvidencePacket>,
        facts: Box<ReviewMomentCommentFacts>,
    },
    #[serde(rename_all = "camelCase")]
    MomentList { moments: Vec<ListedHostMoment> },
    #[serde(rename_all = "camelCase")]
    EvaluatedLine {
        requested_moves: Vec<String>,
        commits: Vec<AlternativeMoveResult>,
    },
    #[serde(rename_all = "camelCase")]
    LearningMaterial {
        ply: u16,
        material: ReviewMomentLearningMaterial,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedHostMoment {
    pub ply: u16,
    pub played_san: String,
    pub classification: HostMomentClassification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HostMomentClassification {
    PositiveHighlight,
    ImprovementOpportunity,
    Neutral,
}

impl From<&ReviewMomentCommentFacts> for HostMomentClassification {
    fn from(facts: &ReviewMomentCommentFacts) -> Self {
        match facts {
            ReviewMomentCommentFacts::Positive { .. } => Self::PositiveHighlight,
            ReviewMomentCommentFacts::Improvement { .. } => Self::ImprovementOpportunity,
            ReviewMomentCommentFacts::Neutral { .. } => Self::Neutral,
        }
    }
}

/// Call id a HostTurn uses when dispatch fails before a result is minted.
pub fn host_capability_call_id(call: &HostCapabilityCall, open_ply: u16) -> String {
    match call {
        HostCapabilityCall::ReadMoment {
            reference: MomentReference::Ply { ply },
        } => format!("call:readMoment:{ply}"),
        HostCapabilityCall::ReadMoment {
            reference: MomentReference::Next { classification },
        } => match classification {
            Some(ReviewMomentReferenceClassification::ImprovementOpportunity) => {
                "call:readMoment:next:improvementOpportunity".to_owned()
            }
            None => "call:readMoment:next".to_owned(),
        },
        HostCapabilityCall::ListMoments => "call:listMoments".to_owned(),
        HostCapabilityCall::EvaluateLine(args) => {
            format!(
                "call:evaluateLine:{}",
                evaluate_line_identity(open_ply, args)
            )
        }
        HostCapabilityCall::LearningMaterial => "call:learningMaterial".to_owned(),
    }
}

pub async fn dispatch(
    store: &HostCapabilityStore,
    open_ply: u16,
    call: &HostCapabilityCall,
) -> Result<HostCapabilityDispatch, HostCapabilityError> {
    match call {
        HostCapabilityCall::ReadMoment { reference } => {
            dispatch_read_moment(store, open_ply, reference)
        }
        HostCapabilityCall::ListMoments => dispatch_list_moments(store),
        HostCapabilityCall::EvaluateLine(args) => {
            dispatch_evaluate_line(store, open_ply, args).await
        }
        HostCapabilityCall::LearningMaterial => dispatch_learning_material(store, open_ply),
    }
}

fn dispatch_read_moment(
    store: &HostCapabilityStore,
    open_ply: u16,
    reference: &MomentReference,
) -> Result<HostCapabilityDispatch, HostCapabilityError> {
    let moment = match reference {
        MomentReference::Ply { ply } => store.moment_at(*ply).ok_or_else(|| {
            HostCapabilityError::new(format!("review session has no moment at ply {ply}"))
        })?,
        MomentReference::Next { classification } => store
            .moments()
            .iter()
            .find(|moment| {
                moment.ply() > open_ply
                    && classification.is_none_or(|wanted| matches_classification(moment, wanted))
            })
            .ok_or_else(|| HostCapabilityError::new("review session has no later moment"))?,
    };
    let grounding = chess_literal_grounding_for(moment.facts(), None);
    Ok(HostCapabilityDispatch {
        call_id: format!("call:readMoment:{}", moment.ply()),
        evidence: HostCapabilityEvidence::Moment {
            ply: moment.ply(),
            packet: Box::new(moment.packet().clone()),
            facts: Box::new(moment.facts().clone()),
        },
        projection: project_comment_facts(moment.facts()),
        allowed_chess_literals: allowed_literals(&grounding),
    })
}

fn dispatch_list_moments(
    store: &HostCapabilityStore,
) -> Result<HostCapabilityDispatch, HostCapabilityError> {
    let moments: Vec<ListedHostMoment> = store
        .moments()
        .iter()
        .map(|moment| ListedHostMoment {
            ply: moment.ply(),
            played_san: moment.facts().moment().played_san.clone(),
            classification: HostMomentClassification::from(moment.facts()),
        })
        .collect();
    let mut grounding = ChessLiteralGrounding::empty();
    for moment in store.moments() {
        grounding.allow_move_san(&moment.facts().moment().played_san);
    }
    Ok(HostCapabilityDispatch {
        call_id: "call:listMoments".to_owned(),
        evidence: HostCapabilityEvidence::MomentList {
            moments: moments.clone(),
        },
        projection: json!({ "moments": moments }),
        allowed_chess_literals: allowed_literals(&grounding),
    })
}

async fn dispatch_evaluate_line(
    store: &HostCapabilityStore,
    open_ply: u16,
    args: &EvaluateLineArgs,
) -> Result<HostCapabilityDispatch, HostCapabilityError> {
    let moment = store.moment_at(open_ply).ok_or_else(|| {
        HostCapabilityError::new(format!(
            "review session has no open moment at ply {open_ply}"
        ))
    })?;
    let exploration = moment.exploration().ok_or_else(|| {
        HostCapabilityError::new("open moment has no alternative-move exploration")
    })?;
    let root = exploration.current_state().await.root_position;
    let mut grounding = ChessLiteralGrounding::empty();
    let mut commits = Vec::new();
    let mut evaluations = Vec::new();
    let mut parent = BranchParent::Root {
        position_ref: root.position_ref.clone(),
    };
    let mut source_position_ref = root.position_ref;
    let mut source_fen = root.fen;
    let line_token = evaluate_line_identity(open_ply, args);

    for (index, requested) in args.moves.iter().enumerate() {
        let commit = explore_one(
            exploration,
            &mut grounding,
            parent,
            source_position_ref,
            parse_requested_move(requested),
            &format!("key:host-evaluate-line:{line_token}:{index}"),
        )
        .await?;
        let requested_san = san_from_uci(&source_fen, &commit.move_uci)
            .ok_or_else(|| HostCapabilityError::new("evaluated move has no SAN"))?;
        grounding.allow_move_san(&requested_san);
        parent = BranchParent::Move {
            branch_ref: commit.branch_ref.clone(),
        };
        source_position_ref = commit.resulting_position.position_ref.clone();
        source_fen = commit.resulting_position.fen.clone();
        let reply = commit.strongest_reply.clone();
        evaluations.push(json!({
            "source": "player",
            "requestedMove": requested_san,
            "selectedMove": commit.evaluation.selected_move,
            "comparison": commit.evaluation.comparison,
        }));
        commits.push(commit);
        let interleave_reply =
            args.opponent_replies == OpponentReplies::EngineBest && index + 1 < args.moves.len();
        if interleave_reply {
            let StrongestReply::Offered { uci } = reply else {
                return Err(HostCapabilityError::new(
                    "evaluateLine engineBest has no strongest reply to continue the line",
                ));
            };
            let reply_commit = explore_one(
                exploration,
                &mut grounding,
                parent.clone(),
                source_position_ref.clone(),
                MoveInput::Uci { uci: uci.clone() },
                &format!("key:host-evaluate-line:{line_token}:{index}:reply"),
            )
            .await?;
            let reply_san = san_from_uci(&source_fen, &reply_commit.move_uci)
                .ok_or_else(|| HostCapabilityError::new("engine reply has no SAN"))?;
            grounding.allow_move_san(&reply_san);
            parent = BranchParent::Move {
                branch_ref: reply_commit.branch_ref.clone(),
            };
            source_position_ref = reply_commit.resulting_position.position_ref.clone();
            source_fen = reply_commit.resulting_position.fen.clone();
            evaluations.push(json!({
                "source": "engine",
                "selectedMove": reply_commit.evaluation.selected_move,
                "comparison": reply_commit.evaluation.comparison,
            }));
            commits.push(reply_commit);
        }
    }

    Ok(HostCapabilityDispatch {
        call_id: format!("call:evaluateLine:{line_token}"),
        evidence: HostCapabilityEvidence::EvaluatedLine {
            requested_moves: args.moves.clone(),
            commits,
        },
        projection: json!({
            "requestedMoves": args.moves,
            "evaluations": evaluations,
        }),
        allowed_chess_literals: allowed_literals(&grounding),
    })
}

async fn explore_one(
    exploration: &AlternativeMoveExploration,
    grounding: &mut ChessLiteralGrounding,
    parent: BranchParent,
    source_position_ref: PositionRef,
    move_input: MoveInput,
    idempotency_key: &str,
) -> Result<AlternativeMoveResult, HostCapabilityError> {
    if exploration.remaining_allowance().await == 0 {
        return Err(HostCapabilityError::new(
            "evaluateLine exhausted the open moment exploration allowance",
        ));
    }
    let commit = exploration
        .explore(
            ExploreAlternativeMoveRequest {
                parent,
                source_position_ref,
                move_input,
                idempotency_key: IdempotencyKey::try_from(idempotency_key.to_owned())
                    .expect("host evaluate-line idempotency key is a valid semantic id"),
            },
            AlternativeMoveCancellation::default(),
        )
        .await?;
    grounding.allow_uci_squares(&commit.alternative_move.move_uci);
    Ok(commit.alternative_move)
}

fn dispatch_learning_material(
    store: &HostCapabilityStore,
    open_ply: u16,
) -> Result<HostCapabilityDispatch, HostCapabilityError> {
    let moment = store.moment_at(open_ply).ok_or_else(|| {
        HostCapabilityError::new(format!(
            "review session has no open moment at ply {open_ply}"
        ))
    })?;
    let material = moment.material().cloned().ok_or_else(|| {
        HostCapabilityError::new(format!(
            "open moment at ply {open_ply} has no authored practice material"
        ))
    })?;
    // Learning material is recited as returned titles and track keys, not
    // paraphrased into chess literals. Those stay on the pre-loaded open-moment
    // vocabulary and on readMoment. An authored empty track list is distinct
    // from a missing critical_moment: the latter is this error, so #435 can
    // tell the Player there is nothing to practise.
    Ok(HostCapabilityDispatch {
        call_id: "call:learningMaterial".to_owned(),
        evidence: HostCapabilityEvidence::LearningMaterial {
            ply: moment.ply(),
            material: material.clone(),
        },
        projection: json!({ "material": material }),
        allowed_chess_literals: Vec::new(),
    })
}

fn matches_classification(
    moment: &StoredHostMoment,
    wanted: ReviewMomentReferenceClassification,
) -> bool {
    match wanted {
        ReviewMomentReferenceClassification::ImprovementOpportunity => {
            matches!(moment.facts(), ReviewMomentCommentFacts::Improvement { .. })
        }
    }
}

fn parse_requested_move(requested: &str) -> MoveInput {
    let trimmed = requested.trim();
    if looks_like_uci(trimmed) {
        MoveInput::Uci {
            uci: trimmed.to_owned(),
        }
    } else {
        MoveInput::San {
            san: trimmed.to_owned(),
        }
    }
}

/// UCI is file-rank-file-rank with an optional promotion piece.
///
/// The previous alphanumeric sniff treated SAN pawn captures (`cxd4`) as UCI
/// because `x` is alphanumeric.
fn looks_like_uci(value: &str) -> bool {
    let bytes = value.as_bytes();
    let (from_file, from_rank, to_file, to_rank) = match bytes {
        [from_file, from_rank, to_file, to_rank] => (from_file, from_rank, to_file, to_rank),
        [from_file, from_rank, to_file, to_rank, b'q' | b'r' | b'b' | b'n'] => {
            (from_file, from_rank, to_file, to_rank)
        }
        _ => return false,
    };
    matches!(from_file, b'a'..=b'h')
        && matches!(from_rank, b'1'..=b'8')
        && matches!(to_file, b'a'..=b'h')
        && matches!(to_rank, b'1'..=b'8')
}

fn evaluate_line_identity(open_ply: u16, args: &EvaluateLineArgs) -> String {
    digest_canonical_json(&json!({
        "ply": open_ply,
        "moves": args.moves,
        "opponentReplies": match args.opponent_replies {
            OpponentReplies::EngineBest => "engineBest",
            OpponentReplies::Supplied => "supplied",
        },
    }))
    .trim_start_matches("sha256:")
    .to_owned()
}

pub(crate) fn san_from_uci(fen: &str, uci: &str) -> Option<String> {
    let position: Chess = fen
        .parse::<Fen>()
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()?;
    let played = UciMove::from_ascii(uci.as_bytes())
        .ok()?
        .to_move(&position)
        .ok()?;
    Some(SanPlus::from_move(position, &played).to_string())
}

fn allowed_literals(grounding: &ChessLiteralGrounding) -> Vec<String> {
    grounding.allowed().map(str::to_string).collect()
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn pawn_captures_parse_as_san_and_uci_stays_a_fallback() {
        assert_eq!(
            parse_requested_move("cxd4"),
            MoveInput::San {
                san: "cxd4".to_owned()
            }
        );
        assert_eq!(
            parse_requested_move("exd5"),
            MoveInput::San {
                san: "exd5".to_owned()
            }
        );
        assert_eq!(
            parse_requested_move("c5d4"),
            MoveInput::Uci {
                uci: "c5d4".to_owned()
            }
        );
    }
}
