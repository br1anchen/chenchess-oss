//! The Coach Turn prose gate.
//!
//! Until this existed, `validate_dimension` checked that each explanation was
//! non-empty and nothing else — no markers, no chess literals, no URL ban —
//! even though both web Language Layer tasks are required to be grounded. This
//! applies the comment path's mechanism here, with the vocabulary and the
//! allowlist derived from the evidence packet rather than from moment facts.
//!
//! Failure granularity is deliberately whole-turn. A rejected dimension is
//! `ProviderUnavailableReason::LanguageLayer` for the entire Coach Turn, not a
//! per-dimension retry, because a Coach Turn has no safe rendering to fall back
//! to: comments degrade to safe-rendered prose, Coach Turns degrade to
//! unavailable. Retrying one dimension would publish a turn assembled from two
//! generations, which no fingerprint could describe.
//!
//! *Whole-turn* is not the same as *one reason*, though, and until
//! #359 the two were
//! conflated: every failure left here as `LanguageLayer`, so a bake-off could
//! see that a candidate lost the turn and never which discipline it lost. That
//! is the same collapse #346
//! undid on the comment path, and the fix is the same shape — resolve at full
//! width internally, narrow at the boundary. The Player-facing behaviour is
//! unchanged by construction: [`CoachTurnRejection::into_wire`] is total.

use serde::Serialize;
use serde_json::{json, Value};
use shakmaty::{fen::Fen, san::SanPlus, uci::UciMove, CastlingMode, Chess, Position};

use crate::{
    chess_literal_grounding::ChessLiteralGrounding,
    critical_moment_comment::{contains_internal_identifier, contains_internal_player_facing_text},
    engine_analysis::PositionEvaluation,
    language_layer_markers::{MarkerForm, MarkerViolation, MarkerVocabulary},
    moment_display::evaluation_display,
    review_session_contract::*,
    types::MoveSide,
};

use super::PreparedCoachTurnTarget;

/// One dimension's grounded explanation: what the Player reads.
pub(crate) struct GroundedDimension {
    pub(super) explanation: String,
}

/// Why one dimension's prose was discarded.
///
/// The Player never reads this — a rejected Coach Turn is unavailable whichever
/// rule it broke. It exists because a candidate that invents a marker and a
/// candidate that names an unlisted square fail for different reasons, and
/// #236 ranks on which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CoachTurnProseRejection {
    EmptyExplanation,
    UnknownMarker,
    RepeatedMarker,
    MissingRequiredMarker,
    BareFigure,
    MisplacedMarker,
    UngroundedChessLiteral,
    Url,
    InternalVocabulary,
    InternalIdentifier,
}

/// Why a Coach Turn was discarded, naming the dimension that did it.
///
/// The structural variants are separated from the prose ones on purpose: the
/// runtime fills `coachTurnId`, `alternativeMoveId` and every evidence ref, so
/// a structural failure is a harness or runtime fault where a prose failure is
/// the candidate's. Counting them together would charge a model for our bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CoachTurnRejection {
    /// The assessment does not name the turn or the alternative it was asked
    /// about.
    TargetMismatch,
    /// The evidence packet cannot be read into a vocabulary at all — an
    /// unparseable FEN or an unplayable recorded move. Ours, never the model's.
    EvidenceUnreadable,
    /// The dimension cites something other than exactly the set it is required
    /// to cite.
    EvidenceRefsMismatch { dimension: CoachTurnDimension },
    Prose {
        dimension: CoachTurnDimension,
        rejection: CoachTurnProseRejection,
    },
}

impl CoachTurnRejection {
    /// Total by design. #233
    /// degrades a Coach Turn to unavailable however it failed, so widening the
    /// diagnosis moves no wire contract and changes nothing a Player sees.
    pub fn into_wire(self) -> ProviderUnavailableReason {
        ProviderUnavailableReason::LanguageLayer
    }
}

