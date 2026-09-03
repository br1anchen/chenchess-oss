import { expect, test } from "vitest"
import type {
  OpeningAnalysisOutcome,
  OpeningAnalysisRequest,
} from "@chenchess/coach-engine-sdk"
import {
  fromAlternativeMoveId,
  fromBranchRef,
  fromPositionRef,
} from "@chenchess/coach-engine-sdk"

import { boardConstraints } from "./coachingBoardConstraints"
import {
  applyExplorationBranches,
  openingBoardDrive,
} from "./coachingBoardDrive"
import {
  coachingBoardSnapshot,
  type CoachingBoardExplorationBranch,
  type CoachingBoardSnapshot,
} from "./coachingBoardSnapshot"
import { evaluateOpeningContinuationOnBoard } from "./openingContinuationTool"
import { parseOpeningLineRef } from "./openingLineRef"
import { openingLineMoves } from "./openingMoves"

const boardLineRef = parseOpeningLineRef("B90-sicilian-najdorf-1a2b")
const foreignRef = "C41-philidor-defense-3c4d"

const rootFen =
  "rnbqkb1r/1p2pppp/p2p1n2/8/3NP3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 6"
const afterFirst =
  "rnbqkb1r/1p2pppp/p2p1n2/8/3NPP2/2N5/PPP3PP/R1BQKB1R b KQkq - 0 6"

function requireRef() {
  if (!boardLineRef) throw new Error("fixture ref must parse")
  return boardLineRef
}

function openingSnapshot(): CoachingBoardSnapshot {
  return coachingBoardSnapshot({
    activeBranchId: null,
    branches: [],
    constraints: boardConstraints(),
    currentPosition: { fen: rootFen, sideToMove: "white" },
    linePlayback: null,
    mainLine: {
      continuesWith: null,
      evaluation: null,
      lastPly: 0,
      reachedBy: null,
    },
    marks: [],
    orientation: "white",
    origin: {
      eco: "B90",
      kind: "openingLine",
      name: "Sicilian Najdorf",
      openingLineRef: requireRef(),
    },
    pendingMove: null,
    playerChangedAtRevision: null,
    revision: 3,
    revisionChangedBy: null,
    shownLine: null,
    study: null,
    viewedPly: 10,
  })
}

const analyzedOutcome: OpeningAnalysisOutcome = {
  line: {
    eco: "B90",
    name: "Sicilian Najdorf",
    openingLineRef: "B90-sicilian-najdorf-1a2b",
    path: "1. e4 c5 2. Nf3 d6",
  },
  outcome: "analyzed",
  plies: [
    {
      evaluation: {
        bestMove: { kind: "centipawns", perspective: "white", value: 30 },
        bestMoveUci: "f1e2",
        comparison: { kind: "centipawns", value: 12 },
        selectedMove: { kind: "centipawns", perspective: "white", value: 18 },
      },
      index: 0,
      mover: "white",
      moveUci: "f2f4",
      resultingFen: afterFirst,
    },
  ],
  root: {
    evaluation: { kind: "centipawns", perspective: "white", value: 30 },
    fen: rootFen,
  },
  verdict: { kind: "completed" },
}

function recordingApply() {
  const applied: CoachingBoardExplorationBranch[][] = []
  return {
    applied,
    applyBranches: (minted: readonly CoachingBoardExplorationBranch[]) => {
      applied.push([...minted])
      return openingSnapshot()
    },
  }
}

test("a continuation naming another line is refused and nothing is analyzed", async () => {
  let analyzed = false
  const result = await evaluateOpeningContinuationOnBoard({
    analyze: async () => {
      analyzed = true
      return analyzedOutcome
    },
    applyBranches: () => openingSnapshot(),
    boardLineRef: requireRef(),
    input: {
      continuation: [{ kind: "san", san: "f4" }],
      openingLineRef: foreignRef,
    },
    snapshot: openingSnapshot(),
  })

  expect(analyzed).toBe(false)
  expect(result).toMatchObject({
    kind: "refused",
    reason: "unreachablePosition",
  })
})

