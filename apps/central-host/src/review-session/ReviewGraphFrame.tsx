import type { ReactNode } from "react"

import { VStack } from "@chenchess/ui"

import { MomentStepper } from "./MomentStepper"
import { reviewSessionShellStyles } from "./ReviewSessionShell.styles"
import type { ReviewMomentMarker } from "./reviewMoments"

/**
 * The evaluation graph with the Critical Moment stepper riding its corner —
 * the assembly that tops the Review Session's session column. It owns the
 * `chen-review-graph-frame` hook that `workspace/review-session.css` reaches
 * into, so hosts compose the frame instead of rebuilding it around that
 * class name.
 */
export function ReviewGraphFrame({
  currentPly,
  disabled,
  graph,
  markers,
  onSelect,
}: {
  currentPly: number
  disabled: boolean
  graph: ReactNode
  markers: readonly ReviewMomentMarker[]
  onSelect: ((ply: number) => void) | undefined
}) {
  return (
    <VStack
      className="chen-review-graph-frame"
      gap={0}
      hAlign="stretch"
      xstyle={reviewSessionShellStyles.graphFrame}
    >
      {graph}
      {markers.length > 0 && onSelect ? (
        <MomentStepper
          currentPly={currentPly}
          disabled={disabled}
          markers={markers}
          onSelect={onSelect}
        />
      ) : null}
    </VStack>
  )
}
