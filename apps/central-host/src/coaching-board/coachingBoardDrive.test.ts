import { beforeAll, describe, expect, test } from "vitest"

import {
  fromAlternativeMoveId,
  fromBranchRef,
  fromGameImportId,
  type AlternativeMoveResult,
  type GameReview,
  type ReviewSide,
} from "@chenchess/coach-engine-sdk"

import {
  fixtureCore,
  fixtureGameReview,
  loadReviewSessionFixtures,
} from "@/review-session/reviewSessionStreamFixtures"

import {
  applyBoardAnnotation,
  applyExplorationBranches,
  applyExploredLine,
  applyOrientation,
  applyPendingMove,
  applySetPosition,
  applyStepLine,
  driveCurrentBoardPosition,
  applyShowLine,
  applyStudyAnswer,
  applyStudyRestart,
  engineArrowUci,
  gameBoardDrive,
  openingBoardDrive,
  snapshotFromDrive,
  viewedLines,
  type CoachingBoardDriveState,
} from "./coachingBoardDrive"
import type { BoardAnnotationRequest } from "./boardAnnotation"
import type { CoachingBoardOrientation } from "./coachingBoardSnapshot"
import { parseSetPosition, parseShowLine } from "./coachingBoardToolInput"
import { parseOpeningLineRef } from "./openingLineRef"
import { openingLineMoves } from "./openingMoves"
import { openingStudyWorlds } from "./openingStudyWorld"

beforeAll(async () => {
  await loadReviewSessionFixtures()
})

const rootId = fromAlternativeMoveId("alternative-move:board:e4")
const missingId = fromAlternativeMoveId("alternative-move:board:missing")
const siblingId = fromAlternativeMoveId("alternative-move:board:d4")

function reviewWithLines(): GameReview {
  const review = fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("fixture Game Review has a Critical Moment")
  moment.objective.lines = {
    best: [{ san: "Nxe5", uci: moment.objective.bestMoveUci || "e2e4" }],
    refutation: [{ san: "e5", uci: "e7e5" }],
  }
  return review
}

function branch(): AlternativeMoveResult {
  const core = fixtureCore()
  const evaluation = {
    kind: "centipawns" as const,
    perspective: "white" as const,
    value: 20,
  }
  return {
    alternativeMoveId: rootId,
    branchRef: fromBranchRef("branch:board:e4"),
    evaluation: {
      bestMove: evaluation,
      bestMoveUci: "e2e4",
      comparison: { kind: "centipawns", value: 0 },
      selectedMove: evaluation,
    },
    moveUci: "e2e4",
    parent: {
      kind: "root",
      positionRef: core.positionSnapshot.positionRef,
    },
    resultingPosition: structuredClone(core.positionSnapshot),
    sourcePositionRef: core.positionSnapshot.positionRef,
    strongestReply: { kind: "terminal" },
  }
}

/** A second branch, so a tree can gain one without replacing the first. */
function sibling(): AlternativeMoveResult {
  return {
    ...branch(),
    alternativeMoveId: siblingId,
    branchRef: fromBranchRef("branch:board:d4"),
    moveUci: "d2d4",
  }
}

function board() {
  return gameBoardDrive({
    branches: [branch()],
    gameImportId: fromGameImportId("game-import:board:drive"),
    importedGame: fixtureCore().importedGame,
    review: reviewWithLines(),
  })
}

/** What a snapshot says about when one branch joined the tree. */
function arrival(state: CoachingBoardDriveState, id = siblingId) {
  const found = snapshotFromDrive(state).exploration.branches.find(
    (candidate) => candidate.alternativeMoveId === id,
  )
  if (!found) throw new Error("the branch is in the tree")
  return { addedAtRevision: found.addedAtRevision, addedBy: found.addedBy }
}

