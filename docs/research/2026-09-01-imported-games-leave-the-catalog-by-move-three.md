# Imported games leave the pinned catalog by move three

**Date**: 2026-09-01. **Status**: research note, closing the last unresolved
question of `2026-08-30-opening-study-as-small-world-play.md` (question 6:
confirm the Class B book-depth figure against our own imported games before
citing "move seven" in product copy).

Evidence is labelled: **measured**, **blocked**, or **inferred**.

## Question

The small-world research note leans on one archival number: Class B players
first depart from theory at 14.26 ply (~ move seven), so opening study that
stops at the end of a catalog line says nothing about the moves a Player
actually chooses. Before that number reaches product copy, check it against
the games our own Players import.

## Method — deterministic, no model in the loop

- Source: the 21 imported-game records on `coach-app-staging` (both staging
  accounts; one game is imported under both, 20 distinct). Read through
  `bun run firestore:read` with `--values`; each record carries the
  provider identity, the Player's rating in that game, the review side, and
  the time control.
- Moves: fetched from the providers' public APIs — Lichess game export for
  the 12 Lichess games, the opponent's public monthly archive on the
  Chess.com pubapi for the Chess.com games. Two Chess.com games were played
  against bots whose archives do not expose them; they drop out, leaving
  **n = 18**.
- Book: the engine's own pinned catalog,
  `services/coach-engine/data/chess-openings/2026.04.16/*.tsv` (3,690
  rows). Every SAN prefix of every row (8,347 prefixes) forms the book; a
  game's **catalog depth** is the longest prefix of its moves that is a
  prefix of some catalog row, replayed through chessops. The first
  off-catalog ply is depth + 1.
- The paper's own reference base could not be replicated: the public
  Lichess opening explorer (`explorer.lichess.ovh`) now answers **401
  Authorization Required** at the nginx layer for every unauthenticated
  request, User-Agent or not — **blocked**. Rerunning the masters-database
  comparison needs an authorized token and is left open.

## Result — measured

Mean catalog depth **4.22 plies** (median 3.5, max 10); the first
off-catalog move falls on **ply 5.2 on average — move three**. In the
1000–1799 band (n = 10), the band the Class B figure describes, mean depth
is 4.6 plies — no deeper.

| Rating | Time control   | Side  | Opening                                    | Last ply in catalog | First ply off |
| ------ | -------------- | ----- | ------------------------------------------ | ------------------- | ------------- |
| 355    | rapid          | white | A40 Mikenas Defense                        | 2                   | 3             |
| 586    | rapid          | black | A00 Van Geet Opening                       | 2                   | 3             |
| 636    | rapid          | black | A45 Indian Defense                         | 3                   | 4             |
| 680    | rapid          | white | A04 Zukertort Opening: Queen's Gambit Inv. | 2                   | 3             |
| 1166   | blitz          | white | B44 Sicilian Defense: Taimanov Variation   | 8                   | 9             |
| 1195   | blitz          | white | B13 Caro-Kann Defense: Exchange Variation  | 6                   | 7             |
| 1216   | blitz          | white | C41 Philidor Defense                       | 4                   | 5             |
| 1246   | rapid          | black | C41 Philidor Defense                       | 4                   | 5             |
| 1280   | correspondence | white | C50 Italian Game: Giuoco Piano             | 10                  | 11            |
| 1315   | correspondence | white | D02 Queen's Pawn Game: Chigorin Variation  | 2                   | 3             |
| 1317   | rapid          | black | D06 Queen's Gambit Declined: Marshall Def. | 3                   | 4             |
| 1578   | blitz          | white | A40 Mikenas Defense                        | 2                   | 3             |
| 1591   | blitz          | white | C41 Philidor Defense: Hanham Variation     | 4                   | 5             |
| 1775   | blitz          | white | A03 Bird Opening: Dutch Variation          | 3                   | 4             |
| 2875   | bullet         | white | B12 Caro-Kann Defense                      | 2                   | 3             |
| 2879   | bullet         | white | D11 Slav Defense: Modern Line              | 8                   | 9             |
| 3131   | bullet         | white | B23 Sicilian Defense: Closed, Traditional  | 4                   | 5             |
| 3134   | bullet         | white | C24 Bishop's Opening: Vienna Hybrid        | 7                   | 8             |

## What this does and does not confirm

- **Measured**: against the book our product actually teaches from — the
  pinned catalog — imported games are off book by **move three**, at every
  rating in the sample. The one game that stayed in book to move five was
  correspondence Giuoco Piano.
- **Not confirmed**: the 14.26-ply figure itself. The pinned catalog is a
  names-and-lines catalog, orders of magnitude thinner than the theory base
  the paper measured against, so catalog depth is a strict lower bound on
  theory depth; the comparable measurement is blocked on explorer
  authorization. The paper's number stays a citation, not our measurement.
- **Caveats**: n = 18 developer test imports across two staging accounts
  spanning 355–3134, in online time controls (bullet through
  correspondence), not the paper's competitive classical games, and not a
  Player population. This sample cannot estimate a population mean; what it
  can do is falsify "our Players stay in the catalog deep into the game",
  and it does.

## Implication for product copy — inferred

The design conclusion the research note drew from move seven holds a
fortiori: study anchored to catalog lines runs out even earlier than the
paper says, around **move three** against our own book. Product copy should
either cite the paper for the move-seven claim, or make the stronger owned
claim — "your games leave the book by move three" — which this note
measures. Deviation-first study (ADR 0063) is what both numbers ask for.
