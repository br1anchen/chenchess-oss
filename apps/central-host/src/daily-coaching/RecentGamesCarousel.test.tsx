// @vitest-environment jsdom

import {
  fromGameImportId,
  type GameReview,
  type ReviewedGameSearchCard,
} from "@chenchess/coach-engine-sdk"
import { cleanup, render, screen, waitFor } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import { ChenTheme } from "@chenchess/ui/theme"

import { gameReviewPath } from "@/game-review/gameReviewRoute"

import { RecentGamesCarousel } from "./RecentGamesCarousel"

afterEach(cleanup)

test("explains an empty recent-games rail", () => {
  renderCarousel([])

  expect(
    screen.getByText("Games appear here once they have been reviewed."),
  ).toBeTruthy()
  expect(screen.queryByRole("link")).toBeNull()
})

test("asks for the first-moment board of each recent Game", () => {
  const loadPreview = vi.fn(async () => previewReview())
  const game = canonicalGameCard()
  renderCarousel([game], loadPreview)

  expect(loadPreview).toHaveBeenCalledWith(game.gameImportId)
})

test("shows the first Critical Moment position on the tile", async () => {
  const game = canonicalGameCard()
  renderCarousel([game], async () => previewReview())

  const tile = await screen.findByRole("link", {
    name: "vs synthetic-white, Lichess",
  })
  expect(
    screen.getByRole("img", { name: "Chessboard. ...Ba6 at move 11" }),
  ).toBeTruthy()
  expect(screen.getByText("vs synthetic-white")).toBeTruthy()
  expect(tile.getAttribute("data-watercolor-control")).toBe("button")
  expect(tile.querySelector(".chen-watercolor-hover-wash")).toBeTruthy()
})

test("keeps the book icon and Game name while the preview is loading", () => {
  const game = canonicalGameCard()
  renderCarousel([game], () => new Promise(() => undefined))

  expect(
    screen.getByRole("link", { name: "vs synthetic-white, Lichess" }),
  ).toBeTruthy()
  expect(screen.getByText("vs synthetic-white")).toBeTruthy()
  expect(screen.queryByRole("img", { name: /Chessboard/ })).toBeNull()
})

test("sends each tile to the address the caller chose", async () => {
  const game = canonicalGameCard()
  render(
    <ChenTheme>
      <RecentGamesCarousel
        games={[game]}
        linkToGame={(gameImportId) => `/app/board/games/${gameImportId}`}
        loadPreview={async () => previewReview()}
      />
    </ChenTheme>,
  )

  const tile = await screen.findByRole("link", {
    name: "vs synthetic-white, Lichess",
  })
  expect(tile.getAttribute("href")).toBe(
    `/app/board/games/${game.gameImportId}`,
  )
})

test("keeps the book icon and Game name when the preview fails", async () => {
  const game = canonicalGameCard()
  renderCarousel([game], async () => {
    throw new Error("The frozen Game Review could not be opened.")
  })

  await waitFor(() => {
    expect(screen.getByText("vs synthetic-white")).toBeTruthy()
  })
  expect(screen.queryByRole("img", { name: /Chessboard/ })).toBeNull()
})

function renderCarousel(
  games: readonly ReviewedGameSearchCard[],
  loadPreview: (
    gameImportId: ReviewedGameSearchCard["gameImportId"],
  ) => Promise<GameReview> = async () => previewReview(),
) {
  render(
    <ChenTheme>
      <RecentGamesCarousel
        games={games}
        linkToGame={gameReviewPath}
        loadPreview={loadPreview}
      />
    </ChenTheme>,
  )
}

function canonicalGameCard(): ReviewedGameSearchCard {
  return {
    digested: true,
    digestDate: "2026-04-28",
    digestId: "daily-2026-04-28",
    endedAt: "2026-04-28T10:07:47Z",
    gameImportId: fromGameImportId("game-import:fixture:1"),
    imported: true,
    learningPathCount: 2,
    learningTrackKeys: [
      { concept: "fork", kind: "curriculum" },
      { concept: "backRankMate", kind: "curriculum" },
    ],
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

function previewReview(): GameReview {
  // SAFETY: RecentGameTile only reads criticalMoments and positionViews.fen.
  return {
    criticalMoments: [
      {
        criticalMomentId: "critical-moment:ba6",
        moveNumber: 11,
        playedSan: "Ba6",
        side: "black",
      },
    ],
    positionViews: [
      {
        criticalMomentId: "critical-moment:ba6",
        positionSnapshot: {
          fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        },
      },
    ],
  } as GameReview
}