/// Everything the three dimensions can name, computed once per turn.
pub(crate) struct CoachTurnProseContext {
    literals: ChessLiteralGrounding,
    /// The evidence the model is shown, built here rather than beside the
    /// prompt so the facts offered and the literals admitted come out of one
    /// walk of the packet. Building them twice is how a model gets shown a move
    /// the gate then rejects it for naming.
    projection: Value,
    alternative_move_san: String,
    alternative_evaluation: String,
    best_move_san: String,
    best_evaluation: String,
    evaluation_loss: String,
    strongest_reply: MarkerForm,
    resulting_evaluation: Option<String>,
    alternative_popularity: Option<String>,
    most_likely_move_san: Option<String>,
    likely_reply_san: Option<String>,
    shared_move_san: Option<String>,
    shared_reply_san: Option<String>,
}

impl CoachTurnProseContext {
    pub(crate) fn prepare(
        target: &PreparedCoachTurnTarget,
        packet: &ReviewSessionEvidencePacket,
    ) -> Result<Self, CoachTurnRejection> {
        let source_fen = &target.source_position.fen;
        let resulting_fen = &target.target.resulting_position.fen;
        let evaluation = &target.target.evaluation;

        let alternative_move_san = san_for(source_fen, &target.target.move_uci)?;
        let best_move_san = san_for(source_fen, &evaluation.best_move_uci)?;
        let strongest_reply_san = match &target.target.strongest_reply {
            StrongestReply::Offered { uci } => Some(san_for(resulting_fen, uci)?),
            StrongestReply::Terminal => None,
        };
        let source_human = packet
            .human_move_model(&target.source_position.position_ref)
            .map(|(_, analysis)| analysis);
        let resulting_human = packet
            .human_move_model(&target.target.resulting_position.position_ref)
            .map(|(_, analysis)| analysis);
        let resulting_engine = packet
            .engine_analysis(&target.target.resulting_position.position_ref)
            .map(|(_, analysis)| analysis);

        let most_likely_move_san = source_human
            .and_then(|analysis| ranked_first(analysis))
            .map(|candidate| san_for(source_fen, &candidate.uci))
            .transpose()?;
        let likely_reply_san = resulting_human
            .and_then(|analysis| ranked_first(analysis))
            .map(|candidate| san_for(resulting_fen, &candidate.uci))
            .transpose()?;

        // Two markers rendering one string reads as a stutter — #359 §10
        // recorded "the Re1 is Re1" when the engine's best reply is also the
        // likeliest human one — so a coincident optional is withheld rather
        // than offered twice. The shared SAN still arrives through the marker
        // that owns it. Withholding is the stutter guard, not the whole
        // answer: when the two replies (or the alternative and the likeliest
        // move) are the same, the projection states that coincidence and the
        // vocabulary offers a marker that licenses naming it once (#364).
        let shared_reply_san = match (&likely_reply_san, &strongest_reply_san) {
            (Some(likely), Some(strongest)) if likely == strongest => Some(likely.clone()),
            _ => None,
        };
        let shared_move_san = most_likely_move_san
            .as_ref()
            .filter(|san| *san == &alternative_move_san)
            .cloned();
        let likely_reply_san =
            likely_reply_san.filter(|san| Some(san.as_str()) != strongest_reply_san.as_deref());
        let most_likely_move_san = most_likely_move_san.filter(|san| *san != alternative_move_san);

        let mut literals = ChessLiteralGrounding::empty();
        literals.allow_move_san(&alternative_move_san);
        literals.allow_move_san(&best_move_san);
        for san in [
            &strongest_reply_san,
            &most_likely_move_san,
            &likely_reply_san,
        ]
        .into_iter()
        .flatten()
        {
            literals.allow_move_san(san);
        }
        // The engine's own line, in the notation a coach speaks. The packet
        // stores it as UCI, which the gate bans reproducing, so without this
        // projection the line could not be quoted at all.
        let source_engine = packet
            .engine_analysis(&target.source_position.position_ref)
            .map(|(_, analysis)| analysis);
        let mut engine_lines = Vec::new();
        for (fen, analysis) in [
            (source_fen, source_engine),
            (resulting_fen, resulting_engine),
        ] {
            let line = match analysis {
                None => Vec::new(),
                Some(analysis) => {
                    let line = san_line(fen, &analysis.principal_variation);
                    for san in &line {
                        literals.allow_move_san(san);
                    }
                    literals.allow_uci_squares(&analysis.best_move_uci);
                    line
                }
            };
            engine_lines.push(line);
        }
        for position in [&target.source_position, &target.target.resulting_position] {
            for occupied in &position.occupied {
                literals.allow_square(occupied.square.as_str());
            }
            if let EnPassantState::Available { square } = &position.en_passant {
                literals.allow_square(square.as_str());
            }
        }

        let projection = project_evidence(
            target,
            EvidenceProjectionInput {
                alternative_move_san: &alternative_move_san,
                best_move_san: &best_move_san,
                strongest_reply_san: strongest_reply_san.as_deref(),
                shared_move_san: shared_move_san.as_deref(),
                shared_reply_san: shared_reply_san.as_deref(),
                source_engine,
                resulting_engine,
                source_engine_line: &engine_lines[0],
                resulting_engine_line: &engine_lines[1],
                source_human,
                resulting_human,
            },
        )?;

        Ok(Self {
            literals,
            projection,
            alternative_move_san,
            alternative_evaluation: evaluation_text(&evaluation.selected_move),
            best_move_san,
            best_evaluation: evaluation_text(&evaluation.best_move),
            evaluation_loss: evaluation_loss_text(&evaluation.comparison),
            // Notation in one branch and prose in the other, so the form is
            // chosen per branch: `Qh4` must never be re-cased, and the prose
            // reads better capitalised when it opens a sentence.
            strongest_reply: match &strongest_reply_san {
                Some(san) => MarkerForm::Literal(san.clone()),
                None => {
                    MarkerForm::Anywhere("no reply at all — the game is over there".to_string())
                }
            },
            resulting_evaluation: resulting_engine
                .map(|analysis| evaluation_text(&analysis.evaluation)),
            alternative_popularity: source_human
                .and_then(|analysis| popularity_text(analysis, &target.target.move_uci)),
            most_likely_move_san,
            likely_reply_san,
            shared_move_san,
            shared_reply_san,
        })
    }

