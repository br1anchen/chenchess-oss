# Address Language Layer exemplars by derived Fact Shape

2026-08-31. Design artifact for
#534, settled by grilling.
Supersedes the frozen nineteen-generation task set of
#345 §5.1, retired with its
harness in `23bcd58c`.

**Steps 0, 1 and 2 are built.** `FactShape` and its derivation land in
`src/critical_moment_comment/fact_shape.rs` (`21e3cf46`), the ladder is
re-reviewed off current rules (`8f6ab109`), which first needed an eligibility fix
(`a6f2d488`), and `census`, `resolve`, the resolution file and the fast test land
in `src/pipeline_evaluation/fact_shape_resolution.rs` with the
`language_layer_prose_regression` binary. Steps 3–5 are not started. Step 0 left
the ladder's `semantic` and `compare` phases stale and unrecomputable here — see
**Step 0's residue**.

## Why the old set died

`frozen-set.json` named its exemplars by `(case, ply)`. Every payoff-rule change
invalidated some of those plies; the last round re-pointed three and found no
replacement anywhere in the corpus for two. Its own test asserted
`entries.len() == 19` against §5.1, so retiring a dead entry was a spec change
rather than a data change. A benchmark whose exemplars are hand-picked ply
numbers cannot survive the rules it measures moving underneath it.

The caller is real and waiting: #536 records Phase D of
`plan-review-commentary-truth-and-voice.md` as *not started, instrument
retired, gated on #534*.

## Measurements this design rests on

Taken 2026-08-31 against the working tree at `b009b1c2` (git `HEAD` lags at
`23bcd58c`).

| Measurement | Value |
| --- | --- |
| §2-style fact shapes in the pinned corpus | 11 over selected Critical Moments; 16 including the six `selectedMoment` cases |
| Moments in the pinned corpus | 46 |
| Shapes still missing | `Improvement / mate / pop`, `Positive / great / capturedPiece / pop` |
| `gotham-ep24` ply 86 | no longer a candidate at all; the selector keeps 7 of 14 there |
| `evaluation/gotham/reviews/` | 142 games, 742 moments, reproduces the 2026-08-13 table **exactly** — it predates the payoff change |
| Marker-name axes in `CommentFactsPolicy::for_facts` | three only: path, `takeaway` present, `playedPopularity` present |
| Cross-product cells occupied in the corpus | **18** |
| `Positive / good / capturedPiece / pop` | splits **9 without a takeaway / 4 with** — the most common production shape, and the old set could not see the split |
| Takeaway renderings present | `passedPawnPromotion` 10, `queenExchange` 7, `forcedMateConversion` 4, `occupyTheCenter` 2, none 23 |
| `MissedForcedMate` residual vs a mate better-evaluation | **21 of 21 on the ladder, exact** — the only axis that recovers `Improvement / mate` |
| Realized Fact Shapes, **refreshed** ladder / corpus | **37 / 23**, both by `FactShape::of` (2026-09-01, after step 0) |
| Refreshed ladder | 142 Games, **721** selected Critical Moments, down from 742 |
| The two combinations that ended the frozen set | **both present in the ladder** — `missedForcedMate`+pop at 17 moments, `great`/`capturedPiece`+pop at 22. Fillable by search; nothing to construct, no gap to record |
| Ladder shape skew | top shape 300 / 721 (42 %), top two 57 % |
| One run at 37 shapes on the pinned route | **$0.028**, under a minute — cost is not the constraint |
| Corpus baselines omit `playedMoveOutcome` | so a census must replay through `recorded_comment_case`, never read `*.baseline.json` — the baseline projection is blind to the analyzed/terminal axis |

Two consequences drove the design:

**Neither candidate key subsumes the other.** §2's fact shape distinguishes
grade, achievement kind and mate-vs-centipawns, all of which the marker *names*
collapse; the marker structure distinguishes `takeaway` presence, which §2
collapses. The key is their product.

**`pop` is `played_move_rank.is_some()`**, not `playedMoveProbability` as #534's
decoding note says. Same moments in practice, different predicate to write down.