describe("Coaching Board drive", () => {
  test("show-line accepts only the closed HostTurnShowLine union", () => {
    expect(parseShowLine({ kind: "engineBest" })).toEqual({
      kind: "ok",
      line: { kind: "engineBest" },
    })
    expect(parseShowLine({ kind: "playedMoveRefutation" })).toEqual({
      kind: "ok",
      line: { kind: "playedMoveRefutation" },
    })
    expect(
      parseShowLine({
        alternativeMoveId: rootId,
        kind: "alternativeMove",
      }),
    ).toEqual({
      kind: "ok",
      line: { alternativeMoveId: rootId, kind: "alternativeMove" },
    })
    expect(parseShowLine({ kind: "inventedLine" })).toEqual({
      kind: "refused",
      reason: "outsideClosedLineUnion",
    })
    expect(
      parseShowLine({ kind: "engineBest", fen: "8/8/8/8/8/8/8/8 w - -" }),
    ).toEqual({
      kind: "refused",
      reason: "outsideClosedLineUnion",
    })
    expect(parseShowLine({ moves: ["e2e4", "e7e5"] })).toEqual({
      kind: "refused",
      reason: "outsideClosedLineUnion",
    })
    expect(
      parseShowLine({ kind: "alternativeMove", alternativeMoveId: "e2e4" }),
    ).toEqual({
      kind: "refused",
      reason: "outsideClosedLineUnion",
    })
  })

  test("set-position accepts a ply of the Game or an explored branch", () => {
    const ply = fixtureCore().importedGame.game.moves[0]?.ply
    if (ply === undefined) throw new Error("fixture Game has a ply")
    expect(parseSetPosition({ kind: "ply", ply })).toEqual({
      kind: "ok",
      target: { kind: "ply", ply },
    })
    expect(
      parseSetPosition({
        alternativeMoveId: rootId,
        kind: "alternativeMove",
      }),
    ).toEqual({
      kind: "ok",
      target: { alternativeMoveId: rootId, kind: "alternativeMove" },
    })
    // A shape the schema rejects is refused for its shape: the board never
    // looked for a position, so it must not claim one was unreachable.
    expect(parseSetPosition({ kind: "fen", fen: "start" })).toEqual({
      kind: "refused",
      reason: "outsideTargetVocabulary",
    })
    expect(parseSetPosition({ target: { kind: "ply", ply } })).toEqual({
      kind: "refused",
      reason: "outsideTargetVocabulary",
    })
  })

  test("an unreachable target is a typed refusal and the board is unchanged", () => {
    const state = board()
    const before = snapshotFromDrive(state)
    const ply = applySetPosition(state, "agent", { kind: "ply", ply: 9999 })
    expect(ply).toMatchObject({
      kind: "refused",
      reason: "unreachablePosition",
      snapshot: { revision: before.revision, viewedPly: before.viewedPly },
    })
    const missing = applySetPosition(state, "agent", {
      alternativeMoveId: missingId,
      kind: "alternativeMove",
    })
    expect(missing).toMatchObject({
      kind: "refused",
      reason: "unreachablePosition",
      snapshot: { revision: before.revision },
    })
    expect(state.revision).toBe(before.revision)
    expect(state.viewedPly).toBe(before.viewedPly)
    expect(state.activeBranchId).toBeNull()
  })

  test("a line with no render option cannot be shown", () => {
    const review = fixtureGameReview()
    for (const moment of review.criticalMoments) {
      moment.objective.lines = null
    }
    const empty = gameBoardDrive({
      gameImportId: fromGameImportId("game-import:board:noline"),
      importedGame: fixtureCore().importedGame,
      review,
    })
    const noBest = applyShowLine(empty, "agent", { kind: "engineBest" })
    expect(noBest).toMatchObject({
      kind: "refused",
      reason: "noRenderOption",
      snapshot: { revision: 1, shownLine: null },
    })
    const noBranch = applyShowLine(empty, "agent", {
      alternativeMoveId: rootId,
      kind: "alternativeMove",
    })
    expect(noBranch).toMatchObject({
      kind: "refused",
      reason: "noRenderOption",
    })
  })

  test("both tools return the updated snapshot and the revision advances", () => {
    const state = board()
    const ply = [...state.positionsByPly.keys()].find(
      (candidate) => candidate !== state.viewedPly,
    )
    if (ply === undefined) throw new Error("fixture Game has a second ply")
    const shown = applyShowLine(state, "agent", { kind: "engineBest" })
    if (shown.kind !== "applied")
      throw new Error("engineBest has a render option")
    expect(shown.snapshot.kind).toBe("coachingBoard")
    expect(shown.snapshot.revision).toBe(state.revision + 1)
    expect(shown.snapshot.shownLine).toEqual({ kind: "engineBest" })
    expect(shown.snapshot.constraints.kind).toBe("constraints")

    const moved = applySetPosition(shown.state, "agent", { kind: "ply", ply })
    if (moved.kind !== "applied") throw new Error("reachable ply applies")
    expect(moved.snapshot.revision).toBe(shown.state.revision + 1)
    expect(moved.snapshot.viewedPly).toBe(ply)
    expect(moved.snapshot.shownLine).toBeNull()
  })

  test("set-position to an explored branch keeps the Game ply and changes the position", () => {
    const state = board()
    const applied = applySetPosition(state, "agent", {
      alternativeMoveId: rootId,
      kind: "alternativeMove",
    })
    if (applied.kind !== "applied") throw new Error("explored branch applies")
    expect(applied.snapshot.exploration.activeBranchId).toBe(rootId)
    expect(applied.snapshot.viewedPly).toBe(state.viewedPly)
    expect(applied.snapshot.revision).toBe(state.revision + 1)
  })
})