    /// The evidence the prompt shows, which is the evidence the allowlist was
    /// built from.
    pub(crate) fn projection(&self) -> &Value {
        &self.projection
    }

    /// Every chess literal the turn may name, in stable order.
    pub(crate) fn allowed_literals(&self) -> Vec<String> {
        self.literals.allowed().map(str::to_string).collect()
    }

    /// Each dimension gets its own vocabulary, matching the evidence it is
    /// required to cite. A findability explanation cannot name the resulting
    /// evaluation, because it does not cite the evidence that would ground it.
    /// No Coach Turn rendering is sentence-shaped — every one is a noun phrase,
    /// so this task never had the comment path's sentence-boundary defect. Its
    /// own seam defect was the article: a noun phrase invites the model's own
    /// "an" in front of it (#364), so the prose forms that need an article
    /// carry their own and substitution absorbs the model's. Notation is still
    /// declared literal, because `e4` opening a sentence must not become `E4`.
    pub(crate) fn vocabulary(&self, dimension: CoachTurnDimension) -> MarkerVocabulary {
        let mut markers = MarkerVocabulary::default();
        markers.require_literal("alternativeMove", self.alternative_move_san.clone());
        match dimension {
            CoachTurnDimension::ObjectiveQuality => {
                markers.require("alternativeEval", self.alternative_evaluation.clone());
                markers.require_literal("bestMove", self.best_move_san.clone());
                markers.require("bestEval", self.best_evaluation.clone());
                markers.offer("evaluationLoss", self.evaluation_loss.clone());
            }
            CoachTurnDimension::Findability => {
                markers
                    .offer_available("alternativePopularity", self.alternative_popularity.clone());
                markers
                    .offer_literal_available("mostLikelyMove", self.most_likely_move_san.clone());
                // `{alternativePopularity}` states how common the alternative is.
                // `{sharedMove}` licenses naming the coincidence (the alternative
                // *is* that most-common choice) as a grounded relation. After the
                // SAN is dropped from the rendering, that relation is the only
                // thing it adds. `{sharedReply}` stays because Resilience has no
                // reply-popularity marker.
                markers.offer_available(
                    "sharedMove",
                    coincidence_text(self.shared_move_san.as_deref()),
                );
            }
            CoachTurnDimension::Resilience => {
                markers.require_form("strongestReply", self.strongest_reply.clone());
                markers.offer_available("replyEval", self.resulting_evaluation.clone());
                markers.offer_literal_available("likelyReply", self.likely_reply_san.clone());
                markers.offer_available(
                    "sharedReply",
                    coincidence_text(self.shared_reply_san.as_deref()),
                );
            }
        }
        markers
    }

