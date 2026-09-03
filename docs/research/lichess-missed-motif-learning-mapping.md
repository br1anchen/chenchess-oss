# Mapping missed chess ideas to Lichess learning material

Research date: 2026-08-03

## Recommendation

Classify a Critical Moment locally from two engine-backed instructional
episodes:

1. the **missed best idea**: the Position before the Player move plus the
   engine-best line; and
2. the **conceded refutation**: the Position before the Player move, the
   played move as the setup/blunder, and the opponent's post-move engine line.

Emit exact Lichess puzzle-theme keys with typed board-and-line evidence. Every
accepted key has a drill resource at
`https://lichess.org/training/<exactThemeKey>`. Add a Lichess Practice link
only when Lichess has a genuinely corresponding, fixed curriculum module.
Practice is not a dynamic position classifier and cannot cover every theme.

This model fills the reported gap without asking the language model to infer a
motif or author a URL. It also preserves the existing Learning Plan boundary:
Rust owns evidence, identity, ranking, and resource materialization; the
language layer only explains selected tracks.

## Primary-source findings

### Puzzle themes are the scalable resource surface

Lichess's current theme registry defines the exact case-sensitive keys and
organizes them into phases, motifs, advanced motifs, mates, special moves,
goals, lengths, and origins. The complete current registry is
[`PuzzleTheme.scala`](https://github.com/lichess-org/lila/blob/305bd69557ec15fe78bf638efd3819e94a153b95/modules/puzzle/src/main/PuzzleTheme.scala#L25-L204);
the player-facing definitions are on the official
[Puzzle Themes page](https://lichess.org/training/themes) and in the
[translation source](https://github.com/lichess-org/lila/blob/305bd69557ec15fe78bf638efd3819e94a153b95/translation/source/puzzleTheme.xml#L1-L156).

Lichess routes a theme key as `/training/:angleOrId` and also supports an
optional `/training/:angle/{white|black|random}` form
([routes](https://github.com/lichess-org/lila/blob/305bd69557ec15fe78bf638efd3819e94a153b95/conf/routes#L167-L186)).
ChenChess should keep the color-neutral URL canonical and let Lichess choose
the puzzle color.

The engine-actionable identifiers are:

| Category           | Exact keys                                                                                                                                                                                                        |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Motifs             | `advancedPawn`, `attackingF2F7`, `capturingDefender`, `discoveredAttack`, `doubleCheck`, `exposedKing`, `fork`, `hangingPiece`, `kingsideAttack`, `pin`, `queensideAttack`, `sacrifice`, `skewer`, `trappedPiece` |
| Advanced motifs    | `attraction`, `clearance`, `collinearMove`, `discoveredCheck`, `defensiveMove`, `deflection`, `interference`, `intermezzo`, `quietMove`, `xRayAttack`, `zugzwang`                                                 |
| Special moves      | `castling`, `enPassant`, `promotion`, `underPromotion`                                                                                                                                                            |
| Phase and material | `opening`, `middlegame`, `endgame`, `rookEndgame`, `bishopEndgame`, `pawnEndgame`, `knightEndgame`, `queenEndgame`, `queenRookEndgame`                                                                            |
| Goal               | `equality`, `advantage`, `crushing`, `mate`                                                                                                                                                                       |
| Mate length        | `mateIn1`, `mateIn2`, `mateIn3`, `mateIn4`, `mateIn5`                                                                                                                                                             |

Thus `fork` maps to [Fork training](https://lichess.org/training/fork),
`hangingPiece` to
[Hanging-piece training](https://lichess.org/training/hangingPiece), and so
on. The registry also contains named mate-pattern keys; those should be added
only with exact terminal-board predicates, not from a generic mate score.

The official puzzle API can fetch a random puzzle filtered by a theme/opening
and difficulty, but it does **not** classify an arbitrary FEN or principal
variation. `GET /api/puzzle/batch/{angle}` requires `puzzle:read`, accepts
one-to-fifty puzzles, and explicitly directs bulk consumers to the public
database
([OpenAPI operation](https://github.com/lichess-org/api/blob/5a47636233bff2715a94389d69ef7c1212247f5b/doc/specs/tags/puzzles/api-puzzle-batch-angle.yaml#L1-L73)).
Linking to `/training/<key>` requires no API integration. ChenChess should
classify locally.

### Lichess Practice is a fixed curriculum

Practice is a static whitelist of 32 studies in five sections
([`PracticeSections.scala`](https://github.com/lichess-org/lila/blob/305bd69557ec15fe78bf638efd3819e94a153b95/modules/practice/src/main/PracticeSections.scala#L7-L70)).
Its routes address a section, study slug, study ID, and optional chapter ID;
there is no public route that accepts a Position or motif and chooses a lesson
([routes](https://github.com/lichess-org/lila/blob/305bd69557ec15fe78bf638efd3819e94a153b95/conf/routes#L348-L354)).

Use the following exact companions:

| Theme key                             | Learn resource                                                                                     |
| ------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `pin`                                 | [The Pin](https://lichess.org/practice/fundamental-tactics/the-pin/9ogFv8Ac)                       |
| `skewer`                              | [The Skewer](https://lichess.org/practice/fundamental-tactics/the-skewer/tuoBxVE5)                 |
| `fork`                                | [The Fork](https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p)                     |
| `discoveredAttack`, `discoveredCheck` | [Discovered Attacks](https://lichess.org/practice/fundamental-tactics/discovered-attacks/MnsJEWnI) |
| `doubleCheck`                         | [Double Check](https://lichess.org/practice/fundamental-tactics/double-check/RUQASaZm)             |
| `intermezzo`                          | [Zwischenzug](https://lichess.org/practice/fundamental-tactics/zwischenzug/ITWY4GN2)               |
| `xRayAttack`                          | [X-Ray](https://lichess.org/practice/fundamental-tactics/x-ray/lyVYjhPG)                           |
| `zugzwang`                            | [Zugzwang](https://lichess.org/practice/advanced-tactics/zugzwang/9cKgYrHb)                        |
| `interference`                        | [Interference](https://lichess.org/practice/advanced-tactics/interference/g1fxVZu9)                |
| `deflection`                          | [Deflection](https://lichess.org/practice/advanced-tactics/deflection/kdKpaYLW)                    |
| `attraction`                          | [Attraction](https://lichess.org/practice/advanced-tactics/attraction/jOZejFWk)                    |
| `underPromotion`                      | [Underpromotion](https://lichess.org/practice/advanced-tactics/underpromotion/49fDW0wP)            |
| `clearance`                           | [Clearance](https://lichess.org/practice/advanced-tactics/clearance/Grmtwuft)                      |

Lichess also has an
[Overloaded Pieces](https://lichess.org/practice/fundamental-tactics/overloaded-pieces/o734CNqp)
module and an
[Undermining](https://lichess.org/practice/advanced-tactics/undermining/udx042D6)
module. These are useful only when ChenChess detects those exact ideas; they
must not be presented as universal aliases for `deflection` or
`capturingDefender`. Conversely, `hangingPiece`, `trappedPiece`, and several
other training themes have no exact Practice module. Their learning track
should contain a drill resource only.

### Motifs require a line, not just an evaluation

The official Lichess database currently publishes more than six million
puzzles with `FEN`, `Moves`, and `Themes`. It specifies that the FEN is before
the opponent's setup/blunder, the first move is applied before showing the
Position to the solver, and the second move begins the solution. It also says
the puzzles were automatically tagged and that player votes subsequently
refine tags
([Lichess puzzle database](https://database.lichess.org/#puzzles)).

That page identifies the Lichess puzzle tagger as the implementation used for
automatic tagging. Its top-level classifier evaluates an alternating line and
can emit multiple themes
([`cook`](https://github.com/ornicar/lichess-puzzler/blob/c188837cd2411d5c17d4f33c59ac38a8722d694f/tagger/cook.py#L32-L167)).
For example:

- a fork inspects the moved attacker's targets, their value, and whether the
  attacker is tactically unsafe
  ([predicate](https://github.com/ornicar/lichess-puzzler/blob/c188837cd2411d5c17d4f33c59ac38a8722d694f/tagger/cook.py#L217-L239));
- a hanging piece inspects the first solution capture, defense, recapture
  context, and retained material
  ([predicate](https://github.com/ornicar/lichess-puzzler/blob/c188837cd2411d5c17d4f33c59ac38a8722d694f/tagger/cook.py#L242-L266));
- discovered attacks/checks, quiet/defensive moves, sacrifices, pins, skewers,
  interference, intermezzi, clearance, and capture-the-defender each have
  separate line-aware predicates in the same primary source.

The tagger is AGPL-3.0
([license](https://github.com/ornicar/lichess-puzzler/blob/c188837cd2411d5c17d4f33c59ac38a8722d694f/LICENSE)).
Use it as a behavior and validation reference. Write independent,
ChenChess-native Rust predicates unless the project deliberately accepts the
license consequences of copying its implementation.

## Gap diagnosed in the pre-fix implementation

The inspected catalog recognized only `fork`, `hangingPiece`, passed-pawn
promotion, and three exact opening mappings
([catalog](../../services/coach-engine/src/learning_plan/catalog.rs#L15-L24)).
Motif selection considered only a material-winning mechanism attributed to the
engine-best move for an improvement opportunity
(motif selector, `learning_plan/motif.rs:63-100` — historical; that module was reorganised after this was written).
It did not classify the opponent's punishment of the played move. The fork
detector also accepts exactly three plies
(fork detector, `learning_plan/fork.rs:56-127` — historical; that module was reorganised after this was written),
while the hanging-piece detector accepts exactly one
(hanging-piece detector, `learning_plan/hanging_piece.rs:54-73` — historical; that module was reorganised after this was written).

The necessary evidence was already retained: a review moment could materialize
both the engine-best line and the post-move refutation
([objective-line construction](../../services/coach-engine/src/review_facts/game_review.rs#L482-L519)).
The missing seam was use of that second line during learning selection.

### The reported `13...Bxb5` position

The fixture records `13...Bxb5` in the canonical game
([PGN](../../services/coach-engine/evaluation/fixtures/Synthet1/lichess-export.pgn#L21)).
Before that move, the recorded best line begins `13...cxd4`
([pre-move analysis](../../services/coach-engine/evaluation/fixtures/Synthet1/review-session-provider-recording.json#L4520));
after the move, Stockfish's best reply begins `14.cxb5`
([post-move analysis](../../services/coach-engine/evaluation/fixtures/Synthet1/review-session-provider-recording.json#L5035)),
capturing the bishop with no immediate recapture.
That is a strong `hangingPiece` **conceded-refutation** candidate and should
materialize:

- Drill: <https://lichess.org/training/hangingPiece>
- No Practice companion, because Lichess has no exact hanging-piece Practice
  module.

The missed-best line `13...cxd4 14.Ng5 Na5 ...` has no clean early motif under
the current evidence. Its +257 centipawn result can support the generic
`advantage` stream only as an explicit fallback, not as a fabricated tactical
motif: <https://lichess.org/training/advantage>.

#### Decision on the later `...b3`

The later `...b3` in that best line does **not** honestly support an exact
`advancedPawn` track. Lichess describes the theme as a pawn “deep into the
opponent position, maybe threatening to promote”
([definition](https://github.com/lichess-org/lila/blob/305bd69557ec15fe78bf638efd3819e94a153b95/translation/source/puzzleTheme.xml#L3-L4)).
More importantly, its automatic tagger checks only solver moves and calls the
stricter `is_very_advanced_pawn_move`
([theme predicate](https://github.com/ornicar/lichess-puzzler/blob/c188837cd2411d5c17d4f33c59ac38a8722d694f/tagger/cook.py#L170-L174)).
That helper requires a black pawn to arrive on rank 2 or 1 (zero-based
`to_rank < 2` after a black move), or a white pawn on rank 7 or 8
([rank predicate](https://github.com/ornicar/lichess-puzzler/blob/c188837cd2411d5c17d4f33c59ac38a8722d694f/tagger/util.py#L18-L30)).
`...b3` arrives on rank 3, so it fails the tagger's exact threshold even though
the helper considers it merely “advanced.”

If a future line does satisfy the exact predicate, the drill URL is
<https://lichess.org/training/advancedPawn>. Lichess Practice contains no
Advanced Pawn module, so there is no exact Practice companion.

For this moment, the decision-ready resource classification is:

| Candidate                                       | Classification                  | Decision                                         | Resource                                                                                                         |
| ----------------------------------------------- | ------------------------------- | ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `hangingPiece` from `13...Bxb5 14.cxb5`         | Exact conceded-refutation motif | Emit                                             | [Training](https://lichess.org/training/hangingPiece); no exact Practice module                                  |
| `advancedPawn` from later `...b3`               | Proposed missed-best motif      | Reject: rank 3 fails Lichess's exact predicate   | [URL exists](https://lichess.org/training/advancedPawn), but must not be attached here; no exact Practice module |
| `advantage` from the normalized +257 evaluation | Outcome/goal fallback           | May emit only when no semantic track outranks it | [Training](https://lichess.org/training/advantage); no Practice module                                           |
| `middlegame` from the existing Position phase   | Phase fallback                  | Lower-priority general drilling only             | [Training](https://lichess.org/training/middlegame); no Practice module                                          |

## Recommended implementation model

### 1. Normalize engine evidence into instructional episodes

Add an internal, non-wire type similar to:

```text
InstructionalEpisode {
  attribution: MissedBest | ConcededRefutation | Reinforcement,
  fen_before_setup,
  setup_move?,          // opponent move or Player blunder
  fen_before_solution,
  solution_moves,       // legal, alternating UCI line
  solver_color,
  normalized_evaluation
}
```

Construct it deterministically:

- **Conceded refutation:** Position before the Player move + played move as
  setup + post-move engine PV. This has exactly Lichess puzzle semantics.
- **Missed best:** preceding opponent move as setup when available + current
  engine-best PV. If the preceding move is unavailable, use a direct-solution
  episode and predicates whose indexing explicitly supports it.
- **Reinforcement:** preceding opponent move + the Player's correct played
  move + continuation.

Reject illegal or perspective-inconsistent lines rather than degrading to
prose.

### 2. Detect typed motifs over the episode

Build independent Rust detectors over the existing `shakmaty` line walker.
Each accepted result should carry:

- exact theme key;
- attribution and solution/setup indices;
- involved pieces and squares;
- the predicate version;
- the supporting Critical Moment ID and ply.

Prioritize the high-value, deterministic subset:

1. existing `hangingPiece` and `fork`, generalized to search the relevant
   solution prefix and both episode kinds;
2. `pin`, `skewer`, `discoveredAttack`, `discoveredCheck`, `doubleCheck`,
   `capturingDefender`, `intermezzo`, `xRayAttack`;
3. exact special moves, promotion/underpromotion, mate/mate length, and
   material-defined sacrifices;
4. the remaining advanced and mate-pattern predicates only after corpus
   precision proves acceptable.

Do not infer `trappedPiece`, `quietMove`, `defensiveMove`, `zugzwang`, or a
named mate pattern from centipawn loss alone. They require explicit mobility,
alternative-move, engine, or terminal-board evidence.

### 3. Make the catalog data-driven but closed

Define a versioned catalog entry per accepted theme:

```text
theme key
training URL
title
optional exact Practice resource
accepted detector/predicate versions
```

The training URL is deterministic from the validated key. Practice URLs remain
fully materialized and release-verified, matching the current catalog's
fail-closed behavior.

For a significant improvement opportunity, selection order should be:

1. conceded-refutation or missed-best semantic motif;
2. exact mate, promotion, endgame, or opening track;
3. one honest phase/goal fallback (`advantage`, `crushing`, or the exact
   Position phase) when no semantic track exists.

Label a fallback as general drilling, not as “the motif you missed.” Continue
to cap the game plan at two tracks, preferring repeated support across moments
and higher-impact improvement evidence.

### 4. Keep the model downstream of selection

Send the complete materialized Learning Plan and active moment subset through
the MCP tool result. The model may explain why the evidence matches the
selected theme, but must not invent a theme, substitute a nearby Practice
module, or author any link. Empty tracks should remain possible only when the
line is unavailable/invalid and no honest phase/goal fallback can be
constructed.

## Validation and release risks

1. **Perspective and indexing:** Lichess's exported FEN precedes the opponent
   setup move, whereas ChenChess's current motif code starts at the Player's
   correct move. Golden tests must cover both episode kinds and colors.
2. **PV instability:** a theme may disappear when engine depth changes or a
   payoff moves outside the retained PV. Pin detector inputs to recorded engine
   evidence and version detector policy independently from Stockfish
   provenance.
3. **False positives:** an attack geometry is not automatically a profitable
   motif. Require the payoff or retained evaluation/material that makes the
   theme instructional.
4. **Multiple tags:** Lichess puzzles commonly have several themes. ChenChess
   must deterministically rank semantic tracks rather than depend on detector
   iteration order.
5. **Goal thresholds:** normalize evaluations to the solver's perspective
   before applying Lichess-style `advantage`/`crushing` thresholds. Keep
   ChenChess's thresholds versioned instead of silently inheriting future
   Lichess changes.
6. **Practice mismatch:** never use a merely adjacent curriculum lesson as an
   exact explanation. Training-only tracks are valid.
7. **External drift:** verify every catalog URL and every theme key during the
   existing Learning Resource release check. Pin source-audit metadata to an
   upstream revision.
8. **License:** do not paste or mechanically translate the AGPL tagger into the
   service without an explicit licensing decision.

Use the public Lichess puzzle CSV as the offline precision corpus: replay each
published `FEN` + `Moves`, run the ChenChess detectors, and compare emitted
keys with Lichess's `Themes` column. The database documentation explicitly
supports this format and bulk use. Track per-theme precision first; absence of
a Lichess tag is useful negative evidence but not absolute ground truth,
because Lichess combines automatic tags with player voting.