describe("the engine arrow the board draws", () => {
  test("a Critical Moment ply offers the Review's best move", () => {
    const state = gameBoardDrive({
      gameImportId: fromGameImportId("game-import:board:arrow"),
      importedGame: fixtureCore().importedGame,
      review: reviewWithLines(),
    })
    expect(engineArrowUci(state)).toBe(viewedLines(state)?.best[0]?.uci)
  })

  test("an explored position offers the engine's strongest reply", () => {
    const state = applyExploredLine(
      gameBoardDrive({
        gameImportId: fromGameImportId("game-import:board:arrow"),
        importedGame: fixtureCore().importedGame,
        review: reviewWithLines(),
      }),
      [{ ...branch(), strongestReply: { kind: "offered", uci: "e7e5" } }],
    )
    expect(engineArrowUci(state)).toBe("e7e5")
  })

  test("a shown line is not argued with by a second engine move", () => {
    const shown = applyShowLine(
      gameBoardDrive({
        gameImportId: fromGameImportId("game-import:board:arrow"),
        importedGame: fixtureCore().importedGame,
        review: reviewWithLines(),
      }),
      "agent",
      { kind: "playedMoveRefutation" },
    )
    if (shown.kind !== "applied")
      throw new Error("the fixture has a refutation")
    expect(engineArrowUci(shown.state)).toBeUndefined()
  })
})

describe("an evaluated line", () => {
  test("folds in and follows to its last move", () => {
    const state = gameBoardDrive({
      gameImportId: fromGameImportId("game-import:board:line"),
      importedGame: fixtureCore().importedGame,
      review: reviewWithLines(),
    })
    const followed = applyExploredLine(state, [branch()])
    expect(followed.activeBranchId).toBe(rootId)
    expect(snapshotFromDrive(followed).exploration.branches).toHaveLength(1)
    expect(followed.revision).toBeGreaterThan(state.revision)
  })

  test("folded without following leaves the board where the Player put it", () => {
    const state = gameBoardDrive({
      gameImportId: fromGameImportId("game-import:board:line"),
      importedGame: fixtureCore().importedGame,
      review: reviewWithLines(),
    })
    const folded = applyExplorationBranches(state, "agent", [branch()])
    expect(folded.activeBranchId).toBeNull()
    expect(snapshotFromDrive(folded).exploration.branches).toHaveLength(1)
  })
})

