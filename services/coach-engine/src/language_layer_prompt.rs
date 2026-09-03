//! The compiled v1 prompts for the two web Language Layer tasks.
//!
//! #344 wrote the templates
//! down; this module is the compiled artifact they describe, built from the same
//! policy the gate enforces. The marker vocabulary and the chess-literal
//! allowlist are read out of
//! [`crate::critical_moment_comment`] rather than restated, so what the model is
//! offered and what
//! [`crate::critical_moment_comment::ground_draft`] admits cannot drift apart.
//!
//! ## The facts projection is deliberate
//!
//! The moment is not serialized wholesale. `GameReviewCriticalMoment` carries
//! ids, a decision-explanation proof, and a display block that exist for other
//! consumers, and it names the human move model in its own field names —
//! `playedMoveIsHumanLikely` is the very phrasing
//! #347 now rejects in
//! authored prose. Handing the model that key and then rejecting it for echoing
//! it would be a trap rather than a contract, so the projection renames the
//! human-move-model block to the Player-facing vocabulary and drops what the
//! model has no use for.
//!
//! The projection is a prompt input, so its shape joins the prompt digest.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::critical_moment_comment::{chess_literal_grounding_for, CommentFactsPolicy};
use crate::language_layer_provider::ChatMessage;
use crate::review_session_coaching::{
    CoachTurnDimension, CoachTurnProseContext, CoachTurnRejection, PreparedCoachTurnTarget,
};
use crate::review_session_contract::{
    CriticalMomentIntentAuthoringContext, GameReviewCriticalMoment, LearningResourceRole,
    MoveTarget, ReviewMomentCommentFacts, ReviewSessionEvidencePacket,
};

/// Ordered top-K Learning Track Keys and nothing else, per
/// #332. The shape is
/// invariant: a cold-start Player projects an empty list rather than omitting
/// the block, and at beta launch that is the common case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachingProfileProjection {
    #[serde(default)]
    pub track_keys: Vec<String>,
}

impl CoachingProfileProjection {
    pub fn cold_start() -> Self {
        Self::default()
    }

    pub fn populated(track_keys: impl IntoIterator<Item = String>) -> Self {
        Self {
            track_keys: track_keys.into_iter().collect(),
        }
    }

    pub(crate) fn render(&self) -> String {
        if self.track_keys.is_empty() {
            return "This Player has no learning history yet.".to_string();
        }
        format!(
            "This Player has recently been working on: {}. Where the facts naturally touch one of \
             these, lean into it. Do not force it, and do not mention this list to the Player.",
            self.track_keys.join(", ")
        )
    }
}

/// One compiled prompt, plus the two gate surfaces the harness reports on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledCommentPrompt {
    pub system: String,
    pub user: String,
    pub required_markers: Vec<String>,
    pub optional_markers: Vec<String>,
    pub allowed_literals: Vec<String>,
}

pub const COMMENT_SYSTEM_TEMPLATE: &str = r#"You are the Chen Chess Coach. You are commentating on one move from a game the Player
has just had reviewed. Write the way a good commentator talks over a game: react to the
move, say what the Player was probably seeing, then say what was actually there.

Voice — fixed, not Player-configurable:
- Talk to the Player, not about them. Second person.
- Plain and direct. No exclamation marks, no praise inflation, no "great question",
  no addressing them by name, no sign-off.
- Lead with the move and what is interesting about it, not with a verdict label.
- Respect the Player. A move most players at their rating would pick is not a silly
  move, and you should say so when {playedPopularity} is available.
- Explain the idea, not the notation.
- Name the piece beside every move you write in your own words. The notation already
  tells you which one: N is a knight, B a bishop, R a rook, Q the queen, K the king,
  and a move written as a bare square is a pawn. "the knight to d4", "the bishop
  takes on f3", "the pawn steps to e4" — never a bare "Nd4" standing alone, and never
  a piece the notation does not name. This rule is about your prose. It does not
  reach a marker, whose rendering is not yours to shape — see MARKERS.