**The ladder cannot be the whole coverage authority.** Measured over the
refreshed 721 moments: every shape is Positive or Improvement, every one
`analyzed`. Not one is Neutral, and not one is Terminal — which is §2's finding
restated by the implementation, because both arise only from a *Player-selected*
moment and full-Game review never produces one. The Neutral branch has its own
marker set, its own forbidden literals and a one-line target, so a census that
took the ladder alone would silently drop a whole gate branch. Decision 3 is
therefore the ladder census **union** the Player-selected family, which is
enumerable from the contract rather than sampled: the reachable
`NeutralReviewReason` sets and the Terminal played-outcome, which the six
constructed `selectedMoment` cases already supply.

## Settled decisions

| # | Decision | Settled as |
| --- | --- | --- |
| 1 | Scope | Fact Shape addressing, coverage census, and a runner against the **one pinned route** (ADR 0050). Preflight, `routes.json`, catalogue checks, budget ceilings stay retired. |
| 2 | Addressing key | Derived from `CommentFactsPolicy::for_facts` — marker structure **×** the enum discriminants that selected each rendering |
| 3 | Coverage authority | Census over the GothamChess ladder, **plus the Player-selected branch it structurally cannot exhibit** (see below). No number pinned anywhere. |
| 4 | Resolution target | Selected Critical Moments |
| 5 | Generator | Search the ladder first; construct (the G16–G19 method) only where the ladder cannot supply. Never synthesize provider evidence. |
| 6 | Run modes | Resolve → **record** → replay. Re-resolution answers "still covered"; replay answers "did the prose change". |
| 7 | Coverage check | Split: a fast `#[test]` over the recorded resolution, a deliberate CLI census over the ladder |
| 8 | Threshold | Any shape the ladder exhibits, n ≥ 1 |
| 9 | Task B / session set | Out of scope |
| 10 | Ownership | `pipeline_evaluation` + a separate `[[bin]]` — the runner spends money, keeping it out of the product CLI is deliberate |
| 11 | Generations per shape | One exemplar, one generation, cold-start |
| 12 | A3 ordering | Ship **before** A3; A3 becomes the proof that Fact Shape addressing works |
| 13 | Stale ladder | `gotham review --force` as step 0 — **done 2026-09-01**, and it needed the eligibility fix in `a6f2d488` first |
| 14 | Verdict | Per-shape delta against the prior recorded run. **No pinned rate floor.** |
| 15 | Records | Resolution beside the corpus; run records in a new directory, checked in only as decision evidence |
| 16 | Naming | **Fact Shape** / **Exemplar** |
| 17 | Unfillable shape | Recorded gap; census stays green |
| 18 | Tie-break | Keep the incumbent if it still matches, else lowest `(case id, ply)` |
| 19 | Candidate census | Dropped. Nearest-miss reporting instead. |
| 20 | Nearest miss | An unfilled shape names the moments one axis away |

Decision 14's reasoning, because it will be re-litigated: at ~30 generations one
failure is over three points, and ADR 0050's 92 % was measured at n ≈ 40. A
floor at this sample size fires on noise. The instrument answers *did this
prompt change make it worse*, which is Phase D's actual question.

---

## The design artifact

### Behavior

**Invariants**

- A Fact Shape is a pure function of one `ReviewMomentCommentFacts`. Two moments
  share a shape iff the Language Layer is handed the same authoring problem:
  same marker slots, same rendering branches.
- No count, no shape list, and no `(case, ply)` pair is pinned in any file that a
  rule change does not also rewrite.
- Every generation a run issues is addressed by a Fact Shape and resolved to a
  concrete `(case, ply)` recorded in that run's record.
- A recorded resolution replays to the identical prompt, byte for byte, given an
  unchanged corpus.

**Success**

- `census` prints the shape multiset the ladder exhibits, which of them the
  corpus supplies, which are gaps, and the nearest miss for each unfilled shape.
  Exit 0 when every censused shape is either filled or a recorded gap.
