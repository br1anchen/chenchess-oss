import { Chess } from "chessops/chess"
import { makeFen } from "chessops/fen"
import { parseSan } from "chessops/san"
import { makeUci } from "chessops/util"

import {
  fromPositionRef,
  type CanonicalGameMove,
} from "@chenchess/coach-engine-sdk"
import { presentationPiecesFromFen } from "@chenchess/review-projection"

import {
  browseBoardAtPly,
  type BrowseBoardPosition,
} from "@/review-session/model"

const startFen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"

export function openingLineMoves(path: string): CanonicalGameMove[] {
  const chess = Chess.default()
  const tokens = path
    .replace(/\d+\./g, " ")
    .split(/\s+/)
    .filter(
      (token) => token.length > 0 && !/^(1-0|0-1|1\/2-1\/2|\*)$/.test(token),
    )
  const moves: CanonicalGameMove[] = []
  for (const san of tokens) {
    const before = makeFen(chess.toSetup())
    const parsed = parseSan(chess, san)
    if (!parsed || !chess.isLegal(parsed)) return moves
    const ply = moves.length + 1
    const side = chess.turn === "white" ? "white" : "black"
    chess.play(parsed)
    moves.push({
      afterPositionRef: positionRefForFen(makeFen(chess.toSetup())),
      beforePositionRef: positionRefForFen(before),
      moveNumber: Math.floor((ply - 1) / 2) + 1,
      ply,
      san,
      side,
      uci: makeUci(parsed),
    })
  }
  return moves
}

/**
 * First-paint ply for an opened line: the last ply of its path.
 *
 * `viewedPly === 1` is only for an empty path. A catalog line already has
 * moves, so next-move branches start from that line, not catalog root.
 */
export function openingLineViewedPly(
  moves: readonly CanonicalGameMove[],
): number {
  return moves.at(-1)?.ply ?? 1
}

export function openingBoardPosition(
  moves: readonly CanonicalGameMove[],
  ply: number,
): BrowseBoardPosition {
  return moves.length === 0 && ply === 0
    ? startingBoardPosition()
    : browseBoardAtPly(moves, ply)
}

/**
 * A board position from a FEN alone.
 *
 * The opening analysis route grounds each analyzed ply as one FEN, so a
 * branch built from it derives its occupied squares here rather than
 * carrying an engine `PositionSnapshot`.
 */
export function openingPositionFromFen(fen: string): BrowseBoardPosition {
  return {
    fen,
    occupied: presentationPiecesFromFen(fen).map(({ piece, square }) => ({
      piece,
      square,
    })),
    sideToMove: fen.split(" ")[1] === "b" ? "black" : "white",
  }
}

export function startingBoardPosition(): BrowseBoardPosition {
  return {
    fen: startFen,
    occupied: presentationPiecesFromFen(startFen).map(({ piece, square }) => ({
      piece,
      square,
    })),
    sideToMove: "white",
  }
}

export function positionRefForFen(fen: string) {
  let hash = 2166136261
  for (const character of fen) {
    hash ^= character.charCodeAt(0)
    hash = Math.imul(hash, 16777619)
  }
  return fromPositionRef(`fnv1a:${(hash >>> 0).toString(16).padStart(8, "0")}`)
}
