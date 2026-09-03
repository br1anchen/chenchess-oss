// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeAll, expect, test, vi } from "vitest"

import { ChenTheme } from "@chenchess/ui/theme"

import { forkLearningMaterial } from "@/review-session/learningMaterialTestFixtures"
import {
  FIXTURE_GAME_IMPORT_ID,
  fixtureCore,
  fixtureGameReview,
  loadReviewSessionFixtures,
} from "@/review-session/reviewSessionStreamFixtures"

import { CoachingBoardChosenGame } from "./CoachingBoardChosenGame"
import { CoachingBoardOpening } from "./CoachingBoardOpening"
import { coachingBoardPage } from "./coachingBoardPage"
import {
  clearModelContextPolyfill,
  installModelContextPolyfill,
} from "./modelContextPolyfill"
import { openingLineCatalog } from "./openingLineCatalog"
import { openingLineMoves } from "./openingMoves"
import { openingStudyWorld } from "./openingStudyWorld"

afterEach(() => {
  cleanup()
  clearModelContextPolyfill()
  vi.unstubAllGlobals()
})

beforeAll(async () => {
  await loadReviewSessionFixtures()
})

test("the chosen-game board strips Review Session leftovers and shows plans", () => {
  const review = fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (moment) {
    moment.learningMaterial = forkLearningMaterial(
      moment.criticalMomentId,
      moment.ply,
    )
  }
  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={review}
      />
    </ChenTheme>,
  )
  expect(
    screen.queryByText("Select a piece to explore a legal move"),
  ).toBeNull()
  expect(screen.queryByText("Frozen Game Review")).toBeNull()
  expect(screen.queryByText("Identified by Lichess export")).toBeNull()
  expect(screen.queryByText("White to move")).toBeNull()
  expect(screen.getByLabelText("Critical moments")).toBeTruthy()
  expect(screen.getByLabelText("Learning plans")).toBeTruthy()
  expect(screen.queryByRole("article", { name: /conversation/i })).toBeNull()
  expect(screen.getByRole("button", { name: "Game or opening" })).toBeTruthy()
  expect(
    document.querySelector(".chen-watercolor-session-subtitle")?.textContent,
  ).toBe("Coaching")
  expect(screen.queryByText("Coaching Board")).toBeNull()
  expect(screen.queryByText("Lobby")).toBeNull()
  const imported = fixtureCore().importedGame
  const white =
    imported.game.white.name.kind === "present"
      ? imported.game.white.name.value
      : "White"
  const black =
    imported.game.black.name.kind === "present"
      ? imported.game.black.name.value
      : "Black"
  expect(
    screen.getByRole("heading", { name: `${white} — ${black}` }),
  ).toBeTruthy()
})

test("the coach's commentary is read beside the position, with nothing to reply to", () => {
  const review = fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("fixture Game Review has a Critical Moment")
  moment.comment = { text: "The knight was hanging after this." }
  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={review}
      />
    </ChenTheme>,
  )
  expect(screen.getByLabelText("Coach commentary")).toBeTruthy()
  expect(screen.getByText("The knight was hanging after this.")).toBeTruthy()
  expect(screen.queryByRole("article", { name: /conversation/i })).toBeNull()
  expect(screen.queryByRole("textbox")).toBeNull()
  expect(screen.queryByRole("button", { name: /send/i })).toBeNull()
})

test("Game or opening swaps the column to the picker without a dialog", async () => {
  const user = userEvent.setup()
  const review = fixtureGameReview()
  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={review}
        targetHost={{
          authorizedPlayerId: "player:board",
          importFailure: null,
          importing: false,
          page: coachingBoardPage(() => undefined),
          onCommitImport: () => undefined,
          playedOpenings: [],
        }}
      />
    </ChenTheme>,
  )
  await user.click(screen.getByRole("button", { name: "Game or opening" }))
  expect(screen.getByRole("button", { name: "Import a game" })).toBeTruthy()
  expect(screen.getByRole("button", { name: "Choose an opening" })).toBeTruthy()
  expect(screen.queryByRole("dialog")).toBeNull()
  expect(screen.getByLabelText("Game and board")).toBeTruthy()
})