describe("a Player's move before the Engine confirms it", () => {
  test("the board shows the derived position while the snapshot keeps the confirmed one", () => {
    const before = board()
    const confirmed = driveCurrentBoardPosition(before).fen

    const after = applyPendingMove(before, "e2e4")

    expect(after.pendingMove?.uci).toBe("e2e4")
    // The board draws the move the Player played.
    expect(driveCurrentBoardPosition(after).fen).toBe(
      after.pendingMove?.derivedPosition.fen,
    )
    expect(driveCurrentBoardPosition(after).fen).not.toBe(confirmed)
    // The snapshot still reports what the Engine confirmed, and says the rest
    // separately. Reporting the derived position as currentPosition is the
    // alternative ADR 0060 rejects: pathFromRoot could not explain it.
    const snapshot = snapshotFromDrive(after)
    expect(snapshot.currentPosition.fen).toBe(confirmed)
    expect(snapshot.currentPosition.fen).not.toBe(
      after.pendingMove?.derivedPosition.fen,
    )
    expect(snapshot.exploration.pathFromRoot).toEqual(
      snapshotFromDrive(before).exploration.pathFromRoot,
    )
    expect(snapshot.pendingMove?.uci).toBe("e2e4")
    expect(snapshot.pendingMove?.derivedPosition.sideToMove).toBe("black")
  })

  test("a pending move carries no evaluation to infer one from", () => {
    const snapshot = snapshotFromDrive(applyPendingMove(board(), "e2e4"))
    expect(snapshot.pendingMove).not.toHaveProperty("evaluation")
    expect(Object.keys(snapshot.pendingMove ?? {}).sort()).toEqual([
      "derivedPosition",
      "uci",
    ])
    expect(
      snapshot.constraints.sentences.some((sentence) =>
        sentence.includes("pendingMove"),
      ),
    ).toBe(true)
  })

  test("an illegal move leaves the board alone rather than guessing", () => {
    const before = board()
    expect(applyPendingMove(before, "e2e5")).toBe(before)
    expect(applyPendingMove(before, "not-a-move")).toBe(before)
  })

  test("the Engine's answer clears the pending move", () => {
    const pending = applyPendingMove(board(), "e2e4")
    expect(pending.pendingMove).not.toBeNull()

    const answered = applyExploredLine(pending, [branch()])

    expect(answered.pendingMove).toBeNull()
    expect(snapshotFromDrive(answered).pendingMove).toBeNull()
  })

  test("navigating away clears it too, the way any board move clears marks", () => {
    const pending = applyPendingMove(board(), "e2e4")
    const moved = applySetPosition(pending, "player", { kind: "ply", ply: 1 })
    expect(moved.kind).toBe("applied")
    if (moved.kind !== "applied") return
    expect(moved.state.pendingMove).toBeNull()
  })
})

