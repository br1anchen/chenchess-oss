// @vitest-environment jsdom

import {
  fromAlternativeMoveId,
  fromBranchRef,
  fromOperationId,
  fromPositionRef,
  fromRequestId,
  type AlternativeMoveResult,
  type MoveInput,
  type OperationCompletion,
  type ReviewSessionCommand,
  type ReviewSessionEvent,
} from "@chenchess/coach-engine-sdk"
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeAll, expect, test } from "vitest"

import { ChenTheme } from "@chenchess/ui/theme"

import { completionFixture } from "../../server/reviewCompletionFixtures"

import { provideReviewSessionTransport } from "@/review-session/client"
import {
  FIXTURE_GAME_IMPORT_ID,
  fixtureCore,
  fixtureGameReview,
  loadReviewSessionFixtures,
} from "@/review-session/reviewSessionStreamFixtures"

import { CoachingBoardChosenGame } from "./CoachingBoardChosenGame"
import type { CoachingBoardSnapshot } from "./coachingBoardSnapshot"
import {
  clearModelContextPolyfill,
  installModelContextPolyfill,
} from "./modelContextPolyfill"

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
const AFTER_E4_FEN =
  "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"

afterEach(() => {
  cleanup()
  clearModelContextPolyfill()
  provideReviewSessionTransport(null)
})

beforeAll(async () => {
  await loadReviewSessionFixtures()
})

test("a Player move is evaluated and becomes the position the board shows", async () => {
  const user = userEvent.setup()
  const kinds: ReviewSessionCommand["kind"][] = []
  const tools = installModelContextPolyfill()
  stubExplorationTransport(kinds)

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  const destination = screen.getByRole("gridcell", {
    name: "e4 empty, legal destination",
  })
  await user.click(destination)

  await waitFor(() => {
    expect(kinds).toContain("exploreAlternativeMove")
  })
  const read = await tools.get("read_coaching_board")?.execute({})
  expect(read?.structuredContent).toMatchObject({
    currentPosition: { fen: AFTER_E4_FEN, sideToMove: "black" },
    exploration: {
      activeBranchId: "alternative-move:web:e2e4",
      branches: [{ active: true, moveUci: "e2e4" }],
      pathFromRoot: ["alternative-move:web:e2e4"],
    },
    kind: "coachingBoard",
  })
})

test("a move after the agent moved the board is rooted where the agent left it", async () => {
  const user = userEvent.setup()
  const explored: string[] = []
  const tools = installModelContextPolyfill()
  stubExplorationTransport([], explored)

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e4 empty, legal destination" }),
  )
  await waitFor(() => expect(explored).toEqual(["e2e4"]))

  // The agent takes the board off the explored branch, back to the ply. The
  // Player's next move is one move from there, not a replay of the line the
  // board has left.
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })
  await user.click(await screen.findByRole("gridcell", { name: /^d2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "d4 empty, legal destination" }),
  )

  await waitFor(() => expect(explored).toEqual(["e2e4", "d2d4"]))
})

test("a second move on the line costs one command, not a replay of the line", async () => {
  const user = userEvent.setup()
  const kinds: ReviewSessionCommand["kind"][] = []
  const commands: ReviewSessionCommand[] = []
  const tools = installModelContextPolyfill()
  stubExplorationTransport(kinds, [], commands)

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e4 empty, legal destination" }),
  )
  await waitFor(() => expect(kinds).toContain("exploreAlternativeMove"))
  const afterFirstMove = kinds.length

  // Black to move on the position the first move produced. The moment root is
  // already open, so this drag is one command — the old whole-line walk spent
  // four re-opening the root and one per ply already played.
  await user.click(await screen.findByRole("gridcell", { name: /^e7 black/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e5 empty, legal destination" }),
  )
  await waitFor(() => expect(kinds.length).toBeGreaterThan(afterFirstMove))

  expect(kinds.slice(afterFirstMove)).toEqual(["exploreAlternativeMove"])

  const second = commands.at(-1)
  if (second?.kind !== "exploreAlternativeMove") {
    throw new Error("the second drag sends one Alternative Move command")
  }
  // Parented to the branch the board is standing on, so no earlier ply is
  // re-sent and the engine roots the move where the Player actually is.
  expect(second.parent).toEqual({
    kind: "move",
    branchRef: "branch:web:e2e4",
  })

  // A third drag, deeper again. The cost per move is constant rather than
  // growing with the line: the old walk spent one command per ply already
  // played, so this move would have cost seven.
  const afterSecondMove = kinds.length
  await user.click(await screen.findByRole("gridcell", { name: /^d7 black/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "d5 empty, legal destination" }),
  )
  await waitFor(() => expect(kinds.length).toBeGreaterThan(afterSecondMove))

  expect(kinds.slice(afterSecondMove)).toEqual(["exploreAlternativeMove"])
  const third = commands.at(-1)
  if (third?.kind !== "exploreAlternativeMove") {
    throw new Error("the third drag sends one Alternative Move command")
  }
  expect(third.parent).toEqual({ kind: "move", branchRef: "branch:web:e7e5" })
})

test("browsing away while Stockfish works keeps the branch without dragging the board back", async () => {
  const user = userEvent.setup()
  const tools = installModelContextPolyfill()
  const release = deferredExploration()

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e4 empty, legal destination" }),
  )
  await waitFor(() => expect(release.pending()).toBe(true))

  // The Player reads on while the engine works. Navigation is no longer held
  // by an evaluation in flight.
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 2 })
  release.settle()

  await waitFor(async () => {
    const read = await tools.get("read_coaching_board")?.execute({})
    expect(read?.structuredContent).toMatchObject({
      exploration: { branches: [{ moveUci: "e2e4" }] },
      viewedPly: 2,
    })
  })
})

