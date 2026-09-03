# Pin gemini-3.5-flash-lite on Vertex global for the web Language Layer

> **Retired 2026-08-31.** The bake-off harness, its frozen task set, and its
> route, preflight, and probe files were removed in #534. Paths below that name
> `evaluation/bake-off/frozen-set.json`, `routes.json`, `preflight-*.json`,
> `probes-*.json`, `replicate-control-*.jsonl`, `marker-seam-smoke-*.jsonl`, or
> `takeaway-marker-slice-*.jsonl` no longer resolve. The recorded generations
> three tests still replay -- `pilot-2026-08-14.jsonl`,
> `full-run-2026-08-16.jsonl`, `challenger-2026-08-17.jsonl` -- survive, and
> `evaluation/bake-off/README.md` says what each is for. This document stays as
> the record of what was measured and why.

## Status

Accepted.

This decision selects the v1 pinned generation contract the posture of ADR 0009,
#294, #330 and #340 was built to constrain, and sets the four inference budget
tiers. It narrows ADR 0009 by fixing Structured Output Mode to `nativeSchema`
for the web path. It does not change the Grounding Gate, the Language Layer fact
boundary, the Evaluation Fingerprint axes of ADR 0049, or the endpoint posture —
the pin rides *inside* the pinned generation contract and takes no fingerprint
axis of its own, so any change recorded here mints a new Explainer Candidate.

## Context

Seven candidate routes were measured against the frozen task set of #345 under
the prompt digest of #357 and #360. The full run (#359) issued 354 generations
across six routes for \$0.7646; a follow-up sweep (this decision) issued 80 more
for \$0.1435 to settle the service tier and to test one late challenger. Every
measurement is recorded under `services/coach-engine/evaluation/bake-off/` and
narrated in `docs/prototypes/openrouter-bake-off-harness.md`.

Three findings framed the choice rather than being consumed by it.

**Inference cost is not the binding constraint.** Every usable route costs under
three cents per Review Session at `HARD_MAXIMUM = 10`, so the selection is a
quality-and-latency decision with a cost tiebreak.

**The gates are the scoring function.** #345 established that the model supplies
coaching language and not chess understanding, so the *authored rate* — the
share of generations the grounding gate of #347 admits — is the quality metric,
and human reading is reserved for the two jobs no gate can do.

**The service tier is selected by the endpoint tag, not by the provider family.**
#359 §7.1 proved a bare `google-vertex` resolves to `global`, so a pin that
declares only the family silently serves — and pays for — a tier it did not
choose.

## Decision

### The pin

| | |
| --- | --- |
| Model | `google/gemini-3.5-flash-lite-20260721` |
| `provider.only` | `google-vertex/global` (the **full endpoint tag**, not the family) |
| `allow_fallbacks` | `false` |
| `require_parameters` | `true` |
| Structured Output Mode | `nativeSchema` |
| Determinism controls | `seed: 0`; **no `temperature`** |
| `max_tokens` | **512** |
| Reasoning | not requested |

`temperature` is unsupported on this exact `(model, tag)` pair, and under
`require_parameters: true` sending an unsupported parameter is fatal rather than
ignored. `CriticalMomentGenerationRandomness::LowestSupported` therefore resolves
to seed alone here. This is a per-route fact read from the live catalogue, not a
model-family property.

`max_tokens: 512` is four times the largest note ever measured on this route
(128 tokens; p95 99; 99/99 generations finished `stop`, never truncated). It
bounds a runaway at \$0.00128 rather than \$0.005. **It is not model-neutral** —
see the challenger below.

### The measurements the pin rests on

At `google-vertex/global`, current prompt digest, 40 generations:

| | |
| --- | --: |
| Authored (grounding gate admitted) | **36 / 39 = 92 %** |
| Schema conformance | **40 / 40 = 100 %** |
| Pin Verification | **40 / 40 passed**, nothing substituted |
| p50 latency | **959 ms** |
| p50 across the 18 real session notes | **974 ms**, max 1 270 ms |
| Cost per Review Moment comment | **\$0.00075** |
| Cost per Review Session at `HARD_MAXIMUM = 10` | **\$0.0075** |
| Task B | B1, B2 authored; B3 emitted the typed `OutOfScope` refusal |

Buying the `global` tier costs no quality: the same model under the same digest
scored 85 % authored at `flex` and 92 % at `global`, at 6 976 ms against 959 ms.
The tier is a latency purchase for 0.37¢ per session at the hard cap.

This route remains, across #359 and this sweep, **the only candidate that has
ever emitted #233's typed `OutOfScope` refusal**. Structural containment only
contains if the candidate uses it, and this one does.

### The budgets

| Tier | Ceiling | What it admits |
| --- | --: | --- |
| Operation (one call and its single retry) | **\$0.005** | worst case is \$0.0045 — 3 203 prompt tokens and a truncating 512-token completion, twice |
| Review Session | **\$0.025** | 10 comments (\$0.0075) plus roughly 15 Coach Turns |
| Player | **\$0.50 / rolling 30 days** | 20 sessions at the session ceiling, or roughly 66 at real cost |
| Global | **\$25 / month** | with the kill switch |

Coach Turns take **no separate count cap**. They are the only Player-driven
spend in the design; bounding them by the Review Session ceiling keeps one
enforcement point, and a Player who talks more spends their own session budget
rather than meeting a second, differently-shaped rule. Exhaustion degrades per
#233 — comments to safe-rendered prose, Coach Turns to unavailable — and is
recorded as the `budget-refused` Capture Outcome of ADR 0049.

The ladder is deliberately tight against usage nobody has measured. Real session
cost is \$0.0075 against a \$0.025 ceiling; the tiers are raised from evidence
once the first cohort produces a usage distribution, which is easier than
explaining a bill.