test("a second Game or opening click restores the session column and keeps typed state", async () => {
  const user = userEvent.setup()
  const review = fixtureGameReview()
  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={review}
        targetHost={{
          authorizedPlayerId: "player:board",
          importFailure: null,
          importing: false,
          page: coachingBoardPage(() => undefined),
          onCommitImport: () => undefined,
          playedOpenings: [],
        }}
      />
    </ChenTheme>,
  )
  const toggle = screen.getByRole("button", { name: "Game or opening" })
  await user.click(toggle)
  const source = screen.getByLabelText("Game URL or PGN")
  await user.type(source, "https://lichess.org/keepme")
  await user.click(toggle)
  expect(screen.getByLabelText("Critical moments")).toBeTruthy()
  expect(toggle.getAttribute("aria-expanded")).toBe("false")
  await user.click(toggle)
  expect(screen.getByLabelText("Game URL or PGN")).toHaveProperty(
    "value",
    "https://lichess.org/keepme",
  )
  expect(toggle.getAttribute("aria-expanded")).toBe("true")
})

test("a line with no authored world still shows its ideas as prose", () => {
  const row = openingLineCatalog.find((one) => !openingStudyWorld(one.ref))
  if (!row) throw new Error("not every catalog row has a study world yet")
  render(
    <ChenTheme>
      <CoachingBoardOpening
        authorizedPlayerId={null}
        openingLineRef={row.ref}
        page={coachingBoardPage(() => undefined)}
      />
    </ChenTheme>,
  )
  expect(screen.getByRole("heading", { name: "Ideas" })).toBeTruthy()
  expect(screen.getByText(row.ideas.plan)).toBeTruthy()
  expect(screen.queryByRole("heading", { name: "Build the world" })).toBeNull()
})

test("the opening board starts a study session, not a graph or moments", () => {
  const row = openingLineCatalog[0]
  if (!row) throw new Error("v1 catalog has at least one Opening Line")
  const world = openingStudyWorld(row.ref)
  if (!world) throw new Error("this row is one the study world is authored for")
  render(
    <ChenTheme>
      <CoachingBoardOpening
        authorizedPlayerId={null}
        openingLineRef={row.ref}
        page={coachingBoardPage(() => undefined)}
      />
    </ChenTheme>,
  )
  expect(screen.getByRole("heading", { name: "Next moves" })).toBeTruthy()
  // The session asks before it tells: the plan prose is the closing summary,
  // not the first thing on the page.
  expect(screen.getByRole("heading", { name: "Build the world" })).toBeTruthy()
  expect(screen.queryByRole("heading", { name: "Ideas" })).toBeNull()
  expect(screen.queryByText(row.ideas.plan)).toBeNull()
  const firstSlot = world.slots[0]
  if (!firstSlot) throw new Error("a world opens by placing a piece")
  for (const option of firstSlot.options) {
    expect(screen.getByRole("button", { name: option })).toBeTruthy()
  }
  expect(screen.queryByLabelText("Critical moments")).toBeNull()
  expect(screen.queryByLabelText("Learning plans")).toBeNull()
  expect(screen.queryByRole("img", { name: /evaluation graph/i })).toBeNull()
  expect(screen.queryByLabelText(/White evaluation share/)).toBeNull()
  expect(
    screen.queryByText("Select a piece to explore a legal move"),
  ).toBeNull()
  expect(screen.queryByText("Frozen Game Review")).toBeNull()
  expect(screen.queryByRole("article", { name: /conversation/i })).toBeNull()
  expect(screen.queryByRole("button", { name: "1. d4" })).toBeNull()
  // The first slot asks where the king's knight belongs, so the board rewinds
  // to the ply before it arrives — a piece sitting on the answer is no question.
  const knightSlot = world.slots[0]
  if (!knightSlot) throw new Error("a world opens by placing a piece")
  expect(screen.getByText(`${knightSlot.playedAtPly - 1} / 10`)).toBeTruthy()
  expect(screen.queryByText("10 / 10")).toBeNull()
  expect(screen.getByRole("button", { name: "Game or opening" })).toBeTruthy()
  expect(
    document.querySelector(".chen-watercolor-session-subtitle")?.textContent,
  ).toBe("Coaching")
  expect(screen.queryByText("Coaching Board")).toBeNull()
  expect(screen.queryByText("Lobby")).toBeNull()
  expect(
    screen.getByRole("heading", { name: `${row.eco} · ${row.name}` }),
  ).toBeTruthy()
  expect(screen.queryByLabelText("Full game move list")).toBeNull()
  expect(
    screen.queryByRole("button", { name: `${row.eco} · ${row.name}` }),
  ).toBeNull()
})

