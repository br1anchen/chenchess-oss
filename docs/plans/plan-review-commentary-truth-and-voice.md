# Review commentary: make the achievement claim true, then make it read well

2026-08-30. From a Player reading of a review that
again opened "won a knight" on a move that won nothing, after the 2026-08-29
prompt edit was supposed to have stopped exactly that sentence.

This plan supersedes the commentary half of the scratch note
`.claude/plan-commentary-prose-and-firebase-resilience.md`. It keeps that note's
Phase B items that survive the diagnosis below and drops the ones the diagnosis
proves cannot work.

## What the 2026-08-29 fix assumed, and what is actually true

`docs/prototypes/web-language-layer-prompt-templates.md:654` records the earlier
reading as: a hosted comment "credited an opening knight development with 'You
won a knight' — **an outcome no fact recorded**". The fix that followed was a
Grounding bullet telling the model not to write such a claim
(`services/coach-engine/src/language_layer_prompt.rs:139-144`).

Both halves of that reading are wrong, and each one alone is enough to explain
why the sentence survived.

**A fact does record it.** `extract_mechanism`
(`services/coach-engine/crates/pipeline/src/causal_facts.rs:203-256`) walks the
engine's principal variation for the position _before_ the Player moved
(`crates/pipeline/src/rule_extractor/facts.rs:127-133`) and sets
`MechanismPayoff::WinsMaterial { role }` at the first ply where the mover
captures and the running net reaches `WINS_MATERIAL_PAWN_UNITS = 2`
(`causal_facts.rs:10`). `positive_achievements`
(`crates/pipeline/src/rule_extractor/positive_highlights.rs:32-41`) then attaches
that payoff to the played move on one test only — `mechanism.moves[0].uci ==
played_move_uci`. Everything after `moves[0]` is hypothetical continuation that
requires the opponent to cooperate.

**The model never writes the claim.** `{achievement}` is a _required_
own-sentence marker (`services/coach-engine/src/critical_moment_comment.rs:773`),
rendered by the runtime as `format!("You {achievement}.")`
(`critical_moment_comment.rs:1321-1323`) from `positive_achievement_text`
(`:1253-1274`, the `won a {role}` arm at `:1267`). Omitting a required marker is
`MissingRequiredMarker` and discards the whole comment
(`services/coach-engine/src/language_layer_markers.rs:334-336`). So on a Positive
Highlight whose first achievement is a `TacticalPayoff`, "You won a knight."
appears in the published text no matter what the prompt says. The Grounding
bullet is addressed to prose the model does not author.

### Measured: a dead-even trade reads as winning a piece

Run against `causal_facts::extract` (temporary test, since reverted):

```
FEN   4k3/8/2p5/3n4/1N6/8/8/4K3 w - - 0 1     (White Nb4, Black Nd5, Black pc6)
PV    b4d5 c6d5                               (Nxd5 cxd5 — dead even)
=>    PAYOFF = WinsMaterial { role: Knight }
      MOVES  = [LineMove { uci: "b4d5", san: "Nxd5" }]
```

The payoff locks in at the capture ply because the loop guards on
`payoff.is_none()`, and `moves.truncate(end + 1)` then discards the recapture
that settles the exchange. The running net is a _prefix_ sum presented as a
verdict. Any capture of a knight, bishop, rook, or queen by the mover clears the
threshold of 2 on its own, so **every recaptured trade the engine endorses is
reported as winning that piece**.

The same rule also mislabels genuine tactics: the existing five-ply test
(`causal_facts.rs:486-513`) sacrifices a pawn to win a rook and reports
`WinsMaterial { Rook }` — net +4, not +5. "Wins the rook for a pawn" is the true
sentence; the contract cannot express it.

### The claim is mandatory on all three surfaces

One wrong fact, three independent renderers, none of which can decline it:

| Surface            | Site                                                                                                          | Form                                                                                       |
| ------------------ | ------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Web hosted prose   | `critical_moment_comment.rs:773`, `:1321-1323`                                                                | required own-sentence marker → "You won a knight."                                         |
| Web safe rendering | `critical_moment_comment.rs:762`, mirrored at `apps/central-host/src/review-session/reviewMoments.ts:397-426` | `"{grade}: {san} {achievement}."`                                                          |
| CLI Coach Skill    | `skills/chenchess-coach/review-writing.md:54`                                                                 | required opening shape; the worked example at `:41` is literally "Good: Qc1+ won a queen." |

It is also registered as `CriticalMomentFactualClaim::PositiveAchievement`
(`critical_moment_comment.rs:766-769`), so the grounding ledger treats it as
verified fact, and `objective_reason`
(`crates/pipeline/src/rule_extractor/classification.rs:145`) can grant
`ExactBestMajorAchievement` on the strength of it — a PV-derived payoff can be
the sole reason a move is graded positive at all.

## The caching question

The re-authoring path the Player expected does exist and landed today
(`1d87e24b`), but it is a _prompt-version_ check, not a quality check.

`is_stale_web_artifact` (`critical_moment_comment.rs:199-206`) compares only the
stored `prompt_digest` / `response_schema_digest` against what this build
compiles. `web_opening_comment`
(`services/coach-engine/src/review_session_processor/session.rs:936-951`) turns
that into `Absent` / `Current` / `Stale`, and the read path
(`services/coach-engine/src/review_session_processor/readiness.rs:701-708`)
re-authors only `Stale`.

Two consequences:

1. Because the prompt digest _did_ move on 2026-08-29, a stored comment would
   have been re-authored on open. It would then have rendered the same required
   marker from the same wrong fact. The Player's "still cached?" reading is
   understandable but the mechanism is not caching — it is a claim the runtime
   substitutes.
2. A **safe rendering carries the compiled digests** (stamped in
   `critical_moment_comment/hosted_author.rs:135-136`), so a moment that fell
   back is classified `Current` and served forever. The code says so at
   `readiness.rs:823-833`: "an outage would cost the Player real coaching
   permanently, because the rendering carries the compiled digests and no later
   open would try again." Only a prompt edit or a `REVIEW_ANALYSIS_GENERATION`
   bump ever revisits it.

### Measured: the current prompt collapsed the publish rate

Quality Outbox (`coach-quality/captures`, identity-free), staging, task
`comment`, grouped by the prompt fingerprint each generation ran under. The
fingerprints are in commit order, read from `pin_record.rs` history:

| Fingerprint        | Landed by                         | Published | Rejected | Publish rate |
| ------------------ | --------------------------------- | --------- | -------- | ------------ |
| `sha256:7a350e29…` | before `27d2f1be`                 | 0         | 3        | 0%           |
| `sha256:1480da4e…` | `27d2f1be` — the grounding bullet | 0         | 10       | 0%           |
| `sha256:cfb91403…` | `532df14f`                        | —         | —        | no captures  |
| `sha256:bda1365a…` | `662f10f1` — own-sentence frames  | 5         | 2        | **71%**      |
| `sha256:ec1f4c1d…` | `3054acb7` — "name the piece"     | 8         | 30       | **21%**      |