- `run` issues one generation per filled shape against the pinned route, scores
  it with the shipping gates, and writes a run record. Exit 0 always; the verdict
  is the delta report, not the exit code.

**Failure semantics**

| Condition | Behavior |
| --- | --- |
| Censused shape has no exemplar and no recorded gap | `census` exits non-zero, names the shape and its nearest misses |
| Recorded resolution names a `(case, ply)` the corpus no longer records | the fast `#[test]` fails, naming shape, case and ply |
| Exemplar's facts digest moved since the resolution was recorded | replay refuses; re-resolution is required, and the run record says so |
| Ladder is absent or unreadable | `census` fails; it never silently censuses a smaller pool |
| Provider call fails, times out, or is refused | recorded as the outcome for that shape; the run continues |
| Estimated spend exceeds `--max-dollars` | refuse before the first request |

### Types and contracts

```rust
// services/coach-engine/src/critical_moment_comment/fact_shape.rs
// Owner: the module that owns CommentFactsPolicy. Nothing else may derive a shape.

/// What makes two Review Moments the same authoring test case.
///
/// Derived from `CommentFactsPolicy::for_facts` — the one place markers are
/// chosen — plus the enum discriminants that selected each rendering. Every
/// derivation is an exhaustive `match`, so a new contract variant is a compile
/// error here rather than a shape that silently collapses into its neighbour.
pub struct FactShape {
    path: CommentPath,                 // Positive | Improvement | Neutral
    markers: Vec<MarkerSlot>,          // sorted by name; carries pop and takeaway presence
    discriminants: ShapeDiscriminants,
}

pub struct MarkerSlot {
    marker: &'static str,
    required: bool,
    form: MarkerFormKind,              // Plain | Literal | Shaped | OwnSentence
}

/// One variant per `CommentPath`, carrying every enum a rendering function
/// matches on. Each is an exhaustive `match`, so a new contract variant is a
/// compile error rather than a silent collapse into a neighbouring shape.
pub enum ShapeDiscriminants {
    Positive {
        grade: PositiveHighlightGrade,
        /// `positive_difficulty_text` branches on `(grade, elo_relative)`.
        /// Great always carries an EloRelative reason, so this splits Good only.
        elo_relative: bool,
        achievement: AchievementKind,           // discriminant of achievements[0]
        payoff: Option<MechanismPayoffKind>,    // Some iff achievement is TacticalPayoff
        played_outcome: PlayedOutcomeKind,      // Analyzed | Terminal
    },
    Improvement {
        outcome: ImprovementOutcomeKind,        // ImprovedAnalyzed | AvoidedTerminal
        /// The ONLY axis that recovers `Improvement / mate`.
        /// `improvement_correction_marker_text` does not branch on mate — both
        /// arms render `display.best_evaluation.score` — so without this the key
        /// cannot see the shape #534 is named after. Measured on the ladder:
        /// `MissedForcedMate` <=> a mate better-evaluation, 21 of 21, exact.
        residual: GameReviewResidualClassification,
        played_outcome: PlayedOutcomeKind,
    },
    Neutral {
        reasons: BTreeSet<NeutralReviewReason>,
        played_outcome: PlayedOutcomeKind,
    },
}

impl FactShape {
    pub fn of(facts: &ReviewMomentCommentFacts) -> Self;
    /// Axes on which two shapes differ. Empty iff equal.
    pub fn difference(&self, other: &Self) -> Vec<ShapeAxis>;
    /// Stable, human-readable, and the key in every record.
    pub fn id(&self) -> FactShapeId;
}
```

`playedPopularity` and `takeaway` **presence** need no discriminant field —
they are marker slots, which is the point of deriving from the policy.

**Takeaway *theme* is deliberately not a discriminant.** It would add 9 shapes
(56 → 65) for an optional marker rendering one of four fixed one-liners carrying
no moment-specific content; the authoring variable is whether the model uses the
slot, which presence already captures. This is the one hand-made coarsening of
the derivation rule, so it is recorded — dated, with its reason — in
`fact-shape-gaps.json` beside the gaps, never left implicit. The exhaustive
`match` still forces a decision when a fifth theme is added.

