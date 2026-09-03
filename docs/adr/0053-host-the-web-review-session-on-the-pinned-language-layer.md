# Host the web Review Session on the pinned Language Layer

## Status

Accepted (2026-08-24). Implemented on `main` through
#433–#436.
#437 certified the
in-process HostTurn journey and observed one bound HostTurn published on
staging on 2026-08-28, recorded in
`docs/hosted-language-layer-rollout.md` Gate 9. Amends ADR 0049, ADR 0050,
ADR 0051 as stated under Consequences.

## Context

The web Review Session talks to the hosted Language Layer through two fixed
authoring tasks (#233): a Review Moment Comment and an Alternative Move
Coach Turn. The web composer therefore carries two modes, `discuss` and
`coach`, and a Player's free-text question reaches the model only when an
Alternative Move is active; otherwise the message is stranded (#417). On
ChatGPT and Claude the host model reads free text, chooses among the MCP
tools, and writes from what they return — a loop the web surface has no
equivalent of.

Four recorded decisions stood in the way of giving the web the same loop:
the provider port issues one completion against one schema and has no tool
surface; ADR 0049 fingerprints one prompt/schema pair per generation; the
Grounding Gate (#347) admits prose against one evidence packet; ADR 0050
admits spend per call against a worst-case estimate.

The MCP tools themselves are the wrong shape for a host we control. They
carry opaque handles in every call because consumer hosts hold no state,
they mount widgets, and they speak pacing vocabulary at an external model.
The web Review Session actor already holds every Review Moment, its evidence
packet, and the exploration runtime in process.

## Decision

The web Review Session becomes a host: one Player message is one
**HostTurn**, authored by the pinned model under a web-specific system
prompt, routing over an **internal capability channel** inside the Review
Session actor, and admitted, fingerprinted, grounded, and recorded as one
generation.

1. **Schema-routed, not tool-called.** Every step is the request shape the
   pin already serves — `response_format: json_schema` — with a step schema
   that is a union of `call`, `answer`, and `refuse`. The provider port is
   unchanged. Pin verification runs on the answer step's generation.
2. **Capability channel.** Four capabilities backed by the session actor:
   `read_moment`, `list_moments`, `evaluate_line`, `learning_material`.
   Rendering is an output field (`focusMoment`, `showLine`) the web acts on
   through existing reducers, never a capability.
3. **Web system prompt**, authored in Coach Engine, pre-loads the open
   moment's packet, the active branch, the Coaching Profile, the last four
   turns' prose, and the Grounding Gate's rules as constraints. Grounding
   sentences are single-sourced in one JSON file read by Rust and TS.
4. **One identity per turn.** The fingerprint is over the existing attested
   axis set on `evaluation-fingerprint/v1`. The step schema and capability
   schemas fold into `responseSchemaDigest` as `digest({step, capabilities})`;
   the pre-loaded evidence schema occupies `evidenceSchemaDigest`; the prompt
   template and profile projection keep their axes. Per-step observations sit
   beside the digest, as ADR 0049 already prescribes for per-call data. The
   Evaluation Contract Version does not bump: the axis *set* is unchanged,
   only the value that occupies `responseSchemaDigest`.
5. **Grounding over the union.** The literal projection is the union of the
   pre-loaded packet and every capability result cited this turn. A
   rejection gets one corrective retry carrying the gate's reason; a second
   rejection makes the whole turn unavailable. Nothing partial is shown.
6. **Per-turn envelope.** Three counted model calls at most, including the
   corrective retry, plus one per-turn transport retry; `4 × operation
   ceiling` is reserved before the first call against the Review Session
   ceiling and released at the end. Denial is unavailable with nothing
   spent. Turn deadline 15 s.
7. **Scope is this Review Session.** Questions the session cannot ground are
   refused with `NotAboutThisReview`; the engine renders refusal text.
8. **The web half of Coach Turn retires** with the composer swap:
   `HostedCoachTurnAuthor` and `HostedTask::CoachTurn`. `StartCoachTurn` and
   its preparation stay for the Coach App and local surfaces, where the
   consumer host is the loop.

## Considered alternatives

- **Native tool calling on the pinned route.** Unproven on the pin through
  OpenRouter, needs a provider-port extension, and adds nothing four
  capabilities need. Rejected.
- **Route the web host through the MCP tools via Central Host.** One extra
  hop per call, a principal re-derivation, and JSON-to-Rust re-marshalling of
  the evidence types the gate consumes natively; and the tools are shaped by
  limits the web does not have. Rejected.
- **Per-step fingerprints.** Fragments the join between captures, feedback,
  and cost rows for no gain. Rejected.
- **Concept-level answers to general chess questions.** The literal gate
  cannot tell a concept from a claim; an invitation to ungrounded theory.
  Rejected for v1.

## Consequences

- ADR 0049: HostTurn occupies the existing attested axes. Step and
  capability schemas fold into `responseSchemaDigest`; the pre-loaded
  evidence schema occupies `evidenceSchemaDigest`. `steps[]` joins the
  Operational Record beside the digest. The Evaluation Contract Version
  stays `evaluation-fingerprint/v1`.
- ADR 0050: a per-turn envelope of `4 × operation` (3 counted steps + one
  transport retry) is the admission unit for web Player-driven spend; the
  Review Session and Player ceilings are unchanged. The 20_000 hold is
  conservative: a HostTurn starts only while committed spend is ≤ 5_000.
  While the envelope is held, Comment and Coach Turn admit against
  committed + reserved + worst-case, so they deny once committed > 500.
- ADR 0051: runtime composition is unchanged; `HostedTask::CoachTurn` is
  removed and `HostedTask::HostTurn` added.
- #417 item 3 is superseded; items 1–2 stand. #427 builds its presentational
  thread against the HostTurn state model.
- The web Review Session protocol revision bump is documentation-only
  (pre-production, no migration, no wire field). The live fence is surface
  ownership: `StartHostTurn` is web-only; `StartCoachTurn` is not
  web-admissible.