    pub(super) fn ground(
        &self,
        dimension: CoachTurnDimension,
        explanation: &str,
    ) -> Result<GroundedDimension, CoachTurnRejection> {
        self.diagnose(dimension, explanation)
            .map_err(|rejection| CoachTurnRejection::Prose {
                dimension,
                rejection,
            })
    }

    /// The dimension gate, resolving its failures at full width.
    fn diagnose(
        &self,
        dimension: CoachTurnDimension,
        explanation: &str,
    ) -> Result<GroundedDimension, CoachTurnProseRejection> {
        if explanation.trim().is_empty() {
            return Err(CoachTurnProseRejection::EmptyExplanation);
        }
        let grounded = self
            .vocabulary(dimension)
            .ground(explanation)
            .map_err(marker_rejection)?;
        if !self.literals.validate(&grounded.authored) {
            return Err(CoachTurnProseRejection::UngroundedChessLiteral);
        }
        if contains_url(&grounded.authored) {
            return Err(CoachTurnProseRejection::Url);
        }
        if contains_internal_player_facing_text(&grounded.text) {
            return Err(CoachTurnProseRejection::InternalVocabulary);
        }
        if contains_internal_identifier(&grounded.text) {
            return Err(CoachTurnProseRejection::InternalIdentifier);
        }
        Ok(GroundedDimension {
            explanation: grounded.text,
        })
    }
}

/// The Coach Turn drops the marker's name where the comment gate keeps it.
///
/// Not because it has no use for one — this enum's whole reason for existing is
/// that #236 ranks candidates on which discipline they lose, and it reaches the
/// bake-off record the same way the comment's does. It is dropped because the
/// change that named markers was a Task A regression fix, and widening Task B's
/// rejection with it would have moved a second prompt surface in the same
/// commit. Carry it here when Task B next needs the diagnosis.
fn marker_rejection(violation: MarkerViolation) -> CoachTurnProseRejection {
    match violation {
        MarkerViolation::UnknownMarker => CoachTurnProseRejection::UnknownMarker,
        MarkerViolation::RepeatedMarker(_) => CoachTurnProseRejection::RepeatedMarker,
        MarkerViolation::MissingRequiredMarker(_) => CoachTurnProseRejection::MissingRequiredMarker,
        MarkerViolation::BareFigure => CoachTurnProseRejection::BareFigure,
        MarkerViolation::MisplacedMarker(_) => CoachTurnProseRejection::MisplacedMarker,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CoachTurnDimension {
    ObjectiveQuality,
    Findability,
    Resilience,
}

impl CoachTurnDimension {
    /// The three dimensions in the order the assessment carries them.
    pub const ALL: [Self; 3] = [Self::ObjectiveQuality, Self::Findability, Self::Resilience];

    /// The name the schema and the prompt use for this dimension.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ObjectiveQuality => "objectiveQuality",
            Self::Findability => "findability",
            Self::Resilience => "resilience",
        }
    }
}

struct EvidenceProjectionInput<'a> {
    alternative_move_san: &'a str,
    best_move_san: &'a str,
    strongest_reply_san: Option<&'a str>,
    shared_move_san: Option<&'a str>,
    shared_reply_san: Option<&'a str>,
    source_engine: Option<&'a EngineAnalysisEvidence>,
    resulting_engine: Option<&'a EngineAnalysisEvidence>,
    source_engine_line: &'a [String],
    resulting_engine_line: &'a [String],
    source_human: Option<&'a HumanMoveModelEvidence>,
    resulting_human: Option<&'a HumanMoveModelEvidence>,
}