```rust
// services/coach-engine/src/pipeline_evaluation/fact_shape_resolution.rs

pub struct ExemplarResolution {
    resolved_at: String,
    corpus_digest: String,
    exemplars: BTreeMap<FactShapeId, Exemplar>,
}

pub struct Exemplar {
    case_id: String,
    ply: u16,
    /// Digests the ReviewMomentCommentFacts. A re-recorded corpus that moves
    /// this invalidates replay instead of silently changing the subject.
    facts_digest: String,
}

pub struct FactShapeCensus {
    /// Every shape the ladder exhibits, with its occurrence count.
    observed: BTreeMap<FactShapeId, usize>,
    filled: BTreeMap<FactShapeId, Exemplar>,
    gaps: Vec<RecordedGap>,
    unfilled: Vec<Unfilled>,
}

pub struct RecordedGap {
    shape: FactShapeId,
    recorded_on: String,
    reason: String,          // why neither search nor construction could supply it
}

pub struct Unfilled {
    shape: FactShapeId,
    /// Corpus moments one axis away, with the axis named.
    nearest: Vec<(String, u16, ShapeAxis)>,
}

pub fn census(ladder: &Path, corpus: &Path, resolution: &Path)
    -> Result<FactShapeCensus, CensusError>;

/// Incumbent-stable: an exemplar that still matches its shape is kept; otherwise
/// the lowest (case id, ply) among matches. Re-resolution churns only where the
/// rules moved.
pub fn resolve(corpus: &Path, prior: Option<&ExemplarResolution>)
    -> Result<ExemplarResolution, ResolveError>;
```

```rust
// services/coach-engine/src/bin/language_layer_prose_regression.rs

enum Command {
    /// Reads the ladder, reports coverage. No spending.
    Census { ladder: PathBuf, corpus: PathBuf, resolution: PathBuf },
    /// Re-resolves exemplars against the corpus and rewrites the resolution.
    Resolve { corpus: PathBuf, resolution: PathBuf },
    /// Issues one generation per filled shape against the pinned route.
    Run { resolution: PathBuf, out: PathBuf, api_key: PathBuf,
          max_dollars: f64, dry_run: bool },
    /// Per-shape delta between two run records. No provider.
    Compare { before: PathBuf, after: PathBuf },
}
```

### Call paths and data flow

```
census:
  gotham/reviews/*.json -> EpisodeReview -> GameReviewCriticalMoment
    -> ReviewMomentCommentFacts::try_from_presented_moment
    -> FactShape::of                                  -> observed multiset
  corpus/*.case.json -> recorded_comment_case          -> FactShape::of per moment
  observed \ (filled ∪ gaps) -> Unfilled + FactShape::difference -> nearest misses
  every shape filled or gapped -> exit 0 | otherwise exit non-zero, shapes named

resolve:
  corpus -> shapes per (case, ply)
    -> incumbent still matches ? keep : lowest (case id, ply)
    -> ExemplarResolution { facts_digest per exemplar } -> written beside the corpus

run:
  ExemplarResolution -> recorded_comment_case(case) -> moment at ply
    -> facts_digest matches ? proceed : refuse, naming the shape
    -> compile_comment_prompt(facts, None, CoachingProfileProjection::cold_start())
    -> LanguageLayerProvider (pinned route, ADR 0050)
    -> diagnose_hosted_comment_text -> authored | CommentProseRejection
    -> RunRecord line { shape, case, ply, prompt digest, completion,
                        verdict, rejection, latency, tokens, cost }
    -> ordered rarest shape first, so a human sample reads the rare shapes
       rather than the fifth capturedPiece

compare:
  two RunRecords, joined on FactShapeId
    -> per-shape { was, now, rejection discipline } -> delta table

failure propagation:
  provider error / refusal   -> recorded as that shape's outcome; run continues
  facts_digest mismatch      -> refuse the whole run; the subject changed
  ladder unreadable          -> census fails; never censuses a smaller pool
```

### Boundaries and seams

