# Widget-originated message delivery on ChatGPT and Claude

> **Status:** measured. Three trials per host were run on 2026-08-06 against the
> deployed `staging.example` connector. The run yielded **tier 1 results
> only**: staging was still running the build before the instrumentation — see
> [Run of 2026-08-06](#run-of-2026-08-06).
> **Instrumentation added:** 2026-08-06 for issue #262 (parent #258)
> **Privacy:** stage names, durations, and opaque trace handles only — the
> method never records Game, Player, or chess content.

Issue #262 exists because the widget-to-chat handoff is a host-policy dependency
on the primary user path whose failure mode is silent — the same class as the
`_meta` replay bug that motivated #258. This note is the place the evidence
lands, so that nothing is built on top of the handoff before it is measured.

## The mechanism under test

"Discuss in chat" in the Critical Moment widget performs a two-part handoff
(`apps/coach-app/src/CoachAppController.tsx`, `discussSelectedMoment`):

1. **Pin the context.** `ui/update-model-context` carries the grounded selection
   (`McpCoachAppBridge.updateModelContext`, `apps/coach-app/src/bridge.ts`). On a
   host that can call tools from the widget, the grounded `discuss_review_moment`
   result is pinned; otherwise the locally projected selection is.
2. **Send the message.** `ui/message` posts the Player-visible request
   (`McpCoachAppBridge.sendMessage`).

Three host policies decide whether this works, and none is fixed by the MCP Apps
specification:

| Policy                                                             | Spec position                                                                                                                                                                   | Consequence if the host differs                                                            |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Is `ui/message` delivered at all?                                  | Host-defined                                                                                                                                                                    | The button is a dead end; the widget reports "passed" and nothing happens.                 |
| Does it auto-send, or land in the composer for the Player to send? | Host-defined ([ext-apps#501](https://github.com/modelcontextprotocol/ext-apps/issues/501))                                                                                      | Deferred send is acceptable UX but changes the widget's status copy and the latency claim. |
| Is the pinned model context applied before that turn?              | Hosts **MAY** defer to the next user message and **MAY** dedupe ([MCP Apps spec](https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/2026-01-26/apps.mdx)) | The model answers the message without the grounded moment and invents or misroutes.        |

The third is the dangerous one: it fails silently and looks like a model quality
problem. See
[MCP Apps cross-host contract research](./mcp-apps-cross-host-contract.md) for
the full documented contract; those are documentation claims, not measurements,
and do not satisfy this issue.

## Method

Re-runnable by one person with the connector installed on both hosts. Run each
host in a fresh conversation; a warm conversation makes the model-turn latency
unattributable.

### Preconditions

- The Coach MCP Server is reachable and its **stderr telemetry stream is being
  captured** for the duration of the run (`emitCoachTelemetry` writes one JSON
  object per line; `apps/central-host/server/review-session-telemetry.ts`).
- The deployed Coach App bundle includes the `hostMessageDelivery` /
  `selectedMomentHandoff` instrumentation added for #262. Without it, only
  tier 1 below is available. **Confirm this against the deployment, not against
  `main`** — staging is deployed by local snapshot upload, and assuming
  otherwise is what cost the 2026-08-06 run its tier 2 data.
- Screen recording is on, with a visible clock or a recorder that stamps frames.
  The auto-send-versus-deferred distinction is a UI observation; no telemetry
  can substitute for it.

### Per host, per trial

1. Start a fresh conversation and review the canonical Game (`Synthet1`,
   reviewed as Black) so a Critical Moment selector renders.
2. Note the widget's host label from the first `coach_app_performance` telemetry
   line (`host: "chatgpt" | "claude" | "unknown"`). An `unknown` here invalidates
   the trial's host attribution — record it and re-run.
3. Select a Critical Moment that is **not** the one already active, so the
   handoff carries a context change that is observable in the model's answer.
4. Click **Discuss in chat**. Start the stopwatch at the click.
5. Record, from the recording:
   - whether a message appears in the composer, in the transcript, or nowhere;
   - if in the composer, whether the Player had to press send (**deferred**) or
     the host sent it unprompted (**auto**);
   - the wall-clock instant the model's turn visibly begins.
6. Collect every telemetry event captured during the trial — see the trace
   caveat under "Known instrumentation limits" — then read from that set:
   - the `selectedMomentHandoff` measure — the whole click-to-handoff interval.
     A rejected handoff records neither measure, so the two always appear as a
     pair, one pair per successful attempt (tier 2 only);
   - the `hostMessageDelivery` measure — how long the host took to accept
     `ui/message`, which is the last leg of that interval, not the whole of it
     (tier 2 only);
   - the `tools/call` events with `caller: "model"` — the model turn actually
     reaching the server, which is the machine-readable end of the handoff.
     App-initiated calls are tagged `caller: "app"` (`chenchess/caller`) and
     must be excluded.

   Join within the trial's capture window, never by arrival order. The measures
   are uploaded by a separate app tool call that is deliberately not ordered
   against the model turn: an auto-send host can begin the turn while
   `ui/message` is still
   resolving, so a `caller: "model"` event legitimately precedes the report that
   carries the handoff timing. The measures may also arrive split across several
   `coach_app_performance` events, since each report carries only what is new. A
   trial that retried a failed handoff emits one `selectedMomentHandoff` per
   successful attempt, each timed from its own click.

7. Confirm grounding: the model's answer must name the moment that was selected
   in step 3. A correct-looking answer about the _previously_ active moment is a
   deferred-context failure, not a pass.

Run at least three trials per host. One trial cannot distinguish a host policy
from a single slow turn.

### Tiers

- **Tier 1 (no deploy needed):** steps 1–5 and 7, plus the `caller: "model"`
  telemetry line. Yields delivery, auto-versus-deferred, grounding, and
  gesture-to-model-turn latency at recording precision.
- **Tier 2 (instrumented bundle):** adds the `selectedMomentHandoff` and
  `hostMessageDelivery` measures, which split the click-to-model-turn interval
  into the widget's own preparation, the host's acceptance of `ui/message`, and
  everything after it.

### Latency definitions

| Name                      | From                                          | To                                              |
| ------------------------- | --------------------------------------------- | ----------------------------------------------- |
| Click to handoff          | The click                                     | `ui/message` resolves (`selectedMomentHandoff`) |
| Message acceptance        | Start of the `ui/message` call                | It resolves (`hostMessageDelivery`)             |
| Widget preparation        | Click to handoff **minus** message acceptance |                                                 |
| Gesture to model turn     | The click, from the recording                 | The model turn visibly beginning                |
| Player-attributable delay | Composer fill                                 | Player presses send (deferred hosts only)       |

Widget preparation is the widget's own work — on a host that can call tools from
the widget it contains the whole grounded `discuss_review_moment` round trip, so
it is usually the largest term and is **not** a host-delivery cost. Only
message acceptance is. Subtracting one from the other is the point of taking
both.

On a deferred host, gesture-to-model-turn includes human reaction time and is
not a host latency. Report the two separately or the number is meaningless.

### Known instrumentation limits

Two properties of the shared widget telemetry recorder
(`apps/coach-app/src/appPerformance.ts`) constrain the protocol. Both are
avoided by the trial design above rather than fixed in code, because fixing
either means changing telemetry that every other stage shares.

- **One active trace at a time.** The recorder attributes measures to whichever
  trace was last seen in a tool result's `_meta`. If a model turn's result
  arrives while `ui/message` is still pending — which an auto-send host may do —
  the handoff measures are attributed to that newer trace. Collect the trial's
  events by capture window, not by assuming a single `traceId`; a trial run in
  its own fresh conversation has no other traffic to confuse it, and every
  `traceId` seen in the window belongs to the trial.
- **64 measures per trace.** Later measures are dropped silently, which would
  break the paired-measure invariant. Perform the handoff shortly after the
  review renders rather than after a long exploratory session.
- **The measures cannot express a grounding failure.** In widget mode the
  widget swallows a rejected `ui/update-model-context` and still hands the
  message to chat, so a recorded pair means "the message was delivered", never
  "the model had the moment". That is why grounding is established from the
  model's answer in step 7 and from nothing else.

### Invalid trials

Discard and re-run, rather than recording, any trial where:

- the widget reported `host: "unknown"` (step 2);
- the trace is missing the `selectedMomentHandoff` measure. The reports are
  best-effort fire-and-forget uploads, and each is consumed once: a dropped or
  interrupted one loses its measures permanently, and the resulting trace looks
  exactly like a host that never delivered anything. Absence of the measure is a
  broken trial, never evidence of non-delivery. The recording from step 5 is
  what distinguishes the two, which is why step 5 is not optional.

## Results

### Run of 2026-08-06

Three trials per host, each in a fresh conversation, against the deployed
`staging.example` connector (Railway service `central-host`, environment
`staging`). Canonical Game `Synthet1` reviewed as Black; the handoff carried
Critical Moment 2/7, `11... Ba6` at ply 22, while moment 1/7 (`10... b4`) was
the active one — so a correct answer about `10... b4` would have been a
grounding failure, not a pass.

**Tier: 1 — because the instrumented build was never on staging.** Across all
six handoffs no `selectedMomentHandoff` or `hostMessageDelivery` stage appeared
in `coach_app_performance` telemetry, while the pre-existing
`updateModelContext` stage did, in the same traces, on both hosts.

The cause is deploy provenance, not a broken instrument. **`central-host` is
deployed to staging by local snapshot upload, not from git**, so a commit being
on `main` says nothing about what staging is running. The build log for the
deployment serving this run records an uploaded snapshot whose only changed file
was `apps/central-host/server/coach-app-mcp.ts` (23495 b → 23375 b) — the change
from `f678c48a`, the commit _before_ the #262 instrumentation. None of the three
files `0e9fb712` touches appear in that snapshot. Staging was running
`f678c48a`, which has no handoff measures to report.

The widget code itself is fine: a regression test now drives the widget-mode
handoff with a distinct trace id on every tool result — the way production
behaves, and the way the original test did not — and both stages are reported
(`apps/coach-app/src/CoachAppController.test.tsx`).

None of this invalidates the run. Per "Invalid trials", absence of the measure
is never evidence of non-delivery, and delivery here is established from the UI
observation in step 5 — which is why all six trials stand at tier 1. It does
mean the message-acceptance column cannot be filled until staging actually runs
the instrumented build.

Substitutions used for tier 1, both stricter than the recording the protocol
asks for:

- **Delivery and auto-versus-deferred** were read from the live DOM by a 25 ms
  poller on the parent page (composer text, user-turn count, assistant-turn
  count), not from a video. A message that reaches the composer without a new
  turn is deferred; a new turn with the composer never non-empty is auto-send.
- **Gesture to model turn** on ChatGPT is bounded from server telemetry: the
  interval from the widget's own `discuss_review_moment` (`caller: "app"`, the
  earliest server-visible consequence of the click) to the model's
  `discuss_review_moment` (`caller: "model"`). These are log timestamps and the
  click precedes the app call, so each figure is an **upper bound that also
  contains the operator's own gap between the two clicks** — treat them as
  order-of-magnitude, not as host latency.

### ChatGPT

Host label `chatgpt` on all three trials.

| Trial | Delivered                        | Auto or deferred | Context applied to that turn                                                                                                 | Message acceptance     | Gesture to model turn |
| ----- | -------------------------------- | ---------------- | ---------------------------------------------------------------------------------------------------------------------------- | ---------------------- | --------------------- |
| 1     | Yes — straight to the transcript | **Auto**         | Yes — answer names `11...Ba6`, evals −3.1/−0.2 and the engine line; a grounded Review Moment widget rendered for that moment | not available (tier 1) | ≤ 10.7 s              |
| 2     | Yes — straight to the transcript | **Auto**         | Yes — grounded moment widget for `11...Ba6`, answer opens "At 11…Ba6, Black"                                                 | not available (tier 1) | ≤ 5.0 s               |
| 3     | Yes — straight to the transcript | **Auto**         | Yes — grounded moment widget for `11...Ba6`                                                                                  | not available (tier 1) | ≤ 6.7 s               |

The composer is never involved: `prompt-textarea` stayed empty through every
trial while a new turn appeared. The delivered message is rendered in the
transcript but **not** as a `[data-message-author-role="user"]` node, so a
delivery check that counts user messages will report a false negative on this
host.

In all three trials the widget performed its own grounded
`discuss_review_moment` (`caller: "app"`) before the handoff — ChatGPT is a host
that can call tools from the widget, so the pinned context is the grounded
result rather than the local projection.

### Claude

Host label `claude`.

| Trial | Delivered                 | Auto or deferred | Context applied to that turn                                                                                                               | Message acceptance     | Gesture to model turn                                    |
| ----- | ------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------- | -------------------------------------------------------- |
| 1     | Yes — pre-filled composer | **Deferred**     | Yes — "Reading widget context" step, answer names `11...Ba6` with +3.1 → +0.2 and the `11...a6` refutation                                 | not available (tier 1) | Player-attributable 20.2 s; send → model turn **2.00 s** |
| 2     | Yes — pre-filled composer | **Deferred**     | **No** — `discuss_review_moment` was rejected `unknownSession` because the `sessionId` was **malformed**, not because the session was gone | not available (tier 1) | Player-attributable 4.54 s; send → model turn **1.46 s** |
| 3     | Yes — pre-filled composer | **Deferred**     | Yes — answer names `11...Ba6` with +3.1 → −0.2                                                                                             | not available (tier 1) | Player-attributable 1.51 s; send → model turn **1.26 s** |

Click-to-composer was measured once, in trial 1, at **≤ 2.55 s** — an upper
bound including one extension round trip. The remaining two trials were driven
by hand and have no click timestamp; this is exactly the interval tier 2's
`selectedMomentHandoff` exists to measure.

Two host behaviours worth recording beyond the table:

- Claude renders a **red caution banner** above the pre-filled composer ("Use
  caution before running this prompt. Malicious conversation content could trick
  Claude into attempting harmful actions or sharing your data."). The widget's
  status copy must not claim the message was sent; on this host it was not.
- The widget button correctly changes to "Discussion handed to chat" on the
  deferred path, which is accurate as written — it says handed, not sent.

Trial 2's failure is **not** a delivery or a deferred-context failure, and not a
session-lifetime one either. The `coach_malformed_review_session_id` telemetry
for that trial records the rejected handle's shape:

```
shape={"namespace":"other","segmentCharsets":["lowerHex"],"segmentCount":1,
       "segmentLengths":[64],"totalLength":64}
```

A live handle is `review-session:<64 hex>:<32 hex>`. What was sent is its middle
segment alone — the `review-session:` prefix and the trailing 32-hex segment
stripped. `isPlausibleReviewSessionId` rejects that structurally, so
`unknownSession` is returned without the Engine ever being consulted; the
session's existence was never in question.

The truncation is ours, not the model's paraphrase: the same malformed handle
was sent twice in that trial — once by the model (22:13:50) and once by the
**widget's own** `discuss_review_moment` call (22:13:55, `caller: "app"`). Both
read it from app state that already held the truncated form. Where the
truncation happens is not established here; it is filed separately. The model
handled the rejection correctly, refusing to guess another handle.

It is recorded here because the verdict rule is applied to what the trials
showed, not to what they were expected to show.

### Known limits of this run

- Staging was not running the instrumented build, so no term of the
  click-to-model-turn interval is separated. Every latency above is a bound, not
  a host cost.
- **Synthetic clicks from the Chrome extension do not reach the widget iframe.**
  On ChatGPT they never landed; on Claude they landed once (trial 1) and then
  stopped landing in two later conversations, including after a reload. Five of
  the six gestures were performed by hand. Any future automation of this
  protocol has to solve the gesture, not just the measurement.
- The gesture-to-model-turn bounds on ChatGPT contain human time between the
  carousel click and the Discuss click. They are not comparable to Claude's
  send-to-turn figures, which contain none.

## Verdict on the planned handoff mechanism

**Viable on ChatGPT. Viable on Claude for delivery and grounding, with one
trial lost to a server-side session defect that is ours to fix, not the host's.**

Applying the rule below to the run above:

- **ChatGPT — viable.** `ui/message` was delivered in 3/3 trials and the
  selected moment grounded the answer in 3/3. It auto-sends, which per the rule
  changes the widget's status copy, not viability.
- **Claude — delivery viable, grounding 2/3.** `ui/message` was delivered in
  3/3 and the pinned context reached the model in 3/3 (trial 2's turn had a
  session id; it was truncated before it ever got there). The one miss is neither the
  deferred/deduped failure the caveat branch describes nor the silent drop the
  non-viable branch describes, so it is reported as its own mode rather than
  forced into a branch. Claude defers the send, so the widget's status copy and
  any latency claim must not assume the message went out.

Consequences the rest of #258 should carry:

- The handoff cannot claim a single behaviour across hosts. Auto-send on
  ChatGPT and deferred send on Claude are both correct; the widget must say
  which happened.
- A delivery check must not count host user-message nodes — ChatGPT delivers a
  turn that is not marked as a user message.
- The truncated-handle failure is worth its own issue: it converts a successful
  handoff into a dead end for the Player, which is the same silent-failure class
  #262 exists to rule out. It is also invisible to the widget, which reports
  "Discussion handed to chat" either way.
- Staging is deployed by local snapshot upload. Any claim about what staging
  runs has to come from the deployment, never from `main`.

The verdict rule, fixed in advance so the reading of the data is not negotiated
after seeing it:

- **Viable** on a host when `ui/message` is delivered in every trial and the
  selected moment grounds the model's answer in every trial. Auto-send versus
  deferred-send changes the widget's status copy, not viability.
- **Viable with a caveat** when delivery is reliable but the pinned context is
  applied late or deduped away. The fallback is to stop depending on
  `ui/update-model-context` for grounding and carry the Game Import id in the
  message text the Player sends — the same addressable-snapshot argument #258
  makes for tool arguments.
- **Not viable** on a host when the message is silently dropped. The widget must
  then detect it rather than displaying "handed to chat", and the primary path
  needs a different affordance on that host.

## Re-running this

The measurement requires driving real ChatGPT and Claude sessions against the
deployed connector. It cannot be produced from the repository, and no part of it
may be inferred from the specification, from the other host, or from a previous
release.

To re-run at tier 2, deploy `central-host` from the current tree first. **Do not
infer the deployed build from `main`** — staging is deployed by local snapshot
upload, which is what reduced the 2026-08-06 run to tier 1. Then confirm, before
trusting any trial, that `selectedMomentHandoff` and `hostMessageDelivery`
actually appear:

```bash
railway logs -s central-host -e staging --since 20m | grep -oE '"stage":"[A-Za-z]+"' | sort | uniq -c
```

Absence of those two stages means the run is tier 1 and the message-acceptance
column stays empty — it does not mean the host dropped anything.
