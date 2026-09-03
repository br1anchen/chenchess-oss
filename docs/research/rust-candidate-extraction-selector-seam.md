# Rust candidate extraction and Selector Policy seam

Research date: 2026-07-13  
Reconciled with Selector Policy v1: 2026-07-23

## Finding

The canonical seam is inside `rule_extractor::extract_with_trace`. Rule
Extraction validates whole-Game provider evidence, derives complete kind-aware
facts for the requested Review Side, and converts every qualifying Automatic
moment into a private selector candidate. `critical_moment_selector::select`
then owns Coaching Episode collapse, the adaptive target, the hard maximum,
Positive reservation, shared utility, soft diversity, deterministic tie
breaking, and restoration of Game order.

The product path calls `rule_extractor::extract` and receives only
`RuleExtraction`. Pipeline Evaluation calls `extract_with_trace` and stores
`{ facts, selectorTrace }`. This keeps ranking telemetry available for exact
reproduction without turning it into Player-facing Game Review state.

- [Rule Extractor](../../services/coach-engine/src/rule_extractor.rs)
- [Critical Moment selector](../../services/coach-engine/src/critical_moment_selector.rs)
- [Pipeline Evaluation](../../services/coach-engine/src/pipeline_evaluation.rs)

## Boundary ownership

Rule Extraction owns:

- whole-Game evidence identity and coverage;
- Review Side scope;
- legal Position and move validation;
- terminal versus analyzed outcome validation;
- Positive Highlight, Improvement Opportunity, or neutral classification;
- concrete achievement, correction, causal, human-model, and teaching facts;
- kind-specific evidence strength and Game phase derived from canonical facts.

The selector owns:

- Coaching Episode collapse before target computation;
- `adaptiveTarget = clamp(2 + ceil(gamePlies / 18), 3, 8)`;
- the independent hard maximum of ten;
- an in-target Positive Highlight reservation when a qualifying Positive
  candidate survives collapse;
- shared kind bands plus quantized `0..99` evidence strength;
- 80-point soft category, phase, and two-sided mover diversity penalties;
- maximum-count, then maximum-utility subset optimization;
- earlier ordered ply-list tie breaking;
- chronological product output and internal priority ranks.

Legality, classification, Review Side, and the hard maximum are policy
boundaries. Selector Weights cannot bypass them. Tactical category and Game
phase contribute diversity signals only; they do not receive direct priority
weight.

## Private candidate shape

The selector consumes a private typed `Candidate<T>` carrying:

- Game ply and mover side;
- kind band;
- tactical or positional category;
- legal-position-derived Game phase;
- quantized evidence strength;
- optional Coaching Episode identity and role;
- the kind-aware fact payload.

The candidate does not carry Player-facing rationale prose. Positive
qualification and Improvement correction already explain why a fact is
admissible; selection telemetry belongs in `SelectorTrace`.

Player-Selected Moments remain on the direct Rule Extraction path. They are not
ranked, may be outside the Automatic Review Side, and may classify as neutral.
They therefore have no Selector Trace.

## Trace-pinned evidence

Every Automatic Pipeline Evaluation baseline stores the complete trace beside
the selected facts:

```json
{
  "gamePlies": 84,
  "adaptiveTarget": 7,
  "hardMaximum": 10,
  "positiveReservationRequired": true,
  "diversityPenalty": 0,
  "candidates": [
    {
      "ply": 10,
      "kind": "goodPositiveHighlight",
      "evidenceStrength": 47,
      "priority": 697,
      "episode": null,
      "episodeOutcome": "retained",
      "selected": true,
      "priorityRank": 6,
      "gameOrderPosition": 1
    }
  ]
}
```

The adaptive target is a ceiling rather than a coverage obligation. Evidence
must compare the realized selected count and the trace, not demand a fixed
number of moments. Consecutive or adjacent candidates are not excluded merely
for proximity; only a shared Coaching Episode can collapse related decisions
and continuations.

The canonical Synthet1 full-Game baseline realizes target seven as seven
chronological moments at plies 10, 22, 26, 34, 52, 72, and 78. Its trace retains
every unselected candidate and the priority order that produced that Game-order
result.

## Regeneration policy

Raw Games, normalized direct provider inputs, provider provenance, and supplied
human material are inputs and remain unchanged. Rule Extraction baselines,
Selector Traces, product review outputs, comparison results, and rendered
reports are derived and are regenerated with the current code.

Fast regeneration is explicit:

```sh
cargo run -p chen-chess-coach-api --bin chenchess -- \
  accept-evaluation --corpus-dir backend/evaluation/corpus

cargo run -p chen-chess-coach-api --bin chenchess -- \
  accept-evaluation \
  --corpus-dir backend/evaluation/fixtures/Synthet1/provider-recordings
```

The ignored `last-accepted.diff` records local review material. Review the
selected facts and trace before accepting a baseline.

Gotham review evidence is regenerated from its exact source Games through the
installed pinned Stockfish and Maia runtime:

```sh
bun run gotham -- review --force
bun run gotham -- compare
```

`review --force` starts from an empty derived episode review, so a stale
incompatible product contract is never used as a migration input.

Internal traces, baselines, transient state, and generated reports do not gain a
generic manifest, certificate, compatibility policy version, or schema-version
wrapper. The baseline's typed direct inputs and embedded provider provenance are
the reproduction boundary.

## Behavioral target

A valid implementation satisfies all of the following:

1. Automatic facts are fully classified, selected under the current adaptive
   policy, and returned in strictly ascending Game ply.
2. The selected facts agree exactly with `selected`,
   `gameOrderPosition`, and the realized target in the trace.
3. At least one qualifying Positive Highlight is selected when one survives
   episode collapse, without overflowing either limit.
4. Raw inputs and supplied human material remain byte-identical during
   regeneration.
5. Derived output contains no obsolete presentation role or free-text selection
   rationale.
6. Product Game Review state contains no Selector Trace or priority telemetry.
7. Pipeline Evaluation and release-proof checks pass against the regenerated
   artifacts.
