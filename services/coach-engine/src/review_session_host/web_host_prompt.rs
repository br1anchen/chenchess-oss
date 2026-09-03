use serde_json::Value;

use super::digest::digest_templates;
use super::grounding::{refusal_text, shared_grounding_block};
use crate::language_layer_prompt::{CoachingProfileProjection, PLAYER_MESSAGE_POINTER};
use crate::review_session_contract::{HostTurnPriorTurn, HostTurnRefusalReason};

pub const WEB_HOST_SYSTEM_TEMPLATE: &str = r#"You are the Chen Chess Coach hosting this Review Session on the web.

1. ROLE AND PLAYER
You talk to one Player about this reviewed game. Second person. Plain and direct.
{{coaching_profile_slot}}

2. PRE-LOADED EVIDENCE
The open moment packet, the active branch, the Coaching Profile projection, and the last four turns of prose are already in the user turn. Answer from those first. Call a capability only when that pre-loaded evidence cannot ground the question.

3. GROUNDING
These constraints discard the whole turn when violated:
{{grounding_sentences}}
Cite only capability call ids this turn returned.

4. LITERAL VOCABULARY
ALLOWED_CHESS_LITERALS is the Chess Literal Projection of the pre-loaded packet. Each capability result extends that vocabulary. Name only what the current vocabulary lists.

5. CAPABILITIES
The step schema is a closed union: call, answer, or refuse. There is no tools field.
- readMoment: another moment, by ply or the next moment, optionally the next Improvement Opportunity.
- listMoments: which moments in this review matter.
- evaluateLine: one proposed line. Reuse the open moment's exploration. Choose engineBest when the moves name only the Player's turns; choose supplied when both sides are already named. Never reconstruct a line from memory. One evaluateLine per proposed line.
- learningMaterial: how to practise the open moment.
Rendering is an output field, never a capability: set focusMoment or showLine only for a ply or line this turn pre-loaded or returned.

6. REFUSAL
Return the refuse variant. The engine writes the Player-facing sentence; never write it yourself.
- notAboutThisReview: the question is not about this reviewed game or its moments.
  Engine text: {{refuse_not_about_this_review}}
- notAboutChess: the question is not about chess.
  Engine text: {{refuse_not_about_chess}}
- unsafeRequest: the request is unsafe.
  Engine text: {{refuse_unsafe}}

7. STYLE
Answer the question first. Two or three short sentences unless the question needs a line. Speak SAN. No exclamation marks, no praise inflation, no sign-off."#;

pub const WEB_HOST_USER_TEMPLATE: &str = r#"ELO:
{{elo}}

COACHING_PROFILE:
{{coaching_profile_projection}}

OPEN_MOMENT:
{{open_moment_packet}}

ACTIVE_BRANCH:
{{active_branch}}

PRIOR_TURNS:
{{prior_turns}}

ALLOWED_CHESS_LITERALS:
{{allowed_chess_literals}}

PLAYER_MESSAGE:
{{player_message_pointer}}"#;

pub fn web_host_system_template() -> String {
    WEB_HOST_SYSTEM_TEMPLATE
        .replace(
            "{{coaching_profile_slot}}",
            "{{coaching_profile_projection}}",
        )
        .replace("{{grounding_sentences}}", &shared_grounding_block())
        .replace(
            "{{refuse_not_about_this_review}}",
            refusal_text(HostTurnRefusalReason::NotAboutThisReview),
        )
        .replace(
            "{{refuse_not_about_chess}}",
            refusal_text(HostTurnRefusalReason::NotAboutChess),
        )
        .replace(
            "{{refuse_unsafe}}",
            refusal_text(HostTurnRefusalReason::UnsafeRequest),
        )
}

pub fn web_host_prompt_digest() -> String {
    digest_templates(&web_host_system_template(), WEB_HOST_USER_TEMPLATE)
}

pub struct HostTurnPromptInput<'a> {
    pub elo: u16,
    pub profile: &'a CoachingProfileProjection,
    pub open_moment_packet: &'a Value,
    pub active_branch: &'a Value,
    pub prior_turns: &'a [HostTurnPriorTurn],
    pub allowed_chess_literals: &'a [String],
}

pub fn compile_web_host_prompt(input: HostTurnPromptInput<'_>) -> (String, String) {
    let system = web_host_system_template()
        .replace("{{coaching_profile_projection}}", &input.profile.render());
    let prior = if input.prior_turns.is_empty() {
        "(none — answer the Player's message on its own)".to_owned()
    } else {
        input
            .prior_turns
            .iter()
            .map(|turn| format!("Player: {}\nCoach: {}", turn.message, turn.answer))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let user = WEB_HOST_USER_TEMPLATE
        .replace("{{elo}}", &input.elo.to_string())
        .replace("{{coaching_profile_projection}}", &input.profile.render())
        .replace(
            "{{open_moment_packet}}",
            &serde_json::to_string_pretty(input.open_moment_packet)
                .expect("open moment packet serializes"),
        )
        .replace(
            "{{active_branch}}",
            &serde_json::to_string_pretty(input.active_branch).expect("active branch serializes"),
        )
        .replace(
            "{{allowed_chess_literals}}",
            &input.allowed_chess_literals.join(" "),
        )
        .replace("{{player_message_pointer}}", PLAYER_MESSAGE_POINTER)
        .replace("{{prior_turns}}", &prior);
    (system, user)
}
