# Pinned opening catalog

These are the exact `a.tsv` through `e.tsv` source bytes from
[`lichess-org/chess-openings` release `2026.04.16`](https://github.com/lichess-org/chess-openings/releases/tag/2026.04.16),
commit `a470acc9d1cdcb26018affa90459a6ec8689af79`.

The files are consumed in alphabetical order. Their concatenated SHA-256 is:

```text
2c0f0fe3f6a9a6e08d0e7b264785b9b3f67da9f1134d841fe42e16bad527be70
```

The snapshot contains 3,690 named positions. Updating it is an explicit data
change: replace all five files, update the release/commit/digest constants,
inspect EPD additions, removals, and label changes, then regenerate and run the
opening-identification fixtures.

The upstream data is dedicated to the public domain under CC0 1.0. The exact
upstream license text is retained in `COPYING.txt`.
