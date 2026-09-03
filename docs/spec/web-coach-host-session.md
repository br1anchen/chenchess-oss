# Web Coach Host Session

## Status

Accepted and implemented on `main`. v1 scope.

This specification is the live implementation contract. ADR 0053 is the
architectural commitment. Children
#433–#436
are on `main`. #437
recorded the in-process HostTurn journey and rollout gate rows as readings,
and closed on 2026-08-28 with one bound HostTurn observed published on
staging. That reading was in the hosted rollout runbook, which this snapshot
does not carry.

Supersedes the two-mode composer (`discuss` | `coach`). #417 items 1–2 and
#426 stay as they are; #417 item 3 is superseded by #436.

## Purpose

ChatGPT and Claude are *hosts*: a model reads free text, picks tools, and
writes only from what the tools returned. The web app becomes a host too,
with the **pinned** model (ADR 0050) in the host seat — but a *better-guided*
one, because the web host is ours: tools are internal, the system prompt is
ours, and the session context is already on screen.

- **Internal capability channel**, not the MCP tools. The MCP seven are
  shaped by consumer-host limits (opaque handles per call, widget-mount
  tools, pacing vocabulary). The web session already holds every moment in
  memory and drives its board from session events.
- **Web system prompt**, not `coachMcpInstructions`. Pre-loads the on-screen
  evidence packet, Coaching Profile, the Grounding Gate rules as constraints,
  and the exact literal vocabulary — so most turns need zero capability calls.
- Attested identity, admission, and money in the product database, which
  consumer hosts never have.

## Decisions (binding)

