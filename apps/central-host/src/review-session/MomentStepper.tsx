import { Icon } from "@chenchess/ui/astryx"
import { HStack, WatercolorButton } from "@chenchess/ui"

import { reviewSessionShellStyles } from "./ReviewSessionShell.styles"
import type { ReviewMomentMarker } from "./reviewMoments"

/**
 * The Review Session's Critical Moment stepper, riding the evaluation graph
 * frame. Props-only, so the landing showcase steps between moments through
 * the same control a signed-in Player uses.
 */
export function MomentStepper({
  currentPly,
  disabled,
  markers,
  onSelect,
}: {
  currentPly: number
  disabled: boolean
  markers: readonly ReviewMomentMarker[]
  onSelect: (ply: number) => void
}) {
  const plies = markers
    .map((marker) => marker.ply)
    .sort((first, second) => first - second)
  const previous = plies.filter((ply) => ply < currentPly).at(-1)
  const next = plies.find((ply) => ply > currentPly)
  return (
    <HStack
      aria-label="Critical moments"
      className="chen-review-moment-stepper"
      gap={1}
      role="group"
      xstyle={reviewSessionShellStyles.momentStepper}
    >
      <WatercolorButton
        aria-label="Previous moment"
        disabled={disabled || previous === undefined}
        onClick={() => previous !== undefined && onSelect(previous)}
        size="icon"
        type="button"
        variant="quiet"
      >
        <Icon icon="arrowLeft" size="sm" />
      </WatercolorButton>
      <WatercolorButton
        aria-label="Next moment"
        disabled={disabled || next === undefined}
        onClick={() => next !== undefined && onSelect(next)}
        size="icon"
        type="button"
        variant="quiet"
      >
        <Icon icon="arrowRight" size="sm" />
      </WatercolorButton>
    </HStack>
  )
}