test("a move whose key is bound to an interrupted operation retries under a fresh identity", async () => {
  const user = userEvent.setup()
  const commands: ReviewSessionCommand[] = []
  const tools = installModelContextPolyfill()
  // The engine answers the first attempt with the conflict a cancelled move
  // leaves behind. Without the fresh-identity retry, replaying a move the
  // Player cancelled would be permanently impossible.
  stubExplorationTransport([], [], commands, { mismatchFirstExplore: true })

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e4 empty, legal destination" }),
  )

  await waitFor(async () => {
    const read = await tools.get("read_coaching_board")?.execute({})
    expect(read?.structuredContent).toMatchObject({
      currentPosition: { fen: AFTER_E4_FEN },
    })
  })

  const explores = commands.filter(
    (command) => command.kind === "exploreAlternativeMove",
  )
  expect(explores).toHaveLength(2)
  expect(explores[0]?.idempotencyKey).not.toBe(explores[1]?.idempotencyKey)
})

test("an explored position is named by the move that reached it", async () => {
  const user = userEvent.setup()
  const tools = installModelContextPolyfill()
  stubExplorationTransport([])

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e4 empty, legal destination" }),
  )

  // The board carries no caption of its own here — the coaching column
  // already names the Game's move — so the branch strip is what names a
  // position off the Game's line.
  const line = await screen.findByLabelText("Branch line")
  await waitFor(() => expect(line.textContent).toContain("e4"))
})

test("a branch offers the engine's reply and the line that reached it", async () => {
  const user = userEvent.setup()
  const explored: string[] = []
  const tools = installModelContextPolyfill()
  stubExplorationTransport([], explored)

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e4 empty, legal destination" }),
  )
  await waitFor(() => expect(explored).toEqual(["e2e4"]))

  const line = await screen.findByLabelText("Branch line")
  expect(
    within(line)
      .getByRole("button", { name: /^e4 / })
      .getAttribute("aria-current"),
  ).toBe("step")

  // The Game's own strip keeps naming the ply the branch left from.
  const gameList = screen.getByLabelText("Full game move list")
  expect(within(gameList).getByRole("button", { current: "step" })).toBeTruthy()

  await user.click(await screen.findByRole("button", { name: "Best move: e5" }))
  await waitFor(() => expect(explored).toContain("e7e5"))
})

test("leaving a branch takes its explored-alternatives list with it", async () => {
  const user = userEvent.setup()
  const tools = installModelContextPolyfill()
  stubExplorationTransport([])

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e4 empty, legal destination" }),
  )
  await screen.findByLabelText("Branch line")

  await user.click(screen.getByRole("button", { name: "Exit branch" }))
  expect(screen.queryByLabelText("Branch line")).toBeNull()
  // Back on the Game's own line the branch list goes with it, so walking the
  // Game never grows a stack of every line ever tried under the board.
  expect(screen.queryByLabelText("Explored alternatives")).toBeNull()

  // The branch itself is still held, and setting the board back onto it
  // returns the position it reached.
  await tools.get("set_board_position")?.execute({
    alternativeMoveId: "alternative-move:web:e2e4",
    kind: "alternativeMove",
  })

  const read = await tools.get("read_coaching_board")?.execute({})
  expect(read?.structuredContent).toMatchObject({
    currentPosition: { fen: AFTER_E4_FEN },
    exploration: { activeBranchId: "alternative-move:web:e2e4" },
  })
})

