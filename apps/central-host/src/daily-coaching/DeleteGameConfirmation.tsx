import { Icon } from "@chenchess/ui/astryx"
import type { ImportedGameListItem } from "@chenchess/coach-engine-sdk"
import {
  HStack,
  Text,
  VStack,
  WatercolorButton,
  WatercolorNotice,
} from "@chenchess/ui"

import { WatercolorOverlay } from "@/overlay/WatercolorOverlay"

/**
 * The confirmation names the Game before anything is removed, the way account
 * deletion does. It promises every rating rather than a count: the card
 * collapses a Game reviewed at several ratings into one row, so the number is
 * not known until the Coach Engine answers.
 */
export function DeleteGameConfirmation({
  busy,
  failure,
  game,
  onCancel,
  onConfirm,
}: {
  busy: boolean
  failure: string | null
  game: ImportedGameListItem | null
  onCancel: () => void
  onConfirm: (game: ImportedGameListItem) => void
}) {
  return (
    <WatercolorOverlay
      onOpenChange={(open) => {
        if (!open) onCancel()
      }}
      open={game !== null}
      title="Delete this game?"
    >
      {game === null ? null : (
        <VStack gap={3} hAlign="stretch">
          <Text as="p" display="block" type="body">
            {gameName(game)} goes, along with its review, your published
            comments on it, and any share link you minted for it. Every review
            of this Game from this side goes, whatever rating each was reviewed
            at. This cannot be undone, though you can import the Game again.
          </Text>
          {failure === null ? null : (
            <WatercolorNotice
              detail={failure}
              glyph={<Icon icon="circleAlert" size="sm" />}
              heading="The game was not deleted"
              tone="vermilion"
            />
          )}
          <HStack gap={2} hAlign="end">
            <WatercolorButton
              onClick={onCancel}
              type="button"
              variant="secondary"
            >
              Keep the Game
            </WatercolorButton>
            <WatercolorButton
              loading={busy}
              onClick={() => onConfirm(game)}
              type="button"
              variant="danger"
            >
              Delete
            </WatercolorButton>
          </HStack>
        </VStack>
      )}
    </WatercolorOverlay>
  )
}

export function gameName(game: ImportedGameListItem): string {
  return `the ${sideLabel(game.reviewSide)} Game against ${game.opponentName ?? "an unknown opponent"}`
}

function sideLabel(side: ImportedGameListItem["reviewSide"]): string {
  return side === "both" ? "two-sided" : side
}

export function deletedMessage(
  game: ImportedGameListItem,
  reviews: number,
): string {
  const noun = reviews === 1 ? "review" : "reviews"
  // `gameName` opens with "the", so the sentence starts on a known letter.
  const named = gameName(game).replace("the", "The")
  return `${named} is gone, with ${reviews} ${noun}.`
}