### The Player-facing privacy claim

A Vertex winner permits **no training, and no retention beyond bounded abuse
monitoring** — not an unqualified zero-retention claim. #340 fixed this: the
route is admissible because Google's GCP ToS §4.3 resolves, through the abuse
page it references, to a stated 90-day flagged-traffic maximum, and the pin
record assumes that unfavorable branch. A genuine zero-retention claim was
available only on Bedrock and only through `claude-haiku-4.5`, which this
decision rejects on prose. The wording and placement of the claim are not
settled here.

## The rejected candidates

Preserved so a future selection reads the history rather than re-deriving it.
Every route below was ZDR-verified and native-structured-output capable on its
exact `(model, tag)` pair, on a counterparty verified from primary sources.

| Route | Measured | Why rejected | Re-entry condition |
| --- | --- | --- | --- |
| `google/gemini-3.7-flash` → `google-vertex/global` | 97 % authored, 100 % schema, **8 660 ms**, \$0.00283/gen | **Reasoning model.** 1 045 reasoning tokens per generation against the pin's 0 — 3.8× the cost and 9× the latency for +5 points of authored rate | A non-reasoning variant, or a session UX that can carry ~9 s per note |
| `anthropic/claude-haiku-4.5` → `amazon-bedrock/global` | 49 % authored, 100 % schema, 2 561 ms, \$0.00286/gen | Half of all notes would degrade to the deterministic safe rendering. The only route buying a genuine zero-retention claim, and it could not buy it with prose | A prompt or gate change that lifts its `missingRequiredMarker`/`repeatedMarker` rate; it is the fastest route in the set |
| `google/gemini-3.1-flash-lite` → `google-vertex/global/flex` | 90 % authored, 98 % schema, 16 197 ms at flex, \$0.00030/gen | 98.84 % uptime, an empty completion and a deadline exhaustion inside 59 generations, and it answered the out-of-scope turn rather than refusing it. Its 90 % against the pin's 85 % is 3 generations — inside the noise band at n = 59 | The pre-verified swap target: same counterparty, same terms, same schema, and it would need re-measuring at `global` |
| `qwen/qwen3-next-80b-a3b-instruct` → `google-vertex/global` | **8 %** authored, 93 % schema | Never once used the full marker set; 46 `missingRequiredMarker` in 59 | — |
| `openai/gpt-oss-120b` → `google-vertex/global` | **3 %** authored, 14 % schema | 17 deadline exhaustions and 17 empty completions in 59; ten times more reasoning than answer, so its nominal cost floor does not exist. Also **has no dated permaslug**, leaving #294's pinning conjunction one leg short | A dated permaslug, and a schema it can emit |
| `anthropic/claude-sonnet-5` → `amazon-bedrock/global` | **0 / 59** | The model, not our request: nine request variants, six identical requests giving three outcomes, and the identical prompt conforming on `claude-haiku-4.5` over the same counterparty. `strict: true` is advisory on Bedrock | Structured Output Mode is the one untested lever, and changing it mints a different Explainer Candidate |
| `openai/gpt-5.6-luna` → `azure` | never run | Struck by #340 before measurement: Azure's Foundry abuse-monitoring clause is operative but resolves to no published maximum, and an undisclosed window is an absent commitment rather than an unknown quantity. The set's only frontier-OpenAI candidate | Microsoft publishes a maximum duration for Foundry abuse-monitoring storage |
| `anthropic/claude-sonnet-5` → `azure` | never run | Struck by #330: Claude in Foundry is a Non-Microsoft Product, so Microsoft's no-training clause does not reach it | — |

**`google/gemini-3.7-flash` replaces `claude-sonnet-5` as the ceiling
reference**, and unlike its predecessor it is a measured one: 97 % authored is
the best prose anything in this map has produced, and it says the pin's 92 % is
five points short of the achievable ceiling rather than an unknown distance
from it.

**The field has no frontier-OpenAI member.** Azure left the admissible set
entirely, so no frontier-OpenAI quality character was measured at all. That is a
known gap in this comparison, not an omission.

## Consequences

**Sticker price is not cost for a reasoning model.** `gemini-3.7-flash` is 25 %
dearer per token and 3.8× dearer per note. Any future candidate must be priced
on measured tokens, never on the catalogue.

**`max_tokens` is coupled to the candidate.** 512 is generous for the pin and
truncating for any reasoning model — `gemini-3.7-flash` produced a 1 985-token
completion. Swapping the model requires re-reading this field, which is one more
reason it is declared configuration rather than a call-site constant.

**The service tier must be declared as a full endpoint tag.** A bare
`google-vertex` serves `global` while a budget computed from `flex` prices
under-counts by half. Note also that `routed_service_tier` reads `null` when the
`global` tag is declared — it reports a tier only when a non-default one is
served — so the declaration, not the response, is the record of what was asked
for.

**Model swap is configuration plus a re-run.** The route was added to
`evaluation/bake-off/routes.json`, preflighted, swept and Pin-Verified without a
code change; #237 must preserve that property in the shipping runtime.

**Two contract defects ship with the pin**, both recorded on #359 and neither
gate-catchable: two distinct markers can render the same string ("the Re1 is
Re1"), and Task B's noun-phrase renderings still meet a model-supplied article
("with an 0.2 pawns of the position"), which this sweep reproduced verbatim on
the pinned route.

**The re-verification obligation is unaffected in shape but changed in target.**
#340 made a counterparty's bounded published ceiling a pin-time precondition;
with a Vertex route pinned, the document to re-read is Google's abuse-logging
page reached through GCP ToS §4.3, not AWS §50.12.2's model list.