| Policy owner | Production implementation | Test substitute |
| --- | --- | --- |
| Which markers a moment offers | `CommentFactsPolicy::for_facts` | none — the real policy; that is the point |
| Fact Shape derivation | `FactShape::of`, exhaustive matches | none |
| What the corpus records | `recorded_comment_case` | the pinned corpus, replayed with no provider |
| Coverage target | `census` over `gotham/reviews/` | a small fixture ladder under `tests/` |
| Prose admission | `diagnose_hosted_comment_text` | recorded generations replayed offline |
| Generation | `LanguageLayerProvider` | recorded `RunRecord`; `--dry-run` compiles and prices without a request |

The seam that carries the design: **exhaustive `match` in `FactShape::of`.** When
A3 adds depth to `TacticalPayoff`, the derivation fails to compile until someone
decides whether depth is a shape axis. That is the property the old string
encoding could not have, and it is enforced by the compiler rather than by a test.

### File map

```
DONE   services/coach-engine/src/critical_moment_comment/fact_shape.rs
         FactShape, MarkerSlot, ShapeDiscriminants, ShapeAxis, FactShape::of,
         seven contract mirrors, 12 unit tests. Lives with CommentFactsPolicy
         because it is derived from it.
DONE   services/coach-engine/src/critical_moment_comment.rs
         mod fact_shape + re-export. No behavioural change to authoring.
DONE   services/coach-engine/src/pipeline_evaluation/fact_shape_resolution.rs
         ExemplarResolution, FactShapeCensus, RecordedGap, census(), resolve(),
         and verify_resolution() — which is what both the fast test and (later)
         the runner's replay refusal read. Two deviations from the sketch above,
         both landed deliberately: census() takes the parsed resolution and gaps
         rather than their paths, because the gaps file is a fourth input the
         signature did not carry and the tests build both inline; and one
         FactShapeResolutionError wraps EvaluationError rather than a separate
         CensusError and ResolveError, because every failure either is an
         evaluation failure or is about the ladder.
DONE   services/coach-engine/src/pipeline_evaluation.rs
         mod + pub use. It already owns "replay a corpus case into authorable
         moments"; resolution is the same responsibility.
DONE   services/coach-engine/src/bin/language_layer_prose_regression.rs
         census | resolve | search | run | compare — the command line and the
         printing only.
DONE   services/coach-engine/src/bin/language_layer_prose_regression/run.rs
         The measurement: the run record wire format, one generation per shape,
         and the delta. Split out because a CLI and a benchmark are two jobs.
DONE   services/coach-engine/Cargo.toml
         one [[bin]] entry, test = false.
DONE   services/coach-engine/tests/fact_shape_resolution.rs
         The fixture ladder is built by the test from corpus moments into a temp
         directory rather than checked in: a checked-in one would pin (case, ply)
         exactly where the design refuses to.
DONE   services/coach-engine/tests/domain.rs
         register the module.
DONE   services/coach-engine/evaluation/corpus/fact-shape-resolution.json
DONE   services/coach-engine/evaluation/corpus/fact-shape-gaps.json
         Recorded gaps, and the dated coarsenings of the derivation rule
         (today: takeaway theme). Both are hand decisions, so both are visible.
         No gap is recorded yet: every one of the 21 unfilled shapes is step 3's
         to search for or construct before any of them is called unfillable.
DONE   services/coach-engine/evaluation/prose-regression/README.md
         what the directory is, the standard a run record must meet to be
         checked in (evidence for a decision, per the bake-off README precedent),
         and what the first baseline says about the prompt.
DONE   services/coach-engine/evaluation/README.md
         census, resolve, run and compare documented beside the existing
         evaluation commands.
modify docs/prototypes/bake-off-review-session-task-set.md
         dated note: §5.1's nineteen is superseded; §2's 13-shape taxonomy is
         superseded by the derived Fact Shape, which is finer on takeaway and
         coarser on nothing.
DONE   docs/adr/0062-address-language-layer-exemplars-by-derived-fact-shape.md
         0060 and 0061 landed on main first, so the number moved.
DONE   CONTEXT.md
         glossary: Fact Shape, Exemplar, Fact Shape Census.
```

