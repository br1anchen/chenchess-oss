# Coach MCP tool and validation interface

Status note (2026-07-29): the final
[Coach App product and implementation specification](./coach-app-product-and-implementation-specification.md)
supersedes this note's tool count, client-carried state, PGN privacy routing,
and widget-rehydration design. ADR 0026 also replaces the typed Move Intent
lifecycle with conversational plan discussion and a stateless Player Plan
Evaluation. This file remains decision history for restricted command
projections and validation rationale.

Decision date: 2026-07-20. Resolves [Design the Coach MCP tool and validation interface](#70) on the [Design and prove the cross-host Coach App](#62) map. Player-confirmed decisions from a grilling session; builds on the fixed constraints in [mcp-apps-cross-host-contract.md](./mcp-apps-cross-host-contract.md) and [coach-mcp-seam.md](./coach-mcp-seam.md).

## TL;DR

Each MCP tool remains a typed projection of a wire command over the existing
NDJSON endpoint. Review Moment Comment publication is a narrow Review
Engine-owned admission boundary: it accepts host prose and a grounding ledger,
but resolves the active Review Session's objective facts itself. Plan
discussion stays in host conversation; `evaluate_player_plan` is an optional,
one-shot prepare/admit workflow with no durable intent state.

## Tool surface

| Tool                            | Command projection                                | Visibility     |
| ------------------------------- | ------------------------------------------------- | -------------- |
| `import_game`                   | `ImportGame`                                      | app/recovery   |
| `start_review_session`          | `StartReviewSession`                              | model + app    |
| `discuss_review_moment`         | open/publish orchestration                        | model + app    |
| `open_review_moment`            | `OpenReviewMoment` compatibility projection       | app by default |
| `publish_review_moment_comment` | `PublishReviewMomentComment` admission projection | model + app    |
| `inspect_position`              | `InspectPosition` compatibility projection        | app by default |
| `evaluate_player_plan`          | `EvaluatePlayerPlan` prepare/admit workflow       | model + app    |
| `explore_alternative_move`      | `ExploreAlternativeMove`                          | model + app    |
| `request_coach_turn`            | `StartCoachTurn`/`PublishCoachTurn` orchestration | model + app    |
| `resume_review_session`         | `ResumeReviewSession`                             | model + app    |
| `cancel_operation`              | `CancelOperation`                                 | app            |

A _restricted projection_ narrows a command's schema (variants or fields) without inventing a parallel command; the adapter maps each tool call to one signed command envelope stamped `surface: coachApp`.

`publish_review_moment_comment` takes the active session and moment handles,
draft text, kind-aware Grounding Ledger, and publication fence. The Review
Engine command seam rejects unknown or expired authority, cross-kind or
otherwise invalid ledgers, and stale fences. It returns only the accepted
comment or deterministic safe rendering; it never returns authoring provenance,
and unsubmitted host chat is not workspace state. Coach Turn publication
remains the separate Alternative Move Assessment admission operation.

The Review Session widget never calls `discuss_review_moment` directly. It
places the exact selected-moment target into model context with
`ui/update-model-context`, then sends the Player's discussion request with
`ui/message`. The model calls `discuss_review_moment`. A host with
sampling/MRTR support completes authoring and publication inside that call. If
sampling is unavailable or malformed, the tool returns one bounded
model-visible authoring handoff in `content`: objective facts, optional Intent
Enrichment, classification-aware instructions, the Grounding Ledger, and exact
publication handles. The model authors one paragraph and calls
`publish_review_moment_comment` exactly once with those supplied arguments.
Only that explicit publication result is canonical.

## Conversational plan model

Hypothesis is **explanation, not a transaction**. Review Moment authoring may
include one explicitly uncertain hypothesis inline. The Player never confirms,
corrects, or skips it through domain commands.

- Agreement needs no state transition.
- Disagreement or a different plan continues as ordinary host conversation.
- When an objective comparison would materially help, the Language Layer calls
  `evaluate_player_plan`; the operation uses ephemeral facts and returns one
  admitted paragraph without storing Player wording.
- Alternative Move Exploration and Coach Turns remain the path for proving a
  plan on the board.

## Client-carried state

- **Opaque state token.** Every model-visible tool result's `content` ends with one adapter-encoded token (session id, current `CoachTurnContext`, publication fence, packet digest). The model echoes it as a single `stateToken` argument; the adapter decodes it into the envelope. Copy-or-fail: the model cannot mutate individual fields into a `StaleContext`. No crypto needed — the Review Engine re-validates via signed snapshots and fence claims.
- **Bulk in the iframe.** `ImportedGame`, evidence packets, and board data live in widget runtime state, hydrated from `structuredContent`/`_meta`; app-only calls attach them directly.
- **Two writers, one session.** After every state-changing app-initiated call the widget emits `ui/update-model-context` with the fresh token (best-effort; hosts may defer/dedupe). On `StaleContext`/`StalePublicationFence` the result's `content` instructs the model to re-sync from the board. No server-side session cache — the adapter stays stateless between calls; the backend keeps only live task handles.

## Result channels

| Channel             | Carries                                                                                                                                                                                                                                      |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `content`           | Moment-scoped prose facts: import summary, position facts, projection/eval facts, validated verdicts and coach-turn text, rejection/conflict reasons with recovery hints, state token. Never raw PGN, evidence packets, or whole-game dumps. |
| `structuredContent` | Widget redraw data for this result (FEN, line slice, card payloads, arrows). Model-safe by construction — ChatGPT puts it in the transcript.                                                                                                 |
| `_meta`             | Bulk: `ImportedGame` (import results), evidence segments, engine lines. UI-only on both hosts.                                                                                                                                               |

Two hard rules:

1. **Moment-scoped `content`, not game-scoped.** The model gets facts for the moment under discussion; `inspect_position` is its pull path. Respects Claude's ~150K initial-result ceiling and keeps narration grounded in Review Engine-produced facts.
2. **Coaching cards render only from tool results.** The widget draws cards exclusively from `structuredContent` of successful Review Engine-validated results. Model chat text may restate a card; it never becomes one.

## PGN privacy

`import_game` accepts both sources for both callers (Player decision). The widget's inline paste form remains the **private route** — an app-initiated call whose PGN never transits model-visible context, satisfying the map's app-only paste path. PGN pasted into chat is already model-visible; accepting it in the model-visible tool is a convenience, not a leak. Pasted PGN never appears in the state token, `content`, or `structuredContent`.

If a transient Review Session expires, the adapter can start a fresh session from the Player's durable Game Import. Raw pasted PGN is discarded after normalization, while the player-owned canonical Imported Game remains subject to account deletion and retention policy. Client-carried facts or prior draft text cannot revive expired publication authority.

## Widget instances and rehydration

Neither host guarantees a durable widget: Claude supersedes instances per UI-linked tool call; ChatGPT creates one per message. Therefore **all model-visible session tools link to the single `ui://` workspace template, and every result is moment-scoped self-sufficient**: a fresh instance redraws the current moment (board, line, this turn's validated cards) from its own result. What a superseded instance loses is whole-game bulk; when the Player navigates beyond the moment, the widget re-imports via app-only call — silent for Lichess URLs, re-paste prompt for pasted PGN. Accepted prototype degradation.

## Mechanics

- **Naming**: snake_case verbs mirroring contract commands (table above); same vocabulary as the Coach Skill; no server prefix.
- **Resources**: one predeclared `ui://` workspace resource (`text/html;profile=mcp-app`), inlined assets, default restrictive CSP (widget speaks only via the postMessage bridge), no `ui.domain` for the prototype. Feature-detect UI capability; on non-UI hosts register the same tools without `_meta.ui` — `content` prose is the mandatory text fallback.
- **Progress**: relay backend progress as `notifications/progress` when a `progressToken` was sent, plus ChatGPT `invoking`/`invoked` strings. Best-effort hygiene; host display behavior is UNCONFIRMED until staging.
- **Cancellation**: MCP `notifications/cancelled` on an in-flight call → adapter issues `CancelOperation` using the operation id from the already-received `accepted` event and the fence from the original call's token — stateful only within one request lifetime. Widget cancel button calls `cancel_operation` directly. Silent abandonment tolerated; backend deadlines bound runaway work.
- **Idempotency**: publication fences make mutating calls retry-safe. A repeated Review Moment Comment fence returns the original canonical comment; a different fence after publication yields `StalePublicationFence`. Reads are naturally idempotent; repeated `import_game` is a deliberate fresh import.
- **Domain outcomes are results, not errors**: `unavailable`/`conflict`/`rejected` become normal tool results whose `content` states reason and recovery in actionable prose. MCP `isError` is reserved for transport/adapter faults.

## Deferred

- Empirical host behaviors (progress display, cancellation emission, Claude `structuredContent` visibility, partial tool-input streaming) → staging proof (#63).
- CoachApp admission pool sizing → #66.
- Workspace bundle implementation and interaction proofs → #68.
