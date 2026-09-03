import { expect, test } from "vitest"
import {
  fromGameImportId,
  mintOperationId,
  mintRequestId,
  type ReviewSessionEvent,
} from "@chenchess/coach-engine-sdk"
import { sharedGroundingSentences } from "@chenchess/shared-assets"

import {
  contractedCoachModelToolNames,
  coachAppOnlyToolNames,
  coachWebToolNames,
} from "../../server/board/tool-surface"
import {
  evaluatePlayerLineDescription,
  listCriticalMomentsDescription,
  openReviewMomentInPlaceDescription,
} from "../../server/board/conversation-policy"
import type {
  AlternativeMoveResult,
  OperationCompletion,
} from "@chenchess/coach-engine-sdk"
import {
  fromAlternativeMoveId,
  fromBranchRef,
} from "@chenchess/coach-engine-sdk"
import { canonicalMovesFromFen } from "@chenchess/review-projection"
import { completionFixture } from "../../server/reviewCompletionFixtures"

import {
  evaluatePlayerLineWebDescription,
  listCriticalMomentsWebDescription,
  openReviewMomentInPlaceWebDescription,
} from "./coachingBoardConstraints"
import {
  evaluatePlayerLineOnBoard,
  listCriticalMomentsOnBoard,
  openReviewMomentInPlaceOnBoard,
  type CoachingBoardCommandExecute,
} from "./coachingBoardCoachTools"
import { boardConstraints } from "./coachingBoardConstraints"
import { loadedBranches } from "./coachingBoardDrive"
import {
  coachingBoardSnapshot,
  type CoachingBoardExplorationBranch,
} from "./coachingBoardSnapshot"

/** For the assertions about content rather than what the board did with it. */
function foldNowhere() {
  return snapshot()
}

function snapshot() {
  return coachingBoardSnapshot({
    activeBranchId: null,
    branches: [],
    constraints: boardConstraints(),
    currentPosition: {
      fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
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
      gameImportId: fromGameImportId("game-import:board:coach-tools"),
      kind: "reviewMoment",
      ply: 1,
      reviewMomentId: null,
      reviewSide: "white",
    },
    pendingMove: null,
    playerChangedAtRevision: null,
    revision: 4,
    revisionChangedBy: null,
    shownLine: null,
    study: null,
    viewedPly: 1,
  })
}

function completed(
  result: Extract<ReviewSessionEvent, { kind: "completed" }>["result"],
): ReviewSessionEvent {
  return {
    kind: "completed",
    result,
  }
}

function scriptedExecute(
  explore: ReviewSessionEvent | ReviewSessionEvent[],
): CoachingBoardCommandExecute {
  const remaining = Array.isArray(explore) ? [...explore] : [explore]
  return async (command, options) => {
    if (command.kind === "exploreAlternativeMove") {
      const operationId =
        options?.operationId ?? mintOperationId("web", "player-line-test")
      options?.onEvent?.({
        event: {
          kind: "progress",
          stage: { kind: "alternativeMoveAllowance", remaining: 20 },
        },
        operationId,
        requestId: mintRequestId("web", "player-line-test"),
        sequence: 0,
      })
      return (
        remaining.shift() ?? {
          kind: "rejected",
          operation: "alternativeMoveEvaluation",
          reason: "illegalMove",
          recovery: { kind: "correctInput" },
        }
      )
    }
    switch (command.kind) {
      case "readGameReviewSnapshot":
        return completed(completionFixture("gameReviewSnapshotRead"))
      case "openAddressedReviewMoment":
        return completed(completionFixture("addressedReviewMomentOpened"))
      case "startReviewSession":
        return completed(completionFixture("reviewSessionStarted"))
      case "openReviewMoment":
        return completed(completionFixture("reviewMomentOpened"))
      case "inspectPosition":
        return completed(completionFixture("positionInspected"))
      default:
        return {
          kind: "rejected",
          operation: "commandAdmission",
          reason: "unknownCommand",
          recovery: { kind: "correctInput" },
        }
    }
  }
}

