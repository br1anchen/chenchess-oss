// @vitest-environment jsdom

import {
  fromOperationId,
  fromRequestId,
  fromReviewContentDigest,
} from "@chenchess/coach-engine-sdk"
import { cleanup, render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeAll, expect, test } from "vitest"

import { ChenTheme } from "@chenchess/ui/theme"

import { provideReviewSessionTransport } from "@/review-session/client"
import {
  FIXTURE_GAME_IMPORT_ID,
  fixtureCore,
  fixtureGameReview,
  loadReviewSessionFixtures,
} from "@/review-session/reviewSessionStreamFixtures"

import { CoachingBoardGame } from "./CoachingBoardGame"
import { coachingBoardPage } from "./coachingBoardPage"
import {
  clearModelContextPolyfill,
  installModelContextPolyfill,
} from "./modelContextPolyfill"

afterEach(() => {
  cleanup()
  clearModelContextPolyfill()
  provideReviewSessionTransport(null)
})

beforeAll(async () => {
  await loadReviewSessionFixtures()
})

test("reads a Game Import without starting a Review Session", async () => {
  const kinds: string[] = []
  provideReviewSessionTransport({
    createCommandEnvelope: (command) => {
      kinds.push(command.kind)
      return {
        command,
        operationId: fromOperationId("operation:web:board"),
        requestId: fromRequestId("request:web:board"),
        surface: "web",
      }
    },
    streamReviewSessionCommand: async ({ envelope, onEvent }) => {
      if (envelope.command.kind !== "readGameReviewSnapshot") return
      const event = {
        event: {
          kind: "completed" as const,
          result: {
            contentDigest: fromReviewContentDigest(
              "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            ),
            gameImportId: envelope.command.gameImportId,
            importedGame: fixtureCore().importedGame,
            kind: "gameReviewSnapshotRead" as const,
            review: fixtureGameReview(),
            reviewMoments: [],
          },
        },
        operationId: envelope.operationId,
        requestId: envelope.requestId,
        sequence: 0,
      }
      onEvent(event)
    },
  })

  render(
    <ChenTheme>
      <CoachingBoardGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
      />
    </ChenTheme>,
  )

  await waitFor(() => {
    expect(kinds).toEqual(["readGameReviewSnapshot"])
  })
  expect(screen.getByLabelText("Full game move list")).toBeTruthy()
  expect(kinds).not.toContain("startReviewSession")
})

test("drive tools do not spend engine compute or write anything durable", async () => {
  const kinds: string[] = []
  const tools = installModelContextPolyfill()
  const review = fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (moment) {
    moment.objective.lines = {
      best: [{ san: "Nxe5", uci: moment.objective.bestMoveUci || "e2e4" }],
      refutation: [{ san: "e5", uci: "e7e5" }],
    }
  }
  provideReviewSessionTransport({
    createCommandEnvelope: (command) => {
      kinds.push(command.kind)
      return {
        command,
        operationId: fromOperationId("operation:web:board-drive"),
        requestId: fromRequestId("request:web:board-drive"),
        surface: "web",
      }
    },
    streamReviewSessionCommand: async ({ envelope, onEvent }) => {
      if (envelope.command.kind !== "readGameReviewSnapshot") return
      onEvent({
        event: {
          kind: "completed",
          result: {
            gameImportId: envelope.command.gameImportId,
            importedGame: fixtureCore().importedGame,
            contentDigest: fromReviewContentDigest(
              "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            ),
            kind: "gameReviewSnapshotRead",
            review,
            reviewMoments: [],
          },
        },
        operationId: envelope.operationId,
        requestId: envelope.requestId,
        sequence: 0,
      })
    },
  })

  render(
    <ChenTheme>
      <CoachingBoardGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
      />
    </ChenTheme>,
  )

  await waitFor(() => {
    expect(kinds).toEqual(["readGameReviewSnapshot"])
    expect(tools.has("set_board_position")).toBe(true)
  })

  const ply = fixtureCore().importedGame.game.moves.find(
    (move) => move.ply !== review.criticalMoments[0]?.ply,
  )?.ply
  if (ply === undefined) throw new Error("fixture Game has a second ply")
  const moved = await tools.get("set_board_position")?.execute({
    kind: "ply",
    ply,
  })
  const shown = await tools.get("show_line")?.execute({ kind: "engineBest" })
  expect(moved?.structuredContent).toMatchObject({
    kind: "coachingBoard",
    viewedPly: ply,
  })
  expect(shown?.structuredContent).toMatchObject({
    kind: "refused",
    reason: "noRenderOption",
  })
  const back = await tools.get("set_board_position")?.execute({
    kind: "ply",
    ply: review.criticalMoments[0]?.ply ?? ply,
  })
  const engine = await tools.get("show_line")?.execute({ kind: "engineBest" })
  expect(back?.structuredContent).toMatchObject({ kind: "coachingBoard" })
  expect(engine?.structuredContent).toMatchObject({
    kind: "coachingBoard",
    shownLine: { kind: "engineBest" },
  })
  expect(kinds).toEqual(["readGameReviewSnapshot"])
  expect(kinds).not.toContain("inspectPosition")
  expect(kinds).not.toContain("exploreAlternativeMove")
  expect(kinds).not.toContain("importGame")
  expect(kinds).not.toContain("startReviewSession")
})

function targetHost(navigate: (href: string) => void) {
  return {
    authorizedPlayerId: "player:board",
    importFailure: null,
    importing: false,
    onCommitImport: () => undefined,
    page: coachingBoardPage(navigate),
    playedOpenings: [],
  }
}

test("an Opening Line target navigates off the game board, and refuses without a host", async () => {
  const navigated: string[] = []
  const tools = installModelContextPolyfill()
  provideReviewSessionTransport({
    createCommandEnvelope: (command) => ({
      command,
      operationId: fromOperationId("operation:web:board-open"),
      requestId: fromRequestId("request:web:board-open"),
      surface: "web",
    }),
    streamReviewSessionCommand: async ({ envelope, onEvent }) => {
      if (envelope.command.kind !== "readGameReviewSnapshot") return
      onEvent({
        event: {
          kind: "completed",
          result: {
            gameImportId: envelope.command.gameImportId,
            importedGame: fixtureCore().importedGame,
            contentDigest: fromReviewContentDigest(
              "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            ),
            kind: "gameReviewSnapshotRead",
            review: fixtureGameReview(),
            reviewMoments: [],
          },
        },
        operationId: envelope.operationId,
        requestId: envelope.requestId,
        sequence: 0,
      })
    },
  })

  const view = render(
    <ChenTheme>
      <CoachingBoardGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        targetHost={targetHost((href) => navigated.push(href))}
      />
    </ChenTheme>,
  )
  await waitFor(() => {
    expect(tools.has("set_board_position")).toBe(true)
  })

  const opened = await tools.get("set_board_position")?.execute({
    kind: "openingLine",
    openingLineRef: "B90-sicilian-najdorf-1a2b",
  })
  expect(navigated).toEqual(["/app/board/openings/B90-sicilian-najdorf-1a2b"])
  expect(opened?.structuredContent).toMatchObject({
    kind: "lobby",
    openingLineRef: "B90-sicilian-najdorf-1a2b",
    outcome: "opened",
  })

  // Without a target host there is nowhere to navigate, so the board is left
  // where it is rather than claiming a move it did not make.
  view.rerender(
    <ChenTheme>
      <CoachingBoardGame
        authorizedPlayerId="player:board"
        fetchAccessToken={async () => "test-token"}
        gameImportId={FIXTURE_GAME_IMPORT_ID}
      />
    </ChenTheme>,
  )
  const refused = await tools.get("set_board_position")?.execute({
    kind: "openingLine",
    openingLineRef: "B90-sicilian-najdorf-1a2b",
  })
  expect(navigated).toHaveLength(1)
  expect(refused?.structuredContent).toMatchObject({
    kind: "refused",
    reason: "unreachablePosition",
  })
})