test("analyzed plies become branches and the result carries their evaluations", async () => {
  const recorder = recordingApply()
  let requested: OpeningAnalysisRequest | null = null
  const result = await evaluateOpeningContinuationOnBoard({
    analyze: async (request) => {
      requested = request
      return analyzedOutcome
    },
    applyBranches: recorder.applyBranches,
    boardLineRef: requireRef(),
    input: {
      continuation: [{ kind: "san", san: "f4" }],
      openingLineRef: requireRef(),
    },
    snapshot: openingSnapshot(),
  })

  expect(requested).toEqual({
    continuation: [{ kind: "san", san: "f4" }],
    openingLineRef: requireRef(),
  })
  expect(recorder.applied).toHaveLength(1)
  expect(recorder.applied[0]).toHaveLength(1)
  expect(result).toMatchObject({
    kind: "openingContinuationEvaluated",
    line: { eco: "B90" },
    root: { fen: rootFen },
    verdict: { kind: "completed" },
  })
  expect(result).toHaveProperty("snapshot")
  expect(result).toHaveProperty("constraints")
  if (!("branches" in result))
    throw new Error("analyzed result carries branches")
  expect(result.branches[0]?.moveUci).toBe("f2f4")
  // The comparison the snapshot's branch list drops rides in the facts.
  expect(result.branches[0]?.evaluation.comparison).toEqual({
    kind: "centipawns",
    value: 12,
  })
})

test("a partial verdict still keeps the evaluated prefix", async () => {
  const recorder = recordingApply()
  const result = await evaluateOpeningContinuationOnBoard({
    analyze: async () => ({
      ...analyzedOutcome,
      verdict: { index: 1, kind: "illegalMove" as const },
    }),
    applyBranches: recorder.applyBranches,
    boardLineRef: requireRef(),
    input: {
      continuation: [
        { kind: "san", san: "f4" },
        { kind: "san", san: "Qz9" },
      ],
      openingLineRef: requireRef(),
    },
    snapshot: openingSnapshot(),
  })

  expect(recorder.applied[0]).toHaveLength(1)
  expect(result).toMatchObject({
    kind: "openingContinuationEvaluated",
    verdict: { index: 1, kind: "illegalMove" },
  })
})

test("a tripped rate limit returns the typed retry and applies nothing", async () => {
  const recorder = recordingApply()
  const result = await evaluateOpeningContinuationOnBoard({
    analyze: async () => ({
      outcome: "rateLimited" as const,
      retry: { kind: "retryAfter" as const, seconds: 30 },
    }),
    applyBranches: recorder.applyBranches,
    boardLineRef: requireRef(),
    input: {
      continuation: [{ kind: "san", san: "f4" }],
      openingLineRef: requireRef(),
    },
    snapshot: openingSnapshot(),
  })

  expect(recorder.applied).toHaveLength(0)
  expect(result).toMatchObject({
    kind: "rateLimited",
    retry: { kind: "retryAfter", seconds: 30 },
  })
})

test("an unavailable engine answers unavailable, carrying the snapshot", async () => {
  const result = await evaluateOpeningContinuationOnBoard({
    analyze: async () => ({
      outcome: "unavailable" as const,
      retry: { kind: "retryAllowed" as const },
    }),
    applyBranches: () => openingSnapshot(),
    boardLineRef: requireRef(),
    input: {
      continuation: [{ kind: "san", san: "f4" }],
      openingLineRef: requireRef(),
    },
    snapshot: openingSnapshot(),
  })

  expect(result).toMatchObject({ kind: "unavailable" })
  expect(result).toHaveProperty("snapshot")
})

test("applying branches advances the revision without showing one", () => {
  const drive = openingBoardDrive({
    eco: "B90",
    moves: openingLineMoves("1. e4 c5 2. Nf3 d6"),
    name: "Sicilian Najdorf",
    openingLineRef: requireRef(),
    viewedPly: 4,
  })
  const minted: CoachingBoardExplorationBranch[] = [
    {
      alternativeMoveId: fromAlternativeMoveId(
        "alternative-move:web-opening-test",
      ),
      branchRef: fromBranchRef("branch:web-opening-test"),
      evaluation: {
        bestMove: { kind: "centipawns", perspective: "white", value: 30 },
        bestMoveUci: "f1e2",
        comparison: { kind: "centipawns", value: 12 },
        selectedMove: { kind: "centipawns", perspective: "white", value: 18 },
      },
      moveUci: "f2f4",
      parent: { kind: "root", positionRef: fromPositionRef("fnv1a:00000000") },
      resultingPosition: {
        fen: afterFirst,
        occupied: [],
        positionRef: fromPositionRef("fnv1a:00000001"),
        sideToMove: "black",
      },
    },
  ]

  const next = applyExplorationBranches(drive, "agent", minted)
  expect(next.branches).toHaveLength(1)
  expect(next.revision).toBe(drive.revision + 1)
  expect(next.activeBranchId).toBeNull()
  expect(next.shownLine).toBeNull()
  expect(next.viewedPly).toBe(drive.viewedPly)
})