function playerLineBoardExecute(
  explore: ReviewSessionEvent[],
): CoachingBoardCommandExecute {
  const remaining = [...explore]
  const started = completionFixture("reviewSessionStarted")
  const detail = completionFixture("reviewMomentDetailRead").detail
  const moment = started.reviewMoments[0]
  if (!moment) throw new Error("started fixture has a Review Moment")
  return async (command, options) => {
    if (command.kind === "exploreAlternativeMove") {
      const operationId =
        options?.operationId ?? mintOperationId("web", "player-line-test")
      options?.onEvent?.({
        event: {
          kind: "progress",
          stage: { kind: "alternativeMoveAllowance", remaining: 20 },
        },
        operationId,
        requestId: mintRequestId("web", "player-line-test"),
        sequence: 0,
      })
      return (
        remaining.shift() ?? {
          kind: "rejected",
          operation: "alternativeMoveEvaluation",
          reason: "illegalMove",
          recovery: { kind: "correctInput" },
        }
      )
    }
    if (command.kind === "openAddressedReviewMoment") {
      return completed({ detail, kind: "addressedReviewMomentOpened" })
    }
    if (command.kind === "startReviewSession") {
      return completed(started)
    }
    if (command.kind === "openReviewMoment") {
      // SAFETY: preparePlayerLineRoot only discriminates on completion.kind.
      return completed({
        kind: "reviewMomentOpened",
      } as OperationCompletion)
    }
    if (command.kind === "inspectPosition") {
      if (moment.authoring.kind !== "prepared") {
        throw new Error("started fixture Review Moment is prepared")
      }
      return completed({
        inspection: {
          context: moment.authoring.core.coachTurnContext,
          evaluation: {
            kind: "centipawns",
            perspective: moment.positionSnapshot.sideToMove,
            value: 0,
          },
          evidencePacket: moment.authoring.core.evidencePacket,
          positionSnapshot: moment.positionSnapshot,
          sideToMove: moment.positionSnapshot.sideToMove,
          textBoard: "",
        },
        kind: "positionInspected",
      })
    }
    return {
      kind: "rejected",
      operation: "commandAdmission",
      reason: "unknownCommand",
      recovery: { kind: "correctInput" },
    }
  }
}

function firstLegalAlternative() {
  const moment = completionFixture("reviewSessionStarted").reviewMoments[0]
  if (!moment) throw new Error("started fixture has a Review Moment")
  const fen = moment.positionSnapshot.fen
  const uci =
    ["e2e4", "e7e5", "g1f3", "d2d4", "c2c4", "b1c3", "b8c6"].find((candidate) =>
      canonicalMovesFromFen(fen, [{ uci: candidate }]),
    ) ?? "e2e4"
  const evaluation = {
    bestMove: {
      kind: "centipawns" as const,
      perspective: moment.positionSnapshot.sideToMove,
      value: 12,
    },
    bestMoveUci: uci,
    comparison: { kind: "centipawns" as const, value: 0 },
    selectedMove: {
      kind: "centipawns" as const,
      perspective: moment.positionSnapshot.sideToMove,
      value: 12,
    },
  }
  return {
    alternative: {
      alternativeMoveId: fromAlternativeMoveId("alternative-move:board:line"),
      branchRef: fromBranchRef("branch:board:line"),
      evaluation,
      moveUci: uci,
      parent: {
        kind: "root" as const,
        positionRef: moment.positionSnapshot.positionRef,
      },
      resultingPosition: moment.positionSnapshot,
      sourcePositionRef: moment.positionSnapshot.positionRef,
      strongestReply: { kind: "terminal" as const },
    } satisfies AlternativeMoveResult,
    move: { kind: "uci" as const, uci },
  }
}

test("web descriptions assemble the MCP sentences and shared grounding, not a fork", () => {
  expect(
    listCriticalMomentsWebDescription.startsWith(
      listCriticalMomentsDescription,
    ),
  ).toBe(true)
  expect(
    evaluatePlayerLineWebDescription.startsWith(evaluatePlayerLineDescription),
  ).toBe(true)
  expect(
    openReviewMomentInPlaceWebDescription.startsWith(
      openReviewMomentInPlaceDescription,
    ),
  ).toBe(true)
  for (const sentence of sharedGroundingSentences) {
    expect(listCriticalMomentsWebDescription).toContain(sentence)
    expect(evaluatePlayerLineWebDescription).toContain(sentence)
    expect(openReviewMomentInPlaceWebDescription).toContain(sentence)
    expect(listCriticalMomentsDescription).not.toContain(sentence)
    expect(evaluatePlayerLineDescription).not.toContain(sentence)
    expect(openReviewMomentInPlaceDescription).not.toContain(sentence)
  }
})

