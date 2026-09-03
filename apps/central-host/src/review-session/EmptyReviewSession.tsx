import type { ImportedGame, GameReview } from "@chenchess/coach-engine-sdk"
import { Icon } from "@chenchess/ui/astryx"
import {
  HStack,
  List,
  ListItem,
  Text,
  VStack,
  WatercolorButton,
  WatercolorCard,
  WatercolorStudio,
} from "@chenchess/ui"

import { AppHeader } from "@/AppHeader"

import { emptyReviewStyles } from "./emptyReviewSession.styles"

import { formatEvaluation, moveLabel } from "./model"

type EmptyReviewSessionProps = {
  snapshot: ImportedGame
  review: GameReview
  disabled: boolean
  onAccountSettings?: () => void
  onOpen: (ply: number) => void
  signOut?: () => Promise<void>
  waiting?: boolean
}

export function EmptyReviewSession({
  snapshot,
  review,
  disabled,
  onAccountSettings,
  onOpen,
  signOut,
  waiting = false,
}: EmptyReviewSessionProps) {
  return (
    <WatercolorStudio as="main" xstyle={emptyReviewStyles.page}>
      <VStack gap={4} hAlign="stretch">
        <AppHeader
          heading="Game review"
          onAccountSettings={onAccountSettings}
          signOut={signOut}
        />
        <WatercolorCard
          aria-label="Game summary"
          eyebrow="Game summary"
          headingLevel={2}
          title="No key moments found"
        >
          <VStack gap={3} hAlign="start">
            <Text as="p" display="block" type="body">
              {review.summary}
            </Text>
            <Text as="p" display="block" type="supporting">
              Pick a move below to review it.
            </Text>
            {waiting ? (
              <HStack
                aria-live="polite"
                data-comment-wait="bounded"
                gap={2}
                role="status"
                vAlign="center"
              >
                <Icon icon="loader" size="sm" />
                <Text type="supporting">Opening the moment…</Text>
              </HStack>
            ) : null}
            <List
              header={<Text className="sr-only">Evaluation timeline</Text>}
              listStyle="decimal"
            >
              {review.evaluationTimeline.map((point) => (
                <ListItem
                  key={point.ply}
                  label={`Move ${point.ply} · ${formatEvaluation(point.evaluation)}`}
                />
              ))}
            </List>
            <HStack aria-label="Full game move list" gap={1} wrap="wrap">
              {snapshot.game.moves.map((move) => (
                <WatercolorButton
                  disabled={disabled}
                  key={move.ply}
                  onClick={() => onOpen(move.ply)}
                  size="sm"
                  type="button"
                  variant="quiet"
                >
                  {moveLabel(move)}
                </WatercolorButton>
              ))}
            </HStack>
          </VStack>
        </WatercolorCard>
      </VStack>
    </WatercolorStudio>
  )
}
