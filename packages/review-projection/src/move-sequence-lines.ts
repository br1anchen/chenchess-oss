/**
 * The canonical continuations one Review Moment offers, and nothing about who
 * is asking for them.
 *
 * A Review Moment's engine line and played-move refutation come from the
 * objective lines the Game Review already froze, so they are the same lines
 * whether a Review Session issues a handle for them or an immutable snapshot
 * addresses them.
 */
import { Chess } from "chessops/chess"
import { makeFen, parseFen } from "chessops/fen"
import { makeSan } from "chessops/san"
import { parseUci } from "chessops/util"

import type {
  GameReviewLineMove,
  GameReviewObjectiveLines,
  ImportedGame,
  MoveSequenceOrigin,
  MoveSequencePresentationKind,
} from "@chenchess/coach-engine-sdk"

import type { BoardSourceMoment } from "./review-moment-board.js"

export type CanonicalMomentLine = {
  initialFen: string
  kind: MoveSequencePresentationKind
  moves: readonly GameReviewLineMove[]
  orientation: "black" | "white"
  title: string
}

/**
 * A line shorter than two plies shows nothing a static board does not, so it is
 * not offered rather than offered and empty.
 */
const minimumRenderableMoves = 2

export function canonicalMomentLines(
  admitted: BoardSourceMoment,
  content: ImportedGame,
) {
  return canonicalLinesFrom(
    momentSequenceOrigin(admitted, content),
    admitted.criticalMoment.objective.lines ?? null,
  )
}

/**
 * Where a Review Moment's continuations start, read off the Game itself.
 *
 * A session-free read is handed the same origin by the Coach Engine instead, so
 * the rule for what a continuation continues from lives once and both paths
 * obey it.
 */
export function momentSequenceOrigin(
  admitted: BoardSourceMoment,
  content: ImportedGame,
): MoveSequenceOrigin {
  return {
    fen: admitted.positionSnapshot.fen,
    reviewSide: content.reviewSide,
    reviewedMoveUci:
      content.game.moves.find(({ ply }) => ply === admitted.occurrence.ply)
        ?.uci ?? null,
    sideToMove: admitted.positionSnapshot.sideToMove,
  }
}

/**
 * A Review Moment reads the way the reviewed side reads, and a review of both
 * sides reads from whoever was to move.
 */
export function sequenceOrientation(origin: MoveSequenceOrigin) {
  return origin.reviewSide === "both" ? origin.sideToMove : origin.reviewSide
}

export function canonicalLinesFrom(
  origin: MoveSequenceOrigin,
  lines: GameReviewObjectiveLines | null,
): CanonicalMomentLine[] {
  if (!lines) return []
  const orientation = sequenceOrientation(origin)
  const candidates: Array<{
    initialFen: string | undefined
    kind: MoveSequencePresentationKind
    moves: readonly GameReviewLineMove[]
    title: string
  }> = [
    {
      initialFen: origin.fen,
      kind: "engineBest",
      moves: lines.best,
      title: "Engine line",
    },
    {
      initialFen: origin.reviewedMoveUci
        ? fenAfterMove(origin.fen, origin.reviewedMoveUci)
        : undefined,
      kind: "playedMoveRefutation",
      moves: lines.refutation,
      title: "Reply to your move",
    },
  ]
  return candidates.flatMap((candidate) => {
    if (
      !candidate.initialFen ||
      candidate.moves.length < minimumRenderableMoves
    ) {
      return []
    }
    const moves = canonicalMovesFromFen(candidate.initialFen, candidate.moves)
    if (!moves) return []
    return [
      { ...candidate, initialFen: candidate.initialFen, moves, orientation },
    ]
  })
}

function fenAfterMove(fen: string, uci: string) {
  const setup = parseFen(fen)
  const move = parseUci(uci)
  if (setup.isErr || !move) return undefined
  const position = Chess.fromSetup(setup.value)
  if (position.isErr || !position.value.isLegal(move)) return undefined
  position.value.play(move)
  return makeFen(position.value.toSetup())
}

/**
 * Canonical notation for a legal UCI move path from one exact Position.
 *
 * Stored Move Sequences and evaluated Player Lines both cross a boundary where
 * UCI is authoritative and SAN is presentation. Keeping the conversion here
 * makes both callers reject an impossible path instead of trusting notation
 * supplied beside it.
 */
export function canonicalMovesFromFen(
  fen: string,
  moves: readonly Pick<GameReviewLineMove, "uci">[],
) {
  const setup = parseFen(fen)
  if (setup.isErr) return undefined
  const position = Chess.fromSetup(setup.value)
  if (position.isErr) return undefined
  const canonical: GameReviewLineMove[] = []
  for (const { uci } of moves) {
    const move = parseUci(uci)
    if (!move || !position.value.isLegal(move)) return undefined
    canonical.push({ san: makeSan(position.value, move), uci })
    position.value.play(move)
  }
  return canonical
}