/// The evidence a Coach Turn may reason from, and nothing else.
///
/// The packet holds every position of the reviewed Game with its provenance and
/// digests; a turn is about one move. Three rules shape what survives, all of
/// them Task A's ([`crate::language_layer_prompt`]) rather than new: no UCI,
/// because it is banned in output and already grounded through its squares; no
/// evidence ids, because the runtime cites them and the model cannot; and the
/// human move model is renamed at the boundary, because the words it names
/// itself with are the words the gate rejects.
fn project_evidence(
    target: &PreparedCoachTurnTarget,
    input: EvidenceProjectionInput<'_>,
) -> Result<Value, CoachTurnRejection> {
    let anchor = &target.context.reviewed_move;
    let source_fen = &target.source_position.fen;
    let resulting_fen = &target.target.resulting_position.fen;

    let mut ancestor_line = Vec::new();
    for ancestor in &target.ancestor_branch {
        let fen = position_fen(target, &ancestor.source_position_ref).unwrap_or(source_fen);
        ancestor_line.push(san_for(fen, &ancestor.move_uci)?);
    }

    Ok(json!({
        "reviewedMove": {
            "moveNumber": anchor.ply.div_ceil(2),
            "side": anchor.side,
            "playedMove": san_for(source_fen, &anchor.played_move_uci)?,
        },
        "alternativeMove": input.alternative_move_san,
        "sharedMove": input.shared_move_san,
        "ancestorLine": ancestor_line,
        "positionBefore": position_projection(&target.source_position),
        "positionAfter": position_projection(&target.target.resulting_position),
        "engineBefore": input.source_engine.map(|analysis| json!({
            "bestMove": input.best_move_san,
            "evaluation": analysis.evaluation,
            "line": input.source_engine_line,
        })),
        "alternativeOutcome": {
            "evaluation": target.target.evaluation.selected_move,
            "evaluationLoss": target.target.evaluation.comparison,
            "strongestReply": input.strongest_reply_san,
            "sharedReply": input.shared_reply_san,
        },
        "engineAfter": input.resulting_engine.map(|analysis| json!({
            "evaluation": analysis.evaluation,
            "line": input.resulting_engine_line,
        })),
        "playersAtThisRatingBefore": human_projection(source_fen, input.source_human)?,
        "playersAtThisRatingAfter": human_projection(resulting_fen, input.resulting_human)?,
    }))
}

/// Whose pieces sit where. The allowlist admits every occupied square of both
/// positions, so this is exactly the board the model is permitted to talk about
/// — showing less would offer squares it cannot see, showing more would offer
/// squares it cannot name.
fn position_projection(position: &PositionSnapshot) -> Value {
    json!({
        "sideToMove": position.side_to_move,
        "status": position.status,
        "pieces": position
            .occupied
            .iter()
            .map(|occupied| {
                (
                    occupied.square.as_str().to_string(),
                    json!(format!("{:?} {:?}", occupied.piece.color, occupied.piece.role)
                        .to_lowercase()),
                )
            })
            .collect::<serde_json::Map<_, _>>(),
    })
}

fn human_projection(
    fen: &str,
    analysis: Option<&HumanMoveModelEvidence>,
) -> Result<Value, CoachTurnRejection> {
    let Some(analysis) = analysis else {
        return Ok(Value::Null);
    };
    let mut candidates = Vec::new();
    for candidate in &analysis.candidates {
        candidates.push(json!({
            "move": san_for(fen, &candidate.uci)?,
            "rank": candidate.rank,
            "probability": candidate.probability,
        }));
    }
    Ok(json!({ "rating": analysis.player_elo, "candidates": candidates }))
}

fn position_fen<'a>(
    target: &'a PreparedCoachTurnTarget,
    position_ref: &PositionRef,
) -> Option<&'a str> {
    if &target.source_position.position_ref == position_ref {
        return Some(&target.source_position.fen);
    }
    target
        .ancestor_branch
        .iter()
        .chain(std::iter::once(&target.target))
        .find(|result| &result.resulting_position.position_ref == position_ref)
        .map(|result| result.resulting_position.fen.as_str())
}

