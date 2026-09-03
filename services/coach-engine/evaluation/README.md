# Pipeline Evaluation

The review-session canonical source and full-game provider recording live in [`fixtures/Synthet1`](fixtures/Synthet1/README.md). Its targeted verifier reconstructs the complete Game, checks capture and provider digests, and replays the deterministic baseline.

Fast Pipeline Evaluation runs the current Rule Extractor against normalized, recorded provider evidence. It does not start Stockfish, Maia, Docker, or a hosted language model.

Run the pinned corpus:

```sh
cargo run -p chen-chess-coach-engine --bin chenchess -- evaluate-fast --corpus-dir services/coach-engine/evaluation/corpus
```

The same replay is a test, in its own `corpus` target:

```sh
cargo test -p chen-chess-coach-engine --test corpus
```

It sits beside `domain`, `session`, `boundary` and `runtime` in
`chenchess-rust#test`, so the release gate runs it. It is separate because it is
the one test whose cost grows with the corpus — every minted case adds a Game to
replay — and at 47 cases it is about five minutes. Keeping it out of `domain`
takes that suite from roughly six minutes back to thirty seconds, which is the
difference between a suite you run while editing and one you avoid.

Adding a test target means naming it in two places: `[[test]]` in
`services/coach-engine/Cargo.toml`, and the command in `turbo.json`. A target
missing from the second silently never runs.

Each `*.case.json` file contains a legal PGN, Elo Profile, Review Side or selected
ply, provider provenance, and the normalized Stockfish/Maia evidence consumed by
Rule Extraction. Its `*.baseline.json` partner is the canonical Rule Extraction
result. Automatic-selection baselines also pin the complete Selector Trace:
Game length, realized adaptive target, hard maximum, candidate classification,
episode outcome, Positive reservation, utility, diversity result, priority rank,
Game-order position, and selected state. The realized selected count may be below
the adaptive target; the target is a ceiling rather than a coverage obligation.

A Game case also records the MultiPV comparison search behind each Critical
Moment's Decision Explanation, in `multiPvEvidence`, beside the `engineProvenance`
of the authoritative single-PV search. Fast evaluation replays those searches
through the real candidate-comparison path — `enrich` and `explain_decision` are
pure over a recorded MultiPV output, so no engine runs — and pins the result in
the baseline's `decisionExplanations`. Each entry states the candidate evidence,
preference proof, and capability in full, plus a `decisionExplanationRef` that
digests the whole explanation, so a change in any other part of the proof still
moves the baseline.

That coverage is the point: without it a Ranked Alternative could restate rank
one's own absolute evaluation and contradict the authoritative one without any
baseline moving (ADR 0041). A Critical Moment that reaches candidate comparison
with no recorded search fails the gate rather than being skipped.

Record the comparison searches with Stockfish alone — a comparison never consults
the Human Move Model, so recording it costs no Maia run and cannot drift against
a Maia image:

```sh
cargo run -p chen-chess-coach-engine --bin chenchess -- record-multi-pv --corpus-dir services/coach-engine/evaluation/corpus
```

The recorder refuses to write a case whose recorded Engine Analysis the running
Stockfish cannot reproduce, so a recorded gap is never measured against evidence
from a different engine. Follow it with `accept-evaluation` to refresh the
baselines. Live acceptance re-records the comparisons on its own; the standalone
command exists to add them to evidence already captured.

When a baseline differs, the command exits with code 6 and prints JSON Pointer field differences. Review the changed plies, categories, classifications, evaluations, probabilities, and ranks before accepting them.

```sh
cargo run -p chen-chess-coach-engine --bin chenchess -- accept-evaluation --corpus-dir services/coach-engine/evaluation/corpus
```

Acceptance rewrites changed fact baselines and writes `services/coach-engine/evaluation/corpus/last-accepted.diff`. That report is ignored by version control but remains in the working tree for inspection; the baseline changes themselves remain visible to Jujutsu.

## Live evaluation

Live Pipeline Evaluation acquires the installed Local Pipeline Runtime lease and refreshes each case through its pinned Stockfish and Maia providers. It never invokes a hosted language model.

```sh
chenchess evaluate-live --corpus-dir services/coach-engine/evaluation/corpus
```