| # | Decision |
| --- | --- |
| D1 | Behaviour vs presentation split with #427: this spec owns the thread state model, composer behaviour, command/event wiring; #427 owns layout and the presentational component, built against the types from #433, with `hostTurn` stories. #436 is **not** blocked by #427. |
| D2 | **Schema-routed loop**, not native tool calling. Every step uses `response_format: json_schema` with a union `{kind:"call", capability, args} \| {kind:"answer", …}`. No `tools` field; no provider-port change; no bake-off gate. Bake-off only measures routing accuracy. |
| D3 | Memory: last 4 turns as prose (message + answer). Prior capability results never re-enter; the active branch does, because it is on screen. |
| D4 | Retire only the web half of Coach Turn: `HostedCoachTurnAuthor`, `bind_hosted_coach_turn_author`, `HostedTask::CoachTurn`, client `startCoachTurn`. `StartCoachTurn` + `CoachTurnPreparation` stay for CoachApp/LocalCoach. Retired in #436, same change as the composer swap. The protocol-revision bump is documentation-only; the live fence is surface ownership (`StartHostTurn` web-only, `StartCoachTurn` not web-admissible). |
| D5 | Grounding rejection → **one corrective retry** (gate's rejection appended, answer step only, counts as a step), then unavailable whole. |
| D6 | Scope = this Review Session. General-chess questions get refusal `NotAboutThisReview` via the model's `refuse` step. Command validation rejects empty or oversize `StartHostTurn` text as `InvalidCommand`. Control characters other than newline, carriage return, and tab are Unavailable before admission and spend nothing. D6 refusals are the model's `refuse` step. |
| D7 | Five children, #433 → #434 → #435 → #436 → #437. |
| D8 | **v1.** Standalone parent, cross-linked to #427 / #417. |
| D9 | Per-step progress shown with fixed product-language labels; never capability names. Labels live in the #433 state model. |
| D10 | `MAX_STEPS = 3` counted model calls, turn deadline 15 s. Envelope = 4 × operation ceiling (3 counted steps + one per-turn transport retry) reserved up-front against the Review Session ceiling, and also denied on the Player rolling-30-day ceiling, the global calendar-month ceiling, or an open provider cooldown; leftover reservation is re-checked before each attempt; released at turn end. The 4 × hold is deliberately conservative: the Review Session ceiling stays 25_000, so a HostTurn starts only while committed spend is ≤ 5_000. A live session supports one worst-case HostTurn, or several typical turns whose billed total stays ≤ 5_000 before the next admit. While a HostTurn envelope is held, Comment and Coach Turn admit against committed + reserved + one operation worst-case, so they are denied once committed > 500 and Comment degrades to safe rendering. After release they again admit at committed + worst-case. |
| D11 | Four capabilities for v1; no `step_line`. |
| D12 | Grounding sentences single-sourced in one JSON file read by Rust (`include_str!`) and TS (import). No codegen, no golden-diff. |
| D13 | One fingerprint per turn over fixed axes; per-step observations beside it (ADR 0049 applied verbatim). |

## Architecture

Everything in Coach Engine, in-process, inside the Review Session actor.

```
web composer ──StartHostTurn──▶ ProcessorSession::host_turn
                                   │ envelope admission, pin, fingerprint
                                   ├─▶ OpenRouter (pinned, json_schema = step schema)
                                   │◀─ {call} | {answer}
                                   ├─ capability dispatch, in-process (typed evidence)
                                   ├─▶ OpenRouter … (≤ 3 steps incl. corrective retry)
                                   ├─ Grounding Gate over ∪ packets (native types)
                                   └─▶ HostTurnCompleted / HostTurnRefused / HostTurnUnavailable
```

## Capability channel

Pre-loaded (not fetched): open moment's full packet, active branch (if any),
Coaching Profile projection, last 4 turns' prose.

| Capability | Backs onto | When |
| --- | --- | --- |
| `read_moment({ply} \| {next, classification?})` | session moments + evidence packet | another moment |
| `list_moments()` | same | "which moments matter" |
| `evaluate_line({moves, opponentReplies})` | `review_session_exploration` (existing allowance + deadline) | "what if I had played …" |
| `learning_material()` | learning plan for the open moment | "how do I practise this" |

Output fields, not tools: `focusMoment?: ply`, `showLine?: {kind} \| {alternativeMoveId}` —
must reference something returned this turn or pre-loaded, else rejected.

When both fields are present, Central Host inspects `showLine` against the
send-time open ply — the Grounding Gate packet for that HostTurn — then
navigates to `focusMoment`. The named line is not shown on the focused
Review Moment; that would apply one ply's inspection to another.

## Step schema

```
{ kind: "call", capability: <enum>, args: <per-capability> }
| { kind: "answer", answer: string, citations: [callId], focusMoment?, showLine? }
| { kind: "refuse", reason: NotAboutThisReview | NotAboutChess | UnsafeRequest }
```

The engine renders refusal text; the model never writes it.

## Web system prompt (`web_host_prompt.rs`)

1. Role + Player: Elo, Coaching Profile projection.
2. Pre-loaded evidence: open moment packet, active branch assessment.
3. Grounding rules as constraints (from the shared JSON, D12): only literals
   in the vocabulary; SAN only; no URLs; no provider/tool/internal vocabulary;
   no plan/intent claims the packet doesn't carry; cite call ids.
4. Literal vocabulary: Chess Literal Projection of the pre-loaded packet,
   extended per capability result.
5. Capability guidance: when to call which; never reconstruct a line; one
   `evaluate_line` per proposed line; `showLine` only for a returned kind.
6. Refusal cases (D6).
7. Style: length, SAN, answer the question first.

## The contract: `HostTurn`

| Field | Definition |
| --- | --- |
| Input | `{ message, priorTurns: [{message, answer}] ≤ 4 }`; screen context is read from the session actor, not the client. HostTurn requires an open Review Moment on the live actor; after residency restore the Player must open a moment first |
| Output | `HostTurnCompleted { answer, focusMoment?, showLine? }`, `HostTurnRefused { reason }`, or `HostTurnUnavailable` |
| Evidence boundary | pre-loaded packet ∪ this turn's capability results |
| Identity | digest(prompt template) + digest({step schema, capability schemas}) as `responseSchemaDigest` + profile projection digest + pre-loaded evidence schema digest; Evaluation Contract Version stays `evaluation-fingerprint/v1` (axis set unchanged; see ADR 0053 item 4) |
| Grounding | literal projection over the union; raw UCI / URL / internal vocabulary rejected whole; D5 retry |
| Steps | ≤ 3 counted model calls (capability call, answer, D5 corrective retry). A transport-retryable fault does not consume a counted call |
| Envelope | 4 × operation ceiling reserved before step 1 against the Review Session ceiling (3 counted steps + one per-turn transport retry); also denied on the Player rolling-30-day ceiling, the global calendar-month ceiling, or an open provider cooldown; leftover reserved micros must still cover one worst-case attempt; released at end; denial before step 1 = unavailable, nothing spent. The hold is worst-case, not typical billed cost: a HostTurn starts only while committed + 20_000 ≤ 25_000. A mid-turn admission denial after billed steps settles one Operational Record as `Admitted` with `denial_reason` set so Player and global ceilings still accrue |
| Deadline | 15 s per turn, shared across steps; partial = unavailable |
| Retry | one transport-retryable retry per turn (the flag is not reset after a completed step); D5 for grounding |
| Cancellation | existing coach-turn cancellation, aborts between steps |
| Fallback | `Unavailable`, whole turn. A capability dispatch error is returned to the model as a result `{error}` and the turn continues so the model can answer or refuse |
| Provenance | one Operational Record per turn with `steps[]` (served model/provider, tokens, cost, closed capability) |
| Pin verification | on every step; `/generation` is telemetry (log, capture, ledger). A mismatch alerts and records both identities. It does not discard the turn. A transport fault with no generation id is unverified and may take the per-step retry |
| Progress | `HostTurn { label }` per step, labels from D9 |
| Idempotency | Replay of a settled HostTurn is keyed on the command idempotency key. The Coach Turn activity lease is HostTurn↔Coach Turn mutual exclusion for the same principal + game import until #436 |

## Children

1. **#433 — thread state model + `StartHostTurn` contract types.** TS thread/composer
   state (`hostTurn` progress, unavailable, refusal; D9 labels), Rust transport
   types for `StartHostTurn` / `HostTurnStep` / `HostTurnCompleted` /
   `HostTurnUnavailable`. No behaviour. The #427 seam.
2. **#434 — capability channel + step schema + web prompt.** `host_capabilities.rs`
   (enum + schemas + in-process dispatch + model projection),
   `web_host_prompt.rs`, shared grounding JSON (D12), goldens for every digest;
   bake-off leg measuring routing accuracy on ~20 canned questions.
3. **#435 — HostTurn runtime.** `HostedTask::HostTurn`, envelope admission, step
   loop, corrective-retry gate, fingerprint + capture + cost rows, command +
   events, cancellation, pin verification.
4. **#436 — web composer swap + Coach Turn web retirement.** Single composer,
   `focusMoment`/`showLine` via existing reducers, unavailable and refusal as
   messages, step labels; retire D4 items; protocol bump; supersedes #417 item 3.
5. **#437 — certification + rollout.** the deployment-certification host-turn leg
   (routes to `read_moment`, prose cites it, grounding-failure rejects whole,
   envelope denial spends nothing, corrective retry publishes); rollout doc
   gate rows; one observed bound HostTurn on staging.

## Out of scope

- Deterministic Review Moment Comments stay a fixed task.
- Re-pinning the model.
- Memory across Review Sessions.
- Presentation (#427), copy sweep (#417 items 1–2), feedback signals (#426).
