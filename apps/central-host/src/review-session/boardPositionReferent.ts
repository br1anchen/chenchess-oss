import {
  PLAYER_VISIBLE_MOVE_FALLBACK,
  type PlayerVisibleSan,
} from "@chenchess/review-projection"

/**
 * What the board is showing, as the caption under it says it: the move that
 * reached the position, the kind of position when it is off the Game's own
 * line, and how far into a shown line the board has walked.
 */
export type ShownPosition = {
  heading: PlayerVisibleSan
  /** Only off-game positions carry one: "Alternative branch", "Engine line". */
  kind: string | null
  /** 0 when the board is not walking a line. */
  lineStep: number
  /**
   * Whether the heading's move is already on the board. The Game's own line
   * and an engine line stand *before* the caption's move — a Critical Moment
   * is named by the move played from it — while a branch and the refutation
   * of a played move stand after it. Getting this wrong points the coach one
   * move away from what the Player sees.
   */
  played: boolean
}

/**
 * What "Ask about this position" puts on the clipboard (#530).
 *
 * The hardest half of deixis is not the coach resolving a referent — it is
 * the Player not knowing how to say "this" from a chat window beside a
 * board. So the sentence repeats exactly what the Player can already see
 * under the board, in the words the board uses. "my Coaching Board" is the
 * hook — the read tool's description says to read before answering any
 * question that points at the board, and this points.
 *
 * Nothing implementation-shaped rides along: no revision, no UCI, no id.
 * The coach reads the board anyway; the sentence only has to make it look.
 */
export function boardPositionReferent({
  heading,
  kind,
  lineStep,
  played,
}: ShownPosition): string {
  const where = placeInWords(heading, kind, lineStep, played)
  return where === null
    ? "About the position on my Coaching Board:"
    : `About the position on my Coaching Board (${where}):`
}

function placeInWords(
  heading: PlayerVisibleSan,
  kind: string | null,
  lineStep: number,
  played: boolean,
) {
  // The fallback heading is the board admitting it cannot name the move, so
  // "before this move" would point at nothing.
  const move = heading === PLAYER_VISIBLE_MOVE_FALLBACK ? null : heading
  const standing = played ? "after" : "before"
  if (kind === null) return move === null ? null : `${standing} ${move}`
  const line = kind.toLowerCase()
  if (lineStep > 0) {
    const steps = `${lineStep} ${lineStep === 1 ? "move" : "moves"} in`
    return move === null
      ? `${line}, ${steps}`
      : `${line} from ${move}, ${steps}`
  }
  return move === null ? line : `${line}, ${standing} ${move}`
}