/// A Coach Turn carries no learning material, so no URL is ever admissible.
fn contains_url(text: &str) -> bool {
    let lowercase = text.to_ascii_lowercase();
    ["http://", "https://", "www."]
        .iter()
        .any(|prefix| lowercase.contains(prefix))
}

/// Noun phrase for a stated coincidence. The shared SAN arrives once through
/// the required marker that owns it (`{alternativeMove}` / `{strongestReply}`).
fn coincidence_text(san: Option<&str>) -> Option<String> {
    san.map(|_| "the same move".to_string())
}

fn ranked_first(analysis: &HumanMoveModelEvidence) -> Option<&HumanMoveCandidateEvidence> {
    analysis
        .candidates
        .iter()
        .min_by_key(|candidate| candidate.rank)
}

fn popularity_text(analysis: &HumanMoveModelEvidence, uci: &str) -> Option<String> {
    let rank = analysis
        .candidates
        .iter()
        .find(|candidate| candidate.uci == uci)
        .map(|candidate| candidate.rank);
    Some(match rank {
        Some(1) => "the most common choice at your rating".to_string(),
        Some(2) => "the second most common choice at your rating".to_string(),
        Some(3) => "the third most common choice at your rating".to_string(),
        Some(_) => "an uncommon choice at your rating".to_string(),
        None => "a move players at your rating rarely consider".to_string(),
    })
}

fn evaluation_text(evaluation: &EngineEvaluation) -> String {
    let (value, perspective) = match evaluation {
        EngineEvaluation::Centipawns { value, perspective } => {
            (PositionEvaluation::Centipawns(*value), *perspective)
        }
        EngineEvaluation::Mate {
            outcome,
            distance_plies,
            perspective,
        } => {
            let distance = i32::from(*distance_plies);
            let signed = match outcome {
                MateOutcome::Win => distance,
                MateOutcome::Loss => -distance,
            };
            (PositionEvaluation::MateIn(signed), *perspective)
        }
    };
    let display = evaluation_display(
        value,
        match perspective {
            Color::White => MoveSide::White,
            Color::Black => MoveSide::Black,
        },
    );
    format!("{}, {}", display.score, display.label)
}

fn evaluation_loss_text(comparison: &EvaluationLoss) -> String {
    match comparison {
        // Article-first on purpose: the bare "0.2 pawns of the position" met a
        // model-supplied article as "an 0.2 pawns of the position" (#364), and
        // a form that brings its own article is one the substitution seam can
        // absorb a doubled article against.
        EvaluationLoss::Centipawns { value } => {
            format!(
                "a {}-pawn margin",
                crate::moment_display::loss_pawns(*value)
            )
        }
        EvaluationLoss::Mate { .. } => "a forced mate".to_string(),
    }
}

fn san_for(fen: &str, uci: &str) -> Result<String, CoachTurnRejection> {
    let position = position_at(fen)?;
    let chess_move = UciMove::from_ascii(uci.as_bytes())
        .ok()
        .and_then(|parsed| parsed.to_move(&position).ok())
        .ok_or(CoachTurnRejection::EvidenceUnreadable)?;
    Ok(SanPlus::from_move(position, &chess_move).to_string())
}

/// The engine line in SAN, stopping at the first move the recorded line cannot
/// legally play rather than guessing past it.
fn san_line(fen: &str, principal_variation: &[String]) -> Vec<String> {
    let Ok(mut position) = position_at(fen) else {
        return Vec::new();
    };
    let mut line = Vec::new();
    for uci in principal_variation {
        let Some(chess_move) = UciMove::from_ascii(uci.as_bytes())
            .ok()
            .and_then(|parsed| parsed.to_move(&position).ok())
        else {
            break;
        };
        line.push(SanPlus::from_move(position.clone(), &chess_move).to_string());
        position.play_unchecked(&chess_move);
    }
    line
}

fn position_at(fen: &str) -> Result<Chess, CoachTurnRejection> {
    fen.parse::<Fen>()
        .ok()
        .and_then(|fen| fen.into_position(CastlingMode::Standard).ok())
        .ok_or(CoachTurnRejection::EvidenceUnreadable)
}
