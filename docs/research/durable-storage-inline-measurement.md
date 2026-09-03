# Durable storage inline-evidence measurement

> Measurement date: 2026-08-01  
> Decision: inline Review Moment evidence  
> Executable fixture:
> `services/coach-engine/src/review_session_checkpoint/durable_storage_measurement.rs`

## Result

The derived-position design passes the Phase 2 gate.

| Gate                          |      Measured |                            Limit |      Headroom |
| ----------------------------- | ------------: | -------------------------------: | ------------: |
| Maximum Moment opaque payload | 533,679 bytes |          716,800 bytes (700 KiB) | 183,121 bytes |
| Initial session creation      |      9 writes |  200-write repository convention |    191 writes |
| Initial session creation      |      9 writes | 500-write Firestore commit limit |    491 writes |

The size test applies a stricter threshold than the architectural ceiling: the
payload must retain at least 100 KiB below 700 KiB. The measured payload retains
about 179 KiB, or 25.5% of the ceiling.

Evidence may therefore move into each Review Moment document when Phase 4
introduces the production serializer. If that serializer exceeds the same
guard, the change must keep the evidence subcollection instead.

## Upper-bound fixture

The fixture derives its cardinalities from the production limits:

- a 400-ply Game, which selects the maximum eight automatic Moments;
- 24 committed Alternative Moves, each using a maximum-depth 12-ply
  root-relative UCI path;
- 12 completed Coach Turns, each retaining three 4,096-byte assessment
  explanations;
- one 4,096-byte published Moment comment;
- 281 provider-evidence entries: the live limit of 256 cached Position, Engine
  Analysis, and Human Move Model entries, plus the maximum 24 Branch entries
  and one initial Provenance entry.

For evidence, the fixture serializes every planned derived variant and fills
the payload with whichever variant is largest. The Branch variant is currently
largest. Its bound uses maximum-width semantic IDs, two evidence dependencies,
provider provenance, and maximum-depth derived source and result paths.
Repeating that largest shape 256 times deliberately overestimates a real
packet, whose Position, Engine Analysis, Branch, and Provenance entries are
mixed.

The Rust command boundary now applies the same 4,096-byte limit to every Coach
Turn assessment explanation that the Central Host already applies before
publication. The measurement therefore depends on a parsed storage invariant,
not a convention held by only one caller. Review Moment comments apply the
same encoded-byte limit at canonical grounding admission so an invalid first
draft still reaches the existing retry and safe-rendering state machine.

The payload stores positions only as a Game ply or a root-relative UCI path.
The test rejects `occupied`, `fen`, and `positionRef`, preventing serialized
board state from returning unnoticed.

Initial `StartReviewSession` persistence is one aggregate root plus eight
Moment documents. `ImportGame` is a preceding public command and writes its
self-contained import independently, so it is not part of the session
creation commit.

## Reproduce

```bash
nix develop --command cargo test -p chen-chess-coach-engine \
  maximum_derived_session_fits_inline_moment_and_commit_guards -- --nocapture
```

The command prints only counts and byte measurements; it emits no chess
content, Player data, or provider payloads.
