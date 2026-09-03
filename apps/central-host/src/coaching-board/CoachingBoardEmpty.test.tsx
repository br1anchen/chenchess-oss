// @vitest-environment jsdom

import {
  fromGameImportId,
  type GameReview,
  type ReviewedGameSearchCard,
} from "@chenchess/coach-engine-sdk"

import {
  fixtureGameReview,
  loadReviewSessionFixtures,
} from "@/review-session/reviewSessionStreamFixtures"
import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, test, vi } from "vitest"

import { ChenTheme } from "@chenchess/ui/theme"

import {
  ANONYMOUS_GAME_STAGING_PER_HOUR,
  memoryAnonymousAttemptStore,
} from "./anonymousRateLimit"
import { CoachingBoardEmpty } from "./CoachingBoardEmpty"
import { coachingBoardPage } from "./coachingBoardPage"
import {
  latestGameControlVisible,
  latestPlayingProfileGameFromRead,
  type CoachingBoardTargetHost,
} from "./coachingBoardTargetSwitch"
import { openingLineCatalog } from "./openingLineCatalog"
import { openingLineLookupFromRows } from "./openingLineFind"
import {
  clearModelContextPolyfill,
  installModelContextPolyfill,
} from "./modelContextPolyfill"
import {
  applyStagedGameImport,
  emptyGameImportFields,
} from "./stagedGameImport"

afterEach(() => {
  cleanup()
  clearModelContextPolyfill()
})

function renderEmpty({
  anonymousAttemptStore,
  authorizedPlayerId = null,
  initialTargetPane,
  latestGame = null,
  playedAggregate,
  recentReviewedGames,
}: {
  anonymousAttemptStore?: ReturnType<typeof memoryAnonymousAttemptStore>
  authorizedPlayerId?: string | null
  initialTargetPane?: "chooser" | "import" | "find"
  latestGame?: CoachingBoardTargetHost["latestGame"]
  playedAggregate?: CoachingBoardTargetHost["playedAggregate"]
  recentReviewedGames?: CoachingBoardTargetHost["recentReviewedGames"]
} = {}) {
  const navigate = vi.fn()
  const onCommitImport = vi.fn()
  const targetHost: CoachingBoardTargetHost = {
    anonymousAttemptStore,
    authorizedPlayerId,
    findOpeningLines: openingLineLookupFromRows(openingLineCatalog),
    importFailure: null,
    importing: false,
    latestGame,
    onCommitImport,
    page: coachingBoardPage(navigate),
    playedAggregate,
    playedOpenings: [{ eco: "A00", name: "Saragossa Opening" }],
    recentReviewedGames,
  }
  render(
    <ChenTheme>
      <CoachingBoardEmpty
        initialTargetPane={initialTargetPane}
        targetHost={targetHost}
      />
    </ChenTheme>,
  )
  return { navigate, onCommitImport }
}

test("no-target /app/board is the board with import and opening in the column", () => {
  renderEmpty()
  expect(screen.getByLabelText("Coaching")).toBeTruthy()
  expect(screen.getByLabelText("Game and board")).toBeTruthy()
  expect(
    document.querySelector(".chen-watercolor-session-subtitle")?.textContent,
  ).toBe("Coaching")
  expect(screen.getAllByText("Coaching").length).toBeGreaterThan(0)
  expect(screen.queryByText("Coaching Board")).toBeNull()
  expect(screen.queryByRole("button", { name: "Game or opening" })).toBeNull()
  expect(screen.queryByRole("dialog")).toBeNull()
  expect(screen.queryByText("Lobby")).toBeNull()
  expect(screen.getByRole("button", { name: "Import a game" })).toBeTruthy()
  expect(screen.getByRole("button", { name: "Choose an opening" })).toBeTruthy()
  expect(screen.getByLabelText("Game URL or PGN")).toBeTruthy()
  expect(screen.getByLabelText("Review side")).toBeTruthy()
  expect(screen.getByLabelText("Elo")).toBeTruthy()
  expect(screen.queryByRole("button", { name: "Latest game" })).toBeNull()
  expect(screen.queryByRole("heading", { name: "Choose a game" })).toBeNull()
  expect(
    screen.queryByRole("heading", { name: "Choose an opening" }),
  ).toBeNull()
  expect(screen.queryByRole("img", { name: /evaluation graph/i })).toBeNull()
  expect(screen.queryByRole("article", { name: /conversation/i })).toBeNull()
})