test("the web target joins the one map without changing MCP model or app name lists", () => {
  expect(contractedCoachModelToolNames).toEqual([
    "get_coaching_digest",
    "search_reviewed_games",
    "connect_playing_profile",
    "review_game",
    "list_critical_moments",
    "open_review_moment",
    "evaluate_player_line",
    "render_move_sequence",
  ])
  expect(coachAppOnlyToolNames).toContain("open_review_moment_in_place")
  expect(coachAppOnlyToolNames).not.toContain("list_critical_moments")
  expect(coachAppOnlyToolNames).not.toContain("evaluate_player_line")
  expect(coachWebToolNames).toEqual([
    "search_reviewed_games",
    "list_critical_moments",
    "evaluate_player_line",
    "open_review_moment_in_place",
    "read_coaching_board",
    "show_line",
    "step_line",
    "set_board_position",
    "annotate_board",
    "evaluate_opening_continuation",
    "list_recent_profile_games",
    "stage_game_import",
    "find_opening_line",
    "list_played_openings",
    "open_opening_line",
    "open_reviewed_game",
    "read_session_status",
  ])
})

test("list_critical_moments returns the routing table plus constraints and snapshot", async () => {
  const board = snapshot()
  const listed = await listCriticalMomentsOnBoard({
    execute: scriptedExecute([]),
    gameImportId:
      board.origin.kind === "reviewMoment" ? board.origin.gameImportId : "",
    snapshot: board,
  })
  expect(listed).toMatchObject({
    constraints: { kind: "constraints" },
    kind: "gameReviewSnapshotRead",
    snapshot: { kind: "coachingBoard", revision: 4 },
  })
  expect(listed).toHaveProperty("reviewMoments")
  expect(listed.constraints.sentences).toEqual(board.constraints.sentences)
})

test("open_review_moment_in_place resolves against the ordered review and carries the snapshot", async () => {
  const board = snapshot()
  const detail = completionFixture("reviewMomentDetailRead").detail
  let movedTo: number | null = null
  const result = await openReviewMomentInPlaceOnBoard({
    execute: async () =>
      completed({ kind: "addressedReviewMomentOpened", detail }),
    gameImportId: detail.gameImportId,
    moment: { kind: "critical", reviewMomentId: detail.reviewMomentId },
    onOpened: ({ ply }) => {
      movedTo = ply
      return { ...board, revision: board.revision + 1, viewedPly: ply }
    },
    snapshot: board,
  })
  expect(movedTo).toBe(detail.ply)
  expect(result).toMatchObject({
    constraints: { kind: "constraints" },
    kind: "addressedReviewMomentOpened",
    ply: detail.ply,
    reviewMomentId: detail.reviewMomentId,
    snapshot: { revision: 5, viewedPly: detail.ply },
  })
})

test("web-minted operation and idempotency keys satisfy the engine semantic-id contract", async () => {
  const semanticId = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/
  const detail = completionFixture("reviewMomentDetailRead").detail
  const first = firstLegalAlternative()
  const mintedKeys = async (playerId: string) => {
    const minted: string[] = []
    const inner = playerLineBoardExecute([
      completed({
        alternativeMove: first.alternative,
        kind: "alternativeMoveEvaluated",
      }),
    ])
    const capture: CoachingBoardCommandExecute = async (command, options) => {
      if (options?.operationId) minted.push(String(options.operationId))
      if ("idempotencyKey" in command && command.idempotencyKey) {
        minted.push(String(command.idempotencyKey))
      }
      return inner(command, options)
    }
    await evaluatePlayerLineOnBoard({
      applyBranches: foldNowhere,
      execute: capture,
      input: {
        gameImportId: detail.gameImportId,
        moment: { kind: "critical", reviewMomentId: detail.reviewMomentId },
        moves: [first.move],
        opponentReplies: "supplied",
      },
      playerId,
      snapshot: snapshot(),
    })
    return minted
  }

  const minted = await mintedKeys("player:board:keys")
  expect(minted.length).toBeGreaterThan(0)
  for (const key of minted) {
    expect(key).toMatch(semanticId)
  }
  // Replaying the same line mints the same keys so the engine dedupes instead
  // of spending the Alternative Move allowance twice; a different Player
  // mints different keys.
  expect(await mintedKeys("player:board:keys")).toEqual(minted)
  expect(await mintedKeys("player:board:other")).not.toEqual(minted)
})

