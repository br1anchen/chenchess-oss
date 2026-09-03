import type { ImportedGame } from "@chenchess/coach-engine-sdk"
import { Text, VStack } from "@chenchess/ui"

import { playerName, reviewSideLabel } from "./model"

/**
 * The game identity beside the Review Session plaque: reviewer vs. opponent,
 * reviewed side and opening — the Imported Games card line, header-sized.
 */
export function ReviewGameHeaderInfo({
  importedGame,
}: {
  importedGame: ImportedGame
}) {
  const { reviewer, opponent } = matchupNames(importedGame)
  const opening = importedGame.game.opening
  const details = [
    reviewSideLabel(importedGame.reviewSide),
    ...(opening.kind === "present" ? [opening.eco, opening.name] : []),
  ].join(" · ")
  return (
    <VStack aria-label="Game details" gap={0} hAlign="start">
      <Text type="body" weight="semibold">
        {reviewer} vs. {opponent}
      </Text>
      <Text type="supporting">{details}</Text>
    </VStack>
  )
}

function matchupNames(importedGame: ImportedGame) {
  const white = playerName(importedGame, "white")
  const black = playerName(importedGame, "black")
  return importedGame.reviewSide === "black"
    ? { reviewer: black, opponent: white }
    : { reviewer: white, opponent: black }
}
