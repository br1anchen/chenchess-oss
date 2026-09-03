import { expect, test } from "vitest"

import {
  fromPositionRef,
  fromSquare,
  type PositionSnapshot,
} from "@chenchess/coach-engine-sdk"

import { browseBoardAtPly, promotionRequired, uciForDestination } from "./model"

const a7 = fromSquare("a7")
const a8 = fromSquare("a8")
const promotionPosition: Pick<PositionSnapshot, "occupied"> = {
  occupied: [
    {
      square: a7,
      piece: { color: "white", role: "pawn" },
    },
  ],
}

test("browse reconstruction is the position before the viewed ply", () => {
  const board = browseBoardAtPly(
    [
      {
        ply: 1,
        moveNumber: 1,
        side: "white",
        san: "e4",
        uci: "e2e4",
        beforePositionRef: fromPositionRef("position:before"),
        afterPositionRef: fromPositionRef("position:after"),
      },
    ],
    1,
  )
  expect(board.sideToMove).toBe("white")
  expect(
    board.fen.startsWith("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR"),
  ).toBe(true)
  const after = browseBoardAtPly(
    [
      {
        ply: 1,
        moveNumber: 1,
        side: "white",
        san: "e4",
        uci: "e2e4",
        beforePositionRef: fromPositionRef("position:before"),
        afterPositionRef: fromPositionRef("position:after"),
      },
    ],
    2,
  )
  expect(after.fen).toContain("/4P3/")
})

test("promotion destinations require an explicit piece instead of defaulting to queen", () => {
  expect(promotionRequired(promotionPosition, a7, a8)).toBe(true)
  expect(() => uciForDestination(promotionPosition, a7, a8)).toThrow(
    "require one explicit promotion piece",
  )
  expect(uciForDestination(promotionPosition, a7, a8, "knight")).toBe("a7a8n")
})