describe("who changed the board", () => {
  test("a board loads with nobody having changed it and no branch newly added", () => {
    const state = board()
    const snapshot = snapshotFromDrive(state)

    expect(snapshot.revisionChangedBy).toBeNull()
    expect(snapshot.playerChangedAtRevision).toBeNull()
    expect(snapshot.exploration.branches).toMatchObject([
      { addedAtRevision: snapshot.revision, addedBy: null },
    ])
  })

  test("the agent's own call cannot erase what the Player did before it", () => {
    const seen = board()
    const browsed = applySetPosition(seen, "player", { kind: "ply", ply: 1 })
    if (browsed.kind !== "applied") throw new Error("ply 1 is reachable")

    // The agent answers without reading first, so its own change is the one
    // revisionChangedBy names — and the browse would otherwise vanish.
    const shown = applyShowLine(browsed.state, "agent", { kind: "engineBest" })
    if (shown.kind !== "applied")
      throw new Error("engineBest has a render option")

    expect(shown.snapshot.revisionChangedBy).toBe("agent")
    expect(shown.snapshot.playerChangedAtRevision).toBe(
      browsed.snapshot.revision,
    )
    expect(shown.snapshot.playerChangedAtRevision).toBeGreaterThan(
      seen.revision,
    )
  })

  test("the Player's own explored move lands on one revision, the one it reports", () => {
    const state = board()
    const played = applyExploredLine(state, [sibling()])

    expect(played.revision).toBe(state.revision + 1)
    expect(played.activeBranchId).toBe(siblingId)
    expect(arrival(played)).toEqual({
      addedAtRevision: played.revision,
      addedBy: "player",
    })
  })

  test("the Player's drag names the Player before the Engine answers", () => {
    const drawn = snapshotFromDrive(applyPendingMove(board(), "e2e4"))

    expect(drawn.revisionChangedBy).toBe("player")
    expect(drawn.playerChangedAtRevision).toBe(drawn.revision)
  })

  test("walking a line names whoever walked it", () => {
    const shown = applyShowLine(board(), "agent", { kind: "engineBest" })
    if (shown.kind !== "applied")
      throw new Error("engineBest has a render option")

    const walked = applyStepLine(shown.state, "player", "next")
    if (walked.kind !== "applied") throw new Error("the line has a next step")

    expect(walked.snapshot.revisionChangedBy).toBe("player")
    expect(walked.snapshot.playerChangedAtRevision).toBe(
      walked.snapshot.revision,
    )
  })

  test("drawing names the agent even though the board did not move", () => {
    const state = board()
    const drawn = applyBoardAnnotation(state, {
      requests: [{ kind: "square", label: "here", square: "e4" }],
      revision: state.revision,
    })
    if (drawn.kind !== "applied") throw new Error("a bare square is drawable")
    expect(drawn.snapshot.revisionChangedBy).toBe("agent")
  })

  test("a branch says which revision it arrived at and who added it", () => {
    const state = board()
    const played = applyExplorationBranches(state, "player", [sibling()])

    expect(arrival(played)).toEqual({
      addedAtRevision: played.revision,
      addedBy: "player",
    })
    // The branch the board loaded with is not new to anyone who reads it now.
    expect(arrival(played, rootId)).toEqual({
      addedAtRevision: state.revision,
      addedBy: null,
    })
  })

  test("re-analysing a branch keeps the arrival it already had", () => {
    const played = applyExplorationBranches(board(), "player", [sibling()])
    const again = applyExplorationBranches(played, "agent", [sibling()])

    expect(again.revision).toBeGreaterThan(played.revision)
    expect(arrival(again)).toEqual(arrival(played))
  })
})