test("the bare board has nothing to go back to", () => {
  renderEmpty()
  expect(
    screen.queryByRole("link", { name: "Back to Coaching Board" }),
  ).toBeNull()
})

test("stacked layout puts Coaching above the board, not in the header", () => {
  const view = window
  const original = view.matchMedia
  view.matchMedia = (query: string) => ({
    matches: query.includes("64rem"),
    media: query,
    onchange: null,
    addEventListener() {},
    addListener() {},
    dispatchEvent() {
      return false
    },
    removeEventListener() {},
    removeListener() {},
  })
  try {
    renderEmpty()
    expect(
      document.querySelector(".chen-watercolor-session-subtitle"),
    ).toBeNull()
    expect(screen.getAllByText("Coaching").length).toBeGreaterThan(0)
    expect(screen.queryByText("Coaching Board")).toBeNull()
    expect(screen.queryByRole("button", { name: "Game or opening" })).toBeNull()
    expect(screen.queryByRole("dialog")).toBeNull()
  } finally {
    view.matchMedia = original
  }
})

test("Import a game restores URL, Elo, and Review side on one line", async () => {
  const user = userEvent.setup()
  renderEmpty()
  await user.click(screen.getByRole("button", { name: "Import a game" }))
  expect(screen.getByLabelText("Game URL or PGN")).toBeTruthy()
  expect(screen.getByLabelText("Review side")).toBeTruthy()
  expect(screen.getByLabelText("Elo")).toBeTruthy()
  expect(screen.queryByRole("button", { name: "Latest game" })).toBeNull()
})

test("Latest game is hidden without a signed-in Player", () => {
  renderEmpty({
    initialTargetPane: "import",
    latestGame: { reviewSide: "white", source: "https://lichess.org/Synthet1" },
  })
  expect(screen.queryByRole("button", { name: "Latest game" })).toBeNull()
  expect(screen.getByLabelText("Game URL or PGN")).toBeTruthy()
})

test("Latest game is hidden when a signed-in Player has no latest game", () => {
  renderEmpty({
    authorizedPlayerId: "player:board",
    initialTargetPane: "import",
  })
  expect(screen.queryByRole("button", { name: "Latest game" })).toBeNull()
  expect(screen.getByLabelText("Game URL or PGN")).toBeTruthy()
  expect(screen.getByLabelText("Review side")).toBeTruthy()
  expect(screen.getByLabelText("Elo")).toBeTruthy()
})

