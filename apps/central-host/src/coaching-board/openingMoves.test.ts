import { expect, test } from "vitest"

import { openingLineCatalog } from "./openingLineCatalog"
import { openingLineMoves, openingLineViewedPly } from "./openingMoves"

test("walk-only opening Positions do not mint FNV-1a as sha256", () => {
  const moves = openingLineMoves("1. e4")
  const before = moves[0]?.beforePositionRef
  if (!before) throw new Error("e4 has a before Position")
  expect(before.startsWith("sha256:")).toBe(false)
  expect(before.startsWith("fnv1a:")).toBe(true)
})

test("an opened line starts on its last ply, not catalog-root ply 1", () => {
  const row = openingLineCatalog[0]
  if (!row) throw new Error("v1 catalog has at least one Opening Line")
  const moves = openingLineMoves(row.path)
  const last = moves.at(-1)
  if (!last) throw new Error("Najdorf path has moves")
  expect(openingLineViewedPly(moves)).toBe(last.ply)
  expect(openingLineViewedPly(moves)).not.toBe(1)
  expect(openingLineViewedPly([])).toBe(1)
})
