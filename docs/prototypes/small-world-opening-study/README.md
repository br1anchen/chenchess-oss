# Small-world opening study — prototype

A throwaway prototype for the design question in
`docs/research/2026-08-30-opening-study-as-small-world-play.md`: can opening
study be a **bounded world you build and then play inside**, rather than a
line you recall?

## Run it

```sh
open docs/prototypes/small-world-opening-study/small-world-opening-study.html
```

One self-contained file. No build, no server, no network.

## What it demonstrates

Five stages over one tabiya. Two worlds: the Giuoco Piano (you play White) and
the Najdorf (you play Black, so the board flips and the opponent moves first).

1. **Build the world.** Three pieces are missing from the tabiya. Put each one
   where the structure wants it. This is Wozniak's _graphic deletion_ applied
   to a board, and it is a direct test of Gobet's template slots — the accepted
   answer is a _set_ of squares (the Italian bishop belongs on c4 **or** b3),
   not one move.
2. **Say the plan.** Free text. The prototype cannot grade it, so it reveals
   the rubric a host agent would grade against. This is the card that cannot
   exist on a board-only input channel, which is the whole competitive point.
3. **Choose the break.** One decision, three legal candidates, one primary.
4. **Off book.** The opponent leaves the catalog on move six or seven — where
   the measured data says a Class B player actually ends up. Every wrong answer
   is legal and plausible; one of them is the _right plan played at the wrong
   moment_, which is the mistake the card exists to catch.
5. **Demolish.** Nothing is saved. No deck, no interval, no due date. See §5 of
   the research note for why that is the design and not a shortcut.

## Honest limits

- **Engine verdicts are authored content**, not engine output. In production
  they come from `evaluate_opening_continuation` (ADR 0058). The prototype
  deliberately does not fake an evaluation number.
- **The free-text card is not graded.** It shows the rubric instead.
- Two worlds, hand-authored. Nothing here says the authoring scales — question
  2 in the research note.
- The prototype grades placement and choice through the board, which is the
  same channel Matuschak identifies as the category ceiling. The difference is
  the _unit_ (a slot, a break) rather than a sequence position; the free-text
  card is the part that actually escapes the ceiling, and it is the part that
  needs an agent.

## Check it still works

```sh
bun docs/prototypes/small-world-opening-study/verify.mjs
```

Walks both worlds end to end in a real browser, answering every card
correctly, and fails on any page error or a tally that does not match the
number of decisions the world contains.

## How the data is built

`build-worlds.ts` replays every authored SAN move through chessops and refuses
to emit data that is not legal chess:

```sh
bun docs/prototypes/small-world-opening-study/build-worlds.ts > worlds.json
```

It validates that each move in the tabiya path is legal, that every slot square
holds a piece and every accepted square exists, that each break is legal from
the learner's own decision point, that each deviation is legal where the
opponent is actually to move, that every distractor is legal from the position
the learner is shown, and that the sound answer changes the board.

That last check exists because it caught a real bug: an edit dropped the
`play(reply)` call, so the "after" position silently equalled the "before" one
and the board never advanced. In total the validator caught five content errors
during the spike — a pawn break blocked by its own knight (`f4` with a knight
on f3), a rook "retreat" to a square the rook already occupied after castling,
a square name that was actually a move, and two moves rooted at a position
where it was the other side's turn.

To regenerate the HTML after editing either file, inject the JSON into the
template:

```sh
python3 - <<'PY'
tpl = open("prototype.template.html").read()
worlds = open("worlds.json").read()
open("small-world-opening-study.html", "w").write(tpl.replace("__WORLDS__", worlds.strip()))
PY
```

`prototype.template.html` holds the markup and logic with a `__WORLDS__`
placeholder; `worlds.json` is the generator's validated output;
`small-world-opening-study.html` is the built artifact you open.
