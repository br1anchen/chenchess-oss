import { Chess } from "chessops/chess"
import { makeFen, parseFen } from "chessops/fen"
import { parseUci } from "chessops/util"

import type { BrowseBoardPosition } from "@/review-session/model"

import { openingPositionFromFen } from "./openingMoves"

/** One ply of a line the board can walk, as the Review Moment states it. */
export type CoachingBoardLineStep = { san: string; uci: string }

/**
 * Which of the Review Moment's two lines the board is walking.
 *
 * The retained exploration tree is deliberately not one of these. Its nodes
 * are already selectable one by one — the branch strip for the Player, an
 * Alternative Move target for the agent — and a cursor over a path that is
 * itself derived from the selected node could only ever sit at its end.
 */
export type CoachingBoardLinePlaybackSource =
  | "engineBest"
  | "playedMoveRefutation"

export type CoachingBoardLinePlayback = {
  /** 0 is the position the line starts from; `steps.length` is its end. */
  index: number
  source: CoachingBoardLinePlaybackSource
  steps: readonly CoachingBoardLineStep[]
}

export const COACHING_BOARD_STEP_DIRECTIONS = [
  "start",
  "previous",
  "next",
  "end",
] as const

export type CoachingBoardStepDirection =
  (typeof COACHING_BOARD_STEP_DIRECTIONS)[number]

export type CoachingBoardStepTarget = number | CoachingBoardStepDirection

/**
 * Where a step lands, or null if it names a position outside the line.
 *
 * The named directions clamp: `next` at the end of a line is a no-op rather
 * than a refusal, because a coach walking to the end and asking again has not
 * made a mistake worth an error. An explicit index outside the line has,
 * so it refuses.
 */
export function resolveStepIndex(
  playback: CoachingBoardLinePlayback,
  target: CoachingBoardStepTarget,
): number | null {
  const total = playback.steps.length
  switch (target) {
    case "start":
      return 0
    case "end":
      return total
    case "next":
      return Math.min(playback.index + 1, total)
    case "previous":
      return Math.max(playback.index - 1, 0)
    default:
      return target >= 0 && target <= total ? target : null
  }
}

/**
 * The position a line reaches after its first `count` moves, or null if the
 * line cannot be played from there.
 *
 * The engine authored these moves against this position, so a line that does
 * not play is the Review and the board disagreeing about the Game. The step
 * transition settles that before it commits, which is what leaves the render
 * path with nothing to handle.
 */
export function linePlaybackPosition(
  baseFen: string,
  ucis: readonly string[],
): BrowseBoardPosition | null {
  const setup = parseFen(baseFen)
  if (setup.isErr) return null
  const position = Chess.fromSetup(setup.value)
  if (position.isErr) return null
  const chess = position.value
  for (const uci of ucis) {
    const move = parseUci(uci)
    if (!move || !chess.isLegal(move)) return null
    chess.play(move)
  }
  return openingPositionFromFen(makeFen(chess.toSetup()))
}