Caveat: a sample, not a census. Captures exist only for accounts with
`captureEnabled`, which on staging is the two test accounts, and the counts are
per _attempt_ (`1..=2` per moment), not per moment.

Staging tracing over 3h28m today (`railway logs --service coach-engine
--environment staging --lines 5000`, window 11:13Z–14:41Z) agrees and is
per-moment:

```
24  coach_hosted_comment_grounding_rejection   — 22 MissingRequiredMarker, 2 RepeatedMarker
12  coach_hosted_comment_authoring_completion  — 11 safe_rendered, 1 published
```

Two things to take from this. **Not one rejection is a grounding or
hallucination discipline** — the gate is not catching invented facts, it is
catching the model failing to emit a marker it was told to emit. And **92% of
moments in that window fell back to the deterministic template**, which is the
one renderer guaranteed to print "won a knight".

**Inferred, not measured — the likely marker.** `MarkerViolation::MissingRequiredMarker`
does not record _which_ marker, so this is a hypothesis. `{playedMove}` is a
required **Literal** marker rendering a bare SAN
(`critical_moment_comment.rs:772`, table at
`docs/prototypes/web-language-layer-prompt-templates.md:63`). The Voice rule
`3054acb7` added says: _"never a bare 'Nd4' standing alone"_. A model obeying
that rule stops emitting `{playedMove}`, and the gate rejects it for the
omission. The publish rate dropping from 71% to 21% on exactly that commit is
consistent with it. Confirming this needs the marker name in the rejection
event — see C1.

## Phase A — make the achievement true

Blocking. Nothing about voice matters while the claim is false.

**A0. Decompose the renderers first.** `critical_moment_comment.rs` is already
1633 lines, and A2/A3 add a three-way split to `positive_achievement_text` plus
setup-versus-accomplishment framing to `achievement_sentence` and the safe
rendering. That is exactly the growth that turns a large file into an unreadable
one. The deterministic renderers (`positive_*`, `played_outcome_sentence`,
`improvement_correction_text`, `residual_consequence_text`, `neutral_reason_text`,
`terminal_outcome_text`, `teaching_takeaway`, `piece_role_text`) are a cohesive
group with no dependency on the gate, and they already have a browser twin in
`reviewMoments.ts` that is organized exactly that way. Move them to a
`critical_moment_comment/rendering.rs` before A2 lands, so the achievement change
is a small diff in a small module rather than more sprawl in a large one.

**A1. Settle the exchange before claiming it.** In `extract_mechanism`, keep
accumulating `net_pawn_units` to the end of the principal variation and record
the settled net. Promote a mover capture at ply _i_ to a payoff only when the net
is at least `WINS_MATERIAL_PAWN_UNITS` at ply _i_ **and never falls below it for
the remainder of the line**. `Nxd5 cxd5` settles at 0 and yields no payoff.

**A2. Carry magnitude beside role.** Change `MechanismPayoff::WinsMaterial {
role }` to `WinsMaterial { role, net_pawn_units }`
(`crates/pipeline/src/causal_facts.rs:63-68`) and mirror it into
`GameReviewMechanismPayoff` (`crates/contract/src/game_review.rs:444-450`). The
renderer then distinguishes three cases it currently collapses:

- `net >= piece_value(role)` → "won a knight" (the piece came free)
- `0 < net < piece_value(role)` → "won the rook for a pawn"
- `net <= 0` → no payoff (A1 already suppresses it)

**A3. Stop crediting the line to the move.** Add the payoff depth to
`PositiveHighlightAchievement::TacticalPayoff`
(`crates/contract/src/game_review.rs:280-296`), and split three ways:

- depth 0 — the played move earned it; keep the past tense.
- depth > 0 that the opponent cannot avoid — render as setup, "Nb4 sets up
  winning a knight", never as accomplishment.
- depth > 0 that the opponent can avoid — emit no achievement at all. A line the
  opponent has to cooperate with is a preference, not something the Player
  earned, and the moment may correctly stop being a Positive Highlight.

**`forcing_index` cannot be that gate — correction to the 2026-08-30 decision.**
The decision as first written said to reuse `forcing_index`. It does not mean
what its name suggests. It is ours, not the engine's: `extract_mechanism`
(`crates/pipeline/src/causal_facts.rs:256-264`) scans the mover's plies of the
truncated line for the first SAN containing `x`, `+`, `#`, or `=`. That is a
string test for "a capture, check, mate, or promotion", not a test that the
opponent had no choice.

Worse, it is vacuous here. A `WinsMaterial` payoff is by construction a mover
capture, whose SAN contains `x`, so the scan always finds at least the payoff
ply itself — the `else { return Ok(None) }` at `causal_facts.rs:262` is
unreachable for exactly the payoffs A3 is about. Gating on it would be a no-op
that reads like a safeguard.

**"Only legal move" is also the wrong test.** It is what the domain already
means by forced — `mechanically_forced = legal_moves <= 1`
(`crates/pipeline/src/rule_extractor/classification.rs:35`) — and it is far too
narrow. A reply is equally compelled when it is the only move that avoids mate,
or the only move that avoids dropping material for nothing. Those are the cases
that actually occur.

Both reduce to one question about the opponent's reply: **is the gap between
their best and second-best move decisive?** Two thresholds over the same
comparison:

- second-best is mate against them while best is not — the only move avoiding
  checkmate;
- second-best is worse than best by a piece or more — the only move avoiding
  material loss with no compensation. Reuse `WINNING_CENTIPAWNS` /
  `FAVORABLE_CENTIPAWNS` (`causal_facts.rs:11-14`) rather than inventing a
  threshold.

The capability exists: `analyze_multi_pv` returns ranked variations
(`crates/pipeline/src/engine_analysis.rs:70-83`). It is not wired to Rule
Extraction — today only Decision Explanation
(`services/coach-engine/src/review_facts/decision_explanation.rs:125`) and the
evaluation recorder call it — so this is new engine work in the hot path.

Keep it to **one** multi-PV call, at the opponent's first reply. That is the
branch point where they can still duck out of the line; by later plies the
material is usually already committed. A per-ply sweep would cost a Stockfish
call per opponent ply for a claim that only needs its branch point tested.

Record the verdict as its own field. Leave `forcing_index` alone — the CLI skill
points the Player at it (`skills/chenchess-coach/review-writing.md:57`) and the
prompt ships it in FACTS, so its meaning must not change under them in the same
commit.

**Why the line still needs this even though it is the engine's.** A principal
variation is best play for _both_ sides, so the payoff is not something the
opponent "cooperates" with. What a PV does not say is whether the opponent had
an equally good alternative that avoids the material and concedes elsewhere.
When one exists, "sets up winning a knight" is the wrong sentence, and the
multi-PV gap at the branch point is exactly what distinguishes the two.

