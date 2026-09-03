import * as stylex from "@stylexjs/stylex"
import { useEffect, useRef } from "react"
import type { GameImportId, GameReview } from "@chenchess/coach-engine-sdk"
import {
  HStack,
  Text,
  VStack,
  WatercolorBadge,
  WatercolorButtonLink,
  WatercolorCard,
} from "@chenchess/ui"

import type { FetchAccessToken } from "@/auth/FirebaseAuthProvider"
import { gameReviewPath } from "@/game-review/gameReviewRoute"
import {
  createCommandEnvelope,
  streamReviewSessionCommand,
} from "@/review-session/client"

export type FrozenReviewState =
  | { kind: "loading" }
  | { kind: "ready"; review: GameReview }
  | { kind: "failed"; message: string }

export function ReviewedGameCard({
  focused = false,
  frame = true,
  gameImportId,
  meta,
  opponentName,
  opening,
  outcome,
  reviewSide,
}: {
  focused?: boolean
  frame?: boolean
  gameImportId: GameImportId
  /** The provider / time control column. Digest rows pass null. */
  meta: { provider: string; timeControl: string | null } | null
  opponentName: string | null
  opening: { eco: string; name: string } | null
  outcome: "win" | "loss" | "draw" | null
  reviewSide: "white" | "black" | "both"
}) {
  const article = useRef<HTMLElement>(null)
  useEffect(() => {
    if (focused) article.current?.scrollIntoView({ block: "center" })
  }, [focused])
  const supporting = meta
    ? [meta.provider, meta.timeControl].filter(Boolean).join(" · ")
    : ""
  return (
    <WatercolorCard
      frame={frame}
      padding="compact"
      ref={article}
      xstyle={reviewedGameCardStyles.card}
    >
      <WatercolorButtonLink
        href={gameReviewPath(gameImportId)}
        hoverWash="bloom"
        variant="quiet"
        xstyle={reviewedGameCardStyles.link}
      >
        <HStack gap={2} vAlign="center" wrap="wrap">
          {outcome ? (
            <WatercolorBadge tone={outcomeTokenTone(outcome)}>
              {outcome}
            </WatercolorBadge>
          ) : null}
          <VStack gap={1} hAlign="start">
            <Text type="body" weight="semibold">
              vs. {opponentName ?? "Unknown opponent"}
            </Text>
            <Text type="supporting">
              {sideLabel(reviewSide)} · {openingLabel(opening)}
            </Text>
            {supporting ? (
              <Text color="secondary" type="supporting">
                {supporting}
              </Text>
            ) : null}
          </VStack>
        </HStack>
      </WatercolorButtonLink>
    </WatercolorCard>
  )
}

export async function readFrozenGameReview(
  gameImportId: GameImportId,
  fetchAccessToken: FetchAccessToken,
): Promise<GameReview> {
  let opened: GameReview | undefined
  await streamReviewSessionCommand({
    envelope: createCommandEnvelope({ gameImportId, kind: "openGameReview" }),
    fetchAccessToken,
    onEvent: ({ event }) => {
      if (
        event.kind === "completed" &&
        event.result.kind === "gameReviewOpened"
      ) {
        opened = event.result.review
      }
    },
  })
  if (!opened) throw new Error("The frozen Game Review could not be opened.")
  return opened
}

function openingLabel(opening: { eco: string; name: string } | null): string {
  return opening ? `${opening.eco} · ${opening.name}` : "Opening unavailable"
}

function sideLabel(side: "white" | "black" | "both"): string {
  return side === "both"
    ? "Both sides"
    : `${side[0]!.toUpperCase()}${side.slice(1)}`
}

const reviewedGameCardStyles = stylex.create({
  card: {
    minWidth: 0,
    width: "100%",
    maxWidth: "100%",
  },
  link: {
    display: "block",
    width: "100%",
    minWidth: 0,
    justifyContent: "flex-start",
    textAlign: "left",
    whiteSpace: "normal",
  },
})

function outcomeTokenTone(outcome: "win" | "loss" | "draw") {
  switch (outcome) {
    case "draw":
      return "neutral" as const
    case "loss":
      return "danger" as const
    case "win":
      return "success" as const
    default: {
      const _exhaustive: never = outcome
      return _exhaustive
    }
  }
}
