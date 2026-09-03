# Prototype: the frozen bake-off Review Session task set

Artifact for [Freeze the bake-off Review Session task set](#345).
**The set in §5 is accepted** — reviewed with the Service Operator on 2026-08-13, with the four decisions
in §10 folded in. #346 records baselines against it.

This is a case list, not a UI study, so the map's "build prototypes from `packages/ui`" policy does not
apply — there is nothing Player-facing to render.

**Two Service Operator decisions, 2026-08-13, both load-bearing:**

1. Own games and GothamChess games are **both** admissible sources.
2. **The model generates coaching language only.** Its chess understanding is not under evaluation —
   the requirement is that it stays true to the chess engine's data.

Decision 2 changes what this set is *for*, so §1 works through what follows from it before proposing
anything.

---

## 1. The model is a renderer, so the set is a coverage problem, not a sampling problem

Every chess fact in a Review Moment note is computed before the model is called: the evaluations, the
better move, the classification, the human-move-model rank, the engine line. Under
#344's slot markers the model may not even *write* a
number — it writes `{bestEval}` and the runtime substitutes the canonical rendering. The model's whole
job is to turn a fixed fact bundle into prose a coach would say.

Three consequences, and they point the same way.

**The unit of work is the moment, not the game.** Verified at the seam:
`CriticalMomentCommentAuthor::author` (`critical_moment_comment.rs:88`) takes
`CriticalMomentCommentAuthorInput`, which is exactly one `ReviewMomentCommentFacts` plus an optional
intent and the generation contract. No game, no other moments, no session history. Each comment is an
independent generation. A "game" is therefore not a test case — it is a *bundle* of ~5 independent test
cases that happen to share a PGN.

**What distinguishes two test cases is the fact shape, not the chess.** Two moments that are both
"Improvement, better move is a forced mate, played move has a human-move-model rank" hand the model the
same marker set, the same template branch, and the same length target. They differ in *which* chess
position produced them — which is precisely the part the model is not being evaluated on. As test
inputs they are near-duplicates.

**Therefore representativeness stops being the selection criterion.** "Do these look like my Players'
games?" was the right question for a set meant to judge coaching *quality in the wild*. For a set meant
to judge whether a renderer stays faithful to its inputs, the right question is: **does the set hand the
model every distinct fact shape it will ever be handed?** Whether a Player at 900 or 1400 produced that
shape does not change what the model must render.

That also retires most of the analysis in earlier drafts of this document — whether the GothamChess
reviewed side is a titled player, whether opponents blunder more, whether the Elo ladder is real. All
true, all no longer load-bearing. What survives is §2 and §3.

---

## 2. The coverage target, measured

Classifying all 742 automatically selected Critical Moments in the GothamChess corpus by the axes that
change what the model must render — the classification branch, the evaluation kind, the achievement kind,
and whether the optional `{playedPopularity}` marker has a value:

**742 real moments collapse to 13 distinct fact shapes.**

| Count | Fact shape |
| ---: | --- |
| 351 | `Positive / good / capturedPiece / pop` |
| 185 | `Positive / good / tacticalPayoff / pop` |
| 40 | `Positive / great / tacticalPayoff / pop` |
| 36 | `Improvement / centipawns / pop` |
| 32 | `Positive / good / advancedPassedPawn / pop` |
| 31 | `Positive / great / tacticalPayoff / nopop` |
| 22 | `Positive / great / capturedPiece / pop` |
| 21 | `Improvement / mate / pop` |
| 9 | `Improvement / centipawns / nopop` |
| 7 | `Positive / great / capturedPiece / nopop` |
| 4 | `Positive / good / capturedPiece / nopop` |
| 3 | `Positive / good / advancedPassedPawn / nopop` |
| 1 | `Positive / great / advancedPassedPawn / pop` |

**The top three shapes are 78 % of all moments.** Real-world frequency is wildly skewed, and a set that
mirrors it spends most of its budget re-testing the same three renderings.

### What automatic selection never produces

Three parts of the contract are unreachable from any number of full-game reviews:

- **Every `Neutral` moment.** Zero of 742. `neutral` classifications arise only from *Player-selected*
  moments, and they carry their own marker set (`{playedMove}` `{reason}` `{observation}`), their own
  one-line length target, and `forbidden_literals` — a whole gate branch, untested without a
  `selectedMoment` case. And the branch is wider than the four `NeutralReviewReason` variants suggest:
  `rule_extractor.rs:634-646` accumulates reasons into a `Vec`, so they are **not mutually exclusive**.
  `MechanicallyForcedMove` can pair with either of the other conditions, `SoundWithoutConcreteAchievement`
  and `NonInstructionalTerminalOutcome` are mutually exclusive (`sound` versus `!sound`), and
  `BelowImprovementThreshold` fires only as a fallback when nothing else did. The reachable reason *sets*
  are six, and a multi-reason set is the hardest case in the whole contract: `{reason}` renders both, and
  the model must fit two reasons into a **one-line** target without manufacturing a lesson.
- **Terminal positions**, where there is no post-move score and the model must not invent one.
- **Both Task B dimensions and the `OutOfScope` refusal**, which have no game-derived representation at
  all.

### And a corollary that saves real work

Because chess understanding is not under evaluation, **a constructed position is as good a test input as
a real one** for the grounding set. The six existing corpus cases — dismissed in earlier drafts as
"unusable one- and two-ply fragments" — turn out to already cover four shapes, including the *only*
Neutral case in the repo:

| Existing case | Fact shape it supplies |
| --- | --- |
| `tactical-white-human-likely` | `Improvement / centipawns / pop` |
| `advanced-both-threshold` | `Improvement / centipawns / nopop` |
| `positional-black-intermediate` | `Improvement / centipawns / nopop` (duplicate) |
| `selected-nonautomatic` | `Neutral / soundWithoutConcreteAchievement` |
| `selected-terminal-mate` | Terminal checkmate, `completedCheckmate` achievement |

They were built to regression-test the selector, but as *renderer* inputs they are sound and already
frozen with pinned provenance. Keep them.

---

## 3. Why a game-based set is the wrong shape, quantified

Measured over the 142 GothamChess games, targeting all 13 shapes:

| Strategy | Recording | Generations per candidate route |
| --- | --- | --- |
| Random real games until all 13 shapes appear | median **83 games** | **431** |
| Best possible hand-picked *games* | 7 games | 48 |
| Hand-picked *moments* | 4 games (305 plies) | **13** |

The earlier nine-game proposal in this document would have produced ~45 generations per route and still
missed shapes: its two GothamChess cases contribute 8 moments covering 4 shapes and 6 moments covering 3.

**A moment-based grounding set is ~3.7× cheaper than the best game-based one and covers strictly more.**

One detail that keeps the costing honest: to freeze an *automatically selected* moment you still record
the whole game, because that is what the selector consumed. Recording cost stays per-game; generation and
human-review cost drops to per-moment. Those are different budgets and only the second was ever the
binding one.

---

## 4. What this does to the metrics

If faithfulness to engine data is the requirement, then #344's
gates *are* the primary scoring function, and they need no human:

- unknown marker, repeated marker, missing required marker → `MissingFactualClaim`
- any evaluation-shaped token, percentage or probability outside a marker
- chess literals outside the allowlist
- surviving `{`/`}` after substitution
- post-substitution: single paragraph, `contains_internal_player_facing_text`, Neutral `forbidden_literals`
- Task B: `cited == required` set equality per dimension

So **the bake-off is mostly a mechanical pass/fail sweep**, and per-candidate marker discipline is a hard
number. Human reading shrinks to a sample — and to **exactly two jobs the gates cannot do**. Both must be
named in the harness spec, because neither is something a reader finds without being told to look:

1. **Repetition across a run.** Because `author()` sees one moment and nothing else, the model cannot
   know what it wrote in the previous note. Repetition across a session is therefore *structural*, not
   stochastic — and every note passes its own gate individually, so six notes opening with the same
   construction is invisible to every gate in the pipeline. Only the session set (§5.3) can show it.
2. **Claims that contradict their own substituted markers.** Markers stop the model writing a wrong
   *number*; they cannot stop it writing a wrong *claim* around one — "the natural choice here" beside a
   marker that renders *2 % of players at your rating*. This is
   #344's accepted residue, and reading is the only
   detector for it.

Everything else is mechanical.

This inverts the sizing conclusion of earlier drafts. Human review capacity is no longer what bounds the
set; shape coverage is. The set below is small because coverage is achievable in 19 generations, not
because reading is expensive.

---

## 5. The proposed set

Conventions unchanged: provenance pinned per case (source URL, PGN SHA-256, Stockfish binary digest, Maia
image digest); **frozen means frozen** — a case added later mints new comparisons rather than new rows in
old ones.

### 5.1 Grounding set — 19 generations per candidate route

Scored mechanically by the §4 gates. This is the set that answers *does the model stay true to the engine
data*.

**Reused from the existing corpus, unchanged, zero recording cost (4 cases)**

| # | Case | Supplies |
| --- | --- | --- |
| G1 | `tactical-white-human-likely` | `Improvement / centipawns / pop` — also #344's worked example, so the expected marker output is already written down |
| G2 | `advanced-both-threshold` | `Improvement / centipawns / nopop` |
| G3 | `selected-nonautomatic` | `Neutral / soundWithoutConcreteAchievement` |
| G4 | `selected-terminal-mate` | Terminal checkmate — no post-move score to invent |

**New moment exemplars, from 4 GothamChess games (11 cases, 305 plies to record)**

*Recorded 2026-08-15 by #358 as four corpus cases —
`gotham-ep27-166213489290`, `gotham-ep24-165723357366`, `gotham-ep33-169724120336`,
`gotham-ep21-164113796562`. Every ply below reproduced as a selected Critical Moment with the SAN and
shape stated here, and each game's whole selected set matched the earlier ladder run exactly. **The
exemplar is the ply, not the case**: those four cases carry 26 Critical Moments between them, of which
these 11 are frozen — see §8 on why `run` must address them by ply.*

| # | Game | Ply | Move | Supplies |
| --- | --- | --- | --- | --- |
| G5 | ep 27 · `166213489290` | 12 | `Bd6` | `Positive / good / tacticalPayoff / pop` |
| G6 | ″ | 20 | `Bxg3` | `Positive / good / capturedPiece / pop` — the single most common shape in production (47 %) |
| G7 | ″ | 38 | `Rae8` | `Positive / great / tacticalPayoff / nopop` |
| G8 | ″ | 40 | `d5` | `Positive / great / tacticalPayoff / pop` |
| G9 | ″ | 72 | `Qxd2+` | `Positive / great / capturedPiece / nopop` |
| G10 | ″ | 76 | `h5` | `Positive / good / advancedPassedPawn / pop` |
| G11 | ep 24 · `165723357366` | 86 | `Kd6` | **`Improvement / mate / pop`** — a missed forced mate, the longest length target in the table |
| G12 | ″ | 76 | `c3` | `Positive / good / advancedPassedPawn / nopop` |
| G13 | ″ | 88 | `Kxc5` | `Positive / great / capturedPiece / pop` |
| G14 | ep 33 · `169724120336` | 33 | `f6` | `Positive / great / advancedPassedPawn / pop` (1 occurrence in 742 — the rarest shape) |
| G15 | ep 21 · `164113796562` | 54 | `Qxa1` | `Positive / good / capturedPiece / nopop` |

**Constructed `selectedMoment` cases for the unreachable Neutral branches (4 cases)**

Constructed is legitimate here per §2's corollary, and each is a two-ply case in the style of
`selected-nonautomatic`. Four of the six reachable reason sets are new; the sixth is skipped as a
near-duplicate of G19.

| # | Case | Supplies |
| --- | --- | --- |
| G16 | `selected-forced-recapture` | `[MechanicallyForcedMove]` |
| G17 | `selected-below-threshold` | `[BelowImprovementThreshold]` — the fallback, and so likely the most common Neutral in production |
| G18 | `selected-terminal-noninstructional` | `[NonInstructionalTerminalOutcome]` |
| G19 | `selected-forced-and-sound` | `[MechanicallyForcedMove, SoundWithoutConcreteAchievement]` — the two-reason one-liner |

Skipped: `[MechanicallyForcedMove, NonInstructionalTerminalOutcome]`, which exercises the same
two-reason rendering as G19.

~~**Constructibility is unconfirmed.**~~ **Constructibility is confirmed — all four, 2026-08-15
(#358).** Each is a one-ply case, and reading the
derivation in `rule_extractor.rs` turned each reason set into a position rather than a search:

- **G16** needs `legal_moves == 1` *with* a non-empty achievement, because a forced move is always its
  own best move and so always sound — without an achievement it would collapse into G19. A king with
  one legal reply, capturing the undefended queen that checks it.
- **G17** is the gap between two thresholds, not a subtle move: `soundness_threshold` is 35 cp at
  intermediate and `mistake_threshold` is 100, so any loss in 36–99 is too costly to be sound and too
  cheap to correct. The Albin Counter-Gambit (`1. d4 d5 2. c4 e5`) lands at 53.
- **G18** must be a **stalemate**, not a checkmate: delivering mate is sound by construction, so only a
  mover who ends the game *without* being mated and *without* soundness qualifies. A queen throwing
  away mate-in-one for stalemate. This is the case that most repays having been constructed — it
  carries `residualOutcome: missedForcedMate` and the `forcedMateConversion` theme, and no volume of
  game review would have produced it.
- **G19** is the easy one: one legal king move, quiet, no capture.

Every position was validated with chessops before recording — legal-move counts, the played move, and
the resulting terminal state — per the Notes rule that constructed chess is checked, not invented. The
recorded classifications came back as exactly the four target reason sets, so no gap needs recording.

### 5.2 Task B — Alternative Move Assessment (3 turn sets)

| # | Turn set | On | Root | What it exercises |
| --- | --- | --- | --- | --- |
| B1 | `turns-own-1246` | `Synthet1` | ply 26 (`13...Bxb5`), alternatives `c5d4` / `e5d4` | **Already recorded** — 18 positions, 17 branches. Covers all three dimensions and the nested ancestor-chain path in `PreparedCoachTurnTarget::capture`. |
| B2 | `turns-steer` | B1's root | same alternative, second turn | The only way to exercise #233's rule that prior text is visible **only** within one alternative. Reuses B1's evidence. |
| B3 | `turns-out-of-scope` | B1's root | off-topic Player message | Must trip #233's `OutOfScope` refusal variant. Reuses B1's evidence. |

**None needs new recording**, and that is the whole of Task B's cost.

A low-Elo `findability` case was proposed and **cut**. It differs from B1's `findability` only in the
Maia numbers, which are marker-substituted — the same fact shape with different digits, and so a
near-duplicate by §1's criterion. It was also the only case in the entire set requiring a new
`EvaluationOperation` variant and a parameterized capture, so cutting it takes corpus-format work to
zero (§8).

The real risk it was reaching for survives and is handled elsewhere: a model can write "this is the
natural choice" as prose while `{playedPopularity}` substitutes *2 % of players at your rating*. That is
#344's accepted residue — an invented claim no
marker catches — and it is not specific to low Elo or to Task B. It belongs to the human sample (§4).

### 5.3 Session set — 3 whole Review Sessions, for cost and latency only

This is what #236's four budget tiers are set from,
and the only place a whole game is the right unit. The Service Operator's own games, so the workload is a
real Player's.

| # | Case | Game | Side | Elo | Plies | Role |
| --- | --- | --- | --- | --- | --- | --- |
| S1 | `session-short` | _(withheld: real game, removed from the public corpus)_ | white | 1131 | ≈36 | Cost **floor** — a short, resigned game |
| S2 | `session-median` | `lichess.org/Synthet1` | black | 1246 | 84 pos | **Already recorded.** Median session, and B1's host |
| S3 | `session-long` | _(withheld: real game, removed from the public corpus)_ | black | 636 | ≈75 | Longest recorded session |

*S1 and S3 recorded 2026-08-15 (#358) at exactly 36
and 75 plies, yielding **4** and **7** Critical Moments. PGNs came from the public Chess.com archive
endpoint, which serves full PGN text and so avoided decoding the callback's encoded `moveList`.*

Three sessions establish the per-moment cost and its variance; the **budget ceiling is then computed**
against the selector's `HARD_MAXIMUM = 10` moments (`critical_moment_selector.rs:10`) rather than sampled
for, since no game in any available source is guaranteed to realize the cap.

Three is also enough for the session set's *other* job — §4's repetition read. That failure mode is
structural rather than stochastic, so if it occurs it occurs in every session; more sessions would buy
confirmation, not detection.

### 5.4 What it costs

**Recording:** 305 plies (grounding exemplars) + ≈8 plies (four constructed Neutral cases) + ≈111 plies
(S1, S3; S2 and B1 already recorded) ≈ **424 plies**. At measured rates — Stockfish median 160 ms, Maia
median 143 ms per position, 7 340 GothamChess plies in ~1 672 s total — that is **under two minutes of
provider time**, plus one ~20 s runtime startup and MultiPV recording per moment. Corpus growth is not
bounded by the runtime.

**Repo size:** ≈**0.9 MB**, scaling from `Synthet1` (126 KB case + 51 KB baseline for 84 positions).

**Generation per candidate route:** 19 grounding + 3 turns + ≈17 session comments ≈ **39**. Across
#236's seven routes at 3 replicates ≈ 820
generations, a few dollars even at the Sonnet 5 ceiling.

**Human reading:** the 22 grounding-and-turn outputs per route are gate-scored first; a human then reads
a sample against §4's two named dimensions. Down from ~350 pieces of prose in the previous draft.

**Coaching Profile Projection:** every grounding case runs twice, cold-start and populated, same seed,
and the outputs are diffed. That doubles grounding generations and stays mechanical. §6 explains why the
diff is the only honest thing to measure on this axis.

---

## 6. One finding that survives, and cuts against a closed decision

The populated Coaching Profile Projection is derivable today by ranking the Learning Track Keys a
player's prior reviewed games surfaced. Over the GothamChess ladder:

| Projection at | Prior exposures | Top 8 |
| --- | --- | --- |
| before ep 26 | 342 | deflection, clearance, intermezzo, collinearMove, attraction, desperado, sacrifice, defensiveMove |
| before ep 34 | 439 | deflection, clearance, intermezzo, desperado, collinearMove, sacrifice, attraction, defensiveMove |
| before ep 40 | 505 | deflection, clearance, intermezzo, sacrifice, collinearMove, desperado, attraction, defensiveMove |

The same eight concepts, barely reordered. Corpus-wide, **26 distinct track keys appear in 142 games and
the top 8 cover 86 % of exposures**, from a catalogue of ~72 `CurriculumLearningConcept` values. The
concentration comes from the detectors and the selector, not from a small catalogue.

#332 holds that "the only thing separating two
Players' notes is which Learning Track Keys their reviewed games surfaced". At K = 8 that is empirically
a weak separator — most populated Players will project nearly the same list. Hence §5.4's cold-start
versus populated **diff**: the honest question is whether the model uses the block at all, not how well
it personalizes.

**This does not reopen #332 yet.** The evidence is
one player's 142 games, and the concentration could be that player's tactical style as easily as the
detectors'. The bake-off produces the decisive measurement for free: if output barely moves between an
empty list and a populated one, the projection is inert *whatever* K is, and that finding is worth more
than the concentration statistic. Nothing is blocked meanwhile — the projection ships either way. The
concentration is recorded as fog on the map, to become a ticket if the diff comes back flat.

One consequence for the harness: **it has to pick a K**, and no value is pinned anywhere in the contract.
The choice is not load-bearing, because the measurement is empty-versus-populated rather than
K-versus-K.

---

## 7. Can the Local Pipeline Runtime produce this? Partly, today

Measured on this host, 2026-08-13:

- **Pinned Stockfish: healthy.** The installed binary at
  `~/.local/share/chenchess/units/0.2.0-local-coach.4/bin/stockfish` is Stockfish 18 and hashes to
  `bc0cac90…63590` — **the exact digest every existing baseline records**. Stockfish-only work
  (`record-multi-pv`) can run now.
- **Maia: blocked.** The Docker daemon is unresponsive — `docker ps` and `docker version` both hang past
  25 s with Docker Desktop's processes running. `chenchess runtime maia-status` cannot be reached.
- **`chenchess runtime doctor` blocks indefinitely on it.** It ran 13 min 40 s without emitting a
  diagnosis or timing out, holding the exclusive `~/.local/state/chenchess/runtime.lock` throughout and
  blocking every other runtime command. It exits only when killed. The shipped runtime-startup limit is
  600 s; `doctor` appears not to enforce one. *(A diagnostic command that cannot diagnose the failure it
  is stuck on deserves its own look. Out of scope here.)*

**A local Docker fault, not a provisioning gap.** Restarting the daemon is the expected fix, after which
recording takes the two minutes costed in §5.4. Recorded because
#346 cannot record a single case until Maia answers:
**a precondition to schedule, not a decision to make.**

**Resolved 2026-08-15 (#358).** It was a local
Docker fault and the expected fix worked, but the graceful path did not: `osascript quit` left
`com.docker.backend` and an orphaned `com.docker.virtualization` running, and both had to be killed
before the app would relaunch. The VM process had started nine hours after the backend, so the two were
already mismatched. The daemon came back as server 29.6.2, and the installed Maia image still hashes to
`ab3b6dc16b75…` — the digest every existing case pins. Recording then ran as costed: **420 plies against
the §5.4 estimate of ≈424**, and the corpus reproduced every earlier ladder selection exactly.

---

## 8. What has to be built

| Need | State |
| --- | --- |
| Full-game corpus case | **Exists.** `Synthet1/provider-recordings`, in format, under test. |
| Recording games into the corpus | **Exists.** `record-multi-pv`, `evaluate-live`, `accept-live-evaluation`. |
| Constructed `selectedMoment` cases (G16–G19) | **Exists.** Same shape as `selected-nonautomatic`. |
| Generating on **one named moment** of a recorded game | **Half-built.** #346 gave `prompts` a `--ply` flag; **`run` never got one**, and iterates every moment of every selected case. Unaddressed, a run over the four gotham cases issues **26** grounding generations per route rather than the frozen 11, and the session cases add theirs on top — so the frozen 19 silently becomes 34 and the coverage claim stops meaning what §2 measured. #359 has to close this, and the ply list in §5.1 is the input it needs. |
| Alternative Move evidence | **Exists, reused as-is.** `capture_review_session_recording.rs` pins its game and root as constants; B1–B3 all sit on that one recording, so no parameterization is needed. |
| A **Coach Turn case in the corpus format** | **Not needed.** `EvaluationOperation` (`pipeline_evaluation.rs:110`) stays at its two variants, `Game` and `SelectedMoment`. |

**Cutting the low-Elo findability case takes new corpus-format work to zero.** What remains is one small
harness capability — addressing a moment by ply — and writing four constructed cases in an existing
shape. Both belong to #346 as preconditions.

---

## 9. Privacy

Smaller than in earlier drafts, because the grounding set is shape-driven rather than Player-driven.

The **session set** is entirely the Service Operator's own games, on their own public Chess.com and
Lichess accounts, reviewed from their own side.

The **grounding set** uses four public Chess.com games featured in public YouTube episodes — real
usernames, retrievable from public `chess.com/game/live/<id>` URLs, broadcast move by move by the account
that played them, with full PGNs **already committed** under the precedent in `evaluation/gotham/games/`.
All four are reviewed from the broadcasting player's own side. The remaining grounding cases are
constructed positions with no player at all.

No Beta Access Player's game, no imported Player game, no private source.

**The rule this establishes:** a bake-off case may only use a game whose *reviewed side* made that game
public, and the case records the public URL that proves it. Constructed positions are exempt, having no
subject.

Residual: opponents are named in PGN headers and prose may characterise their moves. It stays in-repo and
in `coach-quality`, is never shown to them, and the Evaluation Fingerprint records the case id, not a
username.

---

## 10. Decisions taken on this artifact

Settled with the Service Operator, 2026-08-13, and already folded into §§4–8.

| Question | Decision |
| --- | --- |
| Is the session set doing enough? | **Three sessions stand.** The repetition failure mode is structural, not stochastic, so more sessions buy confirmation rather than detection. Instead, intra-session repetition becomes an explicitly named human-read dimension (§4). |
| Neutral coverage | **Four constructed cases, not two.** Reading the derivation showed the reasons accumulate into a `Vec`, so the reachable reason *sets* are six rather than four, and the two-reason one-liner is the hardest case in the contract (§2, §5.1). |
| Keep the low-Elo `findability` case? | **Cut.** A near-duplicate fact shape, and the only case in the set requiring new corpus-format work. The invented-claim risk it was reaching for moves to the human sample (§5.2, §4). |
| Does the projection concentration reopen #332? | **Not yet.** One player's evidence; the bake-off's cold-start-versus-populated diff settles it for free. Recorded as fog, promoted to a ticket if the diff comes back flat (§6). |

Net effect: grounding set 17 → 19 generations, Task B 4 → 3 turn sets, **new corpus-format work reduced
to none**, and two named dimensions for the human sample.

### Still open, deliberately

- **Constructibility of G16–G19** (§5.1) is read off the derivation, not proven by a run. First thing to
  establish when the cases are written; a genuinely unreachable reason set is a gap to record, not to
  force.
- **The Elo ceiling.** The set spans 355–1653 and nothing real in the workspace goes higher. Under §1's
  framing this barely matters — Elo changes Maia's numbers, which are marker-substituted, not the fact
  shape — which is why it is not a blocker. It would matter again if Elo ever reached the prompt as
  anything other than substituted values.
