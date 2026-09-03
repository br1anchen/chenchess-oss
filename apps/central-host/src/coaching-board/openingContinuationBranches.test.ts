import { expect, test } from "vitest"
import type { OpeningAnalyzedPly } from "@chenchess/coach-engine-sdk"

import {
  mergeExplorationBranches,
  openingContinuationBranches,
} from "./openingContinuationBranches"
import type { CoachingBoardExplorationBranch } from "./coachingBoardSnapshot"
import { parseOpeningLineRef } from "./openingLineRef"
import { openingLineMoves } from "./openingMoves"

const lineRef = parseOpeningLineRef("B90-sicilian-najdorf-1a2b")
const otherRef = parseOpeningLineRef("C41-philidor-defense-3c4d")

const rootFen =
  "rnbqkb1r/1p2pppp/p2p1n2/8/3NP3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 6"

function ply(
  index: number,
  moveUci: string,
  resultingFen: string,
): OpeningAnalyzedPly {
  return {
    evaluation: {
      bestMove: { kind: "centipawns", perspective: "white", value: 30 },
      bestMoveUci: "f1e2",
      comparison: { kind: "centipawns", value: 12 },
      selectedMove: { kind: "centipawns", perspective: "white", value: 18 },
    },
    index,
    mover: index % 2 === 0 ? "white" : "black",
    moveUci,
    resultingFen,
  }
}

const firstFen =
  "rnbqkb1r/1p2pppp/p2p1n2/8/3NPP2/2N5/PPP3PP/R1BQKB1R b KQkq - 0 6"
const secondFen =
  "rnbqkb1r/4pppp/p2p1n2/1p6/3NPP2/2N5/PPP3PP/R1BQKB1R w KQkq - 0 7"

test("chains analyzed plies into a path from the line's end", () => {
  if (!lineRef) throw new Error("fixture ref must parse")
  const branches = openingContinuationBranches({
    openingLineRef: lineRef,
    plies: [ply(0, "f2f4", firstFen), ply(1, "b7b5", secondFen)],
    rootFen,
  })

  expect(branches).toHaveLength(2)
  expect(branches[0]?.parent.kind).toBe("root")
  expect(branches[1]?.parent).toEqual({
    branchRef: branches[0]?.branchRef,
    kind: "move",
  })
  expect(branches[0]?.moveUci).toBe("f2f4")
  expect(branches[0]?.resultingPosition.fen).toBe(firstFen)
  expect(branches[0]?.resultingPosition.sideToMove).toBe("black")
  expect(branches[1]?.resultingPosition.sideToMove).toBe("white")
  // Occupied squares are derived from the FEN, not carried by the route.
  expect(branches[0]?.resultingPosition.occupied.length).toBeGreaterThan(0)
})

test("mints ids deterministically per line and move path", () => {
  if (!lineRef || !otherRef) throw new Error("fixture refs must parse")
  const input = {
    openingLineRef: lineRef,
    plies: [ply(0, "f2f4", firstFen)],
    rootFen,
  }
  const first = openingContinuationBranches(input)
  const again = openingContinuationBranches(input)
  const elsewhere = openingContinuationBranches({
    ...input,
    openingLineRef: otherRef,
  })

  expect(again[0]?.alternativeMoveId).toBe(first[0]?.alternativeMoveId)
  expect(again[0]?.branchRef).toBe(first[0]?.branchRef)
  // The same move from a different line is a different branch.
  expect(elsewhere[0]?.alternativeMoveId).not.toBe(first[0]?.alternativeMoveId)
  expect(first[0]?.alternativeMoveId).toMatch(
    /^alternative-move:web-opening-[0-9a-f]{32}$/,
  )
  expect(first[0]?.branchRef).toMatch(/^branch:web-opening-[0-9a-f]{32}$/)
})

test("a shared prefix converges instead of duplicating", () => {
  if (!lineRef) throw new Error("fixture ref must parse")
  const walked = openingContinuationBranches({
    openingLineRef: lineRef,
    plies: [ply(0, "f2f4", firstFen), ply(1, "b7b5", secondFen)],
    rootFen,
  })
  const rewalked = openingContinuationBranches({
    openingLineRef: lineRef,
    plies: [ply(0, "f2f4", firstFen)],
    rootFen,
  })

  const merged = mergeExplorationBranches(walked, rewalked, arriving)
  expect(merged).toHaveLength(2)
  expect(merged.map((branch) => branch.alternativeMoveId)).toEqual(
    walked.map((branch) => branch.alternativeMoveId),
  )
})

test("merging keeps retained order and appends only new branches", () => {
  if (!lineRef) throw new Error("fixture ref must parse")
  const retained = openingContinuationBranches({
    openingLineRef: lineRef,
    plies: [ply(0, "f2f4", firstFen)],
    rootFen,
  })
  const extended = openingContinuationBranches({
    openingLineRef: lineRef,
    plies: [ply(0, "f2f4", firstFen), ply(1, "b7b5", secondFen)],
    rootFen,
  })

  const merged = mergeExplorationBranches(retained, extended, arriving)
  expect(merged).toHaveLength(2)
  expect(merged[0]?.alternativeMoveId).toBe(retained[0]?.alternativeMoveId)
  expect(merged[1]?.alternativeMoveId).toBe(extended[1]?.alternativeMoveId)
})

test("the line's own moves still parse, so the fixture root is reachable", () => {
  expect(openingLineMoves("1. e4 c5 2. Nf3 d6").length).toBe(4)
})

/** The merge builds what the caller's tree holds; these tests hold nothing
 * beyond the engine's own facts. */
const arriving = (branch: CoachingBoardExplorationBranch) => branch