test("Najdorf next moves after 3.d4 are 3…cxd4, not catalog-start 1.e4 / 1.d4", () => {
  const row = openingLineCatalog[0]
  if (!row) throw new Error("v1 catalog has at least one Opening Line")
  const moves = openingLineMoves(row.path)
  const afterD4 = moves.find(
    (move) =>
      move.moveNumber === 3 && move.side === "white" && move.san === "d4",
  )
  if (!afterD4) throw new Error("Najdorf path includes 3. d4")
  render(
    <ChenTheme>
      <CoachingBoardOpening
        authorizedPlayerId={null}
        initialViewedPly={afterD4.ply + 1}
        openingLineRef={row.ref}
        page={coachingBoardPage(() => undefined)}
      />
    </ChenTheme>,
  )
  expect(
    screen.getAllByRole("button", { name: "3… cxd4" }).length,
  ).toBeGreaterThan(0)
  expect(screen.queryByRole("button", { name: "1. d4" })).toBeNull()
})

test("a line outside the study catalog grounds through the engine resolve read", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(
      async () =>
        new Response(
          JSON.stringify({
            outcome: "resolved",
            line: {
              eco: "C41",
              name: "Philidor Defense: Exchange Variation",
              path: "1. e4 e5 2. Nf3 d6 3. d4 exd4",
              openingLineRef: "C41-philidor-defense-exchange-variation-abcd",
            },
          }),
          { headers: { "Content-Type": "application/json" }, status: 200 },
        ),
    ),
  )
  render(
    <ChenTheme>
      <CoachingBoardOpening
        authorizedPlayerId={null}
        // SAFETY: a well-formed Opening Line address fixture; the branded
        // parse happens inside the component under test.
        openingLineRef={"C41-philidor-defense-exchange-variation-abcd" as never}
        page={coachingBoardPage(() => undefined)}
      />
    </ChenTheme>,
  )
  await waitFor(() => {
    expect(
      screen.getByRole("heading", {
        name: "C41 · Philidor Defense: Exchange Variation",
      }),
    ).toBeTruthy()
  })
  // No curated study for an engine row — the move list grounds the plies.
  expect(screen.queryByRole("heading", { name: "Ideas" })).toBeNull()
  expect(screen.getAllByText(/exd4/).length).toBeGreaterThan(0)
})

test("an address outside the pinned catalog says so instead of a bare board", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockImplementation(
      async () =>
        new Response(JSON.stringify({ outcome: "unknownOpeningLine" }), {
          headers: { "Content-Type": "application/json" },
          status: 200,
        }),
    ),
  )
  render(
    <ChenTheme>
      <CoachingBoardOpening
        authorizedPlayerId={null}
        // SAFETY: deliberately unresolvable address fixture for the
        // unknown-line branch.
        openingLineRef={"Z99-not-a-line-ffff" as never}
        page={coachingBoardPage(() => undefined)}
      />
    </ChenTheme>,
  )
  await waitFor(() => {
    expect(
      screen.getByText(/names no Opening Line in the pinned catalog/),
    ).toBeTruthy()
  })
})

test("a failed resolve read is unavailable, never a claim the line does not exist", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockRejectedValue(new Error("network down")),
  )
  render(
    <ChenTheme>
      <CoachingBoardOpening
        authorizedPlayerId={null}
        // SAFETY: well-formed address fixture; the resolve read fails.
        openingLineRef={"C41-philidor-defense-exchange-variation-abcd" as never}
        page={coachingBoardPage(() => undefined)}
      />
    </ChenTheme>,
  )
  await waitFor(() => {
    expect(screen.getByText(/could not be read right now/)).toBeTruthy()
  })
  expect(screen.queryByText(/names no Opening Line/)).toBeNull()
})

test("the host agent can show a grounded line and set a Game ply", async () => {
  const tools = installModelContextPolyfill()
  const review = fixtureGameReview()
  const moment = review.criticalMoments[0]
  if (!moment) throw new Error("fixture Game Review has a Critical Moment")
  moment.objective.lines = {
    best: [{ san: "Nxe5", uci: moment.objective.bestMoveUci || "e2e4" }],
    refutation: [{ san: "e5", uci: "e7e5" }],
  }
  const ply = fixtureCore().importedGame.game.moves.find(
    (move) => move.ply !== moment.ply,
  )?.ply
  if (ply === undefined) throw new Error("fixture Game has a second ply")
  render(
    <ChenTheme>
      <CoachingBoardChosenGame
        authorizedPlayerId="player:board"
        gameImportId={FIXTURE_GAME_IMPORT_ID}
        importedGame={fixtureCore().importedGame}
        review={review}
      />
    </ChenTheme>,
  )
  const shown = await tools.get("show_line")?.execute({ kind: "engineBest" })
  expect(shown?.structuredContent).toMatchObject({
    kind: "coachingBoard",
    revision: 2,
    shownLine: { kind: "engineBest" },
  })
  const moved = await tools.get("set_board_position")?.execute({
    kind: "ply",
    ply,
  })
  expect(moved?.structuredContent).toMatchObject({
    kind: "coachingBoard",
    revision: 3,
    shownLine: null,
    viewedPly: ply,
  })
  const refused = await tools.get("set_board_position")?.execute({
    kind: "ply",
    ply: 9999,
  })
  expect(refused?.structuredContent).toMatchObject({
    kind: "refused",
    reason: "unreachablePosition",
    snapshot: { revision: 3, viewedPly: ply },
  })
})

