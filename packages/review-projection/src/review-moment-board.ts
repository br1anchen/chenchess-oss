/**
 * How one Review Moment looks, independent of what is asking.
 *
 * The board, the arrows, and the labels are a function of the Game Review entry
 * and the imported Game alone — never of a Review Session — so a session-bound
 * presentation and a session-free snapshot render the same moment identically.
 */
import { Chess } from "chessops/chess"
import { makeFen, parseFen } from "chessops/fen"
import { makeSquare, parseUci } from "chessops/util"

import {
  fromPositionRef,
  fromSquare,
  type GameReviewCriticalMoment,
  type ImportedGame,
  type PositionSnapshot,
  type ReviewMomentOccurrence,
  type ReviewSessionMoment,
  type ReviewSessionPresentationArrow,
  type ReviewSessionPresentationArrowKind,
  type ReviewSessionPresentationBoard,
  type Square,
} from "@chenchess/coach-engine-sdk"

import { presentationPiecesFromFen } from "./review-session-presentation-pieces.js"

/** One Review Moment's rendering facts, whatever payload carried them. */
export type BoardSourceMoment = {
  criticalMoment: GameReviewCriticalMoment
  occurrence: ReviewMomentOccurrence
  positionSnapshot: PositionSnapshot
}

/**
 * Pairs an admitted Review Moment with the Game Review entry that describes it.
 *
 * The two arrive as separate lists on every payload that carries a review, and
 * a moment without its entry is a broken review rather than an empty render.
 */
export function boardSourceMoment(
  admitted: ReviewSessionMoment,
  criticalMoments: readonly GameReviewCriticalMoment[],
): BoardSourceMoment {
  const criticalMoment = criticalMoments.find(
    ({ criticalMomentId }) =>
      criticalMomentId === admitted.reviewMoment.momentId,
  )
  if (!criticalMoment) {
    throw new Error(
      `Review Moment ${admitted.reviewMoment.momentId} has no canonical Game Review presentation`,
    )
  }
  return {
    criticalMoment,
    occurrence: admitted.reviewMoment,
    positionSnapshot: admitted.positionSnapshot,
  }
}

export function projectBoardFacts(
  presented: BoardSourceMoment,
  importedGame: ImportedGame,
): ReviewSessionPresentationBoard {
  const { positionSnapshot } = presented
  const reviewedMove = importedGame.game.moves.find(
    ({ ply }) => ply === presented.occurrence.ply,
  )
  if (
    !reviewedMove ||
    reviewedMove.beforePositionRef !== positionSnapshot.positionRef
  ) {
    return {
      announcement: `${positionSnapshot.sideToMove} to move.`,
      checkSquare: checkSquare(positionSnapshot.fen),
      lastMove: null,
      pieces: presentationPiecesFromFen(positionSnapshot.fen),
      positionRef: positionSnapshot.positionRef,
    }
  }

  const setup = parseFen(positionSnapshot.fen)
  const move = parseUci(reviewedMove.uci)
  if (setup.isErr || !move) {
    throw new Error(
      `Review Moment ${presented.occurrence.momentId} has invalid board facts`,
    )
  }
  const parsed = Chess.fromSetup(setup.value)
  if (parsed.isErr || !parsed.value.isLegal(move)) {
    throw new Error(
      `Review Moment ${presented.occurrence.momentId} has an illegal played move`,
    )
  }
  parsed.value.play(move)
  return {
    announcement: `Position after ${reviewedMove.san}. ${parsed.value.turn} to move${parsed.value.isCheck() ? " in check" : ""}.`,
    checkSquare: parsed.value.isCheck()
      ? squareName(parsed.value.board.kingOf(parsed.value.turn))
      : null,
    lastMove: moveSquares(reviewedMove.uci),
    pieces: presentationPiecesFromFen(makeFen(parsed.value.toSetup())),
    positionRef: fromPositionRef(reviewedMove.afterPositionRef),
  }
}