The comparison permits at most 15 centipawns of drift for centipawn values and 0.02 for Maia move or win probabilities. Mate distances, moves, principal variations, candidate ranks, categories, selected plies, Critical Moment membership, and provenance remain exact. The command reports progress once per corpus case and exits with code 6 on a material difference.

Accept a reviewed live change explicitly:

```sh
chenchess accept-live-evaluation --corpus-dir services/coach-engine/evaluation/corpus
```

Acceptance rewrites normalized evidence, exact provider provenance, and the matching fact baseline. It leaves the same `last-accepted.diff` report used by fast evaluation.

## Chronological review implementation evidence

[`chronological-review-proof.md`](chronological-review-proof.md) maps every
accepted mixed-review hard rule to named pass/fail cases and records the local
web and Coach Skill journeys. It is ordinary implementation evidence, not a
certificate or quality benchmark.

## Apple Silicon certification

The shipped limits are 400 plies per Game, 30 seconds per provider position, 600 seconds for runtime startup, four hours for a live command, and five seconds for cancellation. Review Session deadlines are also compiled production constants. Every certification report records the exact values it used.

The report contains p50, p95, and maximum latency for Game Review, Intent Projection, Objective Refutation, Alternative Move Evaluation, concurrent exploration, Coach Turn, cancellation-to-stop, and steer-to-replacement. It also retains Maia CPU and memory output, provider and model provenance, schema and policy versions, fixture and cache state, concurrency, the source revision, and typed deadline/fallback gates.

```sh
chenchess certify-live \
  --corpus-dir services/coach-engine/evaluation/corpus \
  --output services/coach-engine/evaluation/certification/apple-silicon.json
```

`certify-live` refuses to claim certification outside macOS on Apple Silicon. Other platforms still build and run the deterministic fast evaluation.

The complete release gate that runs against a published runtime is not carried
by this snapshot. Beyond `certify-live`, it performed fresh isolated installation, canonical Coach Skill
URL/paste/local-file imports, Opening Identification provenance, eligible
Practice selection and explicit omission, warm reuse, failed-update rollback,
and clean uninstall. The live authenticated HTTP journey covers URL and
pasted-PGN Game Review, pipeline and Player-selected moments, Coach Intent,
Player Plan Evaluation, Alternative Move Exploration, fallback Coach Turn
unavailability, and Stockfish exploration after fallback through the signed web
binding. Retain the resulting certification JSON with the release issue.

Codex manual certification remains a separate blocking judgment and must be retained with the release issue. Record the source revision and every field named in `skills/chenchess-coach/manual-scenarios.md`; do not infer a manual pass from the automated report. Another agent may add corroborating evidence but is not required.

## Fact Shape coverage

Language Layer measurements are addressed by **Fact Shape** — the authoring
problem a Review Moment presents — rather than by `(case, ply)`, so a rule
change re-resolves them instead of stranding them.
`corpus/fact-shape-resolution.json` records which moment stands for each shape
the corpus holds.

The corpus is the coverage authority. The private development repository also
measures against a commentary ladder — recorded human commentary aligned to
positions — but that ladder is derived from third-party games and does not ship
here, so neither does the census that reads it. What the public snapshot keeps
is the half that reads only the corpus.

The `domain` tests hold the recorded resolution to the corpus:
`every_recorded_exemplar_still_resolves_against_the_corpus` fails when a
recorded Exemplar stops resolving, `re_resolving_an_unchanged_corpus_reproduces_the_recorded_resolution`
fails when it is stale, and
`an_exemplar_whose_recorded_facts_moved_is_reported_stale` proves a moved facts
digest is reported rather than silently re-pointed. Resolution is
incumbent-stable, so an unchanged corpus rewrites the file byte for byte and a
rule change rewrites only the entries it moved.

Grow the corpus by seeding `<case-id>.case.json` beside it with the Game's PGN,
its Elo and Review Side, and an empty `evidence` array, then running
`accept-live-evaluation` over a scratch directory holding only that seed — the
live path captures the evidence and the MultiPV comparisons and writes both the
case and its baseline. A seed also needs `provenance`, which the loader requires
and the live pass then overwrites; copy it from any accepted case rather than
authoring one. Copy the accepted pair into `corpus/`, re-resolve, and bump the
pinned case count in
`tests/corpus.rs::repository_corpus_matches_all_pinned_baselines`.