test("the Player's own affordances name the Player and the agent's do not", async () => {
  const user = userEvent.setup()
  const tools = installModelContextPolyfill()
  stubExplorationTransport([])

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))

  const moved = await tools
    .get("set_board_position")
    ?.execute({ kind: "ply", ply: 1 })
  expect(moved?.structuredContent).toMatchObject({
    playerChangedAtRevision: null,
    revisionChangedBy: "agent",
  })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e4 empty, legal destination" }),
  )
  await screen.findByLabelText("Branch line")
  await user.click(screen.getByRole("button", { name: "Exit branch" }))

  // The strip of other explored lines only shows inside a branch, so a second
  // branch is what puts the first one on it.
  await user.click(await screen.findByRole("gridcell", { name: /^d2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "d4 empty, legal destination" }),
  )
  await screen.findByLabelText("Branch line")

  // The branch strip: the affordance that used to drive the board through the
  // agent's host, so a Player's click reported the agent as having moved it.
  const explored = await screen.findByLabelText("Explored alternatives")
  await user.click(within(explored).getByRole("button", { name: /^e4 / }))

  const read = await tools.get("read_coaching_board")?.execute({})
  // SAFETY: the board is on a grounded origin here — the strip click above put
  // it on an explored branch — so the read returns a Coaching Board Snapshot.
  const snapshot = read?.structuredContent as CoachingBoardSnapshot
  expect(snapshot.exploration.activeBranchId).toBe("alternative-move:web:e2e4")
  expect(snapshot.revisionChangedBy).toBe("player")
  expect(snapshot.playerChangedAtRevision).toBe(snapshot.revision)
})

test("a dropped connection says so instead of swallowing the move", async () => {
  const user = userEvent.setup()
  const tools = installModelContextPolyfill()
  provideReviewSessionTransport({
    createCommandEnvelope: (command) => ({
      command,
      operationId: fromOperationId("operation:web:explore"),
      requestId: fromRequestId("request:web:explore"),
      surface: "web",
    }),
    streamReviewSessionCommand: async ({ envelope, onEvent }) => {
      const completion = rootCompletion(envelope.command)
      if (!completion) throw new Error("connection dropped")
      onEvent({
        event: { kind: "completed", result: completion },
        operationId: envelope.operationId,
        requestId: envelope.requestId,
        sequence: 0,
      })
    },
  })

  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  await user.click(await screen.findByRole("gridcell", { name: /^e2 white/ }))
  await user.click(
    screen.getByRole("gridcell", { name: "e4 empty, legal destination" }),
  )

  expect(
    await screen.findByText(
      "The engine could not be reached. Try that move again.",
    ),
  ).toBeTruthy()
})

test("the board browses without offering moves when the engine is out of reach", async () => {
  const user = userEvent.setup()
  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={fixtureGameReview()}
      />
    </ChenTheme>,
  )
  const squares = screen.getAllByRole("gridcell", { name: /^[a-h][1-8]/ })
  const occupied = squares.find((square) =>
    square.getAttribute("aria-label")?.includes("white"),
  )
  if (!occupied) throw new Error("the board renders its occupied squares")
  await user.click(occupied)
  expect(
    screen.queryByRole("gridcell", { name: /legal destination/ }),
  ).toBeNull()
})

function stubExplorationTransport(
  kinds: ReviewSessionCommand["kind"][],
  explored: string[] = [],
  commands: ReviewSessionCommand[] = [],
  { mismatchFirstExplore = false }: { mismatchFirstExplore?: boolean } = {},
) {
  let explores = 0
  provideReviewSessionTransport({
    createCommandEnvelope: (command) => {
      kinds.push(command.kind)
      commands.push(command)
      return {
        command,
        operationId: fromOperationId("operation:web:explore"),
        requestId: fromRequestId("request:web:explore"),
        surface: "web",
      }
    },
    streamReviewSessionCommand: async ({ envelope, onEvent }) => {
      const emit = (event: ReviewSessionEvent, sequence: number) =>
        onEvent({
          event,
          operationId: envelope.operationId,
          requestId: envelope.requestId,
          sequence,
        })
      const command = envelope.command
      if (command.kind === "exploreAlternativeMove") {
        explores += 1
        if (mismatchFirstExplore && explores === 1) {
          emit(
            {
              kind: "conflict",
              operation: "alternativeMoveEvaluation",
              reason: "idempotencyKeyMismatch",
            },
            0,
          )
          return
        }
        if (command.moveInput.kind === "uci") {
          explored.push(command.moveInput.uci)
        }
        // The allowance the engine reports beside every explored move; the
        // evaluator refuses to answer without having seen one.
        emit(
          {
            kind: "progress",
            stage: { kind: "alternativeMoveAllowance", remaining: 4 },
          },
          0,
        )
        emit(
          {
            kind: "completed",
            result: {
              alternativeMove: exploredAlternativeMove(command.moveInput),
              kind: "alternativeMoveEvaluated",
            },
          },
          1,
        )
        return
      }
      const completion = rootCompletion(command)
      if (completion) emit({ kind: "completed", result: completion }, 0)
    },
  })
}

