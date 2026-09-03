import type { ReactNode } from "react"
import type {
  GameImportId,
  GameReview,
  PlayedOpeningAggregate,
  RecentPlayingProfileGamesOutcome,
  ReviewedGameSearchCard,
  ReviewedGameSearchRequest,
  ReviewSide,
} from "@chenchess/coach-engine-sdk"

import type { AnonymousAttemptStore } from "./anonymousRateLimit"
import type { CoachingBoardPage } from "./coachingBoardPage"
import type { OpeningLineLookup } from "./openingLineFind"
import type { PlayedOpening } from "./openingLineCatalog"
import type { GameImportFields } from "./stagedGameImport"

export type LatestPlayingProfileGame = {
  reviewSide: ReviewSide
  source: string
}

export type CoachingBoardTargetHost = {
  anonymousAttemptStore?: AnonymousAttemptStore
  authorizedPlayerId: string | null
  disclosure?: ReactNode
  findOpeningLines?: OpeningLineLookup
  importFailure: string | null
  importing: boolean
  latestGame?: LatestPlayingProfileGame | null
  listPlayedOpenings?: () => object | Promise<object>
  listRecentProfileGames?: () => object | Promise<object>
  playedAggregate?: readonly PlayedOpeningAggregate[]
  /** The page the board is mounted on, which is how the lobby leaves for
   * another target without restarting the page revision. */
  page: CoachingBoardPage
  /** The reviewed Games the lobby offers, with the previews their tiles
   * draw. Absent until a signed-in Player has one. */
  recentReviewedGames?: {
    games: readonly ReviewedGameSearchCard[]
    loadPreview: (gameImportId: GameImportId) => Promise<GameReview>
  }
  searchReviewedGames?: (
    request: ReviewedGameSearchRequest,
  ) => object | Promise<object>
  onCommitImport: (fields: GameImportFields) => void
  playedOpenings: readonly PlayedOpening[]
}

export function latestGameControlVisible({
  authorizedPlayerId,
  latestGame,
}: Pick<CoachingBoardTargetHost, "authorizedPlayerId" | "latestGame">) {
  return Boolean(authorizedPlayerId && latestGame?.source)
}

// Recent reviewed Games are the richer way into a Game the Player already
// reviewed, so they replace the single Latest game button whenever the
// Player has any. The button stays the fallback: it stages an unimported
// profile Game, which no reviewed card can offer.
export function recentGamesCarouselVisible({
  authorizedPlayerId,
  recentReviewedGames,
}: Pick<
  CoachingBoardTargetHost,
  "authorizedPlayerId" | "recentReviewedGames"
>) {
  return Boolean(
    authorizedPlayerId && (recentReviewedGames?.games.length ?? 0) > 0,
  )
}

export function latestPlayingProfileGameFromRead(
  outcome: RecentPlayingProfileGamesOutcome,
): LatestPlayingProfileGame | null {
  if (outcome.outcome !== "found") return null
  const game = outcome.games[0]
  if (!game) return null
  return { reviewSide: game.reviewSide, source: game.source }
}
