# Prototype: v1 prompt templates and response schemas for the two web Language Layer tasks

> **Retired 2026-08-31.** The bake-off harness, its frozen task set, and its
> route, preflight, and probe files were removed in #534. Paths below that name
> `evaluation/bake-off/frozen-set.json`, `routes.json`, `preflight-*.json`,
> `probes-*.json`, `replicate-control-*.jsonl`, `marker-seam-smoke-*.jsonl`, or
> `takeaway-marker-slice-*.jsonl` no longer resolve. The recorded generations
> three tests still replay -- `pilot-2026-08-14.jsonl`,
> `full-run-2026-08-16.jsonl`, `challenger-2026-08-17.jsonl` -- survive, and
> `evaluation/bake-off/README.md` says what each is for. This document stays as
> the record of what was measured and why.

Prototype asset for [Compile the v1 prompt templates and response schemas for the two web Language Layer tasks](#344),
a child of [Ship the tailored OpenRouter web Language Layer to beta](#229).

Code references are to the working tree at the time of writing.

## What the Review Moment note is meant to be

Commentary, close to TakeTakeTake in character: the coach reacting to the move a Player actually
made, in its own words, grounded in the recorded facts. Set by the Service Operator on 2026-08-13.

**Grounded-by-construction survives that.** The first reading of this ticket concluded the two were
incompatible and proposed replacing the gate with allowlists. That was wrong, and the correction
matters: what conflicts with commentary is the **position locking** and the **stiff phrasing** of
`CommentFactsPolicy`'s mandated sentences (`services/coach-engine/src/critical_moment_comment.rs:353`)
— not the guarantee that facts cannot be misstated. Those are separable, and separating them gives a
gate that is stronger than today's while leaving the prose entirely free.

## The gate: slot markers

The model writes prose containing **typed markers** and never a raw figure. The runtime substitutes
the canonical rendering for each marker after the gate has passed.

```text
MODEL WRITES

  You took the pawn on e5, and at your rating that's the natural move —
  {playedPopularity}. The bigger piece was the one you left alone: {betterMove}
  wins the knight and keeps you at {bestEval}. After {playedMove} it's
  {playedEval}. Nothing was blundered — {consequence} by taking the smaller
  thing first, which is the habit worth breaking. {decisionCue}

RUNTIME SUBSTITUTES → what the Player sees

  You took the pawn on e5, and at your rating that's the natural move — the most
  common choice at your rating. The bigger piece was the one you left alone:
  Nxd4 wins the knight and keeps you at +1.3, Slightly better for White. After
  Nxe5 it's +0.2, Roughly balanced. Nothing was blundered — the advantage was
  lost by taking the smaller thing first, which is the habit worth breaking.
  Before committing here, calculate Nxd4 first.
```

### Why this is stronger than the skeleton it replaces

Today a model satisfies `contains(required_literal)` by reproducing `+1.3` — and can write a
second, invented `+0.9` in the same sentence and still pass. Under markers **no evaluation figure
can appear in the model's output at all**; every number the Player reads was rendered by us. The
guarantee moves from "the right fact is present" to "no wrong fact is expressible".

It also **restores the claim ledger's teeth**. `validate_hosted_grounding_ledger` (`:268`) currently
compares a computed ledger against a computed claim set, which is a tautology — the model asserts
nothing checkable. The markers used *are* the claims asserted, so the ledger becomes a real
derivation from the model's output rather than a restatement of the facts.

### Marker vocabulary

The markers are the existing `CommentFactsPolicy` claim set made addressable, one per claim.

| Moment kind | Required markers | Optional |
| --- | --- | --- |
| Improvement | `{playedMove}` `{playedEval}` `{betterMove}` `{bestEval}` `{consequence}` `{decisionCue}` | `{takeaway}` `{playedPopularity}` |
| Positive highlight | `{playedMove}` `{grade}` `{achievement}` `{difficulty}` | `{takeaway}` `{playedPopularity}` |
| Neutral | `{playedMove}` `{reason}` `{observation}` | `{playedPopularity}` |

Canonical renderings for the worked case: `{playedMove}` → `Nxe5` · `{playedEval}` → `+0.2, Roughly
balanced` · `{betterMove}` → `Nxd4` · `{bestEval}` → `+1.3, Slightly better for White` ·
`{consequence}` → `the advantage was lost` · `{decisionCue}` → `before committing here, calculate
Nxd4 first` inside a sentence, `Before committing here, calculate Nxd4 first.` standing alone.

### Renderings have a shape (#357)

The first version of this document gave each marker one canonical rendering, and the pilot found
that a slot is a clause while some renderings are sentences: obeying the instruction above produced
*"You played e4, and After e4, the evaluation is +0.5 — Slightly better for White. because it was
sound"*. The runtime now fits the rendering to the seam rather than asking the model to fit its
sentence to the rendering — the same move as the gate producing the text instead of approving it.

Four shapes, declared per marker:

| Shape | Substitution | Markers |
| --- | --- | --- |
| **Literal** | byte for byte, never re-cased | `{playedMove}` `{betterMove}`, and Task B's notation |
| **Anywhere** | verbatim, capitalised when it opens a sentence | `{grade}` `{consequence}` `{reason}` `{playedEval}` `{bestEval}` `{takeaway}` `{playedPopularity}` |
| **Shaped** | two authored forms, chosen by position | `{decisionCue}` `{observation}` `{difficulty}` |
| **Own sentence** | must stand alone; misplacement is `MisplacedMarker` | `{achievement}` |

Three things this settled:

- **Nothing is ever downcased.** `e4` capitalised at a sentence start is `E4`, and "White delivered
  checkmate" downcased is not the colour, so notation is Literal and the lowercase form of a
  sentence-shaped rendering is *authored beside it* rather than derived. `{difficulty}` shows why
  that has to be authored: the clause drops the demonstrative subject ("especially difficult to find
  for players at your rating"), and "This required precise play." has no subject to keep at all.
- **`{achievement}` has no form that fits.** It renders a subjectless verb phrase and models put it
  in noun slots ("You found secured checkmate"), infinitive slots ("the opportunity to secured
  checkmate") and bare fragments alike — four frames, no common shape. It becomes "You secured
  checkmate." and may only stand alone. This is the one place the gate judges *placement*, which is
  two character comparisons and not the grammar judgement #344 ruled out.
- **The runtime owns the punctuation at the seam**, in both directions: a rendering that ends in a
  full stop swallows the model's ("…for White.."), and a clause form the model ran straight out of
  supplies one ("…calculate e4 first By choosing this path…").

`{playedEval}`'s terminal branch is also wrapped — "the recorded outcome where White delivered
checkmate" — so both of its variants slot the same way, matching what `{bestEval}` already did.

**Measured.** The 20 published pilot generations replay offline against the new renderings
(`tests/marker_seam_replay.rs`, no provider call): 16 publish clean, 4 reject as `MisplacedMarker`
— all four the same `{achievement}` case, and all four genuinely broken prose ("You saw the
opportunity to secured checkmate", "Qe8# delivers secured checkmate"). Those four were written under
the old prompt, which showed the bare verb phrase and carried no placement rule, so the replay
cannot price the rule. A live slice under the new prompt can and did: **10 generations across the two
clean routes, zero misplaced markers, both `{achievement}` uses placed correctly**, the one rejection
an unrelated `missingRequiredMarker`. Records in `evaluation/bake-off/marker-seam-smoke-2026-08-15.jsonl`,
spend \$0.0157.

The replay also caught a defect the first fix introduced. Models routinely leave the full stop to the
rendering — "…and bishop. {observation} The move is consistent…" — so deciding standalone from the
model's *punctuation* took the clause form and swallowed the sentence break, turning 10 correct
placements into run-ons. Standalone is now decided by whether the model kept writing the **same
sentence** (a following lowercase word or `,;:`), not by whether it punctuated the end of one.

`{playedPopularity}` is **new**. The human move model's rank and probability are already in the
facts and have never reached a comment, yet they are exactly the material that makes commentary feel
like a person watching — "at your rating that's the natural move" is a human observation, not an
engine one. It renders canonical Player-facing text (`the most common choice at your rating`), which
is also how the vocabulary rule below is enforced rather than merely requested. It extends the claim
set by one optional claim.

### Gate order

1. **Parse markers** from the raw text. Unknown marker → reject. Marker repeated → reject; a fact
   rendered twice reads as a stutter.
2. **Required markers present** → otherwise `MissingFactualClaim`. This is the ledger.
3. **No figures in raw text.** Any evaluation-shaped token (`+N.N`, `-N.N`, `0.0`, `#N`, `#-N`),
   percentage, or probability outside a marker → reject. Numbers are the dangerous class and they
   are wholly marker-gated.
4. **Chess literals by allowlist.** The model still names squares and lines descriptively — "the
   pawn on e5" — so SAN-parsable tokens are checked against a widened allowlist rather than banned.
   See below.
5. **Substitute**, fitting each rendering to its seam. An own-sentence marker used inside a
   sentence → `MisplacedMarker`.
6. **Post-substitution checks**, all existing: single paragraph, `contains_internal_player_facing_text`
   (`:826`), Neutral's `forbidden_literals`, `learning_grounding::validate`,
   `validate_intent_presentation`.

The `{`/`}` ban in step 6 stops being an arbitrary rule and becomes meaningful: a brace surviving
substitution means substitution failed, and that comment must never ship.

### The chess-literal allowlist must still widen

`ChessLiteralGrounding` (`services/coach-engine/src/chess_literal_grounding.rs`) builds its
allowlist by walking `serde_json` over the facts and splitting strings on whitespace, then requires
every SAN-parsable token to be in that set. Two consequences block descriptive prose:

- **Bare squares are pawn moves.** The facts carry `Nxd4`, and whitespace splitting never yields
  `d4`, so "the knight on d4" is rejected. `e5` passes only because it happens to appear as a
  captured-piece square.
- **The engine line cannot be quoted at all.** The principal variation is serialized as UCI
  (`f3d4`, `e5d4`, …), which never parses as SAN, and `contains_raw_uci` (`:832`) separately bans
  reproducing it.

Replace the incidental `from_serialized` derivation with a purpose-built projection: SAN for the
principal variation, the squares named in the moment's effects and mechanism, and the moves already
carried. This adds chess facts to what the model sees, not Player data, so
#233's minimization rule is unaffected — but
the projection is a prompt input, so its shape joins the prompt digest.

### Vocabulary: never name the model to the Player

Maia is an internal term. The Player-facing phrasing is **"players at your rating"**, matching the
study already in `apps/central-host/src/preview/review-session/ReviewSessionPrototype.tsx:603`
("Most players at your rating…").

Enforce it in `contains_internal_player_facing_text` alongside the existing `{`/`}`, raw-UCI and
analyzed-score bans: **`human model`, `move model`, `maia`, `human-likely`, `human likely`**. A leak
becomes a rejection, not a style miss. Note that `positive_difficulty_text` (`:733`) currently ships
"at the selected Elo" to Players, which has the same stiffness and should move to the same phrasing.

### What no gate can catch

Invented **claims**. "This threatens mate in two" passes every check above, because `mate` is
neither a marker, a figure, nor a SAN token. Markers guarantee that no *fact we render* is wrong;
they cannot guarantee that everything asserted around them is right. That residue is what
[Select the pinned OpenRouter model and inference budgets](#236)'s
human quality read has to look for, and the reason that read cannot be fully automated.

### What this hands the bake-off

Marker discipline is a **per-candidate reliability metric** — how often a model writes a bare number
instead of a marker, invents a marker, or repeats one. It is measurable without human judgement and
it is exactly where cheap models are likely to separate, so
#236 gets a hard number alongside the soft one.

## Length varies by moment severity

Set by the Service Operator. The prompt makes the call per moment:

| Moment | Target |
| --- | --- |
| Improvement, advantage lost or forced mate missed | A short paragraph — what the Player was likely seeing, the refutation, the takeaway |
| Improvement, advantage reduced | Two or three sentences |
| Positive highlight | Two or three sentences; name the achievement and why it was hard to find |
| Neutral | One line. Neutral moments already forbid correction vocabulary |

`max_output_tokens` is set for the longest, so it bounds rather than targets. Per-moment output cost
is a **distribution**, and #236 must price the
per-Review-Session budget from the moment mix its frozen set produces, not from an average.

## Task A — Review Moment Comment authoring

Seam: `CriticalMomentCommentAuthor` (`critical_moment_comment.rs:88`), gated by
`author_grounded_comment` (`:196`).

Worked case: `tactical-white-human-likely` from `services/coach-engine/evaluation/corpus`. Played
`Nxe5` at move 4 as White at Elo 1200; engine prefers `Nxd4`; played eval +22cp, best +126cp,
centipawn loss 104, residual `advantageLost`; effect is a captured pawn on `e5`; mechanism payoff
wins a knight; the played move ranks **first** among players at that rating, at 37%.

**Safe rendering — the degraded path, unchanged:**

> Improvement: After Nxe5, the evaluation is +0.2 — Roughly balanced; the advantage was lost. The better move was Nxd4, leaving the evaluation at +1.3 — Slightly better for White. Before committing here, calculate Nxd4 first.

The target the model writes against is the marker form shown at the top of this document.

### Compiled template (Task A)

````text
SYSTEM
You are the Chen Chess Coach. You are commentating on one move from a game the Player
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
variant rather than inventing content.

USER
COACHING_PROFILE:
{{coaching_profile_projection}}

FACTS:
{{facts_json}}

MARKERS:
{{marker_vocabulary}}

ALLOWED_CHESS_LITERALS:
{{allowed_chess_literals}}

LEARNING_MATERIAL:
{{learning_material}}
````

**Coaching Profile Projection block.** Per
[Settle Coaching Profile control and consent without a settings surface](#332),
ordered top-K Learning Track Keys and nothing else, with an invariant shape — always rendered.
Populated:

```text
This Player has recently been working on: knight-forks, back-rank-safety, opposition.
Where the facts naturally touch one of these, lean into it. Do not force it, and do not
mention this list to the Player.
```

Cold start — the common case at beta launch, so it is rendered, not omitted:

```text
This Player has no learning history yet.
```

### Response schema (Task A)

Native structured output per
[Choose the provider endpoint posture for the pinned model](#294).
Markers live inside the string; the grounding ledger is not authored — it is derived from the
markers used and checked against the computed claim set.

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["outcome"],
  "properties": {
    "outcome": {
      "oneOf": [
        {
          "type": "object",
          "additionalProperties": false,
          "required": ["kind", "text"],
          "properties": {
            "kind": { "const": "comment" },
            "text": { "type": "string", "minLength": 1, "maxLength": 2000 }
          }
        },
        {
          "type": "object",
          "additionalProperties": false,
          "required": ["kind", "reason"],
          "properties": {
            "kind": { "const": "outOfScope" },
            "reason": {
              "type": "string",
              "enum": ["factsInsufficient", "requestNotAboutThisPosition", "unsafeRequest"]
            }
          }
        }
      ]
    }
  }
}
```

## Task B — Alternative Move Assessment authoring

Seam: `AlternativeMoveAssessmentAuthor` (`review_session_coaching.rs:103`), gated by
`validate_coach_turn_publication` (`:858`) into `review_session_coaching/evidence.rs:109`.

**The evidence refs are not authored.** `validate_assessment` requires `cited == required` per
dimension — set equality, not a minimum:

| Dimension | Required evidence refs |
| --- | --- |
| `objectiveQuality` | target branch, source engine analysis, resulting engine analysis |
| `findability` | target branch, source human move model |
| `resilience` | target branch, resulting engine analysis, resulting human move model |

`coachTurnId` and `alternativeMoveId` are checked against the target the same way. All four
non-prose fields are forced, and a mismatch is not a per-dimension retry — it is
`ProviderUnavailableReason::LanguageLayer` for the whole turn. The runtime fills them; the schema
carries three strings.

**This task has no prose grounding at all today.** `validate_dimension`
(`review_session_coaching/evidence.rs:175`) checks only that each explanation is non-empty. No
markers, no chess literals, no URL ban. #233
requires both tasks be grounded; on this seam that is unimplemented. The same marker mechanism
applies, with the vocabulary derived from the evidence packet rather than the moment facts.

### Compiled template (Task B)

````text
SYSTEM
You are the Chen Chess Coach. The Player has asked about a move they were considering
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
- Say "players at your rating". Never mention a model, an engine name, Maia, Stockfish,
  or any internal term.

MARKERS:
- Every evaluation, score, percentage, and probability MUST be written as a marker from
  MARKERS. Never write a number yourself.
- Write markers verbatim, braces included. A marker stands in for a phrase, not a
  sentence: build your sentence around it the way you would around "the knight on d4".

Grounding — violating one discards the turn:
- Every move and square you name in your own words must appear in
  ALLOWED_CHESS_LITERALS. Bare square names count.
- Never write coordinate notation such as "f3d4".
- Every claim must trace to EVIDENCE. Do not invent a line or continue one past what
  EVIDENCE records.
- Never include a URL, a link, or a citation.

If the Player's message is not about this position or this alternative move, return the
refusal variant. Do not answer it, and do not scold them.

USER
COACHING_PROFILE:
{{coaching_profile_projection}}

PLAYER_MESSAGE:
{{player_message}}

ALTERNATIVE_MOVE:
{{target_summary}}

EVIDENCE:
{{evidence_packet_projection}}

MARKERS:
{{marker_vocabulary}}

ALLOWED_CHESS_LITERALS:
{{allowed_chess_literals}}
````

Prior turn text is included only when the prior turn steers this same Alternative Move, per
#233; otherwise `PLAYER_MESSAGE` stands alone.

### Response schema (Task B)

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["outcome"],
  "properties": {
    "outcome": {
      "oneOf": [
        {
          "type": "object",
          "additionalProperties": false,
          "required": ["kind", "objectiveQuality", "findability", "resilience"],
          "properties": {
            "kind": { "const": "assessment" },
            "objectiveQuality": { "type": "string", "minLength": 1, "maxLength": 1200 },
            "findability": { "type": "string", "minLength": 1, "maxLength": 1200 },
            "resilience": { "type": "string", "minLength": 1, "maxLength": 1200 }
          }
        },
        {
          "type": "object",
          "additionalProperties": false,
          "required": ["kind", "reason"],
          "properties": {
            "kind": { "const": "outOfScope" },
            "reason": {
              "type": "string",
              "enum": ["notAboutThisPosition", "notAboutChess", "unsafeRequest"]
            }
          }
        }
      ]
    }
  }
}
```

## Digests

[Define cross-surface evaluation and fingerprinting](#234)
makes the prompt digest and schema digest declared-configuration axes, computable at boot. Both are
SHA-256 over the compiled artifact with placeholders unsubstituted — the template, not the rendered
prompt, since the rendered prompt varies per moment while the candidate identity must not. The fixed
voice rides inside the prompt digest, as #332
intended when it made Coaching Preferences a template constant. **The marker vocabulary and its
canonical renderings join the digest too** — changing how `{bestEval}` renders changes what the
Player reads without changing a byte of model output, so it must mint a new Explainer Candidate.
Landing these retires the placeholder `a…`/`b…` digests in
`CriticalMomentCommentAuthoringProvenance::hosted_generation_contract()` (`:117`).

## What the gate rewrite settled

Landed by [Rebuild the Language Layer grounding gates for free-form prose](#347)
(`language_layer_markers.rs`, `chess_literal_grounding.rs`, `critical_moment_comment.rs`,
`review_session_coaching/prose.rs`). Four things this document left open were decided while
building it:

- **Grounding produces the comment, it does not approve one.** Substitution means the admitted
  text and the authored text are different strings, so every seam that used to publish `draft.text`
  now publishes what the gate returned. That is what makes "no brace survives" enforceable rather
  than aspirational.
- **The ledger is a subset, not an equality.** Required claims must all be asserted and no claim
  outside the facts variant may be, but two admissible comments about one moment legitimately
  differ in their optional claims — so the durable checkpoint check moved from equality with the
  canonical set to that invariant. The stored ledger is now the claims the comment made.
- **Bare integers pass; anything that can carry an evaluation does not.** The no-figures rule
  rejects a sign followed by a digit, a decimal, a mate glyph and a percentage. "wins a knight in 2
  moves" is prose. This is a deliberate floor, not an oversight: a bare integer cannot state an
  evaluation, a probability, or a rank.
- **Task B rejects whole turns.** The open question in the ticket is answered: a rejected
  dimension is `ProviderUnavailableReason::LanguageLayer` for the entire Coach Turn. Retrying one
  dimension would publish a turn assembled from two generations, which no Evaluation Fingerprint
  could describe, and #233 already has Coach
  Turns degrading to unavailable rather than to a safe rendering.

Task B's marker vocabulary is **per dimension**, matching the evidence each is required to cite:
`objectiveQuality` carries `{alternativeMove}` `{alternativeEval}` `{bestMove}` `{bestEval}` and
optional `{evaluationLoss}`; `findability` carries `{alternativeMove}` and optional
`{alternativePopularity}` `{mostLikelyMove}` `{sharedMove}`; `resilience` carries `{alternativeMove}`
`{strongestReply}` and optional `{replyEval}` `{likelyReply}` `{sharedReply}`. A dimension cannot
name what it does not cite, so findability has no way to state the resulting evaluation. An optional
marker whose rendering coincides with another marker's is withheld for that turn — `{likelyReply}`
when the likeliest reply is the strongest one, `{mostLikelyMove}` when the likeliest move is the
alternative itself — and the coincidence is a stated projection fact plus `{sharedReply}` /
`{sharedMove}` so the Coach Turn can name the match once (see the Task B rendering defects below).

## What the identifier leak settled

Landed by [Stop the facts projection leaking internal identifiers into Player prose](#360),
after a live route published `occupyTheCenter` into a Player-facing note.

- **The facts speak two languages, and only one of them is Player-facing.** The projection hands
  the model ~30 machine spellings across eleven paths — `momentKind`, `classification.reasons[]`,
  `qualification.reasons[].reason`, `effects[].kind`, `payoff.kind`, `residualOutcome.classification`,
  `teaching.themes`, `openingPrinciples` — none of which any earlier gate could see. They are not
  notation, not figures, and not the human-model vocabulary.
- **The check is a shape, not a list.** `contains_internal_identifier` rejects a lowercase letter
  immediately followed by an uppercase one. Enumerating the enums is the drift a single derivation
  exists to avoid; a shape needs no maintenance, and catches both a variant added tomorrow and one
  the model invents that the facts never carried. Measured over every SAN token the corpus produces
  and all 60 recorded authored generations: 1 true positive, 0 false positives.
- **URL tokens are exempt, and that is load-bearing.** Resource ids are camelCase by convention
  (`lichess.org/training/hangingPiece`), and reproducing an exact `LEARNING_MATERIAL` line is the
  one place a comment may carry a URL. Every URL is separately held to being a line the facts
  admit, so nothing is lost. This surfaced as a real regression, not a hypothetical.
- **Banning a fact means giving it a voice.** The leak fired on an Improvement moment, the one path
  where a teaching theme had no speakable form. `{takeaway}` is now offered there too, rendering
  through `teaching_takeaway` — the safe path's own words, so no new vocabulary — and asserting a
  new `improvementTakeaway` claim. `{takeaway}` therefore spans two paths and two claims, which is
  why `CommentFactsPolicy` now carries its path.
- **`positionPhase` projects the phase alone.** `policyVersion` (`position-phase/v1`) is metadata
  for another consumer and has no uppercase for the shape rule to catch, so it leaves the
  projection rather than relying on the gate.
- **The Coach Turn path inherits the check but not the resolution.** It sits beside
  `contains_internal_player_facing_text`, which both gates call, so Task B is covered before
  #359 writes its projection — but that path
  still collapses every failure into `ProviderUnavailableReason::LanguageLayer`, so only the
  comment path can report `InternalIdentifier` as its own kind.

Measured live on the two clean routes, 10 generations for \$0.0165
(`evaluation/bake-off/takeaway-marker-slice-2026-08-16.jsonl`): zero identifier leaks, the
previously-leaking case authored clean on both routes, and `{takeaway}` used on 3 of 4 moments that
offer it and on none that do not — no over-firing. The two rejections were `repeatedMarker` and
`missingRequiredMarker`, both in the existing per-route failure profile and neither touching this
change.

## What the Task B rendering defects settled

Landed by [Fix the marker collisions and article defects in Task B renderings](#364),
after #359 §10 recorded "the Re1 is Re1" and
#236's sweep published "with an 0.2 pawns of
the position" on the pinned route. The 2026-08-21 operator decision superseded the first landing
of the collision fix: withholding made the stutter inexpressible, but it was deletion, not
collapse — the model could still *see* the two facts match and had no grounded license to *say*
they match.

- **A coincident optional marker is withheld, and the coincidence is a stated fact.** Two markers
  rendering one string is a substitution-time property the vocabulary build can see, so
  `{likelyReply}` is not offered when it equals `{strongestReply}`, and `{mostLikelyMove}` is not
  offered when it equals `{alternativeMove}`. The stutter becomes inexpressible — naming the
  absent marker is an unknown marker. The shared SAN still arrives through the marker that owns
  it. Withholding is the stutter guard, not the whole answer: the projection states
  `sharedReply` / `sharedMove` (the SAN when they coincide, `null` when they do not), and
  Resilience / Findability offer `{sharedReply}` / `{sharedMove}` rendering as a noun phrase
  ("the same move") so the Coach Turn can name the match once. The shared SAN arrives
  through `{strongestReply}` / `{alternativeMove}`, not through the coincidence marker.
- **Prose renderings that need an article now bring their own, and the seam absorbs the model's.**
  `{evaluationLoss}` renders "a 0.2-pawn margin" (its mate branch was already "a forced mate"),
  and substitution drops a model-written `a`/`an`/`the` sitting directly against a rendering that
  carries its own article — one of the two goes, exactly as one of two full stops does at the
  other end. Notation never absorbs: "the {likelyReply}" is the model's article doing real work in
  front of a bare move, and byte-for-byte substitution stays byte-for-byte.
- **MARKERS instructions, so the prompt stops inviting the article and licenses the coincidence.**
  "Build your sentence around it the way you would around 'the knight on d4'" showed the model an
  article-fronted example; the MARKERS block says a rendering brings its own article and never to
  write one directly before a marker, and that `{sharedReply}` / `{sharedMove}` render as
  "the same move" — the engine's strongest reply and the likeliest reply at this rating, or
  the alternative and the most common choice at this rating, coincide. The SAN is already in
  `{strongestReply}` / `{alternativeMove}`; do not put it in the coincidence marker. **New
  Task B prompt digest**. The response schema digest is untouched. The evidence-projection
  digest now enumerates its keys (mirroring the comment counterpart) so adding `sharedMove` /
  `sharedReply` moves that axis. Task A's templates and digest are unchanged.
- **Residue, recorded:** evaluation renderings still open with a figure ("reach an -2.4, Much
  better for Black" was published once, on a non-pinned route), and a model article before bare
  SAN ("as common as the cxd4") remains grammatical-judgment territory the gate stays out of, per
  #344. `{bestMove}` / `{alternativeMove}`
  coinciding when the explored alternative *is* the engine best remains untouched — both are
  required, so withholding cannot apply.

Measured offline first, per #357's lesson:
every recorded Task B generation from `full-run-2026-08-16.jsonl` and
`challenger-2026-08-17.jsonl` re-grounds under the repaired vocabulary
(`tests/coach_turn_marker_replay.rs`) — 5 replay published with no stutter and no doubled article
(the reproduced "with an {evaluationLoss}" case now reads "with a 0.2-pawn margin"), 6 still
reject for naming the withheld `{likelyReply}` on the colliding frozen case. Those records are
not rewritten; `{sharedReply}` is what new authoring uses. Correctness closes on the replay
test. Authored-rate reads later from Language Layer Operational Records; there is no dedicated
live slice.

## What the line-transcription reading settled

Landed 2026-08-29, from a Player reading of staging next to a competing reviewer: ChenChess
comments were reproducing whole SAN lines ("aiming for Nc3 Bg4 e3 e6, but cxd5 Qxd5 Nc3 Qd6 e4
e5 d5 Nb8 …") where the competitor narrated the pieces, and one hosted comment credited an
opening knight development with "You won a knight" — an outcome no fact recorded. Two template
bullets and two seams moved:

- **Task A system template** gains a Voice bullet — a line in FACTS is evidence, not copy; name
  the piece, where it lands, what it aims at, quoting at most the first move or two — and a
  Grounding bullet: never credit an outcome the move did not earn; material and mate claims
  need a recorded capture, payoff, or achievement for this exact move. **New Task A
  prompt digest** (`sha256:1480da4e…409b` in the staging Evaluation Fingerprint,
  re-pinned in `pin_record.rs`). The intent authoring instructions
  (`intent_authoring_context_for`) say the same for the projected plan and its counterplay; those
  travel inside FACTS, so they move no digest.
- **The safe rendering caps its quoted lines.** `safe_intent_sentence` now quotes a line's first
  three SAN with an ellipsis for the rest (`safe_line_opening`, mirrored byte-for-byte by
  `lineOpening` in the browser's `reviewMoments.ts`). The rollback runbook's sentence spans
  ("may have been aiming for …") are unchanged.

Not yet run against the frozen set — the bake-off `run` for this digest is owed before any
verdict on authored quality.

## What the publish-rate collapse settled

Landed 2026-08-30, from a Player reading a review whose every comment was a
safe rendering. The Quality Outbox, grouped by the prompt fingerprint each
generation ran under, shows the publish rate falling from **71%** under
`sha256:bda1365a…` to **21%** under `sha256:ec1f4c1d…` — the fingerprint that
added the piece-naming Voice rule. Staging tracing over 3h28m that day
(`railway logs --service coach-engine --lines 5000`) is per-moment and agrees:
24 grounding rejections, of which **22 were `MissingRequiredMarker`** and 2
`RepeatedMarker`, against 12 completions of which 11 fell back. Not one
rejection was a grounding or hallucination discipline.

The reading: `{playedMove}` is a required **Literal** marker whose rendering is
a bare SAN, and the new rule said "never a bare 'Nd4' standing alone". A model
obeying the rule drops the marker, and the gate discards the comment for the
omission — so a rule written to improve prose switched the Language Layer off
and handed every Player the one renderer that cannot be tuned.

Two seams moved, and neither is a wording preference:

- **The Voice bullet is scoped to the model's own words** and says outright that
  it does not reach a marker. **The MARKERS block gains the converse**: a
  marker's rendering is not the model's prose, `{playedMove}` renders as bare
  notation by design, and dropping a required marker — or paraphrasing its fact
  instead — discards the comment. **New Task A prompt digest**
  (`sha256:d88ba78e…afbc`), re-pinned in `pin_record.rs`; rollback runbook
  annotated.
- **The gate now names the marker it lost.** The three marker disciplines carry
  the marker's name the whole way out — `MarkerViolation` into
  `CommentProseRejection` — so the existing
  `coach_hosted_comment_grounding_rejection` event prints it without a second
  event, and the bake-off record separates candidates by marker rather than only
  by discipline. `UnknownMarker` names nothing, because the offending text is the
  model's rather than ours. The two completion counters are untouched, so the
  fallback rate still reads the same way.

Not yet measured. The publish rate under `sha256:d88ba78e…` is the verdict, and
it is only readable after a staging deploy: count the two
`coach_hosted_comment_authoring_completion` statuses. If it does not recover,
`coach_hosted_comment_grounding_rejection` now names the marker that is actually
missing, and the carve-out was aimed wrong.

## Handed forward
- The refusal reason enums are proposals. They are Player-invisible, but they become an outcome axis
  in Quality Capture.
- Whether the stored comment keeps its marker form beside the substituted text is open.
  #233 freezes comments on first open, so
  re-rendering is not required — but keeping the marker form makes a rendering change auditable
  after the fact.