test("evaluate_player_line limit outcomes keep every evaluated ply and wrap the snapshot", async () => {
  const board = snapshot()
  const playerId = "player:board:line"
  const detail = completionFixture("reviewMomentDetailRead").detail
  const first = firstLegalAlternative()

  const deadline = await evaluatePlayerLineOnBoard({
    applyBranches: foldNowhere,
    execute: playerLineBoardExecute([
      completed({
        alternativeMove: first.alternative,
        kind: "alternativeMoveEvaluated",
      }),
      {
        kind: "unavailable",
        operation: "alternativeMoveEvaluation",
        reason: { kind: "timeout", provider: "stockfish" },
        retry: { kind: "retryAllowed" },
      },
    ]),
    input: {
      gameImportId: detail.gameImportId,
      moment: {
        kind: "critical",
        reviewMomentId: detail.reviewMomentId,
      },
      moves: [first.move, { kind: "uci", uci: "e7e5" }],
      opponentReplies: "supplied",
    },
    playerId,
    snapshot: board,
  })
  expect(deadline).toMatchObject({
    constraints: { kind: "constraints" },
    operation: "evaluate_player_line",
    outcome: "completed",
    result: {
      kind: "playerLineEvaluated",
      verdict: { kind: "deadlineReached" },
    },
    snapshot: { kind: "coachingBoard", revision: 4 },
  })
  expect(deadline.outcome === "completed" && deadline.result.plies.length).toBe(
    1,
  )

  const exhausted = await evaluatePlayerLineOnBoard({
    applyBranches: foldNowhere,
    execute: playerLineBoardExecute([
      completed({
        alternativeMove: first.alternative,
        kind: "alternativeMoveEvaluated",
      }),
      {
        kind: "rejected",
        operation: "alternativeMoveEvaluation",
        reason: "alternativeMoveLimit",
        recovery: { kind: "correctInput" },
      },
    ]),
    input: {
      gameImportId: detail.gameImportId,
      moment: {
        kind: "critical",
        reviewMomentId: detail.reviewMomentId,
      },
      moves: [first.move, { kind: "uci", uci: "e7e5" }],
      opponentReplies: "supplied",
    },
    playerId,
    snapshot: board,
  })
  expect(
    exhausted.outcome === "completed" && exhausted.result.verdict.kind,
  ).toBe("explorationExhausted")
  expect(
    exhausted.outcome === "completed" && exhausted.result.plies.length,
  ).toBe(1)

  const illegal = await evaluatePlayerLineOnBoard({
    applyBranches: foldNowhere,
    execute: playerLineBoardExecute([
      completed({
        alternativeMove: first.alternative,
        kind: "alternativeMoveEvaluated",
      }),
      {
        kind: "rejected",
        operation: "alternativeMoveEvaluation",
        reason: "illegalMove",
        recovery: { kind: "correctInput" },
      },
    ]),
    input: {
      gameImportId: detail.gameImportId,
      moment: {
        kind: "critical",
        reviewMomentId: detail.reviewMomentId,
      },
      moves: [first.move, { kind: "uci", uci: "a2a2" }],
      opponentReplies: "supplied",
    },
    playerId,
    snapshot: board,
  })
  expect(illegal.outcome === "completed" && illegal.result.verdict.kind).toBe(
    "illegalMove",
  )
  expect(illegal.outcome === "completed" && illegal.result.plies.length).toBe(1)
  expect(
    illegal.outcome === "completed" && illegal.result.renderOptions[0]?.kind,
  ).toBe("playerLine")
})

test("evaluate_player_line folds every evaluated move into the board it answers with", async () => {
  const detail = completionFixture("reviewMomentDetailRead").detail
  const first = firstLegalAlternative()
  const folded: CoachingBoardExplorationBranch[] = []

  const evaluated = await evaluatePlayerLineOnBoard({
    applyBranches: (minted) => {
      folded.push(...minted)
      return coachingBoardSnapshot({
        activeBranchId: null,
        branches: loadedBranches(minted),
        constraints: boardConstraints(),
        currentPosition: {
          fen: first.alternative.resultingPosition.fen,
          sideToMove: first.alternative.resultingPosition.sideToMove,
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
          gameImportId: fromGameImportId("game-import:board:coach-tools"),
          kind: "reviewMoment",
          ply: 1,
          reviewMomentId: null,
          reviewSide: "white",
        },
        pendingMove: null,
        playerChangedAtRevision: null,
        revision: 5,
        revisionChangedBy: null,
        shownLine: null,
        study: null,
        viewedPly: 1,
      })
    },
    execute: playerLineBoardExecute([
      completed({
        alternativeMove: first.alternative,
        kind: "alternativeMoveEvaluated",
      }),
    ]),
    input: {
      gameImportId: detail.gameImportId,
      moment: { kind: "critical", reviewMomentId: detail.reviewMomentId },
      moves: [first.move],
      opponentReplies: "supplied",
    },
    playerId: "player:board:folding",
    snapshot: snapshot(),
  })

  // The agent evaluates and the board keeps the branch, so a later show_line
  // or set_board_position can address it; the answer carries the tree that
  // resulted, not the one that went in.
  expect(folded.map((branch) => branch.moveUci)).toEqual([
    first.alternative.moveUci,
  ])
  expect(evaluated.snapshot?.revision).toBe(5)
  expect(evaluated.snapshot?.exploration.branches).toHaveLength(1)
})
