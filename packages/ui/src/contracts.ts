import type { PieceColor, PieceRole } from "./assets"

export type BoardFile = "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h"
export type BoardRank = "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8"
export type BoardSquare = `${BoardFile}${BoardRank}`

const boardFiles = ["a", "b", "c", "d", "e", "f", "g", "h"] as const
const boardRanks = ["1", "2", "3", "4", "5", "6", "7", "8"] as const
const boardSquareForm = /^[a-h][1-8]$/

export function parseBoardFile(value: unknown): BoardFile {
  const file = boardFiles.find((candidate) => candidate === value)
  if (file === undefined) {
    throw new TypeError("invalid BoardFile")
  }
  return file
}

export function parseBoardRank(value: unknown): BoardRank {
  const rank = boardRanks.find((candidate) => candidate === value)
  if (rank === undefined) {
    throw new TypeError("invalid BoardRank")
  }
  return rank
}

export function parseIsBoardSquare(value: unknown): value is BoardSquare {
  return typeof value === "string" && boardSquareForm.test(value)
}

export function parseBoardSquare(value: unknown): BoardSquare {
  if (!parseIsBoardSquare(value)) {
    throw new TypeError("invalid BoardSquare")
  }
  return value
}

export function fromBoardSquare(value: string): BoardSquare {
  return parseBoardSquare(value)
}
export type BoardOrientation = PieceColor

export type BoardPiece = {
  color: PieceColor
  role: PieceRole
  square: BoardSquare
}

export type BoardMove = {
  from: BoardSquare
  to: BoardSquare
}

export function parseBoardMove(value: { from: string; to: string }): BoardMove {
  return {
    from: parseBoardSquare(value.from),
    to: parseBoardSquare(value.to),
  }
}

/**
 * Which kind of thing the board is drawing. The colour is the theme's
 * business: `engine` is the engine's line, `peer` an Elo-matched player's,
 * `candidate` a move being explored, `coach` something the coach drew about
 * the position (ADR 0059) — kept apart from `candidate` so an agent's
 * assertion never reads as the Player's own exploration.
 */
export type BoardInkTone = "engine" | "peer" | "candidate" | "coach"

export type BoardArrow = BoardMove & {
  label: string
  tone: BoardInkTone
}

/** A square the board has singled out, with the label saying why. */
export type BoardSquareMark = {
  label: string
  square: BoardSquare
  tone: BoardInkTone
}

export type BoardPresentation = {
  id: string
  fen: string
  pieces: readonly BoardPiece[]
  orientation: BoardOrientation
  selectedSquare: BoardSquare | null
  legalDestinations: readonly BoardSquare[]
  lastMove: BoardMove | null
  checkSquare: BoardSquare | null
  promotion: {
    move: BoardMove
    choices: readonly Extract<
      PieceRole,
      "queen" | "rook" | "bishop" | "knight"
    >[]
  } | null
  disabled: boolean
  announcement: string
}

export type ReviewMomentKind = "automatic" | "playerSelected"
export type ReviewMomentTone = "critical" | "positive" | "selected" | "quiet"

export type ReviewMomentPresentation = {
  id: string
  ply: number
  moveLabel: string
  kind: ReviewMomentKind
  tone: ReviewMomentTone
  title: string
  summary: string
}

export type AlternativeMovePresentation = {
  evaluation: string | null
  id: string
  san: string
  label: string
  selected: boolean
  status: "idle" | "active" | "complete" | "cancelled"
  detail: string
  strongestReply: string | null
}

export type RetentionPresentation = {
  available: boolean
  enabled: boolean
  disclosureRequired: boolean
  description: string
  resolving: boolean
}

export type ImportSetupPresentation = {
  source: "chessCom" | "lichess" | "pgn"
  sourceLabel: string
  reviewSide: "white" | "black" | "both"
  eloLabel: string
  status: "ready" | "importing" | "complete" | "failed"
  recovery: string | null
}

export type WorkspacePresentation = {
  playerName: string
  sessionLabel: string
  importSetup: ImportSetupPresentation
  moments: readonly ReviewMomentPresentation[]
  activeMomentId: string
  board: BoardPresentation
  comment: {
    eyebrow: string
    heading: string
    body: string
    status: "admitted" | "draft" | "unavailable"
  }
  alternatives: readonly AlternativeMovePresentation[]
  retention: RetentionPresentation
  statusMessage: string
}

export type WorkspaceAction =
  | { type: "signOutRequested" }
  | { type: "importSourceChanged"; source: ImportSetupPresentation["source"] }
  | { type: "importRequested" }
  | { type: "momentSelected"; momentId: string }
  | { type: "boardSquareSelected"; square: BoardSquare }
  | { type: "boardMoveRequested"; move: BoardMove }
  | {
      type: "promotionRequested"
      move: BoardMove
      role: Extract<PieceRole, "queen" | "rook" | "bishop" | "knight">
    }
  | { type: "alternativeSelected"; alternativeId: string }
  | { type: "strongestReplySelected"; alternativeId: string }
  | {
      type: "alternativeDiscussionRequested"
      alternativeId: string
      message: string
    }
  | { type: "activeWorkCancelled" }
  | { type: "retentionChanged"; enabled: boolean }
  | { type: "retentionDisclosureAcknowledged" }

export type WorkspaceActionHandler = (action: WorkspaceAction) => void
