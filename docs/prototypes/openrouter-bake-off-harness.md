# The OpenRouter bake-off harness, and what building it found

> **Retired 2026-08-31.** The bake-off harness, its frozen task set, and its
> route, preflight, and probe files were removed in #534. Paths below that name
> `evaluation/bake-off/frozen-set.json`, `routes.json`, `preflight-*.json`,
> `probes-*.json`, `replicate-control-*.jsonl`, `marker-seam-smoke-*.jsonl`, or
> `takeaway-marker-slice-*.jsonl` no longer resolve. The recorded generations
> three tests still replay -- `pilot-2026-08-14.jsonl`,
> `full-run-2026-08-16.jsonl`, `challenger-2026-08-17.jsonl` -- survive, and
> `evaluation/bake-off/README.md` says what each is for. This document stays as
> the record of what was measured and why.

Artifact for [Build the OpenRouter bake-off harness and run the candidate measurements](#346)
and [Wire Task B and run the full bake-off measurements](#359),
children of [Ship the tailored OpenRouter web Language Layer to beta](#229).

Code references are to the working tree at the time of writing. Catalogue reads in §1–§5 are dated
**2026-08-14**; §7 onwards is #359's full run,
dated **2026-08-16**. Both must be re-read at pin time.

> **§5.5 is superseded by [§7.1](#71-the-endpoint-tag-is-honoured-and-the-bare-family-is-not-what-346-thought-it-was).**
> The served Vertex service tier *is* observable after the fact, and the bare provider family does
> not resolve to the tier the pilot declared. Read the two together.

---

## 1. The precondition gate passed, with one price correction

#346 makes verification a gate rather than a
formality: numbers taken against a route that has moved are not numbers about the pinned contract. So
the harness reads `/api/v1/endpoints/zdr` and checks every candidate on the **exact `(model, tag)`
pair** before it will spend anything, and `preflight` exits non-zero if any route fails.

Run 2026-08-14 against 752 live ZDR endpoints: **all seven routes are still ZDR and still
`structured_outputs` + `response_format` capable on their exact pair.** Six prices are unchanged.

One is not:

| Route | #236 recorded | Live 2026-08-14 |
| --- | --- | --- |
| `google/gemini-3.6-flash` → `google-vertex/global/flex` | 0.750 / 3.750 | **0.375 / 1.875** |

#236's table recorded the `google-vertex/global`
price against the `google-vertex/global/flex` tag — the same tier confusion its own constraint 1
warns about, made once in its own table. The contingent candidate is **half the price it was
budgeted at**, which changes when it is worth promoting from contingent.

`openai/gpt-oss-120b` → `google-vertex/global` also carries visibly the worst availability in the set
(93.9 % over 24 h against 97.9–99.9 % for the rest). Not disqualifying — it is the cost floor, and
`allow_fallbacks: false` turns unavailability into the existing safe-fallback path rather than a
wrong answer — but a cost floor that is down five times more often than the incumbent is not cheap in
the way the price table suggests.

### 1.1 The cost floor cannot be pinned to a dated revision

#294's pinning conjunction opens with a **dated
permaslug**. Reading `canonical_slug` for all seven candidates:

| Slug | `canonical_slug` |
| --- | --- |
| `anthropic/claude-haiku-4.5` | `anthropic/claude-4.5-haiku-20251001` |
| `anthropic/claude-sonnet-5` | `anthropic/claude-sonnet-5-20260630` |
| `google/gemini-3.1-flash-lite` | `google/gemini-3.1-flash-lite-20260507` |
| `google/gemini-3.5-flash-lite` | `google/gemini-3.5-flash-lite-20260721` |
| `google/gemini-3.6-flash` | `google/gemini-3.6-flash-20260721` |
| `qwen/qwen3-next-80b-a3b-instruct` | `qwen/qwen3-next-80b-a3b-instruct-2509` |
| **`openai/gpt-oss-120b`** | **`openai/gpt-oss-120b`** — undated |

Six of seven have a dated revision. The cost floor does not: its canonical slug *is* its slug. The
other three legs of the conjunction still hold — provider family, `allow_fallbacks: false`, no
`models` array — so the *route* is pinnable, but the **model identity is not version-frozen**, and a
silent weight change under an unchanged slug would not mint a new Explainer Candidate. That is a
genuine asymmetry between the cost floor and every other candidate, and it belongs in
#236's selection alongside price.

### 1.2 The account's privacy posture is not readable, so #294's boot assertion needs rewriting

#294 requires the runtime to **assert the live
OpenRouter account settings against the declaration at boot and refuse to serve on divergence**,
precisely because `openrouter_metadata` reveals the served route but not the account's privacy
posture.

Walking the live OpenAPI spec: **no endpoint returns those settings.** `/api/v1/key` returns limits
and usage; `/api/v1/keys`, `/api/v1/workspaces` and `/api/v1/organization/members` return
administrative shape; there is no privacy, ZDR, or provider-preference read anywhere in the 73
documented paths. The one thing that reflects the posture is `/api/v1/models/user`, documented as the
catalogue "filtered by user provider preferences, **privacy settings**, and guardrails" — which makes
the posture observable only *differentially*, as a catalogue narrower than the public one, and gives
no way to tell **which** setting produced the narrowing.

So the assertion #294 asks for cannot be built as written. Three things are available instead, and
#237 has to choose among them:

1. **Differential boot probe** — compare `/models/user` against `/models` and refuse to serve if the
   catalogue is not narrowed. Weak: it proves *something* is filtering, not that ZDR is on.
2. **Per-request fail-closed** — `provider.zdr: true` plus `data_collection: "deny"` plus
   `allow_fallbacks: false` makes an account whose posture contradicts the declaration produce **no
   admissible endpoint**, which OpenRouter returns as an error rather than as a laxer route. This is
   the strong one, and it is per-request rather than per-boot.
3. **Declared configuration with a startup canary** — one throwaway generation at boot, verified
   through Pin Verification.

The harness records the differential probe in its preflight and relies on (2) for every measurement.
The point for #237 is that **(2) is the real mechanism** and the boot-time reading #294 imagined does
not exist.

---

## 2. What had to be built, and the two things that were missing

| Need | State before | Now |
| --- | --- | --- |
| An HTTP client for any model provider | **None anywhere in the workspace** | `services/coach-engine/src/bin/language_layer_bake_off.rs` |
| Compiled prompt from a moment's facts | Written down in #344, never compiled | `language_layer_prompt.rs` |
| Addressing **one recorded moment** offline | Corpus records whole games and replays them to `MomentFact` | `pipeline_evaluation::recorded_comment_case` |
| Running the real gate over model output | `ground_draft` existed | used directly, not reimplemented |
| A rejection taxonomy the bake-off can read | Collapsed into two wire values | `CommentProseRejection`, narrowing at the boundary |

### 2.1 The prompt and the gate now share one derivation

The compiled prompt reads its marker vocabulary out of `CommentFactsPolicy` and its allowlist out of
`chess_literal_grounding_for` — the same values the gate enforces. Building either twice would let
the model be *offered* a literal the gate then rejects, which would show up in the measurements as
model unreliability when it was ours.

### 2.2 The facts projection is deliberate, and one field name was a trap

`GameReviewCriticalMoment` is not serialized wholesale into `FACTS`. It carries ids, a
decision-explanation proof, and a display block that exist for other consumers — and, load-bearing:

```
GameReviewHumanComparison::played_move_is_human_likely  →  "playedMoveIsHumanLikely"
```

#347 made `human-likely` a **rejection** in
authored prose. Handing the model that key in its input and then discarding the comment for echoing
it is a trap rather than a contract. The projection renames the block to `playersAtThisRating` with
`playedMoveIsACommonChoice`, and a test asserts that no compiled prompt for any corpus case contains
`human model`, `human-likely`, `human likely`, or `move model` outside the system prompt's
instruction not to say them.

Also dropped: every `*Uci` key. UCI is banned in output and is already grounded through its squares,
so showing it can only tempt.

### 2.3 The rejection taxonomy, built by narrowing rather than by widening

The map recorded this as fog: `MarkerViolation` distinguishes unknown, repeated, missing and
bare-figure violations, but the comment path collapses three of four into `ChangedFact`, so the
per-candidate reliability metric #346 wants is
destroyed on the way out.

The fix is not a wider wire enum. The gate now resolves at **full width internally** and narrows at
its boundary:

```rust
pub fn diagnose_hosted_comment_text(..) -> Result<GroundedCommentDraft, CommentProseRejection>
pub(crate) fn ground_hosted_comment_text(..) { diagnose(..).map_err(CommentProseRejection::into_wire) }
```

One code path, two resolutions. `CriticalMomentGroundingRejection` is unchanged, so no wire contract
moves and no Player-facing behaviour changes — the full domain suite passes untouched. Seventeen
variants are now distinguishable, including the four marker kinds, `UngroundedChessLiteral`,
`InternalVocabulary`, `ForbiddenNeutralLiteral`, `ClaimOutsideFacts`, and the intent family. This is
the metric that costs no human judgement and is exactly where cheap models are expected to separate.

### 2.4 What the offline replay does *not* reproduce

`recorded_comment_case` replays a frozen case using only its recorded single-PV evidence, which is
why it works with the Local Pipeline Runtime down. It does **not** replay the Decision Explanation,
because that comes from the MultiPV comparison rather than the single-PV recording — so the
projected learning tracks are absent and `LEARNING_MATERIAL` falls back to the opening projection
alone.

Consequence for the pilot: **learning-resource grounding is under-measured.** For the four reachable
cases `LEARNING_MATERIAL` renders `(none for this moment)`, so the exact-literal rule in
`learning_grounding::validate` is never exercised against a real resource line. That is a gap in the
pilot, not in the harness; a case whose MultiPV evidence is recorded will exercise it.

---

## 3. The request, in full

Every call carries the pinned generation contract rather than an approximation of it:

```json
{
  "model": "<dated permaslug>",
  "messages": [{ "role": "system", ... }, { "role": "user", ... }],
  "max_tokens": 700,
  "temperature": 0,
  "seed": 0,
  "response_format": {
    "type": "json_schema",
    "json_schema": { "name": "review_moment_comment", "strict": true, "schema": { ... } }
  },
  "provider": {
    "only": ["<bare provider family>"],
    "allow_fallbacks": false,
    "require_parameters": true,
    "data_collection": "deny",
    "zdr": true
  }
}
```

No `models` array. One **30 s task deadline covering the single retry**, per
#233 — implemented as a deadline instant, so a
first attempt that burns 28 s leaves the retry 2 s rather than a fresh 30.

`max_tokens: 700` is set for the longest moment, so it **bounds rather than targets**, per
#344. Per-moment output cost is a distribution
and the records carry the per-attempt token counts to characterise it.

`provider.only` carries the **bare provider family**, as #294
declares. It is a per-route field in `routes.json` precisely so the first live run can put a full
endpoint tag in it and settle #231's open
question 3 empirically — the only way to pin a Vertex service tier, and therefore a precondition of
#236's budgets being determinate.

### 3.1 The measurement record

One JSON object per **attempt**, not per case, carrying: the route and the fingerprint axes
(#234's declared-configuration set, including
the prompt and schema digests, computable at boot); the attempt outcome, wall-clock latency, HTTP
status and generation id; the **served** model, provider, region and routing strategy from
`openrouter_metadata`; the **Pin Verification** verdict; `usage.cost` with prompt, completion and
reasoning tokens; structured-output conformance; the model's own outcome (`comment`, `outOfScope:…`,
`unparsable`, `schemaViolation`); the **authored** text; the **published** text after substitution;
and the grounding outcome with the full-width rejection beside its narrowed wire form.

Failed and rejected generations are recorded, per
#234 — a candidate that fails cheaply is not
thereby cheap, and the cost of a discarded attempt is still cost.

### 3.2 Spend is bounded before it is incurred

`run --dry-run` prices a **ceiling** rather than an estimate: every generation retried, every output
at the cap, input at a fixed generous size. `--budget-ceiling-usd` refuses to start above it. The
running total is printed per generation from the recorded `usage.cost`.

---

## 4. What can be measured today, and what cannot

| Set | Cases | State |
| --- | --- | --- |
| Grounding G1–G4 (reused corpus) | `tactical-white-human-likely`, `advanced-both-threshold`, `selected-nonautomatic`, `selected-terminal-mate` | **Reachable now** — recorded evidence replays offline |
| Grounding G5–G15 (GothamChess exemplars) | 11 cases, 305 plies | **Blocked** — needs Maia |
| Grounding G16–G19 (constructed Neutral) | 4 cases, ≈8 plies | **Blocked** — needs Maia |
| Task B (B1–B3) | on `Synthet1` | Evidence exists; the Coach Turn seam is not yet wired into the harness |
| Sessions S1, S3 | ≈111 plies | **Blocked** — needs Maia |
| Session S2 | `Synthet1` | Recorded |

The Docker daemon is **still unresponsive** on this host, exactly as
#345 §7 recorded on 2026-08-13: `docker version`
hangs past 40 s with Docker Desktop's processes running. So 15 of the 19 grounding cases and 2 of the
3 sessions cannot be recorded, and that is a scheduling precondition rather than a decision.

`positional-black-intermediate` also replays cleanly and supplies a fifth reachable moment (a
duplicate `Improvement / centipawns / nopop` shape); `beginner-below-threshold` correctly yields no
Critical Moment at all.

---

## 5. The pilot: 49 generations across six routes, 2026-08-14

Grounding cases G1–G4 — the four that replay without the Local Pipeline Runtime — against all six
enabled routes, cold-start and populated, one replicate. **Measured spend $0.092.** Records:
`services/coach-engine/evaluation/bake-off/pilot-2026-08-14.jsonl`.

| Route | n | Authored | Schema conformed | Pin | p50 ms | p95 ms | $/gen | out tok | reasoning tok |
| --- | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| `gemini-3.5-flash-lite` → vertex/flex | 8 | **8** | **8** | 8 | **1003** | 1414 | 0.00066 | 62 | 0 |
| `claude-haiku-4.5` → bedrock | 8 | 6 | 8 | 8 | 2766 | 3946 | 0.00247 | 98 | 0 |
| `gemini-3.1-flash-lite` → vertex/flex | 8 | 4 | 8 | 8 | 1185 | 1646 | 0.00053 | 73 | 0 |
| `qwen3-next-80b` → vertex | 8 | **0** | 8 | 8 | 4939 | 7727 | 0.00031 | 77 | 0 |
| `gpt-oss-120b` → vertex | 9 | 2 | 2 | 8 | 6043 | **30001** | 0.00036 | 635 | **634** |
| `claude-sonnet-5` → bedrock | 8 | **0** | **0** | 8 | 4895 | 14749 | 0.00717 | 293 | 115 |

**Pin Verification passed 48 of 48**, and 16 of 16 on the control run. No route substituted.

Rejections, at the resolution §2.3 bought:

| Route | Why it failed |
| --- | --- |
| `qwen3-next-80b` | `missingRequiredMarker` ×7, `repeatedMarker` ×1 — **never once used the full marker set** |
| `gpt-oss-120b` | `schemaViolation` ×4, `unparsable` ×1, `emptyCompletion` ×1, `deadlineExhausted` ×1 |
| `claude-sonnet-5` | `schemaViolation` ×8 |
| `gemini-3.1-flash-lite` | `missingRequiredMarker` ×2, `repeatedMarker` ×2 |
| `claude-haiku-4.5` | `missingRequiredMarker` ×1, `unknownMarker` ×1 |

### 5.1 The ceiling reference fails structured output outright

`claude-sonnet-5` → `amazon-bedrock/global` returned a **bare `{}`** on all eight real prompts,
`finish_reason: stop`, 293 completion tokens billed. On a trivial prompt the same route returns
plausible JSON — but with the renamed property `text` and no `refusalReason`, i.e. **not conforming
either**. So Bedrock treats `strict: true` as advisory: `required` is not enforced, property names are
not enforced.

That is measured, not inferred, and it is the most consequential result in the pilot: the **most
expensive candidate is currently the least usable**, and the two routes carrying the genuine
zero-retention claim are the two that enforce the schema least. The harness records
`nonconformingRecovered` separately from `nonconformingUnrecoverable` so a renamed property is not
counted as a quality failure.

### 5.2 The cost floor writes good prose and the wrong shape

`gpt-oss-120b` returns a bare JSON **string** rather than the object — its content is
`"Your move {playedMove} captured the pawn on e5, which is {playedPopularity}; …"`, which is
creditable marker prose wrapped in the wrong envelope. It also burns **634 reasoning tokens per
generation on average against 62 for the winner**, blew the 30 s task deadline once, and is the only
route to do so. Its nominal cost floor does not survive its reasoning: 635 output tokens at 0.360 is
not obviously cheaper than 62 at 1.250.

### 5.3 The Coaching Profile Projection is not inert

The map recorded as fog whether the projection does anything at all, and said the bake-off would
settle it for free by diffing cold-start against populated. It does, and it needed a control to be
honest about it:

- **Cold-start versus populated: 17 of 17 authored outputs differ.**
- **Same prompt, two replicates: 8 of 8 byte-identical** (`replicate-control-2026-08-14.jsonl`,
  `gemini-3.5-flash-lite` and `claude-haiku-4.5`, $0.024).

Generation is deterministic under this contract on both routes, so the difference is attributable to
the projection rather than to sampling. **The diff does not come back flat**, so the concentration
finding in #345 §6 does not become a ticket on
that trigger. What is *not* settled is whether the difference is an improvement — that is the human
read, and it belongs to #236.

### 5.4 The human read found a contract defect, not a candidate difference

Reading the published comments turned up something no gate can see and every route exhibits:

> You played e4, and **After e4, the evaluation is +0.5 — Slightly better for White.** because it was
> sound without a concrete achievement.

> …but **Before committing here, calculate Nxd4 first.** Before you take on e5, your knight can grab
> the knight on d4 instead…

**Several canonical renderings are whole sentences**, capitalised and terminally punctuated —
`{decisionCue}` → "Before committing here, calculate Nxd4 first.", `{observation}` and the Neutral
`{playedEval}` likewise — while #344's contract
tells the model to "write the sentence around them as if the value were already there". A model that
follows the instruction correctly produces broken grammar, because the slot is a clause and the
filler is a sentence.

Every route is affected, so this discriminates between **no** candidates: it is a defect in the
marker vocabulary, and it would ship ungrammatical prose to Players on the routes that otherwise pass
every gate. `{betterMove}` → `Nxd4` and `{bestEval}` → `+1.3, Slightly better for White` read
correctly, which is what makes the sentence-shaped ones stand out. The fix belongs to
#344's vocabulary — clause-shaped renderings, or
a marker that declares its own shape — and changing it mints a new prompt digest.

This is the second job #345 §4 named for the
human sample, arriving one level earlier than expected: not a claim contradicting its marker, but a
*sentence* contradicting its slot.

### 5.5 Endpoint identity is observable; the service tier is not

Every generation's `/generation` record carries a stable `endpoint_id` — one distinct id per route,
constant across all runs. The catalogue does **not** expose endpoint ids, so the id cannot be
resolved to a tag from public data; but because it is stable, one probe per tag would build the
mapping empirically, and after that the served tier is verifiable.

`data_region` is **`"global"` for all six routes**, including the two `google-vertex/global/flex`
ones, so it does not distinguish the service tier. And every generation reports `streamed: true` —
OpenRouter streams upstream whether or not the caller asked, which matters to
#294's cancellation reasoning.

### 5.6 The pinned contract cannot request determinism uniformly

`require_parameters: true` is mandatory under #294,
and it turns an unsupported parameter into **HTTP 404, "No endpoints found"** rather than a silent
ignore. Reading the live catalogue for the exact `(model, tag)` pairs:

| Route | `temperature` | `seed` |
| --- | --- | --- |
| `gpt-oss-120b` → vertex | yes | yes |
| `gemini-3.1-flash-lite` → vertex/flex | yes | yes |
| `qwen3-next-80b` → vertex | yes | yes |
| `gemini-3.5-flash-lite` → vertex/flex | **no** | yes |
| `claude-haiku-4.5` → bedrock | yes | **no** |
| `claude-sonnet-5` → bedrock | **no** | **no** |
| `gemini-3.6-flash` → vertex/flex | **no** | yes |

Sending `temperature: 0, seed: 0` unconditionally — the obvious reading of a pinned generation
contract — **deletes four of seven candidates from the bake-off before they are measured**, including
both incumbents. `CriticalMomentGenerationRandomness::LowestSupported` already anticipates exactly
this, and the harness resolves the controls per route from the live catalogue and records what was
sent as a fingerprint axis. Two consequences: the generation contract legitimately **varies per
candidate**, so it must be an axis rather than a constant; and determinism is nonetheless observed
even where neither control is accepted (§5.3's control run covers `claude-haiku-4.5`, which takes no
seed).

---

## 6. Handed forward

- **The sentence-shaped marker renderings** (§5.4) are a defect in
  #344's vocabulary that affects every route and
  would ship broken grammar. It blocks nothing in the harness and everything in the prose, and fixing
  it mints a new prompt digest — so it should be fixed *before* the full run, not after.
- **`claude-sonnet-5` → bedrock is currently unusable** (§5.1) at 0/8 conformance and the highest
  price. Whether that is the schema, the prompt length, or the route is not yet isolated, and it
  decides whether the ceiling reference is a reference at all.
- **`provider.only` with an endpoint tag** is still unanswered — the pilot ran the bare family
  everywhere. It remains the only way to pin a Vertex service tier, and §5.5 shows the tier is not
  observable after the fact either: `data_region` is `global` for `flex` and `global` alike, and
  `endpoint_id` is stable but unresolvable against a catalogue that publishes no ids. Both halves
  need settling before #236's budgets are
  determinate.
- **#294's boot-time assertion needs restating** as a per-request fail-closed invariant (§1.2).
- **The generation contract varies per candidate** (§5.6), so the determinism controls are a
  fingerprint axis rather than a constant — and `require_parameters: true` makes sending an
  unsupported one fatal rather than harmless.
- **The cost floor's undated slug** (§1.1) is a selection input, not a disqualification.
- **Reasoning tokens are recorded but not suppressed.** §5.2 shows why that matters: the cost floor's
  reasoning is ten times its answer, and the harness deliberately measures the declared contract
  rather than a tuned variant of it.
- **Task B is not yet wired.** The three turn sets need the Coach Turn seam
  (`review_session_coaching/prose.rs`) driven the way `diagnose_hosted_comment_text` drives the
  comment path, and its gate collapses its rejections the same way the comment path used to.
- **The 15 unrecorded grounding cases and 2 unrecorded sessions** (§4) are the remaining measurement
  work, and they need only the Docker daemon back. The harness runs them unchanged: they are corpus
  rows, and `--case` takes them by id.

*Everything in §6 is now closed or superseded by §7–§10 except #294's boot-time assertion, which
still belongs to #237.*

---

# Part two — the full run (#359)

## 7. What had to be settled before spending

### 7.1 The endpoint tag is honoured, and the bare family is not what #346 thought it was

#231's open question 3 is answered on all
counts, and the answer changes how #346's
numbers should be read.

**A full endpoint tag in `provider.only` is accepted and enforced.** The control proves the
enforcement rather than assuming it: `google-vertex/does-not-exist` returns **HTTP 404** with a
message naming the permitted set, so a tag is parsed rather than tolerated and silently dropped.

**The tag selects the Vertex service tier**, and the evidence is the invoice. Three byte-identical
1 979-token prompts on `google/gemini-3.5-flash-lite-20260721`:

| `provider.only` | `usage.cost` | Served tier |
| --- | --: | --- |
| `google-vertex/global/flex` | **0.00038435** | `flex` |
| `google-vertex/global` | 0.00078370 | — |
| `google-vertex` (bare family) | 0.00078120 | — |

So the bare family resolves to **`global`, not `flex`** — and #346's pilot declared
`google-vertex/global/flex` on both flash-lite routes while sending the bare family. **Its two
incumbent routes were served and billed at the global tier**, at roughly twice the flex price it
believed it was measuring. Nothing detected this, because the two fields the pilot checked cannot
detect it (below). `routes.json` now sends `declaredTag` verbatim, so the declared tier and the
served tier are the same string.

**And the tier *is* observable after the fact** — §5.5 concluded it was not, and it was looking in
the wrong two places. `provider_responses[0].routed_service_tier` reads `"flex"` on the flex request
and is **absent** on both others. What §5.5 checked cannot work: `data_region` is `global` for every
route, and `endpoint_id` is `fe0e0167-572a-482b-8145-f262c1797e79` for *both tiers of the same
model*, so it identifies the (model, provider) pair rather than the endpoint and no probe-built
mapping could ever have existed. The harness now records `routedServiceTier` beside the declared
tier on every measurement, which is what makes
#236's budgets determinate on the tier axis.

Evidence: `services/coach-engine/evaluation/bake-off/probes-2026-08-16.json`.

### 7.2 The ceiling reference is unusable, and it is the model rather than our request

#346 §5.1 left three candidate causes for
`claude-sonnet-5` → `amazon-bedrock` returning a bare `{}`: the schema, the prompt length, or the
endpoint. **It is none of them.** Nine variants, all still `{}` or bare prose or nothing:

| Variant | Outcome |
| --- | --- |
| Real prompt, full schema | raw prose, not JSON |
| No `response_format` at all | empty, `finish_reason: length` |
| `strict: false` | empty, `finish_reason: length` |
| Minimal one-property schema | `{}` |
| Prompt truncated to 600 chars | empty, `finish_reason: length` |
| `max_tokens: 2000` | `{}` |
| No system prompt | `{}` |
| `reasoning: {enabled: false}` | `{}`, **0 reasoning tokens**, 244 billed |
| `reasoning: {effort: low}` | `{}`, 0 reasoning tokens, 222 billed |

Two things fall out. **The route is not deterministic**: six replicates of one byte-identical
request produced three different outcomes — `emptyCompletion` ×2, bare `{}` ×3, raw prose ×1 — and
§5.6 already recorded that it accepts neither `temperature` nor `seed`, so no contract change makes
it reproducible. And **reasoning explains only the empty completions**: 160–699 reasoning tokens of
a 700-token cap, hitting the cap outright twice; with reasoning disabled entirely it still bills 244
tokens and returns `{}`.

The control settles the attribution: **the identical prompt and schema on
`anthropic/claude-4.5-haiku-20251001` over the same `amazon-bedrock` counterparty returns conforming
JSON with correct markers.** So it is Sonnet on this route — not Bedrock, not the schema, not the
prompt, not our request. `strict: true` is inert here in a stronger sense than §5.1 found: on a
trivial prompt the same route returns `{"comment": …, "move": "e4"}`, inventing a property the
closed schema forbids.

The one lever left untested is **Structured Output Mode itself**, which
#294 made "a v1 default, revisitable per
candidate". Testing it would measure a candidate with a different pin identity, so it is
#236's call rather than this ticket's.

### 7.3 The frozen set is now addressed by ply, and checked by a test

#358 recorded the precondition: `run` iterated
every Critical Moment of every case, and the four GothamChess cases carry **26** between them
against the **11** that are frozen, so the frozen 19 would have become 34.

The fix is a **manifest**, `evaluation/bake-off/frozen-set.json`, rather than a `--ply` flag on
`run`. A flag would let a run address a ply; nothing would say which plies *constitute* the
measurement, and #345's coverage claim is a
claim about the set. `run --set` resolves it into generations, `--group` and `--only` slice it, and
`every_frozen_grounding_entry_addresses_a_moment_the_corpus_still_records` turns "frozen means
frozen" into a failing test rather than a sentence in a document: if a corpus change moves a
selected ply, the set stops being addressable and CI says so.

Group sizes: **19** grounding, **3** Coach Turns, **18** session comments (S1 4, S2 7, S3 7 — §5.4
estimated ~17). The profile diff doubles the grounding set alone; doubling the sessions would double
the cost figure they exist to establish, and #332
makes cold-start the common case at beta launch, so the other two groups run cold-start once. That
is **59 generations per route**, 354 across six.

## 8. Task B, and what wiring it cost

### 8.1 The Coach Turn gate had one reason where it needed seventeen

The comment path was narrowed by #346 §2.3 and
the Coach Turn path was not: every failure left `review_session_coaching/prose.rs` as
`ProviderUnavailableReason::LanguageLayer`, so a bake-off could see that a candidate lost the turn
and never *which discipline* it lost — the exact metric the ticket exists to produce.

The fix is the same shape, and deliberately so. `CoachTurnRejection` resolves at full width
internally and narrows at the boundary through `into_wire`, which is **total**: a rejected Coach
Turn is unavailable however it failed, per
#233, so widening the diagnosis moves no wire
contract and changes nothing a Player sees. The full domain suite passes untouched.

One thing the comment path did not need: the rejection **names the dimension**, and separates
structural failures from prose ones. The runtime fills `coachTurnId`, `alternativeMoveId` and every
evidence ref — #344 found them set-equal to
computed values — so a structural failure is *ours* and a prose failure is the candidate's. Counting
them together would charge a model for our bug.

### 8.2 Task B's prompt is three vocabularies, not one

`compile_coach_turn_prompt` reads its markers out of `CoachTurnProseContext::vocabulary` — the same
derivation the gate enforces, per §2.1's rule — and there are three of them, one per dimension,
because a `findability` explanation may not name the resulting evaluation: it does not cite the
evidence that would ground it. **A marker offered under one dimension is an unknown marker under
another**, which the prompt says in as many words and a test pins.

The evidence projection is built inside the prose context rather than beside the prompt, for the
same reason: the facts the model is offered and the literals it may name come out of **one walk of
the packet**. It carries the two positions' pieces (exactly the squares the allowlist admits), both
engine analyses with their lines in SAN, both human-move-model candidate lists renamed to
`playersAtThisRating`, and the alternative's own evaluation and loss. Absent: every evidence id,
every UCI string, and every provenance digest.

The response schema is **flattened exactly as Task A's is** — one closed `kind` enum, every property
required, a `none` sentinel on the refusal reason — because §5.1 measured Bedrock rejecting `oneOf`
outright, and #344 wrote Task B as a `oneOf`
under an `outcome` wrapper. Both properties #233 asked for survive: the refusal is a typed variant
and there is no free-form field.

### 8.3 Task B cost nothing to record, and one thing to choose

#345 §5.2 put Task B's recording cost at zero,
and it holds: `recorded_coach_turn_case` replays the exploration that produces the Alternative Move
out of the frozen `Synthet1` provider recording — Stockfish for the alternative and its reply, Maia
for both positions — with no Docker and no Local Pipeline Runtime, then folds the two Maia
predictions into the packet exactly as the live path folds live ones.

**B2's prior turn is frozen rather than taken from B1's own output.** A prior turn that varied per
route would make B2 a different prompt on every candidate, and B2's job is to measure one rule —
#233's prior-text visibility — not to measure a route against its own earlier self. So one canonical
marker-form assessment is run through the real gate, and every route is shown the published result.

**The alternative is `e5d4`, not `c5d4`.** #345 §5.2 named both. `c5d4` turns out to *be* the
engine's best move at that ply, so `{alternativeMove}` and `{bestMove}` render the same string,
`{alternativeEval}` and `{bestEval}` render the same evaluation, and `{evaluationLoss}` renders a
zero — four of the five objective-quality markers collapse into two facts. That is a strictly worse
test: a candidate could confuse two markers and no gate would catch it. On `e5d4` every marker
renders distinctly (`exd4` against `cxd4`, −2.4 against −2.6, a real 0.2-pawn loss, and
`alternativePopularity` at "the second most common choice at your rating").

---

## 9. The run: 354 generations, six routes, 2026-08-16

Preflight re-passed at run time against **765 live ZDR endpoints** — all seven routes still ZDR,
still `structured_outputs` capable, **no price drift**, including the `gemini-3.6-flash` price #346
corrected. One number moved: `gpt-oss-120b`'s 24-hour uptime is **88.55 %**, down from the 93.9 %
#346 already called the worst in the set.

**Measured spend \$0.7646.** 354 generations, 377 attempts. **Pin Verification passed 349, failed
0**, 2 unverified; no route substituted. Records:
`services/coach-engine/evaluation/bake-off/full-run-2026-08-16.jsonl`.

| Route | gens | authored | conformed | \$/gen | p50 ms | served tier |
| --- | --: | --: | --: | --: | --: | --- |
| `gemini-3.1-flash-lite` → vertex/flex | 59 | **90 %** | 98 % | 0.00030 | **16 197** | `flex` |
| `gemini-3.5-flash-lite` → vertex/flex | 59 | 85 % | **100 %** | 0.00037 | 6 976 | `flex` |
| `claude-haiku-4.5` → bedrock | 59 | 49 % | **100 %** | 0.00286 | **2 561** | — |
| `qwen3-next-80b` → vertex | 59 | 8 % | 93 % | 0.00038 | 4 253 | — |
| `gpt-oss-120b` → vertex | 59 | 3 % | 14 % | 0.00031 | 2 793 | — |
| `claude-sonnet-5` → bedrock | 59 | **0 %** | **0 %** | 0.00874 | 5 624 | — |

Rejections, at the resolution §2.3 and §8.1 bought:

| Route | Why it failed |
| --- | --- |
| `claude-sonnet-5` | `schemaViolation` ×51, `unparsable` ×8 — every generation |
| `gpt-oss-120b` | `schemaViolation` ×31, `emptyCompletion` ×17, `deadlineExhausted` ×17, `unparsable` ×3, `missingRequiredMarker` ×6 |
| `qwen3-next-80b` | `missingRequiredMarker` ×46, `httpError` ×8, `repeatedMarker` ×4, `schemaViolation` ×1 |
| `claude-haiku-4.5` | `missingRequiredMarker` ×15, `repeatedMarker` ×10, `misplacedMarker` ×3, `unknownMarker` ×1, `forbiddenNeutralLiteral` ×1 |
| `gemini-3.5-flash-lite` | `misplacedMarker` ×2, `unknownMarker` ×2, `missingRequiredMarker` ×1, `unexpectedIntentHypothesis` ×3 |
| `gemini-3.1-flash-lite` | `missingRequiredMarker` ×2, `unknownMarker` ×2, `repeatedMarker` ×1, `emptyCompletion` ×1, `deadlineExhausted` ×1 |

### 9.1 Pinning the tier is not free, and the incumbent pays for it in latency

This is the run's most consequential number and it exists only because §7.1 fixed the tag.

The pilot recorded `gemini-3.5-flash-lite` at **p50 1 003 ms**. Under a genuinely pinned `flex` tag
it is **6 976 ms** — and `gemini-3.1-flash-lite` goes from 1 185 ms to **16 197 ms**. These are
single attempts, not retry pairs: attempt-1 latency is tightly distributed (`gemini-3.1` min 15 784,
median 16 188, max 18 709), and the probe's own flex call recorded 6 622 ms in its `/generation`
record before the sweep began.

So **neither half of the incumbent's pilot case survives intact**: it was measured at global-tier
latency and budgeted at flex-tier price, and those are different endpoints. The choice is now
explicit rather than accidental — flex at 7 s and 0.00037, or global at roughly 1 s and roughly
twice the price — and it is
#236's to make. A 16-second Review Moment note
is not obviously shippable; `claude-haiku-4.5` at **2 561 ms** is now the fastest route in the set by
a wide margin, at 7.7× the price.

### 9.2 The ranking inverted, and #357 predicted why

`gemini-3.1-flash-lite` was 4/8 authored in the pilot and is **90 %** here; `claude-haiku-4.5` was
6/8 and is **49 %**. #357 recorded that its
fix minted a new prompt digest and therefore **invalidated #346's conformance ranking** while
leaving its prompt-independent findings standing. This run is the replacement, at
`sha256:cf46cc69…` — and it confirms the invalidation was real rather than formal: the two cheap
Gemini routes are now separated by five points instead of thirty-five, and the incumbent Anthropic
route has swapped places with both.

### 9.3 The two failures that are the model rather than the prose

`claude-sonnet-5` reproduces §7.2 exactly at scale: **0 of 59**, every generation a
`schemaViolation` or `unparsable`, at the highest price in the set. The isolation stands.

`gpt-oss-120b` is worse than the pilot suggested: **17 of 59 generations hit the 30 s task deadline**
and 17 returned an empty completion, against a mean 435 reasoning tokens per generation. Its
nominal cost floor (0.00031/gen) is indistinguishable from `gemini-3.1-flash-lite`'s 0.00030 — so
the cost floor is not a cost floor, and §1.1's undated-slug asymmetry costs it nothing it had.

### 9.4 Task B: three of six routes complete a turn

| Route | B1 own | B2 steer | B3 out-of-scope |
| --- | --- | --- | --- |
| `gemini-3.1-flash-lite` | authored | authored | **answered** (gate passed) |
| `gemini-3.5-flash-lite` | authored | authored | **refused** ✓ |
| `qwen3-next-80b` | authored | authored | **answered** (gate passed) |
| `claude-haiku-4.5` | rejected | rejected | rejected |
| `gpt-oss-120b` | schema | schema | schema |
| `claude-sonnet-5` | schema | schema | unparsable |

Haiku's three rejections are all `objectiveQuality` — `repeatedMarker` ×2 and
`missingRequiredMarker` ×1 — which is the per-dimension resolution §8.1 exists to produce, and it
says something specific: the dimension carrying four required markers is the one it cannot hold.

**Only one route in six emitted the typed `OutOfScope` refusal.** That matters because
#233 chose *structural* containment — a typed
variant and a schema with no free-form field — over a classifier, and structural containment only
contains if candidates use it. The mitigating half is that the failure is benign in every observed
case: the two routes that passed the gate on B3 **ignored the digression and assessed the move
anyway**, rather than answering "which opening should I learn next". So the Player is not steered
off-topic; the refusal is simply near-unexercised, and B3 discriminates far less than
#345 §5.2 expected.

### 9.5 The budget tiers, computed

The session set's whole job, per #345 §5.3 —
the ceiling computed against `HARD_MAXIMUM = 10` rather than sampled for:

| Route | \$/moment | \$/Review Session at the cap | session p50 |
| --- | --: | --: | --: |
| `gemini-3.1-flash-lite` → vertex/flex | 0.00029 | **0.0029** | 16.3 s |
| `gpt-oss-120b` → vertex | 0.00031 | 0.0031 | 2.4 s |
| `qwen3-next-80b` → vertex | 0.00031 | 0.0031 | 5.6 s |
| `gemini-3.5-flash-lite` → vertex/flex | 0.00037 | 0.0037 | 7.0 s |
| `claude-haiku-4.5` → bedrock | 0.00271 | **0.0271** | 2.6 s |
| `claude-sonnet-5` → bedrock | 0.00934 | 0.0934 | 7.2 s |

The whole usable range is **under three cents per Review Session at the hard cap**, so inference
cost is not the binding constraint on any admissible route — which reframes
#236's selection as a quality-and-latency
decision with a cost tiebreak, not a cost decision.

## 10. The human read

#345 §4 named two jobs no gate can do. One
did not occur and one did.

**Repetition across a run did not occur.** Checked across every session on the three routes that
author — `gemini-3.5-flash-lite`, `gemini-3.1-flash-lite`, `claude-haiku-4.5` — every note in every
session opens differently (4/4, 5/5, 7/7, 6/6, 3/3 distinct). The structural failure mode #345
predicted, on the grounds that the model cannot see its own previous note, simply is not there.

**Claims contradicting their own substituted markers do occur.** From `gemini-3.5-flash-lite`, the
route with 100 % schema conformance and 85 % authored:

> Playing Bxg3 is a good move and **a notable find for players at your rating**, even though it is
> **the third most common choice at your rating**.

The prose asserts rarity; the marker it sits beside substitutes commonness. This is exactly
#344's accepted residue — markers make a wrong
*fact* inexpressible and leave a wrong *claim* expressible — arriving in the front-runner's output
at a rate a human sample can find in six comments.

Three defects the gate also cannot see, all of them frame errors #357 classed as residue:

- `{decisionCue}` used as a causal clause: "the advantage was lost **because** before committing
  here, calculate Nxd4 first."
- Two appositives with no punctuation between them: "Qe8#, **a good move the most common choice at
  your rating** requiring precise play."
- **Two distinct markers rendering the same string**: `{strongestReply}` and `{likelyReply}` both
  render `Re1` when the engine's best reply is also the most likely human reply, producing "the Re1
  is Re1." Nothing catches this, and it is the same degeneracy §8.3 avoided by choosing `e5d4` —
  which means the choice of test case was load-bearing, and the defect is live in production shape.

And one the run settles rather than finds: **the Coaching Profile Projection is not inert at
scale.** Cold-start against populated differs on **19 of 19** authored pairs for
`gemini-3.5-flash-lite`, 19 of 19 for `claude-haiku-4.5`, and 15 of 18 for
`gemini-3.1-flash-lite` — confirming §5.3's pilot finding at four times the n.

## 11. Handed forward

- **The tier is a decision, not a default** (§7.1, §9.1). `provider.only` must carry the full tag or
  the pin silently serves `global`; and pinning `flex` costs 7 s per note on the incumbent and 16 s
  on its sibling. #236 now chooses tier as well
  as model, and the cheapest route in the set is also the slowest by 6×.
- **Inference cost is not the binding constraint** (§9.5): under three cents per Review Session at
  the hard cap across every usable route.
- **`claude-sonnet-5` → bedrock is out** unless Structured Output Mode changes (§7.2), which would
  mint a different Explainer Candidate and is #236's call.
- **`gpt-oss-120b` and `qwen3-next-80b` are out on prose** (3 % and 8 % authored), and the cost
  floor's price advantage does not exist once reasoning is counted.
- **Only one route emits the typed refusal** (§9.4). #233's structural containment holds in
  practice but is near-unexercised, and B3 discriminates weakly. Whether that needs a contract
  change belongs to #233's owner, not to the bake-off.
- **Two markers can render the same string** (§10), and no gate catches the nonsense it produces.
  This is a vocabulary defect of the same family as
  #357's, on Task B's renderings.
- **Task B's noun-phrase renderings still meet model-supplied articles**: "with an
  {evaluationLoss}" substitutes to "with an 0.2 pawns of the position". #357 concluded Task B needed
  no rendering change because every rendering was already a noun phrase; a noun phrase preceded by
  the model's own article is the case that reasoning missed.
- **The harness buffers every record until the run ends**, so a crash 50 minutes into a 90-minute
  sweep loses all of it. Streaming the JSONL as it goes costs nothing and should land before the
  next full run.
- **Replicates were not run.** #346 §5.3 established byte-identical determinism on two routes and
  this run adds nothing to that; the ticket's ≈820-generation figure assumed 3 replicates, and 354
  buys the same conformance ranking for a third of the spend. Latency percentiles come from 59
  generations per route rather than from repeats of one.

---

# Part three — the tier-and-challenger sweep (#236)

## 12. 80 generations, two `global`-tier routes, 2026-08-17

§9.1 left #236 choosing a
tier against a latency number measured under a superseded prompt digest, and the
candidate set had been frozen since 2026-08-12. This sweep closes both: it runs
the pin at the tier it would actually ship on, and it runs the newest ZDR-eligible
Google route against the same frozen set. **Measured spend \$0.1435**, 80
generations, **Pin Verification 80 passed / 0 failed**, both routes serving their
declared permaslug on `google-vertex`. Records:
`services/coach-engine/evaluation/bake-off/challenger-2026-08-17.jsonl`;
preflight `preflight-2026-08-17.json` against 767 live ZDR endpoints, both new
routes admissible with no price drift.

| Route | authored | schema | \$/gen | p50 ms | reasoning tok/gen |
| --- | --: | --: | --: | --: | --: |
| `gemini-3.5-flash-lite` → vertex/**global** | **36/39 = 92 %** | 40/40 | 0.00075 | **959** | 0 |
| `gemini-3.7-flash` → vertex/**global** | **38/39 = 97 %** | 40/40 | 0.00283 | 8 660 | **1 045** |

### 12.1 The tier is confirmed, and it costs no quality

The pilot's ~1 003 ms was measured on the pre-#357 prompt; this is **959 ms**
under `sha256:cf46cc69…`, and across the 18 notes of the three real sessions the
distribution is tight — **p50 974 ms, max 1 270 ms**. The caveat §9.1 attached to
that number is retired.

The unexpected half is that the tier is not only a latency purchase. The same
model under the same digest scored **85 % authored at `flex` and 92 % at
`global`** — 9 rejections in 59 against 3 in 39. That is very likely noise at
these n, but it forecloses the worry that ran the other way: buying the fast tier
costs nothing in prose.

One small new fact about observability. §9.1 established `routed_service_tier` as
the field that *can* tell the tiers apart; declaring the bare `google-vertex/global`
tag returns **`null`** in it. It reports a tier only when a non-default one is
served, so the *declaration* rather than the response is the record of what was
asked for — which matters because the failure §7.1 found was a declaration that
did not bind, not a response that lied.

### 12.2 The challenger is a reasoning model, and the catalogue does not say so

`google/gemini-3.7-flash-20260813` looked like the cheapest possible upgrade:
same already-verified Google Vertex counterparty, ZDR and native structured
output on `google-vertex/global`, a dated permaslug, an identical parameter shape
(seed yes, temperature no), and a *lower* output price than the pin — 0.375/1.875
against 0.30/2.50. Priced on the pin's measured token counts that is **+19 % per
note**.

It is **3.8×**. The route spends a mean **1 045 reasoning tokens per generation**
against the pin's zero, taking completions from 61 tokens to 1 109. Nothing in
`/api/v1/models/{slug}/endpoints` distinguishes a reasoning model from a
non-reasoning one at the price level — `reasoning` appears in
`supported_parameters` for both, and the pin does not use it. So the finding
generalises past this candidate: **a per-token price is not a per-note cost, and
the difference is not visible before a generation is bought.**

Latency follows the tokens: **8 660 ms p50**, 9× the pin, on the same tier.

It is genuinely better at the prose. **97 % authored** — 38 of 39, one
`repeatedMarker` — is the best any route has scored in this map, and it is worth
recording as the measured ceiling in place of `claude-sonnet-5`, which never
produced a comparable number because it never emitted the schema (§7.2, §9.3).
The pin's 92 % is now known to be five points short of an achievable ceiling
rather than an unknown distance from one.

**A third consequence lands on the generation contract.** The challenger's
largest completion was **1 985 tokens**. #236
set `max_tokens: 512` on the pin — four times its largest measured note and a
runaway bound — which would truncate this candidate outright. `max_tokens` is
therefore coupled to the candidate rather than a property of the task, and a
model swap has to re-read it.

### 12.3 Task B reproduces §9.4 exactly

The pin authored B1 and B2 across all three dimensions and emitted the typed
`OutOfScope` refusal on B3 — the **only** route across 434 generations to have
used #233's structural
containment. The challenger refused B3 too (so the count is now two of seven) and
was rejected on B1 with `repeatedMarker` on `objectiveQuality`, the same
four-marker dimension `claude-haiku-4.5` could not hold in §9.4.

The pin's own B1 output reproduces the article defect §11 handed forward,
verbatim and in production shape:

> Choosing exd4 results in -2.4, Much better for Black, which is nearly as strong
> as cxd4 at -2.6, Much better for Black **with an 0.2 pawns of the position**.

### 12.4 What this sweep did not do

Replicates were not run, and the challenger was measured at n = 39 grounding
generations against the incumbent's 59 in §9 — enough to separate 3.8× cost and
9× latency, not enough to separate 97 % from 92 % with confidence. Nothing here
re-measures the four routes ruled out in §9; their rejections were structural
(schema, markers, deadlines) rather than tier-sensitive.