**Shipped 2026-08-31, in the narrow form.** The three-way split and the
multi-PV avoidability test were not built. What landed is a depth gate on
material alone: `positive_achievements` credits a `winsMaterial*` payoff only
when the mechanism's payoff ply _is_ the played move, and leaves `Mate`,
`Promotion` and `QueenExchange` alone. Depth needs no new contract field —
`moves` is truncated at the payoff ply, so its length already carries it — so
this is a behaviour change with a `REVIEW_ANALYSIS_GENERATION` bump and a corpus
re-baseline, and no contract break at all.

The corpus decided it. Of 20 payoff-bearing Positive Highlights, 15 sat deeper
than the played move, but only 6 of those were material; a blanket depth gate
would also have deleted `gotham-ep27` ply 96, a twenty-ply forced mate that is a
real achievement rather than a misattributed one. After the gate, **no depth > 0
material payoff remains anywhere in the corpus**, which is the whole of the
reported defect.

The multi-PV test keeps its case on paper and lost its evidence: its only value
is preserving deep payoffs as "sets up" prose, and nothing yet shows those read
well, because that sentence has never been rendered. Revisit with the wording
drafted rather than before it — an engine call in the hot path is a large price
for prose no one has read.

**Drafted 2026-09-01, and it retires the rest of A3.** The corpus holds 23
mechanisms whose material payoff sits deeper than the mechanism's first move.
Nine of them start at a move the Player did not play, so `credited_payoff` never
sees them — its `moves.first().uci == played_move_uci` filter excludes them
before depth is consulted. That leaves **14 candidates, and every one of them is
already a Positive Highlight on another achievement**: thirteen on
`capturedPiece`, one on `advancedPassedPawn`. So the remaining A3 work moves no
moment's classification anywhere in the corpus. The classification defect was
the whole of A3's value, and the shipped depth gate is all of it.

What is left is prose, and the prose cannot reach the Player either.
`positive_achievement_text(&qualification.achievements[0])`
(`critical_moment_comment.rs:989`) renders the *first* achievement only, and
`positive_achievements` pushes the effects before the payoff — so a re-admitted
"sets up" achievement lands at index 1 or 2 in all fourteen and is never
rendered at all. Reading it drafted against the corpus is the last argument
against it:

| Played | Depth | Rendered today | Would-be second achievement |
| --- | --- | --- | --- |
| `Bxe5` | 2 | You captured the knight on e5. | Bxe5 sets up winning a bishop |
| `Rxe3` | 8 | You captured the rook on e3. | Rxe3 sets up winning a pawn |
| `Qxa3` | 14 | You captured the pawn on a3. | Qxa3 sets up winning a pawn |
| `Qxd5` | 12 | You captured the bishop on d5. | Qxd5 sets up winning a queen |

A Player who has just been told they captured a rook does not also need to hear
that the move sets up winning a pawn eight plies later. So the three-way split
and the multi-PV avoidability test are **withdrawn**, not deferred: they would
add a contract field, a Stockfish call to the hot path, and a renderer branch,
to change nothing any Player reads. Reopen only if a moment ever reaches a deep
material payoff *without* an effect-derived achievement ahead of it — none does
today, and the depth gate is what makes that true.

**A3 is a classification change, not only a rendering one — and it is what
stops opening moves being highlighted.** Quiet opening moves are currently
flagged as Positive Highlights, and the achievement they carry is either a
too-deep PV gain or nothing a Player would recognise. The cause is structural,
not a threshold: `positive_achievements`
(`crates/pipeline/src/rule_extractor/positive_highlights.rs:9-49`) has exactly
four producers — a `CapturedPiece` effect, an `AdvancedPassedPawn` effect, a
terminal `CompletedCheckmate`, and the PV-derived `TacticalPayoff`. The first
three all require the played move to have _done_ something. So a quiet
developing move can reach Positive Highlight by **one path only**: the PV
payoff. Kill the unearned payoff and the opening false positives go with it,
because nothing else can produce an achievement for a move that captured
nothing.

That is why the third A3 case must drop the achievement rather than merely
soften its wording. `!achievements.is_empty()` gates the whole classification
(`classification.rs:53`), and the same list grants
`ObjectiveExcellenceReason::ExactBestMajorAchievement` (`:145`) — which an
opening move satisfies easily, since playing the engine's top move is what
theory _is_. One corrected set feeds classification, the excellence reason, and
the rendering; no split "qualifying versus narratable" set, and no second rule.

**Do not reach for a move-number or opening-phase gate.** There is no book or
theory signal in Rule Extraction to hang one on — the only opening concept is
`OpeningPrinciple::OccupyTheCenter`, computed for `move_number == 1` alone
(`rule_extractor/facts.rs:228-231`) — so a phase gate would be a new invented
threshold. It would also suppress the most instructive moments this Player has:
at 1200, a piece hung on move six and taken is real coaching, and that move
qualifies honestly through its `CapturedPiece` effect.

**While in this file:** `PositiveHighlightAchievement::PreservedForcedMate`
(`crates/contract/src/game_review.rs:280-296`) has no producer.
`positive_achievements` never constructs it, and the only `PreservedForcedMate`
that is ever built is the unrelated `ObjectiveExcellenceReason`
(`classification.rs:144`). The renderer still carries an arm for it
(`critical_moment_comment.rs:1265`). Delete the variant with the A2/A3 contract
break rather than leaving a dead case in a closed enum.

**A4. Propagate.** `positive_achievement_text`
(`critical_moment_comment.rs:1253-1274`), `achievement_sentence` (`:1321-1323`),
the browser twin (`reviewMoments.ts:397-426`, which must stay byte-compatible),
and the CLI skill's opening shape and worked example
(`skills/chenchess-coach/review-writing.md:41,54`).

**A5. Re-check the classifier.** `ExactBestMajorAchievement`
(`crates/pipeline/src/rule_extractor/classification.rs:145`) may now grant fewer
Positive Highlights. That is the correct outcome — a move that won nothing was
never a highlight on those grounds — but it changes the adaptive selection and
therefore the `Synthet1` seven-moment baseline.

Re-baseline rather than blocking, but make the selection delta a _reviewed
artifact_: run the fast evaluation before and after, diff the selected moment
set, and carry that diff in the PR body. The diff is the evidence Phase A
worked. An instructive moment disappearing is the signal that A1/A3 went too
far. The required-field bump on `MechanismPayoff` needs the jq corpus bootstrap
before `gotham refresh-explanations`.

**Measured on the corpus, 2026-08-30.** `gotham-ep21-164113796562` ply 34 is the
Player-facing shape of the defect, found by the re-baseline rather than by
looking for it. The played move is `Qc7` — a quiet queen move that captures
nothing — and it is stored as a Positive Highlight whose only achievement is
`tacticalPayoff { winsMaterial: knight }`. Its mechanism line is
`Qc7, c4, Qxe5, …`: the capture arrives two plies later, in the engine's
continuation.

**Corrected 2026-08-31.** This paragraph claimed the moment stops qualifying
under A1. It does not, and the accepted corpus said so for a day: A1 settles the
_net_ and ply 34's net stays positive to the end of the line, so `Qc7` kept its
knight. A1 and A3 fix two different axes — settled net, and attribution depth —
and only the first had shipped. Five more moments held the same shape
(`gotham-ep21` ply 28, `gotham-ep33` plies 31 and 33, `gotham-ep27` ply 48,
`session-long` ply 16). A3 is what removes them; see the decision below.

