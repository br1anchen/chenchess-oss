import * as stylex from "@stylexjs/stylex"
import { useEffect } from "react"

import type {
  GameImportId,
  GameReview,
  ReviewedGameSearchCard,
} from "@chenchess/coach-engine-sdk"
import { Icon } from "@chenchess/ui/astryx"
import {
  Heading,
  HStack,
  Text,
  VStack,
  WatercolorButtonLink,
  WatercolorCard,
  WatercolorChessboard,
  WatercolorNotice,
  WatercolorPlaque,
} from "@chenchess/ui"

import { reviewedGameProviderLabel } from "./dailyCoachingPresentation"
import { recentGamesStyles } from "./dashboardWorkspace.styles"
import { previewBoardFromReview } from "./recentGamePreview"
import type { FrozenReviewState } from "./ReviewedGameCard"
import { useFrozenReviews } from "./useFrozenReviews"

export function RecentGamesCarousel({
  games,
  linkToGame,
  loadPreview,
}: {
  games: readonly ReviewedGameSearchCard[]
  linkToGame: (gameImportId: GameImportId) => string
  loadPreview: (gameImportId: GameImportId) => Promise<GameReview>
}) {
  const { loadReview, reviews: previews } = useFrozenReviews(loadPreview)

  useEffect(() => {
    for (const game of games) void loadReview(game.gameImportId)
  }, [games, loadReview])

  return (
    <WatercolorCard padding="compact" xstyle={recentGamesStyles.card}>
      <VStack gap={3} hAlign="stretch">
        <Heading level={3}>
          <WatercolorPlaque size="sm">Recent games</WatercolorPlaque>
        </Heading>
        {games.length === 0 ? (
          <WatercolorNotice
            appearance="compact"
            detail="Games appear here once they have been reviewed."
            frame={false}
            glyph={<Icon icon="bookOpen" size="sm" />}
            heading="No games yet"
          />
        ) : (
          <ul
            aria-label="Recent games"
            {...stylex.props(recentGamesStyles.scroller)}
          >
            {games.map((game) => (
              <li
                key={game.reviewedGameKey}
                {...stylex.props(recentGamesStyles.item)}
              >
                <RecentGameTile
                  game={game}
                  href={linkToGame(game.gameImportId)}
                  review={previews.get(game.gameImportId)}
                />
              </li>
            ))}
          </ul>
        )}
      </VStack>
    </WatercolorCard>
  )
}

function RecentGameTile({
  game,
  href,
  review,
}: {
  game: ReviewedGameSearchCard
  href: string
  review?: FrozenReviewState
}) {
  const opponent = game.opponentName ?? "Unknown opponent"
  const provider = reviewedGameProviderLabel(game.provider)
  const name = `vs ${opponent}`
  const hoverLabel = `${name}, ${provider}`
  const board =
    review?.kind === "ready"
      ? previewBoardFromReview(review.review, game)
      : undefined
  return (
    <WatercolorButtonLink
      aria-label={hoverLabel}
      href={href}
      variant="quiet"
      xstyle={recentGamesStyles.tile}
    >
      <HStack
        as="span"
        hAlign="center"
        vAlign="center"
        xstyle={recentGamesStyles.board}
      >
        {board ? (
          <WatercolorChessboard
            board={board}
            density="preview"
            xstyle={recentGamesStyles.chessboard}
          />
        ) : (
          <Icon icon="bookOpen" size="lg" />
        )}
      </HStack>
      <Text type="supporting" xstyle={recentGamesStyles.caption}>
        {name}
      </Text>
    </WatercolorButtonLink>
  )
}