describe("turning the board around", () => {
  test("orientation is a set-position target, and only the two sides", () => {
    expect(
      parseSetPosition({ kind: "orientation", orientation: "black" }),
    ).toEqual({
      kind: "ok",
      target: { kind: "orientation", orientation: "black" },
    })
    expect(
      parseSetPosition({ kind: "orientation", orientation: "sideways" }),
    ).toEqual({ kind: "refused", reason: "outsideTargetVocabulary" })
    expect(parseSetPosition({ kind: "orientation" })).toEqual({
      kind: "refused",
      reason: "outsideTargetVocabulary",
    })
  })

  test("a Game board opens from the Player's own side", () => {
    expect(snapshotFromDrive(gameBoardFrom("black")).orientation).toBe("black")
    expect(snapshotFromDrive(gameBoardFrom("white")).orientation).toBe("white")
    // A Game reviewed from both sides has no side of its own.
    expect(snapshotFromDrive(gameBoardFrom("both")).orientation).toBe("white")
  })

  test("an Opening Line board opens from White's side, and turns too", () => {
    const opening = openingBoard()
    expect(snapshotFromDrive(opening).orientation).toBe("white")

    expect(turnedTo(opening, "black").snapshot.orientation).toBe("black")
  })

  // Marks and a pending move never coexist — drawing clears the move in
  // flight, and a drag clears the marks — so each survives the turn from its
  // own board.
  test("what the coach drew survives the turn", () => {
    const state = board()
    const drawn = applyBoardAnnotation(state, {
      requests: [{ kind: "square", label: "here", square: "e4" }],
      revision: state.revision,
    })
    if (drawn.kind !== "applied") throw new Error("a bare square is drawable")
    expect(drawn.snapshot.marks).toHaveLength(1)

    const turned = turnedTo(drawn.state, "black")

    expect(turned.snapshot.orientation).toBe("black")
    expect(turned.snapshot.marks).toEqual(drawn.snapshot.marks)
    expect(turned.snapshot.currentPosition).toEqual(
      drawn.snapshot.currentPosition,
    )
    expect(turned.snapshot.viewedPly).toBe(drawn.snapshot.viewedPly)
  })

  test("a move still in flight survives it too", () => {
    const pending = applyPendingMove(board(), "e2e4")
    expect(pending.pendingMove).not.toBeNull()

    const turned = turnedTo(pending, "black")

    expect(turned.snapshot.pendingMove).toEqual({
      derivedPosition: pending.pendingMove?.derivedPosition,
      uci: "e2e4",
    })
  })

  test("the view did change, so the revision advances and names who turned it", () => {
    const state = board()
    const turned = turnedTo(state, "black")

    expect(turned.snapshot.revision).toBe(state.revision + 1)
    expect(turned.snapshot.revisionChangedBy).toBe("agent")
    expect(turned.snapshot.playerChangedAtRevision).toBeNull()
  })

  test("a mark held against the revision before the turn is stale, not drawn", () => {
    const state = board()
    const turned = turnedTo(state, "black")

    const request = [
      { kind: "square", label: "here", square: "e4" },
    ] as const satisfies readonly BoardAnnotationRequest[]
    // The board did not move, so the marks would still be true — but the
    // revision did advance, and annotation is contracted on the revision the
    // agent read, not on whether the position changed.
    expect(
      applyBoardAnnotation(turned.state, {
        requests: request,
        revision: state.revision,
      }),
    ).toMatchObject({ kind: "refused", reason: "staleRevision" })
    expect(
      applyBoardAnnotation(turned.state, {
        requests: request,
        revision: turned.snapshot.revision,
      }),
    ).toMatchObject({ kind: "applied" })
  })

  test("the side survives a move of the board", () => {
    const browsed = applySetPosition(
      turnedTo(board(), "black").state,
      "player",
      {
        kind: "ply",
        ply: 1,
      },
    )
    if (browsed.kind !== "applied") throw new Error("ply 1 is reachable")

    expect(browsed.snapshot.orientation).toBe("black")
  })
})

/** Turning the board cannot refuse, so a test reads the state it produced
 * rather than unwrapping a refusal it can never get. */
function turnedTo(
  state: CoachingBoardDriveState,
  orientation: CoachingBoardOrientation,
) {
  const next = applyOrientation(state, "agent", orientation)
  return { snapshot: snapshotFromDrive(next), state: next }
}

function gameBoardFrom(reviewSide: ReviewSide) {
  return gameBoardDrive({
    gameImportId: fromGameImportId("game-import:board:orientation"),
    importedGame: { ...fixtureCore().importedGame, reviewSide },
    review: null,
  })
}

function openingBoard() {
  return openingBoardDrive({
    eco: "B90",
    moves: openingLineMoves("1. e4 c5 2. Nf3 d6"),
    name: "Sicilian Najdorf",
    openingLineRef: openingLineRef(),
    viewedPly: 4,
  })
}

function openingLineRef() {
  const ref = parseOpeningLineRef("B90-sicilian-najdorf-1a2b")
  if (!ref) throw new Error("the opening address parses")
  return ref
}

