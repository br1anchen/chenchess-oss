import type { ComparisonBoardArrow } from "@chenchess/review-projection"
import { parseBoardSquare, type BoardArrow } from "@chenchess/ui"

/**
 * The one crossing from a projected arrow to a drawn one.
 *
 * Which arrows a board shows is the projection's business; both review
 * surfaces draw whatever it hands back, so the square translation lives here
 * rather than once per surface.
 */
export function boardArrowsFrom(
  arrows: readonly (ComparisonBoardArrow | undefined)[],
): BoardArrow[] {
  return arrows.flatMap((arrow) =>
    arrow
      ? [
          {
            from: parseBoardSquare(arrow.from),
            label: arrow.label,
            to: parseBoardSquare(arrow.to),
            tone: arrow.tone,
          },
        ]
      : [],
  )
}