/** A transport that holds the Alternative Move open until the test releases
 * it, so the board can be driven while an evaluation is genuinely in flight. */
function deferredExploration() {
  let arrived = false
  let release: () => void = () => {}
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  provideReviewSessionTransport({
    createCommandEnvelope: (command) => ({
      command,
      operationId: fromOperationId("operation:web:explore"),
      requestId: fromRequestId("request:web:explore"),
      surface: "web",
    }),
    streamReviewSessionCommand: async ({ envelope, onEvent }) => {
      const emit = (event: ReviewSessionEvent, sequence: number) =>
        onEvent({
          event,
          operationId: envelope.operationId,
          requestId: envelope.requestId,
          sequence,
        })
      const command = envelope.command
      if (command.kind === "exploreAlternativeMove") {
        arrived = true
        await gate
        emit(
          {
            kind: "progress",
            stage: { kind: "alternativeMoveAllowance", remaining: 4 },
          },
          0,
        )
        emit(
          {
            kind: "completed",
            result: {
              alternativeMove: exploredAlternativeMove(command.moveInput),
              kind: "alternativeMoveEvaluated",
            },
          },
          1,
        )
        return
      }
      const completion = rootCompletion(command)
      if (completion) emit({ kind: "completed", result: completion }, 0)
    },
  })
  return { pending: () => arrived, settle: () => release() }
}

function rootCompletion(
  command: ReviewSessionCommand,
): OperationCompletion | null {
  const started = completionFixture("reviewSessionStarted")
  const moment = started.reviewMoments[0]
  if (!moment) throw new Error("started fixture has a Review Moment")
  if (moment.authoring.kind !== "prepared") {
    throw new Error("started fixture Review Moment is prepared")
  }
  switch (command.kind) {
    case "openAddressedReviewMoment":
      return {
        // The Player explores from the Game's first ply, so that is the ply
        // the engine roots the line at.
        detail: {
          ...completionFixture("reviewMomentDetailRead").detail,
          ply: 1,
        },
        kind: "addressedReviewMomentOpened",
      }
    case "startReviewSession":
      return started
    case "openReviewMoment":
      // SAFETY: preparePlayerLineRoot only discriminates on completion.kind.
      return { kind: "reviewMomentOpened" } as OperationCompletion
    case "inspectPosition":
      return {
        inspection: {
          context: moment.authoring.core.coachTurnContext,
          evaluation: { kind: "centipawns", perspective: "white", value: 18 },
          evidencePacket: moment.authoring.core.evidencePacket,
          // The line the engine walks starts from the Game's opening
          // position, which is where the Player's move is played.
          positionSnapshot: {
            ...moment.positionSnapshot,
            fen: START_FEN,
            sideToMove: "white",
          },
          sideToMove: "white",
          textBoard: "Fixture opening board",
        },
        kind: "positionInspected",
      }
    default:
      return null
  }
}

function exploredAlternativeMove(moveInput: MoveInput): AlternativeMoveResult {
  const core = fixtureCore()
  const uci = moveInput.kind === "uci" ? moveInput.uci : "e2e4"
  const evaluation = {
    kind: "centipawns" as const,
    perspective: "white" as const,
    value: 24,
  }
  return {
    alternativeMoveId: fromAlternativeMoveId(`alternative-move:web:${uci}`),
    branchRef: fromBranchRef(`branch:web:${uci}`),
    evaluation: {
      bestMove: evaluation,
      bestMoveUci: "e2e4",
      comparison: { kind: "centipawns", value: 0 },
      selectedMove: evaluation,
    },
    moveUci: uci,
    parent: { kind: "root", positionRef: core.positionSnapshot.positionRef },
    resultingPosition: {
      ...core.positionSnapshot,
      fen: AFTER_E4_FEN,
      positionRef: fromPositionRef(
        "sha256:9999999999999999999999999999999999999999999999999999999999999999",
      ),
      sideToMove: "black",
    },
    sourcePositionRef: core.positionSnapshot.positionRef,
    strongestReply: { kind: "offered", uci: "e7e5" },
  }
}
