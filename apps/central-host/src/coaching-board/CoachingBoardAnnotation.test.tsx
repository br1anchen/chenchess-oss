// @vitest-environment jsdom

import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react"
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

afterEach(() => {
  cleanup()
  clearModelContextPolyfill()
})

beforeAll(async () => {
  await loadReviewSessionFixtures()
})

/** The board browses without a token, which is all annotation needs: the page
 * settles every mark against the position it is already rendering. */
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

async function openingRevision(tools: ReturnType<typeof openBoard>) {
  await waitFor(() => expect(tools.has("annotate_board")).toBe(true))
  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 1 })
  const read = await tools.get("read_coaching_board")?.execute({})
  // SAFETY: the board is on a grounded origin here — set_board_position just
  // put it on ply 1 — so the read returns a Coaching Board Snapshot.
  const snapshot = read?.structuredContent as CoachingBoardSnapshot
  return snapshot.revision
}

test("verified marks are drawn on the board and said in words beneath it", async () => {
  const tools = openBoard()
  const revision = await openingRevision(tools)

  const annotated = await tools.get("annotate_board")?.execute({
    marks: [
      { from: "b1", kind: "defends", label: "holds d2", to: "d2" },
      { kind: "square", label: "the break", square: "e4" },
    ],
    revision,
  })

  expect(annotated?.structuredContent).toMatchObject({
    kind: "coachingBoard",
    marks: [
      { from: "b1", kind: "arrow", label: "holds d2", to: "d2" },
      { kind: "square", label: "the break", square: "e4" },
    ],
  })
  const legend = await screen.findByLabelText("What the coach drew")
  expect(within(legend).getByText("holds d2")).toBeTruthy()
  expect(within(legend).getByText("the break")).toBeTruthy()
})

test("a relation the position does not support is refused, not drawn", async () => {
  const tools = openBoard()
  const revision = await openingRevision(tools)

  // The knight on b1 does not reach d4; nothing in the opening position does.
  const refused = await tools.get("annotate_board")?.execute({
    marks: [{ from: "b1", kind: "attacks", label: "hits d4", to: "d4" }],
    revision,
  })

  expect(refused?.structuredContent).toMatchObject({
    kind: "refused",
    reason: "relationNotOnBoard",
  })
  expect(screen.queryByLabelText("What the coach drew")).toBeNull()
})

test("a board that moved since the read refuses the marks it would mislabel", async () => {
  const tools = openBoard()
  const revision = await openingRevision(tools)

  const stale = await tools.get("annotate_board")?.execute({
    marks: [{ kind: "square", label: "the break", square: "e4" }],
    revision: revision - 1,
  })

  expect(stale?.structuredContent).toMatchObject({
    kind: "refused",
    reason: "staleRevision",
  })
})

test("moving the board clears what the coach drew on the position it left", async () => {
  const tools = openBoard()
  const revision = await openingRevision(tools)

  await tools.get("annotate_board")?.execute({
    marks: [{ kind: "square", label: "the break", square: "e4" }],
    revision,
  })
  await screen.findByLabelText("What the coach drew")

  await tools.get("set_board_position")?.execute({ kind: "ply", ply: 2 })

  const read = await tools.get("read_coaching_board")?.execute({})
  expect(read?.structuredContent).toMatchObject({ marks: [] })
  await waitFor(() =>
    expect(screen.queryByLabelText("What the coach drew")).toBeNull(),
  )
})

test("a call outside the mark vocabulary is refused before any geometry", async () => {
  const tools = openBoard()
  const revision = await openingRevision(tools)

  const refused = await tools.get("annotate_board")?.execute({
    marks: [{ kind: "pin", label: "pinned", square: "e4" }],
    revision,
  })

  expect(refused?.structuredContent).toMatchObject({
    kind: "refused",
    reason: "outsideMarkVocabulary",
  })
})