test("the host agent turns the board, and the board the Player sees turns", async () => {
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
  const turned = await tools.get("set_board_position")?.execute({
    kind: "orientation",
    orientation: "black",
  })
  expect(turned?.structuredContent).toMatchObject({
    kind: "coachingBoard",
    orientation: "black",
  })
  await waitFor(() => {
    expect(firstDrawnSquare()).toBe("h1")
  })

  await tools.get("set_board_position")?.execute({
    kind: "orientation",
    orientation: "white",
  })
  await waitFor(() => {
    expect(firstDrawnSquare()).toBe("a8")
  })
})

/** The top-left square of the drawn board: a8 from White's side, h1 from
 * Black's. Read off the rendered grid, so this proves the turn reached the
 * board the Player sees rather than only the snapshot. */
function firstDrawnSquare() {
  return document.querySelector("[data-square]")?.getAttribute("data-square")
}

test("the host agent reads the study session the Player is running, and the plan card hands off to the coach", async () => {
  const user = userEvent.setup()
  const tools = installModelContextPolyfill()
  const row = openingLineCatalog[0]
  if (!row) throw new Error("v1 catalog has at least one Opening Line")
  const world = openingStudyWorld(row.ref)
  if (!world) throw new Error("this row is one the study world is authored for")
  const firstSlot = world.slots[0]
  const wrong = firstSlot?.options.find(
    (option) => !firstSlot.accepts.includes(option),
  )
  if (!firstSlot || !wrong) throw new Error("a slot offers a square it refuses")
  render(
    <ChenTheme>
      <CoachingBoardOpening
        authorizedPlayerId="player:board"
        openingLineRef={row.ref}
        page={coachingBoardPage(() => undefined)}
      />
    </ChenTheme>,
  )
  await waitFor(() => expect(tools.has("read_coaching_board")).toBe(true))

  // Before the Player answers, the coach can read the card they are on.
  const opened = await tools.get("read_coaching_board")?.execute({})
  expect(opened?.structuredContent).toMatchObject({
    study: {
      answered: [],
      card: { kind: "slot", position: 1, title: "Build the world" },
    },
  })

  // A wrong slot answer: the page grades it, the coach reads the verdict and
  // that the Player, not the coach, moved the board.
  await user.click(screen.getByRole("button", { name: wrong }))
  expect(screen.getByText("Not that")).toBeTruthy()
  const graded = await tools.get("read_coaching_board")?.execute({})
  expect(graded?.structuredContent).toMatchObject({
    revisionChangedBy: "player",
    study: {
      answered: [{ answer: wrong, verdict: { kind: "incorrect" } }],
      card: { position: 2 },
    },
  })

  // Through the rest of the slots to the plan card.
  for (const slot of world.slots.slice(1)) {
    await user.click(
      screen.getByRole("button", { name: slot.accepts[0] ?? "" }),
    )
  }
  expect(screen.getByRole("heading", { name: "Say the plan" })).toBeTruthy()
  const plan = "Get the knight to f6 and fight for e4 before castling."
  await user.type(screen.getByRole("textbox"), plan)
  await user.click(screen.getByRole("button", { name: "Answer" }))

  // The plan is on the board for the coach, ungraded, with its rubric — and
  // the Player has a press that copies the referent to ask for the marking.
  const marked = await tools.get("read_coaching_board")?.execute({})
  expect(marked?.structuredContent).toMatchObject({
    study: {
      answered: expect.arrayContaining([
        expect.objectContaining({
          answer: plan,
          card: expect.objectContaining({ kind: "plan" }),
          verdict: { kind: "ungraded", rubric: world.rubric },
        }),
      ]),
    },
  })
  expect(screen.getByText("For your coach to mark")).toBeTruthy()
  await user.click(
    screen.getByRole("button", { name: "Ask the coach to mark my plan" }),
  )
  await waitFor(() => {
    expect(
      screen.getByText("Copied. Paste it into the chat, then ask."),
    ).toBeTruthy()
  })
  expect(await navigator.clipboard.readText()).toBe(
    "About the plan I wrote in the opening study:",
  )
})
