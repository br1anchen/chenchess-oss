# Chessiro and Chessvia learning-path pipelines

Research date: 2026-08-08

## Scope and evidence labels

This note asks a narrow question: how do Chessiro and Chessvia turn a reviewed game into mistake labels, themes, and a training or study recommendation? It uses only first-party pages, public deployed code, public API schemas, and the authors' papers/repositories.

- **Observed** means the behavior is present in a public client asset or OpenAPI schema.
- **Claimed** means the vendor describes it, but the implementation is not public.
- **Inferred** is used only where the evidence supports a likely interpretation; it is not treated as fact.

## Chessiro: two related pipelines

Chessiro now has both a per-game `Recommended Training` pipeline and a cross-game `Smart Shuffle` pipeline. The per-game classifier is mostly visible in deployed browser code. Smart Shuffle calls same-origin server APIs, so its inputs and outputs are visible while its selection formula is not.

### Per-game review and mistake-to-theme mapping

1. **Browser engine analysis (claimed and observed).** Chessiro says Stockfish 18 analyzes every move in the browser. The review client then consumes per-ply evaluations, best moves, principal variations, and FENs. Browser execution makes the basic review immediate and cheap, but the public material does not claim that every device runs identical search limits. [Chessiro Game Review article](https://chessiro.com/blog/inside-chessiro-game-review), [live review client](https://chessiro.com/_next/static/chunks/f2a9a524fee4452d.js)

2. **Move labels (observed).** Centipawn/pawn evaluations are converted through a logistic win-percentage function. The move-quality function compares the mover's win percentage before and after the move. Outside special cases, losses of at least 7, 10, and 20 percentage points map to `inaccuracy`, `mistake`, and `blunder`; best-move, book, terminal, "great," and "brilliant" handling adds other branches. The client therefore labels error severity from engine outcome change, not from an LLM diagnosis. [move-quality module](https://chessiro.com/_next/static/chunks/8065e6198a7faac3.js)

3. **Candidate comparison and concept detectors (observed).** For a reviewed error, the browser replays the played move and, when present, Stockfish's best move. It derives board facts for each branch and tests concrete differences: forks, discoveries, hanging captures, mate threats, pins, skewers, sacrifices, intermezzi, promotion, passed-pawn handling, rook activity, pawn breaks, king-and-pawn technique, king safety, structural damage, trades, conversion, and defensive resources. Evidence records include the ply, FEN, played and best moves, move quality, evaluation loss, a confidence, a score, and sometimes an exact puzzle theme. [focus-area classifier](https://chessiro.com/_next/static/chunks/67e987ff2700068b.js)

4. **Broader fallbacks (observed).** Not every recommendation comes from an exact tactical contrast. The same classifier adds phase and habit evidence: underdevelopment, a knight on the rim, early queen development, repeated middlegame/endgame errors, persistent pawn weaknesses, or a passed pawn that repeatedly failed to advance. These rules make the product cover more games, but they are weaker evidence than a candidate-specific motif proof. [focus-area classifier](https://chessiro.com/_next/static/chunks/67e987ff2700068b.js)

5. **Aggregation (observed).** Each theme retains at most five high-scoring evidence items. Scores combine error quality, evaluation loss, confidence, theme weights, recurrence, and game length. Low-confidence singletons are filtered. The classifier emits up to four game focus areas, a phase summary, up to three puzzle themes, and separately derived strengths such as accurate phases, verified tactics, resourceful defense, and clean conversion. [focus-area classifier](https://chessiro.com/_next/static/chunks/67e987ff2700068b.js)

6. **Recommended Training selection (observed).** A second deterministic selector maps focus-area tags to concrete training theme IDs. It prefers exact evidence, applies quality/evaluation/confidence gates, ranks by severity and specificity, merges repeated evidence for the same theme, limits duplicate families, synthesizes a calculation recommendation after multiple tactical misses, adds phase-wide fallbacks, and returns at most four recommendations with a `trainHref`. The panel displays "Needs Work" and "Nailed It." [recommendation selector](https://chessiro.com/_next/static/chunks/0cd2a99da926e3ea.js)

7. **Learning material (claimed and partly observed).** Chessiro says each recommended theme opens a definition and visual demonstration, then links to a puzzle session for that exact topic. Its August 2026 article still says a full lesson is "coming soon." Thus the current per-game "learning path" is principally *review evidence → named theme → definition/demo → themed puzzles*, not a prerequisite-ordered curriculum. [Game Review article](https://chessiro.com/blog/inside-chessiro-game-review)

The deployed taxonomy is substantially wider than the small display list. It groups track definitions under openings, tactics, calculation, strategy, conversion, and endgames, and maps each track to recognized theme IDs, mistake/concept IDs, fact IDs, and allowed evidence sources such as opening book, move list, engine evaluation, or clock. Examples include forks, pawn structure decisions, weak squares, rook coordination, simplification, pawn endgames, and fortresses/draws. This is a routing catalog, not proof by itself. [learning-track taxonomy](https://chessiro.com/_next/static/chunks/ae0c8f829ced6608.js)

### Cross-game skill profile and Smart Shuffle

Chessiro's July 2026 first-party description says a player should tag errors by phase and type, count recurrence across roughly 20–30 games, and prioritize frequency times cost. It claims Smart Shuffle uses the resulting skill profile, the player's own mistakes, weak themes, and resurfaced earlier puzzles. [Find Your Chess Weaknesses](https://chessiro.com/blog/find-your-chess-weaknesses)

The current client confirms the surrounding contract:

- `GET /api/train/tailoring` returns counts such as games analyzed and retrainable mistakes, a current focus, top weaknesses, failed retries, due "woodpecker" repetitions, and a rating window. A fallback `GET /api/analysis/skill-profile` returns reviewed-game/moment counts, top weaknesses, and average normalized rating. [tailoring client](https://chessiro.com/_next/static/chunks/e959bcc539d4b97a.js)
- `POST /api/train/session` requests a `smart-shuffle` session of 15 puzzles and receives `puzzles`, `focus`, and `plan`. Puzzle origins exposed to the UI include `own-mistake`, `weakness-theme`, `retry-failed`, `retry-attempted`/`resurfaced`, `woodpecker`, and `master-game`. [training-session client](https://chessiro.com/_next/static/chunks/f8560691771c1fd0.js)
- `POST /api/train/progress` records puzzle ID, result, mode, and up to six training themes; `POST /api/train/seen` records exposure. This is enough for an adaptive server selector to update future sessions, but does not disclose its ranking or spacing equations. [training-session client](https://chessiro.com/_next/static/chunks/f8560691771c1fd0.js)

**Known versus unknown.** It is high-confidence that game-level concept detection and recommendation ranking run in the browser, and that cross-game session assembly and progress persistence run behind server APIs. It is unknown how the server aggregates history, resolves conflicting tags, chooses the session mix, schedules repetition, estimates mastery, or validates a puzzle derived from the player's game. Public client fields show those capabilities' shape, not their implementation.

### Relationship to coaching prose

Coaching is a parallel pipeline. The review client sends PGN plus per-ply SAN, evaluations, quality, best move, PV, and FEN data to `POST /api/ai-coach`; the server streams prose back. The bulk request visible in the client does not include the selected focus-area/path object. That means Chessiro's comment and its training recommendation share engine inputs but are not visibly forced to agree through one proof record. [coach client](https://chessiro.com/_next/static/chunks/f2a9a524fee4452d.js)

## Chessvia: a hosted result contract, not a disclosed planner

Chessvia exposes `POST /analyze_game`. Its OpenAPI description says the endpoint combines Stockfish and coaching, but Chessvia calls the system proprietary and publishes no server source, thresholds, prompts, study-plan algorithm, grounding checks, or reproducible evaluation. [OpenAPI](https://api.chessvia.ai/openapi.json), [Game Analysis API](https://www.chessvia.ai/api/game-analysis)

The documented contract covers:

1. **Input.** A PGN is required. Optional controls include player color, free-form `user_stats`, player rating/skill level, motif detection, all-moves versus errors-only output, variations, "key/none/all" positional-analysis scope, per-move LLM coaching, language, style/detail, model and reasoning effort. Some controls are partner-only or ignored on standard keys. [OpenAPI](https://api.chessvia.ai/openapi.json)
2. **Per-move review.** Responses can contain before/after evaluations, expected points and loss, move classification, best move in SAN/UCI, before/after FENs, game phase, factual review text, deeper coaching text, and best-move explanation. Error lists add win-probability swing and `inaccuracy`/`mistake`/`blunder` severity. [OpenAPI](https://api.chessvia.ai/openapi.json)
3. **Game features.** The response can include phase prose, turning points, a win-probability curve, tactical-theme counts per side plus missed counts, opening deviation and principle violations, critical lines, piece activity, and game summary/accuracy. [OpenAPI](https://api.chessvia.ai/openapi.json)
4. **Study plan.** `GameStudyPlan` contains only `for_color`, string arrays for strengths and weaknesses, and four nullable free-text recommendations: `opening_work`, `tactics_work`, `strategic_work`, and `endgame_work`. It has no resource/theme IDs, evidence links, confidence, priority, prerequisite, exercise, schedule, mastery state, or cross-game identity. [OpenAPI](https://api.chessvia.ai/openapi.json)

Chessvia markets the plan as "derived from the game," with specific drills, and supports passing recent user statistics for personalization. The public contract does not reveal whether the plan is authored by rules, an LLM, or both; whether motif detection is symbolic or generated; or whether plan statements are checked against per-move evidence. No public implementation or study-plan accuracy evaluation was found. The only defensible architectural claim is that Chessvia returns a convenient one-call *report-shaped plan*, not that it maintains a durable learning path. [Game Analysis API](https://www.chessvia.ai/api/game-analysis), [OpenAPI](https://api.chessvia.ai/openapi.json)

## What the two commentary papers add

Neither paper implements mistake-to-curriculum planning. Both are useful at the narrower seam between verified chess signals and prose.

**Concept-guided Chess Commentary (CCC).** The authors sample 200,000 Lichess positions, use Stockfish 8 concept evaluators to label the top and bottom 5% for each concept, and train linear SVM probes on LeelaChessZero T78's internal representation. At inference, changes in concept scores before versus after a move prioritize what the LLM should discuss; expert evaluation, attacks, few-shot examples, and prompting are supplied to GPT-4o. This is a learned concept salience model, not a proof graph or study scheduler. Its released pipeline requires old Stockfish/LCZero/TensorFlow dependencies and an OpenAI key. [paper](https://arxiv.org/html/2410.20811), [official repository](https://github.com/ml-postech/concept-guided-chess-commentary)

The paper's human study found CCC more informative and fluent than its baselines, but correctness was 0.60 after rescaling; concept guidance did not remove wrong positional or move-evaluation claims. Its concept probes also vary substantially in accuracy. CCC is therefore useful as an optional salience/ranking signal or evaluation benchmark, not as an authority that can replace deterministic evidence. [paper, Tables 1–2 and 6](https://arxiv.org/html/2410.20811)

**Symbolic reasoning plus a controllable language model.** Lee et al. train BART to generate commentary conditioned on game state, move, and tags. Training-time tag extractors recover commentary type, move quality, suggested move, pronouns, and length from annotated text. At inference, Leela supplies move quality and best-line controls; board input explicitly lists pieces and attacks. Human judges preferred the fully conditioned model to prior baselines, but the authors report that generated details can still be false and multi-facet comments create more opportunities for error. This supports a typed, narrow claim plan feeding language generation; it does not supply a learning-track ontology or durable learner model. [paper](https://arxiv.org/abs/2212.08195)

## ChenChess comparison and integration decision

ChenChess already has the stronger semantic spine:

```text
PGN and Elo evidence
-> deterministic move classification
-> bounded Critical Moment selection
-> legal candidate replay
-> Atomic Chess Facts and Semantic Outcomes
-> concept validation and minimal Explanation Path
-> exact Learning Track and resources
-> constrained prose and Grounding Gate
```

Stockfish owns move quality and candidate order. Deterministic Rust rules own facts, concepts, outcomes, proof validity, track identity, and resource mapping. The language model arranges already-authorized claims. This is closer to the separation advocated by the symbolic paper than either external product's public contract. [Decision Explanation design](../spec/decision-explanation-proof-pipeline.md), [comment-authoring ADR](../adr/0019-author-kind-aware-review-moment-comments.md)

### Gaps in the accepted product design

1. **No cross-game learning policy.** Cross-game learner state, mastery, recurrence, scheduling, and adaptive proof minimization are explicitly deferred. The frozen `LearningPlan` is a per-game union of tracks and support, not a weekly or longitudinal study plan. It has no time budget, due date, mastery estimate, reassessment rule, or priority across games. [Decision Explanation design](../spec/decision-explanation-proof-pipeline.md), [Learning Plan ADR](../adr/0035-select-learning-plans-in-the-game-review-engine.md)

2. **No honest own-game recall or validated transfer exercise.** The presentation design defers novel transfer positions and rejects calling the already-reviewed supporting moment "Real-game application." It does not define delayed recall of that position as a separate exercise type. Chessiro's most useful product shortcut is exactly this cheaper retrieval loop. [Learning-path presentation ADR](../adr/0037-present-learning-tracks-as-missing-idea-paths.md)

3. **No operational semantic-commentary benchmark.** The Grounding Gate proves claim identity and literal grounding, not whether the prose selects the central idea, explains it correctly, or helps a learner. Existing fast and live evaluation do not invoke a hosted language model. The Gotham missing-idea benchmark is informational, has only seven explicit labels, and evaluates concept alignment rather than generated prose. [comment-authoring ADR](../adr/0019-author-kind-aware-review-moment-comments.md), [evaluation README](../../services/coach-engine/evaluation/README.md), [Gotham semantic benchmark](../../services/coach-engine/evaluation/gotham/semantic/README.md)

4. **No pedagogical salience objective.** Proof minimization optimizes sufficiency and compactness. It does not claim to choose the idea a learner most needs. CCC addresses a related focus problem, but its learned score measures feature change rather than instructional value and is not reliable enough to become proof.

### Narrow implementation drift to fix first

The broad graph design describes concepts, rules, goals, procedures, prerequisites, and resource relations, but those nodes should not all be implemented merely because the papers use the word "concept." The current product needs a smaller repair:

- The detector cascade short-circuits on the first match and persists one path, although the accepted design permits a second independent payoff or resulting pattern. Collect a bounded transient set of valid matches, select at most two deterministically, and persist only the selected minimal paths. [detector dispatch](../../services/coach-engine/src/decision_explanation/detectors.rs), [proof selection](../../services/coach-engine/src/decision_explanation/facts.rs)
- The comment contract strongly grounds correction-shaped facts but does not require the selected Explanation Path or concept to be its focal claim. Extend the existing claim structure instead of creating a parallel commentary planner. [comment policy](../../services/coach-engine/src/critical_moment_comment.rs)
- Game-level aggregation filters opening tracks even though the accepted Learning Plan decision describes opening evidence in the union. This needs an explicit decision or correction. [aggregation](../../services/coach-engine/src/decision_learning.rs), [Learning Plan ADR](../adr/0035-select-learning-plans-in-the-game-review-engine.md)
- Central-host policy calls the supporting moment a "Real-game application" while ADR 0037 explicitly defers that stage. Rename it or introduce a distinct `own-game recall` exercise. [conversation policy](../../apps/central-host/server/coach-app-conversation-policy.ts), [Learning-path presentation ADR](../adr/0037-present-learning-tracks-as-missing-idea-paths.md)

### Safe use of the papers

- **Do not integrate either released model.** CCC publishes no trained probe cache or packaged model, and its repository has no top-level license. The symbolic paper publishes no fine-tuned model or author code. Reimplement only ideas that prove useful with ChenChess-owned types and licensed data.
- **Keep CCC offline at first.** Record positions where two proof-valid concepts genuinely compete, collect expert choices of the best teaching focus, and compare a simple deterministic ranker before training a probe. A learned scorer that changes the selected Learning Track is part of canonical pedagogical policy even when it runs "after proof"; it therefore needs its own evidence, versioning, and validation before promotion.
- **Borrow control, not BART.** ChenChess already has stronger equivalents for commentary type, move quality, suggested move, board facts, length, and factual restrictions. Add only a focal path reference and allowed causal/payoff claims to the current generation contract.
- **Use GCC-Eval only as a soft offline signal.** Measure relevance, completeness, clarity, and fluency after deterministic admission. Keep separate human-rated checks for chess correctness, beginner comprehension, and later transfer. GCC-Eval does not score factual correctness and cannot be a runtime safety gate.

### Smallest useful cross-game loop

Do not begin with a mastery model or a fully event-sourced scheduler. First add one idempotent `PracticeEvent` for an exercise attempt, retaining the player, stable concept identity, source Explanation Path, exercise identity, result, assistance, duration, and time. Build sessions with a transparent query:

1. failed own-game retries;
2. recurring severe proof-backed concepts;
3. due previous attempts;
4. rating-matched exact-theme variety.

Use this learning sequence opportunistically rather than requiring every stage:

```text
immediate own-game retry
-> delayed recall of the same position
-> independent exact-theme puzzle
-> validated novel transfer position later
```

Own-game recall is retrieval practice, not evidence of transfer or mastery. Persist attempts, not generated session plans. Add a versioned mastery projection, prerequisite traversal, and time-budget scheduler only after real practice data creates a need to distinguish recurrence, recency, and competence.

## Compact comparison

| System | Mistake authority | Concept/track mapping | Plan output | Durable adaptation | Public confidence |
| --- | --- | --- | --- | --- | --- |
| Chessiro per game | Local engine deltas plus deterministic board-fact rules | Broad explicit taxonomy; scored evidence; top-four selector | Theme definitions/demos and themed puzzle links | No, within this stage | High for client behavior |
| Chessiro Smart Shuffle | Consumes cross-game profile and progress server-side | `topWeaknesses`, focus, puzzle themes, origin mix | 15-puzzle adaptive session | Claimed and contract-visible | Medium; selector is private |
| Chessvia | Proprietary Stockfish + coaching pipeline | Motif tallies and free-text strengths/weaknesses | Four free-text study buckets | `user_stats` accepted, no learner record returned | High for schema, low for derivation |
| CCC / symbolic-LM papers | Expert-model concepts or engine control tags | Commentary salience/control only | Natural-language commentary | None | Research evidence, not production contract |

The practical distinction is that Chessiro exposes a real mistake-to-theme implementation and a separate adaptive practice service, while Chessvia exposes a richer report schema but no inspectable learning algorithm. The papers strengthen the design of the commentary boundary; they do not replace a knowledge graph, evidence model, or curriculum planner.