test("Latest game sits full width above URL or PGN and stages the import path", async () => {
  const user = userEvent.setup()
  renderEmpty({
    authorizedPlayerId: "player:board",
    initialTargetPane: "import",
    latestGame: { reviewSide: "black", source: "https://lichess.org/Synthet1" },
  })
  const latest = screen.getByRole("button", { name: "Latest game" })
  const source = screen.getByLabelText("Game URL or PGN")
  expect(
    latest.compareDocumentPosition(source) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(screen.queryByText("Lobby")).toBeNull()
  expect(screen.queryByText("Coaching Board")).toBeNull()
  await user.click(latest)
  expect(source).toHaveProperty("value", "https://lichess.org/Synthet1")
  expect(screen.getByRole("button", { name: "Import" })).not.toHaveProperty(
    "disabled",
    true,
  )
})

test("recent reviewed Games replace the Latest game control and open on the board", async () => {
  renderEmpty({
    authorizedPlayerId: "player:board",
    initialTargetPane: "import",
    latestGame: { reviewSide: "black", source: "https://lichess.org/Synthet1" },
    recentReviewedGames: {
      games: [reviewedGameCard()],
      loadPreview: async () => previewReview(),
    },
  })
  expect(screen.queryByRole("button", { name: "Latest game" })).toBeNull()
  const tile = await screen.findByRole("link", {
    name: "vs synthetic-white, Lichess",
  })
  expect(tile.getAttribute("href")).toBe(
    "/app/board/games/game-import%3Afixture%3A1",
  )
  expect(screen.getByLabelText("Game URL or PGN")).toBeTruthy()
})

test("Latest game stays the fallback when no reviewed Game is recent", () => {
  renderEmpty({
    authorizedPlayerId: "player:board",
    initialTargetPane: "import",
    latestGame: { reviewSide: "black", source: "https://lichess.org/Synthet1" },
    recentReviewedGames: {
      games: [],
      loadPreview: async () => previewReview(),
    },
  })
  expect(screen.getByRole("button", { name: "Latest game" })).toBeTruthy()
  expect(screen.queryByText("Recent games")).toBeNull()
})

test("an exhausted anonymous visitor sees the refusal at initial render, not the form", () => {
  const now = Date.now()
  const store = memoryAnonymousAttemptStore({
    gameStaging: Array.from(
      { length: ANONYMOUS_GAME_STAGING_PER_HOUR },
      (_, index) => now - index,
    ),
  })
  renderEmpty({ anonymousAttemptStore: store })
  expect(screen.getByText("Try again later.")).toBeTruthy()
  expect(screen.queryByLabelText("Game URL or PGN")).toBeNull()
})

test("Latest game click is the Player's own edit and replaces typed fields", async () => {
  const user = userEvent.setup()
  renderEmpty({
    authorizedPlayerId: "player:board",
    initialTargetPane: "import",
    latestGame: { reviewSide: "black", source: "https://lichess.org/Synthet1" },
  })
  const source = screen.getByLabelText("Game URL or PGN")
  await user.type(source, "https://lichess.org/typed")
  await user.click(screen.getByRole("button", { name: "Latest game" }))
  expect(source).toHaveProperty("value", "https://lichess.org/Synthet1")
  expect(screen.getByLabelText("Review side")).toHaveProperty("value", "black")
})

test("anonymous staging rate-limits import-form openings, not Sign-in refuse", async () => {
  const user = userEvent.setup()
  const now = Date.now()
  const store = memoryAnonymousAttemptStore({
    gameStaging: Array.from(
      { length: ANONYMOUS_GAME_STAGING_PER_HOUR },
      (_, index) => now - index,
    ),
  })
  renderEmpty({ anonymousAttemptStore: store })
  await user.click(screen.getByRole("button", { name: "Choose an opening" }))
  await user.click(screen.getByRole("button", { name: "Import a game" }))
  expect(screen.getByText("Try again later.")).toBeTruthy()
  expect(screen.queryByLabelText("Game URL or PGN")).toBeNull()
})

test("the search control offers the top five played openings before the Player types", async () => {
  const user = userEvent.setup()
  const aggregate = Array.from({ length: 6 }, (_, index) => ({
    eco: "A00",
    lastPlayedAtUnixMilliseconds: 1_000 + index,
    name: `Opening ${index}`,
    openingLineRef: openingLineCatalog[0]?.ref,
    path: "1. e4 e5 2. Nf3 d6",
    playCount: 6 - index,
  }))
  const { navigate } = renderEmpty({
    authorizedPlayerId: "player:board",
    playedAggregate: aggregate,
  })
  await user.click(screen.getByRole("button", { name: "Choose an opening" }))
  expect(screen.getByText("Your openings")).toBeTruthy()
  expect(
    screen.getByRole("button", { name: /Opening 0 · 6 played/ }),
  ).toBeTruthy()
  expect(screen.getByRole("button", { name: /Opening 4/ })).toBeTruthy()
  expect(screen.queryByRole("button", { name: /Opening 5/ })).toBeNull()
  await user.click(screen.getByRole("button", { name: /Opening 0 · 6 played/ }))
  expect(navigate).toHaveBeenCalledWith(
    expect.stringMatching(/^\/app\/board\/openings\//),
  )
})

test("a Player with no imported games sees the placeholder only", async () => {
  const user = userEvent.setup()
  renderEmpty({ authorizedPlayerId: "player:board", playedAggregate: [] })
  await user.click(screen.getByRole("button", { name: "Choose an opening" }))
  expect(screen.queryByText("Your openings")).toBeNull()
  expect(screen.getByLabelText("Find an opening")).toBeTruthy()
})

test("Choose an opening finds catalog rows", async () => {
  const user = userEvent.setup()
  const { navigate } = renderEmpty({ authorizedPlayerId: "player:board" })
  await user.click(screen.getByRole("button", { name: "Choose an opening" }))
  await user.type(screen.getByLabelText("Find an opening"), "Najdorf")
  await user.click(screen.getByRole("button", { name: "Find" }))
  await waitFor(() => {
    expect(screen.getByRole("button", { name: /Najdorf/ })).toBeTruthy()
  })
  await user.click(screen.getByRole("button", { name: /Najdorf/ }))
  expect(navigate).toHaveBeenCalledWith(
    expect.stringMatching(/^\/app\/board\/openings\//),
  )
})

test("Latest game stays hidden when the profile read returns nothing", () => {
  expect(
    latestPlayingProfileGameFromRead({ outcome: "noPlayingProfile" }),
  ).toBeNull()
  expect(
    latestPlayingProfileGameFromRead({
      games: [],
      outcome: "found",
    }),
  ).toBeNull()
  expect(
    latestPlayingProfileGameFromRead({
      outcome: "unavailable",
      reason: "providerUnreachable",
      retry: { kind: "retryAllowed" },
    }),
  ).toBeNull()
  expect(
    latestPlayingProfileGameFromRead({
      games: [
        {
          provider: "lichess",
          reviewSide: "black",
          source: "https://lichess.org/abcdefgh",
        },
      ],
      outcome: "found",
    }),
  ).toEqual({ reviewSide: "black", source: "https://lichess.org/abcdefgh" })
})

test("Latest game visibility is signed-in plus a source", () => {
  expect(
    latestGameControlVisible({
      authorizedPlayerId: "player:board",
      latestGame: {
        reviewSide: "white",
        source: "https://lichess.org/Synthet1",
      },
    }),
  ).toBe(true)
  expect(
    latestGameControlVisible({
      authorizedPlayerId: null,
      latestGame: {
        reviewSide: "white",
        source: "https://lichess.org/Synthet1",
      },
    }),
  ).toBe(false)
  expect(
    latestGameControlVisible({
      authorizedPlayerId: "player:board",
      latestGame: null,
    }),
  ).toBe(false)
})

test("agent staging refuses a bad field per field, sharing the form validator", async () => {
  const tools = installModelContextPolyfill()
  renderEmpty({ authorizedPlayerId: "player:board" })
  const stage = tools.get("stage_game_import")
  const badElo = await stage?.execute({
    elo: "not-a-number",
    source: "https://lichess.org/Synthet1",
  })
  expect(badElo?.structuredContent).toMatchObject({
    kind: "lobby",
    outcome: "refused",
    refusals: { elo: "Elo must be a whole number between 100 and 3500." },
  })
  const badSource = await stage?.execute({ source: "not a game" })
  expect(badSource?.structuredContent).toMatchObject({
    outcome: "refused",
    refusals: {
      source:
        "Paste one completed game URL, or the game's full PGN including its result.",
    },
  })
  const staged = await stage?.execute({
    source: "https://lichess.org/Synthet1",
  })
  expect(staged?.structuredContent).toMatchObject({ outcome: "applied" })
  await waitFor(() =>
    expect(screen.getByLabelText("Game URL or PGN")).toHaveProperty(
      "value",
      "https://lichess.org/Synthet1",
    ),
  )
})

function reviewedGameCard(): ReviewedGameSearchCard {
  return {
    digested: true,
    digestDate: "2026-04-28",
    digestId: "daily-2026-04-28",
    endedAt: "2026-04-28T10:07:47Z",
    gameImportId: fromGameImportId("game-import:fixture:1"),
    imported: true,
    learningPathCount: 2,
    learningTrackKeys: [],
    opening: { eco: "A00", name: "Saragossa Opening" },
    opponentName: "synthetic-white",
    opponentRating: 1245,
    outcome: "win",
    provider: "lichess",
    reviewedGameKey: "reviewed-game:fixture:lichess-Synthet1",
    reviewSide: "black",
    timeControlClass: "rapid",
  }
}

async function previewReview(): Promise<GameReview> {
  await loadReviewSessionFixtures()
  return fixtureGameReview()
}

test("staging keeps Player-typed source", () => {
  const current = {
    ...emptyGameImportFields,
    source: "https://lichess.org/player",
  }
  expect(
    applyStagedGameImport(
      current,
      { ...emptyGameImportFields, source: "https://lichess.org/agent" },
      true,
    ).kind,
  ).toBe("kept")
})
