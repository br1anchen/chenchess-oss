import { beforeAll, describe, expect, test } from "vitest"

import {
  fromAlternativeMoveId,
  fromBranchRef,
  fromGameImportId,
  type AlternativeMoveResult,
} from "@chenchess/coach-engine-sdk"

import {
  fixtureCore,
  loadReviewSessionFixtures,
} from "@/review-session/reviewSessionStreamFixtures"

import { boardConstraints, lobbyResult } from "./coachingBoardConstraints"
import { loadedBranches } from "./coachingBoardDrive"
import {
  coachingBoardSnapshot,
  advancedPageRevision,
} from "./coachingBoardSnapshot"

beforeAll(async () => {
  await loadReviewSessionFixtures()
})

const rootId = fromAlternativeMoveId("alternative-move:board:e4")
const siblingId = fromAlternativeMoveId("alternative-move:board:d4")
const childId = fromAlternativeMoveId("alternative-move:board:e5")
const rootRef = fromBranchRef("branch:board:e4")
const siblingRef = fromBranchRef("branch:board:d4")
const childRef = fromBranchRef("branch:board:e5")
function branch(
  alternativeMoveId: AlternativeMoveResult["alternativeMoveId"],
  branchRef: AlternativeMoveResult["branchRef"],
  moveUci: string,
  parent: AlternativeMoveResult["parent"],
): AlternativeMoveResult {
  const core = fixtureCore()
  const evaluation = {
    kind: "centipawns" as const,
    perspective: "white" as const,
    value: 20,
  }
  return {
    alternativeMoveId,
    branchRef,
    evaluation: {
      bestMove: evaluation,
      bestMoveUci: moveUci,
      comparison: { kind: "centipawns", value: 0 },
      selectedMove: evaluation,
    },
    moveUci,
    parent,
    resultingPosition: structuredClone(core.positionSnapshot),
    sourcePositionRef: core.positionSnapshot.positionRef,
    strongestReply: { kind: "terminal" },
  }
}

function treeBranches() {
  const positionRef = fixtureCore().positionSnapshot.positionRef
  return [
    branch(rootId, rootRef, "e2e4", {
      kind: "root",
      positionRef,
    }),
    branch(siblingId, siblingRef, "d2d4", {
      kind: "root",
      positionRef,
    }),
    branch(childId, childRef, "e7e5", { kind: "move", branchRef: rootRef }),
  ]
}

describe("Coaching Board Snapshot", () => {
  test("returns every sibling including the abandoned one, plus the active path", () => {
    const snapshot = coachingBoardSnapshot({
      activeBranchId: childId,
      branches: loadedBranches(treeBranches()),
      constraints: boardConstraints(),
      currentPosition: {
        fen: "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2",
        sideToMove: "white",
      },
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
        gameImportId: fromGameImportId("game-import:board:tree"),
        kind: "reviewMoment",
        ply: 2,
        reviewMomentId: "review-moment:board:1",
        reviewSide: "white",
      },
      pendingMove: null,
      playerChangedAtRevision: null,
      revision: 4,
      revisionChangedBy: null,
      shownLine: null,
      study: null,
      viewedPly: 2,
    })

    expect(snapshot.kind).toBe("coachingBoard")
    expect(snapshot.exploration.branches.map((item) => item.moveUci)).toEqual([
      "e2e4",
      "d2d4",
      "e7e5",
    ])
    expect(
      snapshot.exploration.branches.find((item) => item.moveUci === "d2d4")
        ?.active,
    ).toBe(false)
    expect(snapshot.exploration.activeBranchId).toBe(childId)
    expect(snapshot.exploration.pathFromRoot).toEqual([rootId, childId])
    expect(snapshot.constraints.sentences.length).toBeGreaterThan(0)
    expect(snapshot.revision).toBe(4)
  })

  test("advancing the page revision increases it and names the actor", () => {
    const moved = advancedPageRevision(
      { playerChangedAtRevision: null, revision: 1, revisionChangedBy: null },
      "player",
    )
    expect(moved).toEqual({
      playerChangedAtRevision: 2,
      revision: 2,
      revisionChangedBy: "player",
    })
    // The agent's own advance never erases what the Player did before it.
    expect(advancedPageRevision(moved, "agent")).toEqual({
      playerChangedAtRevision: 2,
      revision: 3,
      revisionChangedBy: "agent",
    })
  })

  test("lobbyResult carries constraints and no snapshot", () => {
    const lobby = lobbyResult()
    expect(lobby.kind).toBe("lobby")
    expect(lobby).not.toHaveProperty("origin")
    expect(lobby).not.toHaveProperty("exploration")
    expect(lobby.constraints.kind).toBe("constraints")
  })
})
