import { expect, test } from "vitest"

import {
  verifyBoardAnnotation,
  type BoardAnnotationRequest,
} from "./boardAnnotation"

/**
 * White Nd5, Pf6, Pe2, Ra1, Ke1. Black Rc7, Qe7, Ke8.
 *
 * The knight bears on both black pieces at once and defends its own pawn;
 * the rook owns the a-file while the king blocks its rank. The e2 pawn is
 * there to shut the e-file, so White is not in check and its moves are legal.
 */
const FEN = "4k3/2r1q3/5P2/3N4/8/8/4P3/R3K3 w Q - 0 1"

function verify(
  requests: readonly BoardAnnotationRequest[],
  grounded: readonly string[] = [],
) {
  return verifyBoardAnnotation({
    fen: FEN,
    groundedMoveUcis: new Set(grounded),
    requests,
  })
}

test("a piece bearing on an enemy is drawn, on an empty square is refused", () => {
  expect(
    verify([{ from: "d5", kind: "attacks", label: "hits the rook", to: "c7" }]),
  ).toEqual({
    kind: "annotated",
    marks: [{ from: "d5", kind: "arrow", label: "hits the rook", to: "c7" }],
  })
  expect(
    verify([{ from: "d5", kind: "attacks", label: "hits nothing", to: "b4" }]),
  ).toEqual({ kind: "refused", reason: "relationNotOnBoard" })
})

test("defends reads the same geometry against a friendly occupant", () => {
  expect(
    verify([{ from: "d5", kind: "defends", label: "holds f6", to: "f6" }]),
  ).toMatchObject({ kind: "annotated" })
  // c7 is the enemy rook: bearing on it is an attack, and calling it defence
  // is the claim the position does not support.
  expect(
    verify([{ from: "d5", kind: "defends", label: "holds c7", to: "c7" }]),
  ).toEqual({ kind: "refused", reason: "relationNotOnBoard" })
})

test("multiAttack needs two enemies the same piece actually reaches", () => {
  expect(
    verify([
      {
        from: "d5",
        kind: "multiAttack",
        label: "forks them",
        targets: ["c7", "e7"],
      },
    ]),
  ).toEqual({
    kind: "annotated",
    marks: [
      { from: "d5", kind: "arrow", label: "forks them", to: "c7" },
      { from: "d5", kind: "arrow", label: "forks them", to: "e7" },
    ],
  })
  expect(
    verify([
      { from: "d5", kind: "multiAttack", label: "forks them", targets: ["c7"] },
    ]),
  ).toEqual({ kind: "refused", reason: "relationNotOnBoard" })
  // f6 is White's own pawn, so this is not a fork of two enemies.
  expect(
    verify([
      {
        from: "d5",
        kind: "multiAttack",
        label: "forks them",
        targets: ["c7", "f6"],
      },
    ]),
  ).toEqual({ kind: "refused", reason: "relationNotOnBoard" })
})

test("controls reaches an empty square but stops at the first blocker", () => {
  expect(
    verify([
      { from: "a1", kind: "controls", label: "owns the file", to: "a8" },
    ]),
  ).toMatchObject({ kind: "annotated" })
  // The king on e1 blocks the rank, so h1 is not a square this rook reaches.
  expect(
    verify([
      { from: "a1", kind: "controls", label: "owns the rank", to: "h1" },
    ]),
  ).toEqual({ kind: "refused", reason: "relationNotOnBoard" })
  // A knight is not a slider; it never controls a line.
  expect(
    verify([{ from: "d5", kind: "controls", label: "owns it", to: "c7" }]),
  ).toEqual({ kind: "refused", reason: "relationNotOnBoard" })
})

test("a bare square highlight asserts nothing beyond the square existing", () => {
  expect(
    verify([{ kind: "square", label: "the weak square", square: "e4" }]),
  ).toEqual({
    kind: "annotated",
    marks: [{ kind: "square", label: "the weak square", square: "e4" }],
  })
  expect(verify([{ kind: "square", label: "nowhere", square: "z9" }])).toEqual({
    kind: "refused",
    reason: "relationNotOnBoard",
  })
})

test("a move arrow must be grounded and still legal here", () => {
  expect(
    verify([{ kind: "move", label: "takes the rook", uci: "d5c7" }], ["d5c7"]),
  ).toEqual({
    kind: "annotated",
    marks: [{ from: "d5", kind: "arrow", label: "takes the rook", to: "c7" }],
  })
  // Legal, but ChenChess never put it on this board.
  expect(verify([{ kind: "move", label: "invented", uci: "d5e3" }])).toEqual({
    kind: "refused",
    reason: "moveNotGrounded",
  })
  // Named by the board once, but not playable in the position on screen.
  expect(
    verify([{ kind: "move", label: "stale", uci: "a2a4" }], ["a2a4"]),
  ).toEqual({ kind: "refused", reason: "moveNotGrounded" })
})

test("one relation that is not there refuses the whole set", () => {
  expect(
    verify([
      { from: "d5", kind: "attacks", label: "hits the rook", to: "c7" },
      { from: "d5", kind: "attacks", label: "hits nothing", to: "b4" },
    ]),
  ).toEqual({ kind: "refused", reason: "relationNotOnBoard" })
})

test("the cap counts drawn marks, not requests", () => {
  const fork: BoardAnnotationRequest = {
    from: "d5",
    kind: "multiAttack",
    label: "forks them",
    targets: ["c7", "e7"],
  }
  // Three requests draw six arrows, which is the cap exactly.
  expect(verify([fork, fork, fork])).toMatchObject({ kind: "annotated" })
  // A fourth draws eight, and eight arrows is not something a Player reads.
  expect(verify([fork, fork, fork, fork])).toEqual({
    kind: "refused",
    reason: "tooManyMarks",
  })
})

test("more marks than a Player can read is refused", () => {
  const seven: BoardAnnotationRequest[] = Array.from({ length: 7 }, () => ({
    kind: "square",
    label: "here",
    square: "e4",
  }))
  expect(verify(seven)).toEqual({ kind: "refused", reason: "tooManyMarks" })
})
