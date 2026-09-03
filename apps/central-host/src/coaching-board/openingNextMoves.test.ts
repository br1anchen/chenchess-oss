import { expect, test } from "vitest"

import { openingLineCatalog } from "./openingLineCatalog"
import { openingNextMoves } from "./openingNextMoves"
import { openingLineMoves, openingLineViewedPly } from "./openingMoves"

test("next moves from the start include this line and catalog siblings", () => {
  const row = openingLineCatalog[0]
  if (!row) throw new Error("v1 catalog has at least one Opening Line")
  const moves = openingLineMoves(row.path)
  const next = openingNextMoves(row.ref, moves, 1)
  expect(next[0]).toMatchObject({ onCurrentLine: true, san: "e4" })
  expect(next.map((branch) => branch.san)).toEqual(["e4", "d4"])
})

test("next moves on the opened Najdorf line are 5…a6, not catalog-root 1.e4 / 1.d4", () => {
  const row = openingLineCatalog[0]
  if (!row) throw new Error("v1 catalog has at least one Opening Line")
  const moves = openingLineMoves(row.path)
  const next = openingNextMoves(row.ref, moves, openingLineViewedPly(moves))
  expect(next.map((branch) => branch.san)).toEqual(["a6"])
  expect(next.map((branch) => branch.san)).not.toEqual(["e4", "d4"])
})

test("next moves after 3.d4 are catalog continuations from that ply", () => {
  const row = openingLineCatalog[0]
  if (!row) throw new Error("v1 catalog has at least one Opening Line")
  const moves = openingLineMoves(row.path)
  const afterD4 = moves.find(
    (move) =>
      move.moveNumber === 3 && move.side === "white" && move.san === "d4",
  )
  if (!afterD4) throw new Error("Najdorf path includes 3. d4")
  const next = openingNextMoves(row.ref, moves, afterD4.ply + 1)
  expect(next.map((branch) => branch.san)).toEqual(["cxd4"])
  expect(next[0]).toMatchObject({ onCurrentLine: true, san: "cxd4" })
})
