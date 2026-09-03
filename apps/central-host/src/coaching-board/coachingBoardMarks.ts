import {
  parseBoardSquare,
  type BoardArrow,
  type BoardSquareMark,
} from "@chenchess/ui"

import type { CoachingBoardMark } from "./boardAnnotation"

/**
 * What the coach drew, in the two shapes the board renders.
 *
 * One pass rather than two, because a mark is either a line or a square and
 * the board wants both lists at once. Kept apart from the verifier so that
 * stays a pure geometry check with no view types in it. Every mark carries
 * the `coach` tone: an agent's claim must never read as the Player's own
 * exploration (ADR 0059).
 */
export type CoachMarkOverlay = {
  arrows: BoardArrow[]
  squares: BoardSquareMark[]
}

export function coachMarkOverlay(
  marks: readonly CoachingBoardMark[],
): CoachMarkOverlay {
  const arrows: BoardArrow[] = []
  const squares: BoardSquareMark[] = []
  for (const mark of marks) {
    if (mark.kind === "arrow") {
      arrows.push({
        from: parseBoardSquare(mark.from),
        label: mark.label,
        to: parseBoardSquare(mark.to),
        tone: "coach",
      })
    } else {
      squares.push({
        label: mark.label,
        square: parseBoardSquare(mark.square),
        tone: "coach",
      })
    }
  }
  return { arrows, squares }
}