describe("naming the move on screen", () => {
  test("mainLine names what reached the viewed ply, what the Game played next, and the Review's verdict", () => {
    const core = fixtureCore()
    const review = fixtureGameReview()
    const moves = core.importedGame.game.moves
    const second = moves[1]
    if (!second) throw new Error("fixture Game has two plies")
    const state = gameBoardDrive({
      gameImportId: fromGameImportId("game-import:board:main-line"),
      importedGame: core.importedGame,
      review,
      viewedPly: second.ply,
    })
    const line = snapshotFromDrive(state).mainLine
    // The board at a ply stands before that ply's move, so the caption's move
    // is the continuation and the one before it reached the position.
    expect(line.continuesWith).toMatchObject({
      ply: second.ply,
      san: second.san,
      side: second.side,
      uci: second.uci,
    })
    expect(line.continuesWith?.label).toBe(
      `${second.moveNumber}${second.side === "black" ? "…" : "."} ${second.san}`,
    )
    expect(line.reachedBy).toMatchObject({
      ply: moves[0]?.ply,
      san: moves[0]?.san,
    })
    expect(line.lastPly).toBe(moves.at(-1)?.ply)
    expect(line.evaluation).toEqual(
      review.evaluationTimeline.find((point) => point.ply === second.ply)
        ?.evaluation ?? null,
    )
  })

  test("the first ply reached nothing, the last ply continues with nothing, and the Player's side is named", () => {
    const core = fixtureCore()
    const first = gameBoardDrive({
      gameImportId: fromGameImportId("game-import:board:main-line"),
      importedGame: { ...core.importedGame, reviewSide: "black" },
      review: null,
      viewedPly: 1,
    })
    const start = snapshotFromDrive(first)
    expect(start.mainLine.reachedBy).toBeNull()
    expect(start.mainLine.continuesWith?.ply).toBe(1)
    expect(start.mainLine.evaluation).toBeNull()
    expect(start.origin).toMatchObject({
      kind: "reviewMoment",
      reviewSide: "black",
    })
    const lastPly = core.importedGame.game.moves.at(-1)?.ply
    if (lastPly === undefined) throw new Error("fixture Game has moves")
    // A Game board stands before its last move at most; the Opening Line is
    // the one that adds the position after its final ply.
    const moved = applySetPosition(first, "player", {
      kind: "ply",
      ply: lastPly,
    })
    if (moved.kind !== "applied") throw new Error("the last ply is reachable")
    expect(moved.snapshot.mainLine.continuesWith?.ply).toBe(lastPly)
    expect(moved.snapshot.mainLine.reachedBy?.ply).toBe(lastPly - 1)
    expect(moved.snapshot.origin).toMatchObject({ reviewSide: "black" })
  })

  test("an Opening Line names its catalog moves and carries no Review evaluation", () => {
    const line = snapshotFromDrive(openingBoard()).mainLine
    expect(line.reachedBy?.san).toBe("Nf3")
    expect(line.continuesWith?.label).toBe("2… d6")
    expect(line.evaluation).toBeNull()
    expect(line.lastPly).toBe(4)
  })
})

