import type { PlayerLineCommandOutcome } from "../../server/board/player-line-evaluate"

/** The longest line the engine will walk in one evaluation. */
export const BOARD_EXPLORATION_MOVE_LIMIT = 12

export const boardExplorationUnreachableNotice =
  "The engine could not be reached. Try that move again."

export const boardExplorationLimitNotice =
  "This line has reached the engine's twelve-move limit. Step back to explore another move."

/**
 * What the Player is told when their own move did not reach an evaluation.
 *
 * An evaluated move says nothing: the board itself is the answer. Every other
 * outcome names what stopped, because the board would otherwise sit unchanged
 * with no reason given. Keyed on the Coach Engine outcome itself rather than a
 * restatement of it, so an outcome kind added to the contract has to be given
 * words here instead of falling into a catch-all.
 *
 * `completed` reaches this only when the engine answered with a completion for
 * an operation the board did not ask for, and `idempotencyKeyMismatch` only
 * when the retry under a fresh identity also mismatched. Neither is something
 * the Player can act on differently from an unreachable engine.
 */
export function gameExplorationRefusalNotice(
  outcome: PlayerLineCommandOutcome,
): string {
  switch (outcome.kind) {
    case "illegalMove":
      return "That move is not legal from this position."
    case "deadlineReached":
      return "The engine ran out of time on that line."
    case "explorationExhausted":
      return "You have used up this game’s exploration for now."
    case "completed":
    case "failed":
    case "idempotencyKeyMismatch":
      return boardExplorationUnreachableNotice
    default: {
      const _exhaustive: never = outcome
      return _exhaustive
    }
  }
}