Nothing under `evaluation/bake-off/` is touched. It stays the archive of the
retired harness, described by its own README.

### Validation

**Behavioral unit tests** (`tests/fact_shape_resolution.rs`)

- Two moments with different SANs, evaluations and squares but the same policy
  branch derive the **same** shape. The chess differs; the authoring problem
  does not.
- `Positive / good / capturedPiece / pop` **with** a takeaway and **without**
  derive different shapes. The measured 9-versus-4 split in the current corpus
  is the fixture.
- Grade, achievement kind, improvement outcome kind and neutral reason set each
  move the shape.
- `FactShape::difference` returns exactly one axis for a one-axis pair, and that
  is what nearest-miss reporting prints.
- Incumbent-stable tie-break: re-resolving an unchanged corpus returns a
  byte-identical resolution.

**Boundary-level integration checks**

- Every exemplar in the recorded resolution still resolves against the corpus,
  and its `facts_digest` still matches. This is the honest replacement for the
  dead `every_frozen_grounding_entry_addresses_a_moment_the_corpus_still_records`
  test, and it reads only the corpus — never the 65 MB ladder.
- `census` over a small fixture ladder under `tests/` produces the expected shape
  multiset, and an artificially absent shape is reported unfilled with its
  nearest miss named.
- `run --dry-run` compiles a prompt for every filled shape and prices the run
  without issuing a request.
- A recorded `RunRecord` replays to the same gate verdicts with no provider —
  the pattern `tests/marker_seam_replay.rs` already uses.

**Not tested, deliberately:** that adding a contract variant changes the shape.
It is a compile error, which is stronger than a test and cannot be asserted from
inside the same crate.

---

## Sequence