The gate reports this as `UnusedMultiPv(34)` from
`repository_corpus_matches_all_pinned_baselines` — a recording for a ply that is
no longer a comparable moment. That is the check working: its own doc
(`pipeline_evaluation/multi_pv_recording.rs:185-189`) says a corpus that drifts
into a different Critical Moment set must "fail the gate loudly rather than
silently dropping the comparison it never recorded". Treat a drop as evidence to
read, never as noise to re-pin past.

**Use `accept-live-evaluation`, not `record-multi-pv` + `accept-evaluation`.**
The live command re-captures Candidate Evidence and the comparison searches in
one pass, and `pipeline_evaluation.rs` says why beside the call: "Leaving the
previous recording beside freshly captured evidence would pin a gap to a search
of a Position that may no longer be a Critical Moment." Re-recording alone
computes its comparable set from stale evidence, so it disagrees with the accept
path permanently — it reports an unused recording, you prune it, and it then
reports a missing one. `Synthet1` happened to converge under the wrong pairing
and its baseline is accepted; the repository corpus did not.

Two traps around it. The live command refuses to run when `STOCKFISH_PATH`,
`STOCKFISH_DEPTH`, or `MAIA_BASE_URL` is set, which `./tooling/nix-develop`
does — clear them with `env -u`. And it prints `runtime failure: …` while
exiting **0**, as does `runtime doctor`, so a gate reading the exit code will
call a failed acceptance a pass.

**Was blocked 2026-08-30; cleared 2026-08-31.** This paragraph previously said
the runtime could not be reinstalled without a release-workflow manifest,
several gigabytes of Stockfish archive, and a Maia image pull. That was wrong
about the cost. The block was two much smaller things:

1. A **stale schema stamp** — the installed runtime advertised `schema 3`
   against a checkout pinning `RUNTIME_MANIFEST_SCHEMA_VERSION = 1`.
   `~/.config/chenchess/runtime.json` now reads `schemaVersion: 1`.
2. A **stopped Docker daemon**, which surfaces as
   `runtime failure: runtime is unhealthy: Maia service is not running`.
   `chenchess runtime doctor` exits **7** on this, not 0 — do not weaken a gate
   on the exit-0 claim below without re-measuring it.

Neither needed a download. The pinned Maia image
`maia-runtime@sha256:ab3b6dc1…` was already in the local
image store, and the canonical Stockfish
(`bc0cac90…`) was already on disk at
`~/.local/share/chenchess/units/0.2.0-local-coach.4/bin/stockfish`. Bring-up is
`open -a Docker` then `chenchess runtime maia-start`.

With the runtime healthy, `accept-live-evaluation --corpus-dir
services/coach-engine/evaluation/corpus` re-accepted nine cases:
`beginner-below-threshold`, `gotham-ep21-164113796562`,
`gotham-ep24-165723357366`, `gotham-ep27-166213489290`,
`gotham-ep33-169724120336`, `selected-forced-recapture`, `session-long`,
`session-short`, `tactical-white-human-likely`.

The bake-off's G5 entry still needs a re-point rather than a retirement — the
"Positive / good / tacticalPayoff" combination it exemplifies is still produced
in sixteen moments, so retiring it would leave a coverage hole that G7 and G8
(both _great_) do not fill — and its target can now be chosen from a corpus that
has actually been re-accepted.

`WINS_MATERIAL_PAWN_UNITS = 2` stays. The defect was naming a role the net does
not support, not the threshold; once A2 lands, a settled +2 says "two pawns
ahead" and never "a knight". Revisit only if this diff shows a noisy highlight
set.

Cost: a `GameReview` contract break (pre-production, so a version bump rather
than a migration), a `REVIEW_ANALYSIS_GENERATION` bump so stored reviews
re-derive, a comment prompt digest move, and a corpus baseline refresh with
re-pinned digests in both places (`gotham refresh-explanations`, then
`refresh-comparison-mirror --write` / `--check`).

Regression tests: the even-trade FEN above asserting _no_ mechanism; the existing
five-ply test asserting `net_pawn_units == 4` and the "for a pawn" rendering; a
depth-> 0 case asserting setup wording.

## Phase B — stop serving a dead fallback forever

`is_stale_web_artifact` ignores `self.outcome`, although the discriminant is
already computed one line away (`session.rs:943-947`) and already consulted by
the anti-clobber guard (`readiness.rs:834-854`).

Add a fourth case to `WebOpeningComment`: a stored `SafeRendered` under the
current digests is _retryable_, not `Current`. Bound it with a retry counter on
the provenance, capped at one — not once per `REVIEW_ANALYSIS_GENERATION`. The
generation bump is a global lever that re-derives every stored review, so tying
fallback retry to it means a single bad comment cannot be repaired without
paying for everything else. A counter is local, observable, and composes with
Phase C: the moment the publish rate recovers, every stored fallback takes its
one retry and converts. The prompt-digest staleness path still handles later
prompt moves, so one is enough.

Convergence is the hazard the earlier plan flagged
(`docs/plans/plan-web-commentary-prompt-staleness.md:143-146`); a counter
answers it by construction. Keep the existing rule that a fresh fallback never
supersedes prose the Language Layer actually authored.

**This is what repairs reviews the Player has already opened.** The game that
prompted this plan
(`game-import:2bb7de7f…:45c2c93d…`) has eight stored safe renderings and zero
published comments; without Phase B, neither Phase C nor Phase A ever reaches
that page.

## Phase C — recover the publish rate

This is the highest-value phase per unit of work, and it is nearly free. A
92% fallback rate means the Language Layer is effectively switched off: the
Player is reading a template, so no amount of Phase D voice work reaches them.

**C1. Name the marker, in the event and in the capture.** Two halves.

The _live_ half: carry the marker's name on the three marker disciplines of
`MarkerViolation` and `CommentProseRejection`, so the existing
`coach_hosted_comment_grounding_rejection` event prints it and the bake-off
record separates candidates by marker rather than only by discipline. No second
event, and the two completion counters that read as the fallback rate stay as
they are. `UnknownMarker` names nothing: the offending text is the model's
rather than ours. The record's `rejection` field is read by jq and never
deserialized, so a shape change fails as zero matches — move `RECORD_VERSION`
with it and lock both shapes in a test.

