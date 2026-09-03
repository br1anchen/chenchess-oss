// @vitest-environment jsdom

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

/**
 * The Review Moment at ply 1 offers `1.c3 Nf6 2.d4` as its best line.
 *
 * These are the FENs the page derives, which name an en-passant square only
 * when a capture could actually be made — so they read `-` where an engine
 * FEN for the same position says `e3`.
 */
const AFTER_C3 = "rnbqkbnr/pppppppp/8/8/8/2P5/PP1PPPPP/RNBQKBNR b KQkq - 0 1"
const AFTER_C3_NF6 =
  "rnbqkb1r/pppppppp/5n2/8/8/2P5/PP1PPPPP/RNBQKBNR w KQkq - 1 2"

afterEach(() => {
  cleanup()
  clearModelContextPolyfill()
})

beforeAll(async () => {
  await loadReviewSessionFixtures()
})

function openBoard() {
  const tools = installModelContextPolyfill()
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
  return tools
}

async function showEngineBest(tools: ReturnType<typeof openBoard>) {
  await waitFor(() => expect(tools.has("step_line")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })
  const shown = await tools.get("show_line")?.execute({ kind: "engineBest" })
  if (!shown) throw new Error("a board surface registers show_line")
  return shown
}

test("showing a line offers its steps without walking any of them", async () => {
  const tools = openBoard()
  const shown = await showEngineBest(tools)

  expect(shown.structuredContent).toMatchObject({
    linePlayback: {
      index: 0,
      source: "engineBest",
      steps: [{ san: "c3" }, { san: "Nf6" }, { san: "d4" }],
    },
  })
})

test("stepping walks the line the engine authored, position by position", async () => {
  const tools = openBoard()
  await showEngineBest(tools)

  const first = await tools.get("step_line")?.execute({ to: "next" })
  expect(first?.structuredContent).toMatchObject({
    currentPosition: { fen: AFTER_C3, sideToMove: "black" },
    linePlayback: { index: 1 },
  })

  const second = await tools.get("step_line")?.execute({ to: "next" })
  expect(second?.structuredContent).toMatchObject({
    currentPosition: { fen: AFTER_C3_NF6, sideToMove: "white" },
    linePlayback: { index: 2 },
  })

  const back = await tools.get("step_line")?.execute({ to: "start" })
  expect(back?.structuredContent).toMatchObject({ linePlayback: { index: 0 } })
})

test("the named directions stop at the ends, an index outside the line does not", async () => {
  const tools = openBoard()
  await showEngineBest(tools)

  await tools.get("step_line")?.execute({ to: "end" })
  const past = await tools.get("step_line")?.execute({ to: "next" })
  expect(past?.structuredContent).toMatchObject({ linePlayback: { index: 3 } })

  const outside = await tools.get("step_line")?.execute({ to: 9 })
  expect(outside?.structuredContent).toMatchObject({
    kind: "refused",
    reason: "unreachablePosition",
  })
})

test("a shown refutation stands on its own root before a single step", async () => {
  const tools = openBoard()
  await waitFor(() => expect(tools.has("step_line")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  // Index 0 is the line's root, and the refutation's root is the position
  // after the played move — not the ply the moment sits on.
  const shown = await tools
    .get("show_line")
    ?.execute({ kind: "playedMoveRefutation" })
  expect(shown?.structuredContent).toMatchObject({
    currentPosition: { fen: AFTER_C3 },
    linePlayback: { index: 0, source: "playedMoveRefutation" },
  })
})

test("the refutation is walked from after the played move, not before it", async () => {
  const tools = openBoard()
  await waitFor(() => expect(tools.has("step_line")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })
  await tools.get("show_line")?.execute({ kind: "playedMoveRefutation" })

  // The refutation answers 1.c3, so its first move is 1...Nf6 — which is not
  // even legal from the position the best line starts in.
  const walked = await tools.get("step_line")?.execute({ to: "next" })
  expect(walked?.structuredContent).toMatchObject({
    currentPosition: { fen: AFTER_C3_NF6 },
    linePlayback: {
      index: 1,
      source: "playedMoveRefutation",
      steps: [{ san: "Nf6" }, { san: "d4" }, { san: "d6" }],
    },
  })
})

test("there is nothing to walk until a line is shown", async () => {
  const tools = openBoard()
  await waitFor(() => expect(tools.has("step_line")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })

  const read = await tools.get("read_coaching_board")?.execute({})
  expect(read?.structuredContent).toMatchObject({ linePlayback: null })

  const refused = await tools.get("step_line")?.execute({ to: "next" })
  expect(refused?.structuredContent).toMatchObject({
    kind: "refused",
    reason: "noLineShown",
  })
})

test("the Player walks the same line without asking the coach", async () => {
  const user = userEvent.setup()
  const tools = openBoard()
  await showEngineBest(tools)

  const transport = await screen.findByLabelText("Line playback")
  expect(within(transport).getByText("Line 0 of 3 · c3")).toBeTruthy()

  await user.click(within(transport).getByRole("button", { name: "Next move" }))

  await waitFor(() =>
    expect(
      within(screen.getByLabelText("Line playback")).getByText(
        "Line 1 of 3 · Nf6",
      ),
    ).toBeTruthy(),
  )
  const read = await tools.get("read_coaching_board")?.execute({})
  expect(read?.structuredContent).toMatchObject({
    currentPosition: { fen: AFTER_C3 },
  })
})

test("walking off the line clears what the coach drew on it", async () => {
  const tools = openBoard()
  const shown = await showEngineBest(tools)
  // SAFETY: show_line was accepted on a grounded origin, so its result is a
  // Coaching Board Snapshot.
  const { revision } = shown.structuredContent as CoachingBoardSnapshot

  await tools.get("annotate_board")?.execute({
    marks: [{ kind: "square", label: "the break", square: "c3" }],
    revision,
  })
  await screen.findByLabelText("What the coach drew")

  await tools.get("step_line")?.execute({ to: "next" })

  const read = await tools.get("read_coaching_board")?.execute({})
  expect(read?.structuredContent).toMatchObject({ marks: [] })
})