| Step | Work | Precondition |
| --- | --- | --- |
| 0 | `gotham review --force` over 142 games — the census authority is stale by a whole payoff change | **Verified available 2026-08-31**: Docker server 29.7.2 responsive, Maia container healthy for 8 h on `127.0.0.1:38271` at digest `ab3b6dc16b75c360…` (the digest every corpus case pins), Stockfish at `units/0.2.0-local-coach.4/bin/stockfish`. §7's hang does not reproduce. ~28 min of provider time at §5.4's measured rates. One observation of a daemon that has failed before — re-check at the time, do not assume. |
| 1 | `FactShape` + derivation + unit tests — **done**; 12 tests, fmt and clippy clean, all Cargo targets sweep | — |
| 2 | `census`, `resolve`, the resolution file, the fast test — **done 2026-09-01**. The corpus resolves to **23** Exemplars; the census observes **43** shapes (37 ladder + 6 Player-selected), fills 22, and reports 21 unfilled with their nearest misses. It reads the 142-Game ladder in under two seconds and exits 1 while any shape is unfilled, so it is step 3's work list rather than a gate today. Five `domain` tests, 0.2 s | 0, 1 |
| 3 | Fill unfilled shapes: search the ladder, mint corpus cases; construct where search fails; record genuine gaps — **done 2026-09-01**. `search` was added to do the finding: it names the ladder Games behind each unfilled shape and prints a greedy covering set. Every one of the 20 was reachable from the ladder, so nothing was constructed and no gap was recorded. **14 Games cover all 20**, minted in one `accept-live-evaluation` pass over a scratch corpus (~11 min). Corpus 16 -> 31 cases; the census reads **43 observed, 43 filled, 0 unfilled, exit 0**, and `resolve` holds 44 Exemplars — the extra is a shape the corpus supplies that the ladder no longer exhibits. Cost: the corpus replay grew from 84 s to about five minutes, so it moved to its own `corpus` test target (gate-run, beside `domain`/`session`/`boundary`/`runtime`) and `domain` came back to 30 s | 2 |
| 4 | Runner + `compare`; take the **baseline run** on the pinned route — **done 2026-09-01**. At full coverage: 43 generations, **30 published**, 13 rejected, **$0.0411**, recorded at `evaluation/prose-regression/baseline-2026-09-01.jsonl`. Two independent same-prompt pairs each moved **0 shapes**, so with temperature off and seed on one generation per shape is enough to read a delta. The instrument's first catch was the runner itself — it authored every shape with no intent context, which production supplies for every Positive and Improvement moment; fixing that moved the then-23-shape run from 13/23 to 18/23. Eight of the 13 remaining rejections are the `{playedMove}` marker, repeated or replaced by prose. Three prompt edits aimed at the one Neutral intent rejection each cost five shapes and were reverted; see `evaluation/prose-regression/README.md` | 3, an OpenRouter key |
| 5 | Hand to Phase D (#536) — **ready 2026-09-01**: the baseline is recorded, `compare` reads it, and the instrument has already refused three prompt edits on measured evidence | 4 |

A3 lands after step 4's baseline. Its corpus re-accept then re-resolves the
exemplars, and the step-4 baseline versus the post-A3 run is the first real
exercise of the whole design.

## Glossary entries proposed for `CONTEXT.md`

**Fact Shape**:
The authoring problem one Review Moment presents to the Language Layer: the
marker slots its facts offer, with the rendering branch each slot took. Two
Review Moments share a Fact Shape when the Language Layer is handed the same
problem, whatever chess produced them. Derived, never enumerated.
_Avoid_: fact bundle, moment kind, Critical Moment Classification, prompt shape

**Exemplar**:
The one Review Moment a Fact Shape resolves to for measurement. Addressed by its
Fact Shape and resolved against the pinned evaluation corpus, never named by ply.
_Avoid_: frozen case, task, grounding entry, test case

**Fact Shape Census**:
The count of Fact Shapes the GothamChess corpus exhibits, and which of them the
pinned evaluation corpus supplies an Exemplar for. It is the coverage authority;
no Fact Shape count is pinned anywhere else.
_Avoid_: frozen set, task set, coverage baseline

## ADR 0062, written 2026-09-01

[`docs/adr/0062-address-language-layer-exemplars-by-derived-fact-shape.md`](../adr/0062-address-language-layer-exemplars-by-derived-fact-shape.md).
*Address Language Layer exemplars by derived Fact Shape.* Records that §5.1's
nineteen and §2's hand-written thirteen are superseded; that the coverage
authority is a census rather than a spec section; that the count follows
coverage; and that a Fact Shape is derived from `CommentFactsPolicy` so a
contract change is a compile error rather than a stale pointer. It narrows
nothing in ADR 0050 — the pin, the budgets and the privacy claim stand.

## Step 0's residue

Re-reviewing the ladder invalidated everything derived from it, and not all of it
can be rebuilt on this machine.

| Artifact | State |
| --- | --- |
| `reviews/`, `index.json` | refreshed, 142 Games, 0 failures |
| `semantic/`, `results/`, `reports/` | **refreshed 2026-09-02**. `raw/` keeps PGNs and review events but not the YouTube transcripts both phases read, and this workspace had never fetched its own, so both phases failed on all 40 for four sessions. One `gotham fetch` restored all 40 transcripts and both ran clean. The artifacts now describe the re-reviewed ladder: Critical Moments compared 749 → 721, matched 208 → 184, Decision Explanation supports 643 → 720, idea agreement 1 → 2, decision-window agreement 2 → 0. `recompare-semantic --check` rebuilds all 42 semantic artifacts from committed data with no transcript in hand, exit 0. Episode 40 carries the one source drift: YouTube regenerated its captions since the July fetch, moving every cue timestamp and dropping two commentary moments, while the other 39 reproduce their old commentary sets exactly |
| `decision-explanation-concepts.baseline.json` | **accepted 2026-09-02** at 720 replayed moments and 720 paths, once the one line the corpus growth did not explain was traced. `DefensiveMove` fell 137 → 57 while every other concept rose with the ladder. Matching moments by Game and ply across the two `reviews/` trees (`1b88118f^` against `1b88118f`): 105 of the 137 are no longer selected Critical Moments at all, 29 kept the concept, 3 were re-explained, and 27 of the 57 are new moments, mostly `queenExchange`. 95 of the 105 were `positiveHighlight` / `winsMaterialOutright` — the highlights the payoff-depth gate (`e49faa7c`) made Neutral — and the detector fires only when the played move was the engine's first choice, so it lived in exactly the cohort that gate retired. The replay reproduces every stored explanation exactly; nothing in the detector moved. `accept-explanation-baseline` refused first: `CANONICAL_REPLAYED_MOMENTS` in `explanation_replay.rs` was a hand pin at 643, and is now 720 — the ladder's 721 less the one moment (episode 8, ply 66) that abstained with no proof-valid concept |

`gotham review` prints `benchmark failure: N review operation(s) failed` as its
last line, and the first attempt's ten failures were visible nowhere else.
**It does exit non-zero, though — the earlier claim here that it exits 0 was
wrong.** Measured 2026-09-02 against a scratch index whose corpus is absent:
`gotham review --episode 1` exits **6** and `gotham semantic --episode 1` exits
**6**, both printing that line. `run_selected_episodes` ends in
`ensure!(failures == 0)`, `run_review` propagates it, and `CliError::Benchmark`
maps to `EXIT_EVALUATION = 6`, the same code `replay-explanations` returns. A
run that appeared to succeed was reporting the exit status of a pipeline stage
rather than the CLI's — one more reason not to pipe a long run through `tail`.

One rejection also aborts its whole episode, so ten failing Games cost 29 —
that is why the first attempt left the ladder 80 % updated with `index.json`
regressed to `failed`, and why it was restored rather than kept.

## Questions settled after drafting

All five closed on 2026-08-31, four of them by measurement rather than judgment.

1. **`residualOutcome` and played-move-outcome kind are discriminants.** Reading
   the renderings settled it: `improvement_correction_marker_text` does not
   branch on mate, so without the residual the key cannot see
   `Improvement / mate` — the shape #534 is named after. `elo_relative` joins
   them, since `positive_difficulty_text` branches on it for a required marker.
   The first draft was the derivation rule applied selectively, which is
   hand-curation in a derivation's clothes. Cost: 23 → 36 shapes, $0.027 a run
   (measured against the implementation, not estimated).
2. **Takeaway theme: presence only**, recorded as the one deliberate coarsening.
   Neutral offers no `{takeaway}` at all, so the axis exists on two paths only.
3. **Typed rendering returns: measured 2026-09-02, and not needed.**
   `FactShape::of` reads the contract enums, not the rendering functions, so the
   risk was the discriminant *list* drifting from what the renderings branch on.
   `every_ladder_fact_shape_renders_one_frame` renders all 721 ladder moments
   the way the safe fallback does, plus every marker each offers, normalises
   out notation, squares, roles, colours and figures, and groups the frames by
   shape. Once five closed enumerations the shape deliberately does not carry
   are bucketed, no shape renders two frames: the evaluation label splits 44 of
   the ladder's 109 shapes, the human rank wording 51, what the opponent's
   resource does 26, the takeaway theme 9 (the coarsening in 2 above), and
   whether the better move takes or hits its target 3. Nothing else splits a
   shape, so the list is complete today, and the test holds it from here: a
   renderer branch keyed on something the shape does not name fails with the
   shape and both frames. Typed returns would buy nothing that test does not
   already hold.
4. **Step 0's runtime precondition is satisfied.** See the sequence table.
5. **36 shapes, affordable.** One generation each stands. Re-measure after the
   step-0 re-review, since 36 comes off the stale ladder. The binding limit is
   not money but how much prose a human will read for §4's two jobs, so the run
   record sorts by shape rarity and a sample reads the rare shapes first.

## Unresolved

None blocking. #578 carried what this plan and
`plan-review-commentary-truth-and-voice.md` left behind. Three of its items are
now settled here — the Decision Explanation baseline (accepted, see the residue
table), `gotham review`'s exit code (it was never 0), and typed rendering
returns (3 above, closed by measurement). The two that remain, the publish-rate
re-read and the before/after of the Coaching Board, need live traffic and a
running app.