describe("the study session in the snapshot", () => {
  const world = [...openingStudyWorlds.values()][0]
  if (!world) throw new Error("the study catalog authors at least one world")
  const moves = openingLineMoves(
    "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6",
  )

  function studiedBoard() {
    return openingBoardDrive({
      eco: "B90",
      moves,
      name: "Sicilian Najdorf",
      openingLineRef: openingLineRef(),
      viewedPly: moves.length,
      world,
    })
  }

  test("a Game board and a line without a world carry no study", () => {
    expect(snapshotFromDrive(openingBoard()).study).toBeNull()
    expect(snapshotFromDrive(gameBoardFrom("white")).study).toBeNull()
  })

  test("the snapshot shows the card the Player is on, in the words the page asks it", () => {
    const study = snapshotFromDrive(studiedBoard()).study
    if (!study) throw new Error("an authored world starts a session")
    const slot = world.slots[0]
    if (!slot) throw new Error("a world opens by placing a piece")
    expect(study.card).toMatchObject({
      accepts: slot.accepts,
      ask: { kind: "choice", options: slot.options },
      kind: "slot",
      position: 1,
      prompt: `Where does the ${slot.piece.toLowerCase()} belong in this structure?`,
      title: "Build the world",
      viewedPly: slot.playedAtPly - 1,
    })
    expect(study.answered).toEqual([])
    expect(study.cardCount).toBe(
      world.slots.length + 2 + world.deviations.length,
    )
    expect(study.side).toBe(world.side)
  })

  test("answering is the Player's change: it grades, moves the board to the next card, and advances the revision", () => {
    const state = studiedBoard()
    const slot = world.slots[0]
    const wrong = slot?.options.find((option) => !slot.accepts.includes(option))
    if (!slot || !wrong) throw new Error("a slot offers a square it refuses")
    const answered = applyStudyAnswer(state, wrong)
    const snapshot = snapshotFromDrive(answered)
    expect(snapshot.revision).toBe(state.revision + 1)
    expect(snapshot.revisionChangedBy).toBe("player")
    expect(snapshot.playerChangedAtRevision).toBe(snapshot.revision)
    expect(snapshot.study?.answered).toHaveLength(1)
    expect(snapshot.study?.answered[0]).toMatchObject({
      answer: wrong,
      card: { kind: "slot", position: 1 },
      verdict: { kind: "incorrect", why: slot.why },
    })
    expect(snapshot.study?.card?.position).toBe(2)
    expect(snapshot.viewedPly).toBe(snapshot.study?.card?.viewedPly)
    // Answering is a move of the board: what the coach drew is gone with it.
    expect(snapshot.marks).toEqual([])
  })

  test("the plan is carried ungraded with its rubric for the coach to mark", () => {
    let state = studiedBoard()
    for (const slot of world.slots) {
      state = applyStudyAnswer(state, slot.accepts[0] ?? "")
    }
    const plan = "Castle, then push the e-pawn and fight for the centre."
    const marked = snapshotFromDrive(applyStudyAnswer(state, plan)).study
    expect(marked?.answered.at(-1)).toMatchObject({
      answer: plan,
      card: { kind: "plan", title: "Say the plan" },
      verdict: { kind: "ungraded", rubric: world.rubric },
    })
    expect(marked?.tally).toEqual({
      graded: world.slots.length,
      right: world.slots.length,
      ungraded: 1,
    })
  })

  test("building the world again empties the answers and returns to the first card", () => {
    const answered = applyStudyAnswer(
      studiedBoard(),
      world.slots[0]?.accepts[0] ?? "",
    )
    const rebuilt = snapshotFromDrive(applyStudyRestart(answered))
    expect(rebuilt.study?.answered).toEqual([])
    expect(rebuilt.study?.card?.position).toBe(1)
    expect(rebuilt.revisionChangedBy).toBe("player")
    expect(rebuilt.viewedPly).toBe(rebuilt.study?.card?.viewedPly)
  })

  test("a board with no session ignores an answer", () => {
    const state = openingBoard()
    expect(applyStudyAnswer(state, "e4")).toBe(state)
  })
})

describe("where a study session opens", () => {
  const world = [...openingStudyWorlds.values()][0]
  if (!world) throw new Error("the study catalog authors at least one world")
  const moves = openingLineMoves(
    "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6",
  )
  const firstCardPly = (world.slots[0]?.playedAtPly ?? 2) - 1

  function opened(viewedPly: number) {
    return openingBoardDrive({
      eco: "B90",
      moves,
      name: "Sicilian Najdorf",
      openingLineRef: openingLineRef(),
      viewedPly,
      world,
    })
  }

  test("a line opened at its end starts on the first card, without anyone having changed the board", () => {
    const snapshot = snapshotFromDrive(opened(moves.length))
    expect(snapshot.viewedPly).toBe(firstCardPly)
    expect(snapshot.revisionChangedBy).toBeNull()
    expect(snapshot.playerChangedAtRevision).toBeNull()
  })

  test("a line opened at a named ply stays there: the address and the Player outrank the session", () => {
    expect(snapshotFromDrive(opened(3)).viewedPly).toBe(3)
  })
})
