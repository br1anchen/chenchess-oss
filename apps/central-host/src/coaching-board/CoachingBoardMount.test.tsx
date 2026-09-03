// @vitest-environment jsdom

import { useState } from "react"
import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, test, vi } from "vitest"

import { ChenTheme } from "@chenchess/ui/theme"

import { TestFirebaseAuthProvider } from "@/auth/FirebaseAuthProvider"

import { CoachingBoardMount } from "./CoachingBoardMount"
import {
  betaAuthorizedResponder,
  verifiedIdentity,
} from "./coachingBoardMountFixtures"
import {
  parseCoachingBoardRoute,
  type CoachingBoardRoute,
} from "./coachingBoardRoute"
import type { CoachingBoardSnapshot } from "./coachingBoardSnapshot"
import {
  clearModelContextPolyfill,
  installModelContextPolyfill,
} from "./modelContextPolyfill"
import { openingLineCatalog } from "./openingLineCatalog"
import { openingLineMoves, openingLineViewedPly } from "./openingMoves"
import { openingNextMoves } from "./openingNextMoves"
import { openingStudyWorld } from "./openingStudyWorld"

afterEach(() => {
  cleanup()
  clearModelContextPolyfill()
  vi.unstubAllGlobals()
})

type BoardRoute = Exclude<
  CoachingBoardRoute,
  { kind: "none" } | { kind: "invalid" }
>

type ModelContextTools = ReturnType<typeof installModelContextPolyfill>

/** The mount as the app renders it: one page, whose route the board moves. */
function BoardPage({ start }: { start: BoardRoute }) {
  const [route, setRoute] = useState(start)
  return (
    <CoachingBoardMount
      navigate={(href) => {
        const next = parseCoachingBoardRoute(href)
        if (next.kind === "none" || next.kind === "invalid") {
          throw new Error(`the board navigated off its own path: ${href}`)
        }
        setRoute(next)
      }}
      route={route}
    />
  )
}

function renderBoardPage(start: BoardRoute) {
  render(
    <ChenTheme>
      <TestFirebaseAuthProvider
        value={{
          fetchAccessToken: vi.fn().mockResolvedValue("firebase-token"),
          identity: verifiedIdentity(),
        }}
      >
        <BoardPage start={start} />
      </TestFirebaseAuthProvider>
    </ChenTheme>,
  )
}

async function readBoard(tools: ModelContextTools) {
  const read = await tools.get("read_coaching_board")?.execute({})
  // SAFETY: the board read answers with a Coaching Board Snapshot on both
  // grounded origins, and this page never leaves one.
  return read?.structuredContent as CoachingBoardSnapshot | undefined
}

/** The snapshot of the board the navigation asked for, once it is there. */
async function boardOn(tools: ModelContextTools, openingLineRef: string) {
  let arrived: CoachingBoardSnapshot | undefined
  await waitFor(async () => {
    arrived = await readBoard(tools)
    expect(arrived?.origin).toMatchObject({ openingLineRef })
  })
  return arrived
}

/**
 * A catalog line whose next-move list offers a move that leaves it — the
 * Player's own way to change origin without touching the target dialog.
 *
 * Neither end has an authored study world: a session standing at its line's
 * end drives the ply itself on arrival, which would advance the revision for
 * a reason this test is not about and let a board that restarted look as
 * though it had carried the count.
 */
function lineWithADeparture() {
  for (const row of openingLineCatalog) {
    if (openingStudyWorld(row.ref)) continue
    const moves = openingLineMoves(row.path)
    const departure = openingNextMoves(
      row.ref,
      moves,
      openingLineViewedPly(moves),
    ).find(
      (next) => !next.onCurrentLine && !openingStudyWorld(next.openingLineRef),
    )
    if (departure) return { departure, row }
  }
  throw new Error("no catalog line offers a next move that leaves it")
}

test("changing origin keeps the page revision climbing and names who navigated", async () => {
  vi.stubGlobal("fetch", betaAuthorizedResponder())
  const user = userEvent.setup()
  const tools = installModelContextPolyfill()
  const { departure, row } = lineWithADeparture()
  renderBoardPage({ kind: "opening", openingLineRef: row.ref })

  expect(await boardOn(tools, row.ref)).toMatchObject({
    playerChangedAtRevision: null,
    revision: 1,
    revisionChangedBy: null,
  })

  // Something the board itself did, so the next origin has a revision to
  // carry rather than only the one it was mounted with.
  await tools
    .get("set_board_position")
    ?.execute({ kind: "orientation", orientation: "black" })

  // The Player leaves for another line from the board's own next-move list.
  await user.click(await screen.findByRole("button", { name: departure.label }))
  expect(await boardOn(tools, departure.openingLineRef)).toMatchObject({
    playerChangedAtRevision: 3,
    revision: 3,
    revisionChangedBy: "player",
  })

  // The agent's own navigation climbs from there and claims nothing for the
  // Player: the revision it left behind is the one the agent must beat.
  await tools.get("set_board_position")?.execute({
    kind: "openingLine",
    openingLineRef: row.ref,
  })
  expect(await boardOn(tools, row.ref)).toMatchObject({
    playerChangedAtRevision: 3,
    revision: 4,
    revisionChangedBy: "agent",
  })
})
