/**
 * Where a swipe is going, and how a row resists at its edges.
 *
 * Separated from the component because these are the two decisions that make a
 * swipe feel like a throw rather than a slider: they are pure, they are the
 * numbers a reviewer would argue about, and a component test cannot see them.
 */

/** iOS's scroll deceleration, which is what makes a flick feel thrown. */
const DECELERATION_RATE = 0.998

/**
 * Where a throw comes to rest, as scroll deceleration would put it.
 *
 * The textbook `v² / 2a` is not what a touch platform uses; this is the
 * exponential-decay form, so a flick lands where the Player aimed it rather
 * than where their finger happened to leave the glass.
 */
export function projectedRestingPoint(velocityPerSecond: number): number {
  return (
    ((velocityPerSecond / 1000) * DECELERATION_RATE) / (1 - DECELERATION_RATE)
  )
}

/**
 * The row follows the pointer inside its travel and resists outside it.
 *
 * Real things slow before they stop. A hard clamp at the reveal reads as a
 * frozen interface; resistance reads as "there is nothing more this way". Past
 * the closed edge the resistance is scaled to the reveal rather than the row,
 * because there is no action that way and the nudge should stay a nudge.
 */
export function resistedOffset(
  offset: number,
  reveal: number,
  rowWidth: number,
): number {
  if (offset > 0) return rubberband(offset, reveal)
  if (offset < -reveal) return -reveal - rubberband(-reveal - offset, rowWidth)
  return offset
}

function rubberband(
  overshoot: number,
  dimension: number,
  constant = 0.55,
): number {
  return (overshoot * dimension * constant) / (dimension + constant * overshoot)
}
