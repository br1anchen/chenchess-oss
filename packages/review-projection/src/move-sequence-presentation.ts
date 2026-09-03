import { Chess } from "chessops/chess"
import { makeFen, parseFen } from "chessops/fen"
import { makeSquare, parseUci } from "chessops/util"

import {
  fromSquare,
  type GameReviewLineMove,
  type MoveSequencePresentationMove,
  type Square,
} from "@chenchess/coach-engine-sdk"

import { presentationPiecesFromFen } from "./review-session-presentation-pieces.js"

/**
 * Plays a canonical line out one board at a time.
 *
 * A rendered continuation is the same boards whether a Review Session issued
 * the line or an immutable address named it, so the play-out lives once. An
 * illegal move here is a corrupt line rather than a bad request: the moves came
 * from a frozen Game Review, so nothing a caller supplied can reach this.
 */
export function projectSequenceMoves(
  initialFen: string,
  moves: readonly GameReviewLineMove[],
): MoveSequencePresentationMove[] {
  const setup = parseFen(initialFen)
  if (setup.isErr) throw new Error("Canonical Move Sequence FEN is invalid")
  const parsed = Chess.fromSetup(setup.value)
  if (parsed.isErr) {
    throw new Error("Canonical Move Sequence position is invalid")
  }
  const moveCount = moves.length
  return moves.map(({ san, uci }, offset) => {
    const move = parseUci(uci)
    if (!move || !parsed.value.isLegal(move)) {
      throw new Error("Canonical Move Sequence contains an illegal move")
    }
    parsed.value.play(move)
    const index = offset + 1
    const fen = makeFen(parsed.value.toSetup())
    return {
      board: {
        announcement: `Move ${index} of ${moveCount}: ${san}. ${parsed.value.turn} to move${parsed.value.isCheck() ? " in check" : ""}.`,
        checkSquare: parsed.value.isCheck()
          ? squareName(parsed.value.board.kingOf(parsed.value.turn))
          : null,
        lastMove: {
          from: fromSquare(uci.slice(0, 2)),
          to: fromSquare(uci.slice(2, 4)),
        },
        pieces: presentationPiecesFromFen(fen),
      },
      index,
      san,
    }
  })
}

function squareName(square: number | undefined): Square | null {
  return square === undefined ? null : fromSquare(makeSquare(square))
}