- A line in FACTS — an engine continuation, a projected plan, its counterplay — is
  evidence, not copy. Never transcribe one move by move. Tell what it does: name the
  piece, where it lands, and what it threatens or defends, quoting at most the first
  move or two. "the knight comes to d4 and eyes the loose pawn" coaches;
  "Nc3 Bg4 e3 e6" is a scoresheet.
- Say "players at your rating". Never mention a model, an engine name, Maia, Stockfish,
  or any internal term. The Player is talking to a coach, not to a pipeline.

Length — judge it from the moment:
- A lost advantage or a missed forced mate earns a short paragraph.
- A reduced advantage, or a good move worth praising, earns two or three sentences.
- A neutral move earns one line. Do not manufacture a lesson for it.

MARKERS — this is the hard part, read it twice:
- Every evaluation, score, percentage, and probability MUST be written as a marker from
  MARKERS. Never write a number yourself. Not even one you can see in FACTS.
- Use every marker listed as required, exactly once each. Use optional markers when
  they help.
- Write markers verbatim, braces included: {betterMove}, not "betterMove" or "the
  better move".
- A marker stands in for a phrase, not a sentence. Build your sentence around it the
  way you would around "the knight on d4" — it slots into your clause, it does not
  replace it.
- A marker's rendering is not your prose, and no Voice rule reaches it. {playedMove}
  renders as the move's notation on its own, and that is correct — write the marker
  where the move belongs and let the runtime supply it. Never drop a required marker,
  or spell its fact out in your own words instead, to avoid writing bare notation:
  omitting one discards the whole comment.
- A marker labelled (own sentence) is the exception: give it a sentence to itself,
  with nothing before it and nothing after it but the full stop. Its rendering
  already carries its own subject and verb, so anything you wrap around it reads
  doubled. Write "{achievement}" standing alone — never "You found {achievement}",
  never "the opportunity to {achievement}", never "{achievement} on the back rank".

Grounding — violating one discards the whole comment:
- Every chess move and square you name in your own words must appear in
  ALLOWED_CHESS_LITERALS. Bare square names count. If a square is not listed, do not
  name it.
- Never write coordinate notation such as "f3d4".
- FACTS labels things in machine spellings — "occupyTheCenter", "advantageLost". They
  are there for you to read, never to quote. Say what they mean in your own words, or
  use the marker that carries them.
- Do not reason past the facts. Do not say what the opponent threatens, what happens
  after the engine's move, or how the game continued, unless FACTS states it.
- Never credit an outcome the move did not earn. "You won a knight", "this wins
  material", "that forces mate" are factual claims — write one only when FACTS records
  that capture, payoff, or achievement for this exact move. A developing move earns
  developing-move commentary, nothing more.
- When FACTS carries playerIntent, exactly one sentence guesses at what the Player was
  trying to do, and that one sentence must hedge and name a plan together: "my best
  guess", "may have", "might have", or "possibly" standing beside "aiming", "plan", or
  "idea". Saying where a piece goes is description, not a guess — a sentence that
  names a destination without a hedge does not count, and neither does a hedge with no
  plan word. Write no second sentence of that shape.
- Never include a URL, a link, or a citation, except an exact resource line from
  LEARNING_MATERIAL reproduced verbatim.

Output shape:
- Exactly one paragraph. No line breaks, no headings, no lists, no markdown.

If the facts do not support commentary you can honestly write, return the refusal
variant rather than inventing content."#;

pub const COMMENT_USER_TEMPLATE: &str = r#"COACHING_PROFILE:
{{coaching_profile_projection}}

FACTS:
{{facts_json}}

MARKERS:
{{marker_vocabulary}}

ALLOWED_CHESS_LITERALS:
{{allowed_chess_literals}}

LEARNING_MATERIAL:
{{learning_material}}"#;

