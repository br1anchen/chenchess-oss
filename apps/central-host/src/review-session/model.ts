import { Chess } from "chessops/chess"
import { makeFen, parseFen } from "chessops/fen"
import { makeSquare, makeUci, parseSquare, parseUci } from "chessops/util"

import {
  fromEloRating,
  fromSquare,
  type CanonicalGameMove,
  type EloRating,
  type EngineEvaluation,
  type ImportedGame,
  type OccupiedSquare,
  type PieceRole,
  type PositionSnapshot,
  type ReviewSessionCoreContract,
  type ReviewSide,
  type RejectionRecovery,
  type Square,
} from "@chenchess/coach-engine-sdk"
import {
  presentationPiecesFromFen,
  playerVisibleSanLiteral,
  type PlayerVisibleSan,
} from "@chenchess/review-projection"

export {
  evaluationPointPresentation as evaluationPoint,
  formatEvaluation,
  type EvaluationPointPresentation as EvaluationPoint,
} from "@chenchess/ui"

export type ImportMode = "chessCom" | "lichess" | "pgn"

export type PromotionRole = Extract<
  PieceRole,
  "knight" | "bishop" | "rook" | "queen"
>

type OccupiedPosition = Pick<PositionSnapshot, "occupied">

export function parseEloRating(value: string): EloRating | null {
  const rating = Number(value)
  if (Number.isInteger(rating) && rating >= 100 && rating <= 3500) {
    return fromEloRating(rating)
  }
  return null
}

export function moveLabel(move: CanonicalGameMove): PlayerVisibleSan {
  return playerVisibleSanLiteral(
    `${move.moveNumber}${move.side === "black" ? "…" : "."} ${move.san}`,
  )
}

export function playerName(
  snapshot: ImportedGame,
  side: "white" | "black",
): string {
  const player = snapshot.game[side]
  return player.name.kind === "present"
    ? player.name.value
    : side === "white"
      ? "White"
      : "Black"
}

/**
 * Which side a Game's board opens from. A Game reviewed from both sides has no
 * side of its own, so it opens from White's.
 *
 * Returns the two literals rather than a named orientation type: both the
 * board's UI contract and the Coaching Board Snapshot's own take it, and
 * neither should have to borrow the other's name to read this.
 */
export function reviewSideOrientation(side: ReviewSide): "black" | "white" {
  return side === "black" ? "black" : "white"
}

export function reviewSideLabel(side: ReviewSide): string {
  return side === "both"
    ? "Both sides"
    : `${side[0]!.toUpperCase()}${side.slice(1)}`
}

export type BrowseBoardPosition = Pick<
  PositionSnapshot,
  "fen" | "occupied" | "sideToMove"
>

export function browseBoardAtPly(
  moves: readonly CanonicalGameMove[],
  ply: number,
): BrowseBoardPosition {
  const chess = Chess.default()
  for (const move of moves) {
    if (move.ply >= ply) break
    const parsed = parseUci(move.uci)
    if (!parsed || !chess.isLegal(parsed)) {
      throw new Error(
        "Browse reconstruction requires legal imported Game moves",
      )
    }
    chess.play(parsed)
  }
  const fen = makeFen(chess.toSetup())
  const occupied: OccupiedSquare[] = presentationPiecesFromFen(fen).map(
    ({ piece, square }) => ({ piece, square }),
  )
  return {
    fen,
    occupied,
    sideToMove: chess.turn === "white" ? "white" : "black",
  }
}

export function evaluationFromCore(
  core: ReviewSessionCoreContract,
): EngineEvaluation | null {
  const positionRef = core.positionSnapshot.positionRef
  for (const entry of core.evidencePacket.entries) {
    if (entry.kind === "engineAnalysis" && entry.positionRef === positionRef) {
      return entry.analysis.evaluation
    }
  }
  return null
}

export function bestMoveUciFromCore(
  core: ReviewSessionCoreContract,
): string | null {
  const positionRef = core.positionSnapshot.positionRef
  for (const entry of core.evidencePacket.entries) {
    if (entry.kind === "engineAnalysis" && entry.positionRef === positionRef) {
      return entry.analysis.bestMoveUci
    }
  }
  return null
}

export function legalDestinations(
  position: Pick<PositionSnapshot, "fen">,
  from: Square,
): Square[] {
  const square = parseSquare(from)
  if (square === undefined) {
    throw new Error("A decoded board square must use algebraic coordinates")
  }
  const setup = parseFen(position.fen).unwrap()
  const chess = Chess.fromSetup(setup).unwrap()
  return [...chess.dests(square)].map((destination) =>
    fromSquare(makeSquare(destination)),
  )
}

export function uciForDestination(
  position: OccupiedPosition,
  from: Square,
  to: Square,
  promotion?: PromotionRole,
): string {
  const fromSquare = parseSquare(from)
  const toSquare = parseSquare(to)
  if (fromSquare === undefined || toSquare === undefined) {
    throw new Error("A decoded board square must use algebraic coordinates")
  }
  if (promotionRequired(position, from, to) !== (promotion !== undefined)) {
    throw new Error("Promotion moves require one explicit promotion piece")
  }
  return makeUci({ from: fromSquare, to: toSquare, promotion })
}

export function promotionRequired(
  position: OccupiedPosition,
  from: Square,
  to: Square,
): boolean {
  const piece = position.occupied.find((entry) => entry.square === from)?.piece
  return piece?.role === "pawn" && (to.endsWith("1") || to.endsWith("8"))
}

export function recoveryMessage(recovery: RejectionRecovery): string {
  switch (recovery.kind) {
    case "selectReviewSide":
      return "Choose the side you want to review, then try again."
    case "provideEloProfile":
      return "This game has no rating for that side. Enter a rating and try again."
    case "chooseLegalMove":
      return recovery.matchingMoves.length > 0
        ? `Choose one of the matching legal moves: ${recovery.matchingMoves.join(", ")}.`
        : "Choose one of the legal destinations shown on the board."
    case "retryAfter":
      return `Try again in ${recovery.seconds} seconds.`
    case "startNewReviewSession":
      return "This review is out of date. Open the game again."
    case "correctInput":
      return "Correct the highlighted input and try again."
    case "none":
      return "Nothing changed."
  }
}