export function projectArrows(
  moment: GameReviewCriticalMoment,
  elo: number,
): ReviewSessionPresentationArrow[] {
  const arrow = (
    uci: string | undefined,
    kind: ReviewSessionPresentationArrowKind,
    label: string,
  ): ReviewSessionPresentationArrow | undefined => {
    const move = uci ? moveSquares(uci) : null
    return move
      ? {
          from: move.from,
          kind,
          label,
          to: move.to,
        }
      : undefined
  }
  return [
    arrow(moment.objective.bestMoveUci, "engineBest", "Engine"),
    arrow(moment.human.mostLikelyMoveUci, "maia", `Elo ${elo} player`),
  ].flatMap((candidate) => (candidate ? [candidate] : []))
}

export function momentSummary(moment: GameReviewCriticalMoment) {
  return (
    moment.comment?.text ??
    `${moment.display.playedEvaluation.label}; best was ${moment.display.bestEvaluation.label}.`
  )
}

export function momentGlyph(moment: GameReviewCriticalMoment) {
  return moment.classification.kind === "positiveHighlight" ? "!" : "?!"
}

export function momentTone(moment: GameReviewCriticalMoment) {
  return toneFromClassificationKind(moment.classification.kind)
}

export function toneFromClassificationKind(
  kind: GameReviewCriticalMoment["classification"]["kind"],
): "positive" | "improvement" | "selected" {
  switch (kind) {
    case "positiveHighlight":
      return "positive"
    case "improvementOpportunity":
      return "improvement"
    case "neutral":
      return "selected"
    default: {
      const exhaustive: never = kind
      return exhaustive
    }
  }
}

/** Coach App / widget contract: Improvement paints as `critical`. */
export function reviewMomentToneFromClassificationKind(
  kind: GameReviewCriticalMoment["classification"]["kind"],
): "positive" | "critical" | "selected" {
  switch (kind) {
    case "positiveHighlight":
      return "positive"
    case "improvementOpportunity":
      return "critical"
    case "neutral":
      return "selected"
    default: {
      const exhaustive: never = kind
      return exhaustive
    }
  }
}

export function isNeutralPlayerSelectedClassification(
  kind: GameReviewCriticalMoment["classification"]["kind"] | null | undefined,
  playerSelected: boolean,
) {
  return playerSelected && kind === "neutral"
}

export function moveLabel(moment: GameReviewCriticalMoment) {
  return `${moment.moveNumber}${moment.side === "black" ? "…" : "."} ${moment.playedSan}`
}

export function occurrenceMoveLabel(
  occurrence: Pick<ReviewMomentOccurrence, "precedingMove">,
) {
  const move = occurrence.precedingMove
  if (!move) return undefined
  return `${move.moveNumber}${move.side === "black" ? "…" : "."} ${move.san}`
}

export function classificationLabel(
  kind: GameReviewCriticalMoment["classification"]["kind"],
) {
  if (kind === "positiveHighlight") return "Positive highlight"
  if (kind === "improvementOpportunity") return "Improvement opportunity"
  return "Your pick"
}

export function boardOrientation(
  importedGame: ImportedGame,
  firstMoment: PositionSnapshot | undefined,
) {
  return importedGame.reviewSide === "both"
    ? (firstMoment?.sideToMove ?? "white")
    : importedGame.reviewSide
}

function checkSquare(fen: string): Square | null {
  const setup = parseFen(fen)
  if (setup.isErr) throw new Error("Presentation FEN is invalid")
  const parsed = Chess.fromSetup(setup.value)
  if (parsed.isErr) throw new Error("Presentation position is invalid")
  return parsed.value.isCheck()
    ? squareName(parsed.value.board.kingOf(parsed.value.turn))
    : null
}

function squareName(square: number | undefined): Square | null {
  return square === undefined ? null : fromSquare(makeSquare(square))
}

function moveSquares(uci: string) {
  if (!/^[a-h][1-8][a-h][1-8][qrbn]?$/.test(uci)) return null
  return {
    from: fromSquare(uci.slice(0, 2)),
    to: fromSquare(uci.slice(2, 4)),
  }
}