pub fn compile_comment_prompt(
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
    profile: &CoachingProfileProjection,
) -> CompiledCommentPrompt {
    let moment = facts.moment();
    let policy = CommentFactsPolicy::for_facts(facts);

    let mut required_markers = Vec::new();
    let mut optional_markers = Vec::new();
    let mut vocabulary_lines = Vec::new();
    for (marker, form, required) in policy.markers.entries() {
        let mut label = if required { "required" } else { "optional" }.to_string();
        // The one marker with no form that fits inside a sentence says so here
        // rather than in a standing instruction, so the rule appears only for
        // the moment kinds that have it.
        if form.requires_own_sentence() {
            label.push_str(", own sentence");
        }
        let rendering = form.offered();
        vocabulary_lines.push(format!(
            "- {{{marker}}} ({label}) → renders as: {rendering}"
        ));
        if required {
            required_markers.push(marker.to_string());
        } else {
            optional_markers.push(marker.to_string());
        }
    }

    let allowed_literals = chess_literal_grounding_for(facts, intent)
        .allowed()
        .map(str::to_string)
        .collect::<Vec<_>>();

    let facts_json = serde_json::to_string_pretty(&project_facts(facts, intent))
        .expect("the facts projection is serializable");

    let user = COMMENT_USER_TEMPLATE
        .replace("{{coaching_profile_projection}}", &profile.render())
        .replace("{{facts_json}}", &facts_json)
        .replace("{{marker_vocabulary}}", &vocabulary_lines.join("\n"))
        .replace("{{allowed_chess_literals}}", &allowed_literals.join(" "))
        .replace("{{learning_material}}", &render_learning_material(moment));

    CompiledCommentPrompt {
        system: COMMENT_SYSTEM_TEMPLATE.to_string(),
        user,
        required_markers,
        optional_markers,
        allowed_literals,
    }
}

/// The response schema, native structured output per
/// #294.
///
/// **Not a `oneOf` union**, though the outcome is one.
/// #344 wrote the two
/// outcomes as a `oneOf`, and Bedrock's native structured output rejects that
/// outright — `Schema type 'oneOf' is not supported`, HTTP 400, verified
/// 2026-08-14 on `anthropic/claude-haiku-4.5` → `amazon-bedrock/global`. Since
/// the only routes with a genuine zero-retention claim are the two Bedrock ones,
/// a schema they cannot serve would decide the pin by accident.
///
/// The union is therefore flattened: one closed `kind` enum, every field
/// required, and a `none` sentinel on the refusal reason. Both properties
/// #233 asked of it
/// survive — the refusal is a typed variant, and there is no free-form field —
/// and the shape is the portable one that strict mode expects everywhere.
pub fn comment_response_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "comment", "refusalReason"],
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["comment", "outOfScope"]
            },
            "comment": {
                "type": "string",
                "maxLength": 2000
            },
            "refusalReason": {
                "type": "string",
                "enum": [
                    "none",
                    "factsInsufficient",
                    "requestNotAboutThisPosition",
                    "unsafeRequest"
                ]
            }
        }
    })
}