The _durable_ half: put the full-width rejection on the Quality Capture, so the
next diagnosis needs no Railway access. This is the half that makes C1 "the
instrument every later decision reads", and it is the harder one. Two obstacles,
both real. `EvaluationFingerprintObservations`
(`services/coach-engine/src/evaluation_fingerprint.rs:98-113`) is shared by all
three hosted tasks while `CommentProseRejection` belongs to one, so putting it
there repeats the drift its `steps` field already shows ("HostTurn per-step
observations. Empty for Comment and Coach Turn"). And the full-width reason is
not reachable at the capture site: `ground_draft` narrows to the wire enum
before `record_capture` runs (`critical_moment_comment.rs:311-322`). Settle the
boundary before writing the field — see Still open.

**C2. Reconcile the Voice rule with the required Literal markers.** Carve out
the markers; do **not** revert `3054acb7`. A revert costs the piece-naming rule,
which is the TakeTakeTake direction this plan wants, and it still needs a
deploy — so it is not cheaper, only worse. The rule's intent is sound and it
simply fails to exempt what the model does not control: a Literal marker's
rendering _is_ a bare SAN. One clause fixes it — the piece-naming rule governs
moves the model names _in its own words_, while `{playedMove}` is how it names
the move and the runtime owns that rendering. State it in the MARKERS block,
where the model is already told markers are runtime-substituted, not in Voice.

Ship C1 and C2 in one deploy, so that if the rate does not recover, C1's data
names the marker that is actually missing.

**C3.** Re-measure the publish rate on the next fingerprint before any further
wording change. Make it a **post-deploy** verification step, not a pre-merge
gate — a publish rate cannot be measured without live traffic. Put a threshold
in the release runbook (roll the prompt back below ~50% over the first fifty
completions) and read it by counting the two
`coach_hosted_comment_authoring_completion` statuses, which needs no Firestore
round trip.

Pair it with a pre-merge check that _is_ free and deterministic: fail when the
digest in `pin_record.rs` moves without a recorded bake-off run beside it. That
one has to be local, because nothing runs in CI here
(`turbo-inputs-allowlist-stales-gates`). The scratch note's owed bake-off
belongs after C1/C2 **and** after Phase A: measuring prose quality while most
generations are discarded for a marker omission measures nothing, and Phase A
changes the FACTS payload, which invalidates any run before it. One run, after
A and C, before D.

## Phase D — the voice gap against TakeTakeTake

Only after A–C. Comparing
`services/coach-engine/evaluation/comparisons/Synthet1/taketaketake-review.md`
with what ChenChess emits, the gap is not prompt wording — it is that the
competitor states consequences ChenChess has no fact for, and the prompt
correctly forbids inventing them ("Do not reason past the facts … unless FACTS
states it", `language_layer_prompt.rs:137-138`).

TakeTakeTake: _"Placing a pawn on a6 would have forced the knight out of squares,
eventually winning the piece for a pawn."_ — better move, its destination, what
it does to a named enemy piece, and the settled material verdict.

ChenChess already ships the raw material into FACTS (`project_facts`,
`language_layer_prompt.rs:677-763`): `engine.engineLine`, `refutationLine`,
`mechanism.moves` + `forcingIndex` + `payoff`, `playerIntent.hypothesis` /
`projectedPlan` / `objectiveCounterplay`. The allowlist already admits every SAN
in those lines and every square in the PV
(`critical_moment_comment.rs:1080-1105`). What is missing is _semantics_ — the
model is shown a line and told not to transcribe it, with nothing else to say
about it.

Derive and add, so narration stops being invention. One issue per fact family —
each is an independent contract, prompt, and gate change that can land on its
own — in this order:

1. **The opponent's resource**: the first move of `lines.refutation` and what it
   restores, which is TakeTakeTake's "Now White can reinforce the center with c3,
   and the advantage evaporates." First because `lines.refutation` is already in
   FACTS and already in the literal allowlist; the only missing piece is a fact
   stating what that move restores. Smallest change, largest TakeTakeTake-shaped
   gain. It must still be fact-backed — a prompt-only version of this is exactly
   the move that has failed twice.

   **The fact shipped 2026-09-01; the sentence did not.** `refutationEffects`
   derives what the opponent's first reply does by pointing
   `causal_facts::played_move_effects` — the rule that already computes the
   played move's effects — at that reply, and projects it as `opponentResource`
   with its squares admitted to the allowlist. Measured on the instrument: the
   fact alone moved the publish rate 30 → 33 of 43 and produced the sentence in
   **0 of 43** generations; adding one clause to an existing Voice bullet
   naming the key produced it in 2 of 43 and cost five shapes (33 → 28), so the
   clause was reverted and the prompt template is byte-identical to before.
   Four prompt edits in this area have now each cost about five shapes. The
   remaining mechanism is a **marker** — the way every other fact reaches the
   Player — which is the next increment and a larger one, because a marker adds
   a slot, a ledger claim, and a Fact Shape axis. Evidence:
   `evaluation/prose-regression/README.md`.

   **`{opponentResource}` followed, 2026-09-01.** An optional marker on the
   Positive and Improvement paths, rendering "Black can answer with e5, hitting
   the knight on f3" from the fact above, carrying its own
   `CriticalMomentFactualClaim::OpponentResource` so the ledger records what the
   prose asserts, and rendering only the first effect the way `{achievement}`
   renders only the first achievement. Not offered on Neutral: one line is that
   path's only length discipline and an optional clause works against it.

   **It works.** Nine of 54 generations used the marker and seven published,
   against 0 of 43 for the fact alone and 2 of 43 for the prompt clause. The
   sentence the plan went looking for now exists: "Black can answer with Qxd5,
   taking the queen on d5, and the advantage…". And it costs nothing on the work
   that already existed — across the 28 Fact Shapes present in both runs,
   published went 22 → 23. The whole-run rate reads 33/43 → 38/54 only because
   the marker creates 26 new authoring problems, which publish at 15 of 26.

   Two costs, both real. The corpus went **44 → 56 Fact Shapes** and the observed
   population **43 → 69**, so thirteen more ladder Games had to be minted to
   close the census. And the census fell to 21 filled of 43 until the ladder was
   re-reviewed, because a 142-Game snapshot on disk cannot exhibit a shape whose
   marker did not exist when it was written. That is now recorded in ADR 0062 as
   an operating rule: a change to the fact surface plans its ladder re-review
   in, not after.
2. **Destination and target**: the moving piece and landing square of the best
   line's first move, and the enemy piece it attacks or defends. This is the
   "widen the allowlist to occupied squares" item from the scratch note (owed #4),
   but expressed as a _fact_ rather than a bare literal permission — a permitted
   square the model has no fact about is still an invitation to hallucinate.

   **`{moveTarget}` followed, 2026-09-02.** An optional marker on the Positive
   and Improvement paths carrying `CriticalMomentFactualClaim::MoveTarget`,
   reading `GameReviewCriticalMoment::move_target()`, the one door. Which move
   is the fact's to say. A Positive highlight reads the _played_ move, whose
   capture `{achievement}` already names, so only a piece it newly attacks is
   left: "your move also hits the queen on d5". An Improvement opportunity
   reads the _better_ move, which nothing narrates beyond its notation, so its
   first capture or attack is the target: "the better move takes the pawn on
   c4", "the better move hits the queen on f3". Neither rendering opens with
   notation, because an `Anywhere` marker is capitalised at a sentence start
   and "E4" is not a move. The allowlist widens by exactly the target's
   square, through the fact that names it, which is the width of the claim.

   The better move's half needed a fact stored Game Reviews did not carry.
   `bestMoveEffects` points `first_move_effects` — the rule that already
   derives the played move's effects and the opponent's reply — at the best
   line's first move from the pre-move position. The played move's half reads
   `effects`, already on every stored review. "Moving piece and landing
   square" is what `{betterMove}`'s notation already says, so the fact adds
   only the target. "Defends" is derived by no rule and is left out rather than
   guessed.

   **Two deterministic queries over the 142-Game ladder set its scope**, with
   the effect rule replicated in chessops and agreeing with the stored effects
   on 720 of 721 moments.

   _The played move's target was already derived and already unreadable._
   `AttackedPiece` sits in the effects of **214 of 653** Positive moments — 175
   of them beside a capture — and `positive_achievements` drops it, so
   `{achievement}` could never say it. That is item 3's finding again: a fact
   the Player never heard because no rendering reached it.

   _The better move captures or attacks something in 37 of 68 Improvement
   moments_ — 19 captures, 18 attacks — and `lines.best[0]` is the better move
   in 68 of 68, so the sentence attributes correctly. Two passed-pawn pushes
   are not targets and are declined.

   **Priced before it was built, as item 3 asked.** Every Improvement moment
   already offers six markers or more (7 markers: 24 moments, 8: 31, 9: 12),
   so on that path the marker lands above the saturation line by construction.
   Of the 214 Positive moments, 45 sit at four or five markers, 109 at six,
   and 60 at seven or eight. The expectation set going in was the population
   rate — about 86% on new shapes — and zero collateral on the shapes that
   already existed.

   **The census moved twice, and the second move is the one ADR 0062 wrote
   down.** The played move's half reads stored data, so the corpus exhibited
   its shapes at once; the better move's half reads a field no stored review
   carried, so until the ladder was re-reviewed the census reported the
   corpus's own Improvement Exemplars as unfilled against shapes the ladder
   could not yet show — 104 observed, 79 filled, 25 unfilled. One
   `gotham review --force` later (142 Games, the same 721 moments, about
   thirty minutes) it read **115 observed, 94 filled, 21 unfilled**, and
   `search` named a 19-Game covering set. One `accept-live-evaluation` pass
   over those seeds closed it: corpus **47 → 66 cases**, census **115 filled,
   0 unfilled, exit 0**. The prompt template is byte-identical again; the
   vocabulary is compiled per moment.

   **Measured 2026-09-02: 115 shapes, 95 published, $0.057.** The run needed
   the pinned route repaired first — Google moved Gemini 3.x's zero-retention
   endpoint to the `flex` service tier, so `pin-record.json`'s
   `google-vertex/global` matched nothing and every generation returned a 404
   under `provider.zdr`. Same model, same guarantee, new tag.

   *Zero collateral, on the strongest control this instrument has offered.*
   The corpus grew and the ladder moved, so `resolve` re-pointed 20 of the 65
   shared shapes onto different Exemplars. Splitting on that: the **45 shapes
   whose prompt was byte-identical to the previous run moved not at all** —
   40 published before, 40 after, no shape changing verdict in either
   direction. The 6 shapes that did move all sit among the 20 re-pointed ones
   (net −4), which is a different chess position rather than a regression, and
   is the price ADR 0062 already names for growing coverage.

   *The marker publishes at the population rate.* Its 50 shapes published
   **41, or 82%**, against the 86% the saturation table predicted and the 100%
   no shape at this marker count has ever managed. It is written into 29 of
   the 50 drafts it is offered in, and those shapes stand for 251 of the
   ladder's 727 moments. The sentences are the ones this phase went looking
   for:

   > The bishop to `{betterMove}` was stronger because **the better move hits
   > the queen on h4**, bringing evaluation to `{bestEval}`.

   > `{playedMove}` is a good move and a notable find for players at your
   > rating, as it claims the rook on d8 and **your move also hits the pawn on
   > c7**.

   *Two findings for whoever goes next.* `betterMove` joins `playedMove` as a
   dropped-required-marker class — 4 rejections against 11 — which is the
   standing discipline appearing on the Improvement path for the first time.
   And the Positive rendering carries its own subject, so a model that puts it
   in a predicate slot writes "c3 is your move also hits the rook on d2". That
   is `{difficulty}`'s problem exactly, and its answer is the same: a `Shaped`
   form with a clause beside the sentence. Both are rendering-side, both are
   cheap, and neither should be changed without a run to price it.

   **Priced 2026-09-02, and neither pays.** The pricing needed no run — the run
   record carries `authoredText`, `publishedText` and `observedMoments`, so both
   are queries. The `betterMove` requirement is genuinely redundant, because
   `{decisionCue}` is required on every Improvement moment and names the move in
   both its forms, and 32 of 32 Improvement generations used it; dropping the
   requirement publishes all four refusals and costs `+betterMove` in every
   Improvement Fact Shape id. It is worth **6 of 727 ladder moments**. And the
   `Shaped` form is not available for the Positive target at all: `Seam` reads
   standalone against embedded, not predicate against coordinate, so the one
   clause it can author is shared by the 2 broken slots and the 8 working ones —
   the participle that fixes the copular slot breaks two that read correctly
   today and leaves the infinitive complement broken. That break is worth **2 of
   727**. Both sit under the 2.6% tail this phase already declared not worth a
   measured run. Evidence: `evaluation/prose-regression/README.md`.

   **Phase D has no remaining rendering-side increment**, and the one fact
   family it never derived does not pay either. "Defends" was sized the same
   way, over the same ladder: the replica replays all 721 moments with no SAN
   mismatch and reproduces the stored `AttackedPiece` effects on 653 of 653
   Positive moments. A move newly defends *something* in 288 of 721 moments and
   something the opponent attacks in 96, but applying the mirror of the filter
   `AttackedPiece` already carries — the cheapest attacker worth no more than
   the piece — leaves **17 of 721**, and the strictest reading, where the move
   supplies the only defender, leaves **19**. Both are the 2.6% tail already
   declared not worth a run. The 96 is bought by dropping that filter, and then
   the fact is incidental: captures whose landing square happens to bear on a
   friendly pawn, 26 of them already carrying `{moveTarget}`. Evidence:
   `evaluation/prose-regression/README.md`.

   **Phase D is therefore complete.** Three fact families shipped
   (`{opponentResource}`, `{moveTarget}`, `{materialVerdict}`), both rendering
   follow-ups priced out, and the last candidate sized and declined.
3. **Settled material verdict** for the best line — Phase A's `net_pawn_units`,
   which is what "winning the piece for a pawn" is. Free once Phase A lands.

   **`{materialVerdict}` followed, 2026-09-01.** An optional marker on the
   Positive and Improvement paths, rendering "the line settles three pawns
   ahead" beside the capture and "the better line wins a rook and settles three
   pawns ahead" beside the better move, carrying its own
   `CriticalMomentFactualClaim::MaterialVerdict`. It reads
   `GameReviewCriticalMoment::material_verdict()`, the one door: a Positive
   highlight reads the *credited* achievement, so a comment can never claim a
   piece the opponent still has to walk into, and an Improvement opportunity
   reads the mechanism, whose first move is the better move in 36 of 36 ladder
   moments.

   **Three deterministic queries over the 142-Game ladder set its scope**, and
   two of them moved it off what this item assumed.

   *A2 shipped into an arm the corpus never reaches.* `TacticalPayoff {
   WinsMaterialNet }` occurs 37 times in Positive achievement lists and **never
   at index 0**, while `positive_achievement_text` renders `achievements[0]`
   alone. A credited material payoff requires `moves.len() == 1` — the played
   move *is* the capture — which puts `CapturedPiece` in front of it every
   time. So the settled verdict was derived, stored, and unreadable, and this
   item was not "free once Phase A lands" so much as unfinished by it.

   *Mate is a restatement, not a verdict.* All 17 Improvement moments with a
   `mate` payoff carry `missedForcedMate`, which the required `{consequence}`
   already states. Offering a second marker for them buys a clause and no fact.

   *`WinsMaterialOutright` is the capture sentence again.* It settles at or
   above the captured piece's value, so "captured the rook on h8" already is the
   verdict — 136 Positive and 8 Improvement moments of pure restatement, and
   restatement is what the four priced prompt edits kept paying for. Only
   `WinsMaterialNet` says the half a Player cannot read off the capture.

   Population: **37 Positive + 6 Improvement of 721 ladder moments.**

   It is cheaper than `{opponentResource}` in the one way that matters. That
   marker needed `refutation_effects`, a field stored Game Reviews did not
   carry, so the census fell to 21 of 43 until the ladder was re-reviewed. This
   one reads `mechanism.payoff` and the achievement list, both already on every
   stored review and in the 142-Game snapshot, so the census exhibits the new
   shapes immediately: **77 observed, 74 filled from the existing corpus, 3
   unfilled**, closed by one `accept-live-evaluation` pass over the three ladder
   Games `search` named — corpus 44 → 47 cases, census **77 filled, 0 unfilled,
   exit 0**. It names no square and no notation, so the literal allowlist is
   untouched, and the prompt template is byte-identical — the vocabulary is
   compiled per moment.

   **Measured 2026-09-01, and it ships with a cost named.** 77 shapes, **68
   published**, $0.075. Across the 67 shapes both runs hold, published went **60
   to 60** — zero collateral, with all seven prior rejections reproducing on the
   same discipline at the same weight. The marker is used in 5 of the 10 shapes
   offering it and published in 3, and the sentence this whole phase went looking
   for now exists: "…which leaves the evaluation at {bestEval} because the better
   line wins a rook and settles three pawns ahead."

   It cost **25 ladder moments**, and weighted by frequency the run reads 683 of
   727 against the previous 708. One shape is 24 of those 25, and it lost
   `{playedMove}` to prose by writing the Voice rule's own example back at it —
   the standing class, not a new failure.

   **The run's real finding is a budget, not a verdict.** Counting the markers
   each shape offers, every shape at five or fewer publishes (15 of 15) and the
   rest publish at about 86%; all six `missingRequiredMarker` rejections sit at
   six markers or more. The same split holds in the previous run (15 of 15 and 47
   of 54), so the vocabulary saturates at around six markers regardless of this
   change. `{materialVerdict}` published 8 of its 10 shapes, the population rate;
   it moved eight shapes across the line and one heavy one fell.

   **Item 2 spends from the same account.** It adds another optional marker to
   the same crowded Positive and Improvement shapes. Plan it against a shape
   below the line, or expect to buy it at 86%. Evidence:
   `evaluation/prose-regression/README.md`.

**Measured 2026-09-01, on the instrument #534 built.** The Fact Shape runner
took its first baseline against the pinned route: 23 shapes, one generation
each, **18 published**. Two same-prompt pairs each moved zero shapes, so the
instrument is deterministic and a single generation per shape is readable.

Two things it settled that this plan had been guessing at.

*The intent hypothesis rule has a hole, and it cannot be closed by wording.* A
Neutral moment carries no `playerIntent`, and the Grounding block states only
the positive branch, so nothing tells the model to write no plan guess there.
Three edits were measured — a second bullet stating the prohibition, one clause
appended to the existing bullet, and the rule shipped per moment by the compiler
so the model has no condition to evaluate. They scored 12/23, 13/23 and 13/23
against the unedited 18/23, each costing five shapes to fix one, and the damage
landed on *marker discipline* — the model paraphrasing the played move instead
of using `{playedMove}` — rather than on the rule edited. All three were
reverted. This is the third failed prompt-only fix in this area and the first
one with a per-shape cost attached, which is the evidence Phase D should carry
into any wording change: this prompt is at a local optimum, and edits to it are
not local.

*The largest remaining rejection class is the played move written in prose.*
"The pawn steps to f5", "The pawn takes on c4" — two of the five remaining
rejections, and the class both failed edits inflated. That, not intent, is the
next subject worth an experiment.

**Amended 2026-09-01: two of the three classes were the gate, not the prose.**
At the 69-shape baseline the rejections were 5 `repeatedMarker` on `playedMove`,
5 `missingUncertainty`, 4 `missingRequiredMarker` on `playedMove` and three
singletons. Two gate changes cleared ten of the seventeen, each measured against
the run before it, each moving **only** its own target class:

| Change | Published | Shapes moved |
| --- | --- | --- |
| A repeated `MarkerForm::Literal` marker is reference, not a repeated claim | 52 → **57** of 69 | 5, all `repeatedMarker` |
| The runtime writes the intent guess the model left out | 57 → **62** of 69 | 5, all `missingUncertainty` |

Neither touched the prompt, so the template digest is unchanged, and neither
cost a single shape elsewhere — against four prior prompt edits that each bought
one shape for about five. Weighted by how often each shape occurs in the ladder,
the pair took **470 → 708 of 727** moments, because `missingUncertainty` was
refusing the first and fourth most common shapes there. The reasoning and the
records are in `services/coach-engine/evaluation/prose-regression/README.md`.

This sharpens the standing finding rather than replacing it. Prose capability
still cannot be added by instruction; what these two show is that a rejection is
worth reading as a question about the *gate* before it is read as a question
about the model.

*And the class that survives is the gate working.* `missingRequiredMarker` on
`playedMove` is 4 of the 7 remaining rejections, and all five generations that
lost the move to prose wrote one of the three examples the Voice rule itself
supplies — "the knight to d4" appears verbatim in the prompt and verbatim in a
failure. Two of the five then wrote the marker's name unbraced after the
read-out, so substituting would read "The queen to d8 check Qxd8+"; the other
three name the move only in the model's own words, which is the thing markers
exist to prevent. There is no rendering-side answer, and the prompt-side one has
been priced four times at about five shapes.

Weighted by ladder frequency the whole remaining tail is **19 of 727 moments —
2.6%**, so it no longer pays for a measured run. The next increment is
capability: Phase D's remaining fact families, or A3. `{opponentResource}` on
the Neutral path is off the list for the same reason — all five Neutral shapes
already publish, so offering it there adds authoring problems and fixes none.

Leave `{achievement}` as an own-sentence marker. That form was _measured_ — 20
replayed generations plus a live slice, recorded at
`docs/prototypes/web-language-layer-prompt-templates.md:95-113` — and models
demonstrably cannot place the bare verb phrase in a clause. Improve the
_rendering text_ instead, which is free: Phase A's three-way split already turns
"You won a knight." into "You won the knight on d5 for free." or "You won the
rook for a pawn."

## Order of work

The phases are numbered by subject, not by sequence. Agreed order:

**C1 + C2 in one deploy → B → A → C3 → D.**

**Amended 2026-08-31.** C3 ran early and cheaply, before A, because a publish
rate needs live traffic and staging had none: the reading is in
`docs/hosted-language-layer-rollback.md`. The single bake-off run this order ends
with has no harness — it was retired in `23bcd58c` because its frozen set
addresses exemplars by `(case, ply)` and A3 moves those again. D is therefore
gated on #534 or on an explicit decision to run it without a prose benchmark.

C first because a 92% fallback rate means the Language Layer is switched off:
until it publishes, no Phase A or Phase D work is visible to anyone. C1 is also
the instrument every later decision reads — without it, the next prompt edit is
tuned as blindly as the last two.

B second, and earlier than a subject-order reading would put it. Stored
fallbacks carry the compiled digests and are classified `Current` forever, so a
review the Player has already opened — including the one that prompted this
plan — never picks up any later fix without it.

A third: the Player's actual complaint, the largest change, and the one with a
contract break and a corpus re-baseline attached. D last, after the single
bake-off run.

## Validation

- `cargo test -p coach-engine-pipeline` for Phase A extraction.
- `cargo test -p chen-chess-coach-engine` for renderers, markers, and the
  readiness three-way match.
- Rust sweep once after implementation (`scoped-validation`); no `cargo clean`.
- `bun run test --filter=@chenchess/central-host` for the browser twin.
- Corpus: `gotham refresh-explanations`, re-pin the baseline digest in both
  places, then `refresh-comparison-mirror --write` and `--check`.
- Screenshots of one Positive Highlight before and after, to the Player. There
  are no layout gates; the shot is evidence, not a check.

## Decided 2026-08-30

The eight questions this plan opened were answered by the Player the same day.
Each decision is written into the phase it governs; collected here so a reader
does not have to reconstruct them.

| Question                                            | Decision                                                                                                                                                                    |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A3 — drop a depth > 0 payoff or render it as setup? | Decided 2026-08-30 as setup-when-forcing. Superseded 2026-08-31: no achievement at all for a depth > 0 **material** payoff, no multi-PV call, other payoff kinds untouched. |
| Is `WINS_MATERIAL_PAWN_UNITS = 2` still right?      | Yes. The defect was naming a role the net does not support, not the threshold.                                                                                              |
| Phase B bound                                       | A retry counter on the provenance, capped at one. Not the generation bump.                                                                                                  |
| `Synthet1` re-baseline                              | Re-baseline, with the selection diff carried in the PR body as evidence.                                                                                                    |
| Phase D shape                                       | One issue per fact family; opponent's resource first.                                                                                                                       |
| Bake-off timing                                     | One run, after A **and** C, before D.                                                                                                                                       |
| Publish rate as a gate                              | Post-deploy verification with a rollback threshold, plus a free pre-merge check that a `pin_record.rs` digest move carries a bake-off run.                                  |
| Revert `3054acb7`?                                  | No. Carve the markers out of the Voice rule and ship it with C1.                                                                                                            |

## Still open

- **Where the Quality Capture carries a rejection reason.** _Recommended: the
  task-shaped side._ `EvaluationFingerprintObservations` observes the
  _generation identity_ — served provider, pin verification, capture outcome —
  which is true of any hosted call. A rejection discipline is task vocabulary:
  18 codes for the comment, a different set for the Coach Turn, another for the
  HostTurn. The capture draft already carries `task` and a task-shaped
  `failure_excerpt`, so the reason belongs beside those and the shared struct
  stays task-agnostic. This also fixes the reachability blocker in the same
  edit: `author_grounded_comment` already holds the full-width rejection before
  `ground_draft` narrows it, so it only has to keep what it currently discards.
  Do it with the rest of C after Phase B.

  _Settled and shipped._ The task-shaped side won. `RecordedProseRejection`
  (`quality_capture/model.rs`) carries `discipline` and, where the discipline
  names one, `marker`, stored beside `task` on the
  `LanguageLayerGeneration` capture; `hosted_author.rs` passes the full-width
  rejection before `ground_draft` narrows it, exactly as the reachability note
  predicted. `EvaluationFingerprintObservations` stayed task-agnostic. The
  rollback runbook reads the stored fields instead of Railway.
- **C2 is a bet, not a confirmed fix.** C2 was written as conditional on C1
  naming `{playedMove}`. C1 cannot name it until it is deployed, so the carve-out
  ships on correlational evidence: publish rate bucketed by prompt fingerprint,
  plus 24 staging rejections that name the discipline and not the marker. The
  falsifier is explicit — if the rate does not recover, the event now names the
  marker that is actually missing and the carve-out was aimed wrong.

  _One reading, not yet enough._ 2026-08-31, digest `sha256:d88ba78e…`: 9
  completions, 56% published, `MissingRequiredMarker` 0 against the failures
  that took the previous fingerprint to 21%. Above the threshold and pointing
  the right way, but nine completions is short of the fifty the threshold is
  written for and eight of them belong to one eagerly authored Game Import, so
  the bet is not settled. Re-read on a boot with real traffic — #578 item 2.
- **C3's two rules disagree about this change.** C3 asks for a pre-merge check
  that a `pin_record.rs` digest move carries a bake-off run beside it, and also
  defers the single bake-off run to after A and C. _Recommended: drop the
  pre-merge check._ It would have blocked both prompt fixes this plan is made
  of, for reasons that were right each time — the bake-off costs money and is
  worth running once, on a settled prompt. A gate that is correctly waived on
  its first three uses teaches people to waive it. Keep only the post-deploy
  publish-rate threshold: it is free, it reads two counters already being
  emitted, and it catches the actual regression rather than its proxy.

  _Settled as recommended._ Nothing implements the pre-merge check, and the
  post-deploy threshold is written into `docs/hosted-language-layer-rollback.md`
  with its first reading beneath it. The bake-off harness it would have gated
  was itself retired in `23bcd58c`; ADR 0062 replaced its frozen set with the
  Fact Shape census.

- **A3's multi-PV call adds engine work to the hot path.** _Closed 2026-09-01:
  the fallback was taken and the question is moot._ Depth > 0 material payoffs
  are dropped outright, no engine call at all, and the drafted "sets up" prose
  showed the only loss is a sentence no Player could have read — all fourteen
  corpus candidates already carry an effect-derived achievement ahead of it.
  See Phase A above.