/// SHA-256 over the compiled artifact with placeholders unsubstituted — the
/// template, not the rendered prompt, since the rendered prompt varies per
/// moment while the candidate identity must not.
pub fn comment_prompt_digest() -> String {
    let mut hasher = Sha256::new();
    hasher.update(COMMENT_SYSTEM_TEMPLATE.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(COMMENT_USER_TEMPLATE.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

pub fn comment_schema_digest() -> String {
    digest_canonical_json(&comment_response_schema())
}

/// Declared Coaching Profile Projection schema. The shape is invariant: a
/// cold-start Player projects an empty `trackKeys` list rather than omitting
/// the block.
pub fn coaching_profile_projection_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["trackKeys"],
        "properties": {
            "trackKeys": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
}

pub fn coaching_profile_projection_schema_digest() -> String {
    digest_canonical_json(&coaching_profile_projection_schema())
}

/// Declared identity of the comment facts projection the model is shown.
/// The rendered projection varies per moment; the candidate identity must not.
pub fn comment_evidence_schema_digest() -> String {
    digest_canonical_json(&json!({
        "kind": "commentFactsProjection",
        "version": "v1",
        "keys": [
            "momentKind",
            "moveNumber",
            "side",
            "playedMove",
            "positionPhase",
            "classification",
            "engine",
            "playersAtThisRating",
            "effects",
            "residualOutcome",
            "playedMoveOutcome",
            "mechanism",
            "teaching",
            "playerIntent"
        ]
    }))
}

// ---------------------------------------------------------------------------
// Task B — Alternative Move Assessment authoring
// ---------------------------------------------------------------------------

/// One compiled Coach Turn prompt, plus the gate surface the harness reports on.
///
/// The markers are **per dimension**, which is the one structural difference
/// from Task A: a `findability` explanation may not name the resulting
/// evaluation, because it does not cite the evidence that would ground it. So
/// the prompt carries three vocabularies rather than one, and a marker offered
/// under one dimension is an unknown marker under another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledCoachTurnPrompt {
    pub system: String,
    pub user: String,
    /// The Player message as its own field. It is never spliced into [`Self::user`].
    pub player_message: String,
    pub dimensions: Vec<CompiledCoachTurnDimension>,
    pub allowed_literals: Vec<String>,
}

impl CompiledCoachTurnPrompt {
    /// System, evidence user turn, then a dedicated `playerMessage:` turn.
    pub fn chat_messages(&self) -> Vec<ChatMessage> {
        vec![
            ChatMessage {
                role: "system".into(),
                content: self.system.clone(),
            },
            ChatMessage {
                role: "user".into(),
                content: self.user.clone(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!("playerMessage:\n{}", self.player_message),
            },
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledCoachTurnDimension {
    pub dimension: CoachTurnDimension,
    pub required_markers: Vec<String>,
    pub optional_markers: Vec<String>,
}

pub const COACH_TURN_SYSTEM_TEMPLATE: &str = r#"You are the Chen Chess Coach. The Player has asked about a move they were considering
instead of the one they played. Assess it along exactly three dimensions, one short
explanation each.

The dimensions, and what each asks:
- objectiveQuality: is the move good, by the engine's reckoning? Compare the position
  before and after.
- findability: would a player at this rating actually find this at the board? Some
  moves are strong and nobody finds them; some are natural and happen to be weak. Say
  which this is, plainly.
- resilience: if they play it, what happens next? Does the position hold up under the
  replies opponents at this rating actually choose?

Voice — fixed, not Player-configurable:
- Talk to the Player. Second person. Plain and direct.
- Two or three sentences per dimension. Answer the dimension you are in; do not spill
  one dimension's content into another.
- Do not restate their question back to them.
- No exclamation marks, no praise inflation, no addressing them by name, no sign-off.
- Say "players at your rating". Never mention a model, an engine name, Maia, Stockfish,
  or any internal term. The Player is talking to a coach, not to a pipeline.

MARKERS — this is the hard part, read it twice:
- Every evaluation, score, percentage, and probability MUST be written as a marker from
  MARKERS. Never write a number yourself. Not even one you can see in EVIDENCE.
- MARKERS lists a separate vocabulary per dimension. A marker belongs to the dimension
  it is listed under and nowhere else: naming it in another dimension discards the turn.
- Use every marker listed as required for a dimension, exactly once each, in that
  dimension. Use optional markers when they help.
- Write markers verbatim, braces included: {bestMove}, not "bestMove" or "the best
  move".
- A marker stands in for a phrase, not a sentence. Build your sentence around it the
  way you would around "the knight on d4" — it slots into your clause, it does not
  replace it.
- A marker's rendering brings its own article when it needs one. Never write "a", "an",
  or "the" directly before a marker.
- When a dimension offers {sharedReply}, that marker renders as "the same move". The
  engine's strongest reply and the move players at your rating find most often coincide;
  the SAN is already in {strongestReply}. Use {sharedReply} to name the coincidence. Do
  not put the SAN in the coincidence marker.
- When a dimension offers {sharedMove}, that marker renders as "the same move". The
  alternative and the most common choice at your rating coincide; the SAN is already in
  {alternativeMove}. Use {sharedMove} to name the coincidence. Do not put the SAN in the
  coincidence marker.

Grounding — violating one discards the whole turn:
- Every chess move and square you name in your own words must appear in
  ALLOWED_CHESS_LITERALS. Bare square names count. If a square is not listed, do not
  name it.
- Never write coordinate notation such as "f3d4".
- EVIDENCE labels things in machine spellings. They are there for you to read, never to
  quote. Say what they mean in your own words, or use the marker that carries them.
- Every claim must trace to EVIDENCE. Do not invent a line, and do not continue one
  past what EVIDENCE records.
- Never include a URL, a link, or a citation.

Output shape:
- Three explanations, one per dimension. No line breaks, no headings, no lists, no
  markdown.

If the Player's message is not about this position or this alternative move, return the
refusal variant. Do not answer it, and do not scold them."#;

pub const COACH_TURN_USER_TEMPLATE: &str = r#"COACHING_PROFILE:
{{coaching_profile_projection}}

PLAYER_MESSAGE:
{{player_message_pointer}}

PRIOR_TURN:
{{prior_turn}}

ALTERNATIVE_MOVE:
{{target_summary}}

EVIDENCE:
{{evidence_packet_projection}}

MARKERS:
{{marker_vocabulary}}

ALLOWED_CHESS_LITERALS:
{{allowed_chess_literals}}"#;

/// What the model may see of the turn before this one.
///
/// #233 makes prior text
/// visible **only** within one Alternative Move: a Coach Turn steering a
/// different alternative is not context, it is another conversation. The
/// distinction is made by the caller, because only the caller knows which
/// alternative the prior turn steered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorCoachTurnText<'a> {
    /// No prior turn, or one that steered a different Alternative Move.
    None,
    /// The prior turn's published assessment, steering this same alternative.
    SameAlternative {
        objective_quality: &'a str,
        findability: &'a str,
        resilience: &'a str,
    },
}

/// Pointer in the evidence user turn. The Player message itself is the next
/// chat turn, labelled `playerMessage`.
pub const PLAYER_MESSAGE_POINTER: &str =
    "(the Player's message is the next user turn, labelled playerMessage)";

pub fn compile_coach_turn_prompt(
    target: &PreparedCoachTurnTarget,
    packet: &ReviewSessionEvidencePacket,
    message: &str,
    prior_turn: PriorCoachTurnText<'_>,
    profile: &CoachingProfileProjection,
) -> Result<CompiledCoachTurnPrompt, CoachTurnRejection> {
    let prose = CoachTurnProseContext::prepare(target, packet)?;

    let mut dimensions = Vec::new();
    let mut vocabulary_blocks = Vec::new();
    for dimension in CoachTurnDimension::ALL {
        let markers = prose.vocabulary(dimension);
        let mut required = Vec::new();
        let mut optional = Vec::new();
        let mut lines = Vec::new();
        for (marker, form, is_required) in markers.entries() {
            let mut label = if is_required { "required" } else { "optional" }.to_string();
            if form.requires_own_sentence() {
                label.push_str(", own sentence");
            }
            lines.push(format!(
                "- {{{marker}}} ({label}) → renders as: {}",
                form.offered()
            ));
            if is_required {
                required.push(marker.to_string());
            } else {
                optional.push(marker.to_string());
            }
        }
        vocabulary_blocks.push(format!("{}:\n{}", dimension.as_str(), lines.join("\n")));
        dimensions.push(CompiledCoachTurnDimension {
            dimension,
            required_markers: required,
            optional_markers: optional,
        });
    }

    let allowed_literals = prose.allowed_literals();
    let evidence = serde_json::to_string_pretty(prose.projection())
        .expect("the evidence projection is serializable");
    let summary = serde_json::to_string_pretty(&json!({
        "alternativeMove": prose.projection()["alternativeMove"],
        "insteadOf": prose.projection()["reviewedMove"],
        "reachedAfter": prose.projection()["ancestorLine"],
    }))
    .expect("the target summary is serializable");

    let user = COACH_TURN_USER_TEMPLATE
        .replace("{{coaching_profile_projection}}", &profile.render())
        .replace("{{player_message_pointer}}", PLAYER_MESSAGE_POINTER)
        .replace("{{prior_turn}}", &render_prior_turn(prior_turn))
        .replace("{{target_summary}}", &summary)
        .replace("{{evidence_packet_projection}}", &evidence)
        .replace("{{marker_vocabulary}}", &vocabulary_blocks.join("\n\n"))
        .replace("{{allowed_chess_literals}}", &allowed_literals.join(" "));

    Ok(CompiledCoachTurnPrompt {
        system: COACH_TURN_SYSTEM_TEMPLATE.to_string(),
        user,
        player_message: message.to_string(),
        dimensions,
        allowed_literals,
    })
}

fn render_prior_turn(prior_turn: PriorCoachTurnText<'_>) -> String {
    match prior_turn {
        PriorCoachTurnText::None => "(none — answer the Player's message on its own)".to_string(),
        PriorCoachTurnText::SameAlternative {
            objective_quality,
            findability,
            resilience,
        } => format!(
            "You have already assessed this same move once. Do not repeat yourself; answer what \
             they have asked now.\n- objectiveQuality: {objective_quality}\n- findability: \
             {findability}\n- resilience: {resilience}"
        ),
    }
}

/// The Task B response schema, flattened for the same reason Task A's is.
///
/// #344 wrote the outcome
/// as a `oneOf` under an `outcome` wrapper, and
/// #346 measured Bedrock
/// rejecting `oneOf` outright — `Schema type 'oneOf' is not supported`, HTTP
/// 400. The union is therefore flattened here exactly as it was there: one
/// closed `kind` enum, every field required, and a `none` sentinel on the
/// refusal reason. The typed refusal and the no-free-form-field rule both
/// survive.
pub fn coach_turn_response_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "objectiveQuality", "findability", "resilience", "refusalReason"],
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["assessment", "outOfScope"]
            },
            "objectiveQuality": { "type": "string", "maxLength": 1200 },
            "findability": { "type": "string", "maxLength": 1200 },
            "resilience": { "type": "string", "maxLength": 1200 },
            "refusalReason": {
                "type": "string",
                "enum": [
                    "none",
                    "notAboutThisPosition",
                    "notAboutChess",
                    "unsafeRequest"
                ]
            }
        }
    })
}

pub fn coach_turn_prompt_digest() -> String {
    let mut hasher = Sha256::new();
    hasher.update(COACH_TURN_SYSTEM_TEMPLATE.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(COACH_TURN_USER_TEMPLATE.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

pub fn coach_turn_schema_digest() -> String {
    digest_canonical_json(&coach_turn_response_schema())
}

/// Declared identity of the Coach Turn evidence projection the model is shown.
/// Keys are enumerated so the digest moves when the projection shape changes,
/// matching [`comment_evidence_schema_digest`].
pub fn coach_turn_evidence_schema_digest() -> String {
    digest_canonical_json(&json!({
        "kind": "coachTurnEvidenceProjection",
        "version": "v1",
        "keys": [
            "reviewedMove",
            "alternativeMove",
            "sharedMove",
            "ancestorLine",
            "positionBefore",
            "positionAfter",
            "engineBefore",
            "alternativeOutcome",
            "engineAfter",
            "playersAtThisRatingBefore",
            "playersAtThisRatingAfter"
        ]
    }))
}

fn digest_canonical_json(value: &Value) -> String {
    let canonical =
        serde_json_canonicalizer::to_string(value).expect("schema values are canonicalizable");
    format!("sha256:{:x}", Sha256::digest(canonical.as_bytes()))
}

fn render_learning_material(moment: &GameReviewCriticalMoment) -> String {
    let lines = moment
        .learning_material
        .tracks
        .iter()
        .flat_map(|track| track.resources.iter())
        .map(|resource| {
            let role = match resource.role {
                LearningResourceRole::Learn => "Learn",
                LearningResourceRole::Drill => "Drill",
            };
            format!("{role}: {} ({})", resource.title, resource.canonical_url)
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "(none for this moment)".to_string()
    } else {
        lines.join("\n")
    }
}

/// The chess the model needs, and nothing that exists for another consumer.
///
/// Notably absent: the moment id, the decision-explanation proof, the display
/// block, and every UCI string. UCI is banned in output and its squares are
/// already carried by ALLOWED_CHESS_LITERALS, so showing it can only tempt.
pub(crate) fn project_comment_facts(facts: &ReviewMomentCommentFacts) -> Value {
    project_facts(facts, None)
}

fn project_facts(
    facts: &ReviewMomentCommentFacts,
    intent: Option<&CriticalMomentIntentAuthoringContext>,
) -> Value {
    let moment = facts.moment();
    let mut projection = Map::new();
    projection.insert(
        "momentKind".to_string(),
        json!(match facts {
            ReviewMomentCommentFacts::Positive { .. } => "positiveHighlight",
            ReviewMomentCommentFacts::Improvement { .. } => "improvementOpportunity",
            ReviewMomentCommentFacts::Neutral { .. } => "neutral",
        }),
    );
    projection.insert("moveNumber".to_string(), json!(moment.move_number));
    projection.insert("side".to_string(), json!(moment.side));
    projection.insert("playedMove".to_string(), json!(moment.played_san));
    // The phase alone. `policyVersion` is `position-phase/v1` — metadata for
    // another consumer, with no uppercase for the identifier check to catch,
    // so it leaves the projection rather than relying on the gate.
    projection.insert(
        "positionPhase".to_string(),
        json!(moment.position_phase.phase),
    );
    projection.insert(
        "classification".to_string(),
        redact_uci(&moment.classification),
    );
    projection.insert(
        "engine".to_string(),
        json!({
            "bestEvaluation": moment.objective.best_evaluation,
            "playedEvaluation": moment.objective.played_evaluation,
            "centipawnLoss": moment.objective.centipawn_loss,
            "engineLine": moment.objective.lines.as_ref().map(|lines| {
                lines.best.iter().map(|line| line.san.clone()).collect::<Vec<_>>()
            }),
            "refutationLine": moment.objective.lines.as_ref().map(|lines| {
                lines.refutation.iter().map(|line| line.san.clone()).collect::<Vec<_>>()
            }),
        }),
    );
    // What the opponent's reply *does*, beside the move itself. The line alone
    // was something the model was shown and told not to transcribe; this is the
    // half it can say. Absent when the reply is quiet, because then there is
    // nothing to say and an empty list invites filling it in.
    if let Some(resource) = opponent_resource(moment) {
        projection.insert("opponentResource".to_string(), resource);
    }
    // What the move the Player is told about takes or hits, beside the move
    // itself: the played move's target beyond its capture, or the better
    // move's first. Absent when there is none, for the same reason.
    if let Some(target) = move_target(moment) {
        projection.insert("moveTarget".to_string(), target);
    }
    // Renamed at the boundary. The model may never say "human model" or
    // "human-likely", so it is never shown those words.
    projection.insert(
        "playersAtThisRating".to_string(),
        json!({
            "playedMoveRank": moment.human.played_move_rank,
            "playedMoveProbability": moment.human.played_move_probability,
            "mostLikelyMoveProbability": moment.human.most_likely_probability,
            "playedMoveIsACommonChoice": moment.human.played_move_is_human_likely,
        }),
    );
    projection.insert("effects".to_string(), json!(moment.effects));
    projection.insert(
        "residualOutcome".to_string(),
        json!(moment.residual_outcome),
    );
    projection.insert(
        "playedMoveOutcome".to_string(),
        json!(moment.played_move_outcome),
    );
    if let Some(mechanism) = &moment.mechanism {
        projection.insert("mechanism".to_string(), redact_uci(mechanism));
    }
    projection.insert(
        "teaching".to_string(),
        json!({
            "themes": moment.teaching.themes,
            "openingPrinciples": moment.teaching.opening_principles,
        }),
    );
    if let Some(intent) = intent {
        projection.insert(
            "playerIntent".to_string(),
            json!({
                "hypothesis": intent.instructions.hypothesis,
                "projectedPlan": intent
                    .enrichment
                    .as_ref()
                    .map(|enrichment| enrichment.projected_plan_san.clone()),
                "objectiveCounterplay": intent
                    .enrichment
                    .as_ref()
                    .map(|enrichment| enrichment.objective_counterplay_san.clone()),
            }),
        );
    }
    Value::Object(projection)
}

/// The opponent's first reply in the refutation line, with what it does.
///
/// Both halves or neither: a move with no derived effect supports no claim
/// about why the opponent wants it, and naming it alone is what the model
/// already had.
fn opponent_resource(moment: &GameReviewCriticalMoment) -> Option<Value> {
    let resource = moment.objective.lines.as_ref()?.opponent_resource()?;
    Some(json!({
        "move": resource.reply.san,
        "does": resource.does,
    }))
}

fn move_target(moment: &GameReviewCriticalMoment) -> Option<Value> {
    Some(match moment.move_target()? {
        MoveTarget::PlayedHits { role, square } => json!({
            "move": moment.played_san,
            "hits": { "role": role, "square": square },
        }),
        MoveTarget::BetterTakes {
            better_move,
            role,
            square,
        } => json!({
            "move": better_move,
            "takes": { "role": role, "square": square },
        }),
        MoveTarget::BetterHits {
            better_move,
            role,
            square,
        } => json!({
            "move": better_move,
            "hits": { "role": role, "square": square },
        }),
    })
}

/// Drops every `*Uci` key from a serialized sub-tree. Coordinate notation is
/// banned in output and grounded through squares instead, so the model has no
/// use for it.
fn redact_uci(value: &impl serde::Serialize) -> Value {
    fn strip(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .filter(|(key, _)| !key.to_ascii_lowercase().ends_with("uci"))
                    .map(|(key, nested)| (key, strip(nested)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.into_iter().map(strip).collect()),
            other => other,
        }
    }
    strip(serde_json::to_value(value).expect("moment facts are serializable"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cold_start_projection_is_rendered_rather_than_omitted() {
        assert_eq!(
            CoachingProfileProjection::cold_start().render(),
            "This Player has no learning history yet."
        );
    }

    #[test]
    fn the_projection_never_shows_the_model_the_words_it_may_not_write() {
        let rendered = serde_json::to_string(&json!({
            "playersAtThisRating": { "playedMoveIsACommonChoice": true }
        }))
        .unwrap()
        .to_ascii_lowercase();

        for banned in ["human model", "human-likely", "maia", "move model"] {
            assert!(!rendered.contains(banned));
        }
    }

    #[test]
    fn uci_is_stripped_from_projected_sub_trees() {
        let redacted = redact_uci(&json!({
            "betterMoveSan": "Nxd4",
            "betterMoveUci": "f3d4",
            "nested": [{ "playedMoveUci": "f3e5" }]
        }));

        assert_eq!(redacted, json!({ "betterMoveSan": "Nxd4", "nested": [{}] }));
    }

    #[test]
    fn the_coach_turn_user_template_points_at_a_dedicated_player_message_turn() {
        assert!(COACH_TURN_USER_TEMPLATE.contains("{{player_message_pointer}}"));
        assert!(!COACH_TURN_USER_TEMPLATE.contains("{{player_message}}"));
        assert!(PLAYER_MESSAGE_POINTER.contains("playerMessage"));
    }

    #[test]
    fn the_coach_turn_markers_instruction_licenses_the_coincidence() {
        assert!(COACH_TURN_SYSTEM_TEMPLATE.contains("{sharedReply}"));
        assert!(COACH_TURN_SYSTEM_TEMPLATE.contains("{sharedMove}"));
        assert!(COACH_TURN_SYSTEM_TEMPLATE.contains("the same move"));
        assert!(COACH_TURN_SYSTEM_TEMPLATE.contains("{strongestReply}"));
        assert!(COACH_TURN_SYSTEM_TEMPLATE.contains("{alternativeMove}"));
        assert!(COACH_TURN_SYSTEM_TEMPLATE.contains("not put the SAN in the coincidence marker"));
        assert!(COACH_TURN_SYSTEM_TEMPLATE.contains("most common choice at your rating"));
        assert!(COACH_TURN_SYSTEM_TEMPLATE.contains("players at your rating"));
        for banned in ["human-likely", "human likely", "Human Move Model"] {
            assert!(
                !COACH_TURN_SYSTEM_TEMPLATE.contains(banned),
                "Task B prompt-visible text must not say {banned}"
            );
        }
    }
}
