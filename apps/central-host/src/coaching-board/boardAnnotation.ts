import { attacks } from "chessops/attacks"
import { Chess } from "chessops/chess"
import { parseFen } from "chessops/fen"
import { parseSquare, parseUci } from "chessops/util"
import type { Piece, Square as ChessopsSquare } from "chessops/types"

import { fromSquare, type Square } from "@chenchess/coach-engine-sdk"

/**
 * What the coach may point at, and what the page checks before it draws.
 *
 * ADR 0059: the page is the authority on the geometry of the position on
 * screen; Coach Engine remains the sole authority on evaluation. Every kind
 * below is decidable from the FEN the board is rendering, so a relation that
 * is not on the board is refused rather than drawn.
 */
export type BoardBearingRequest = { from: string; label: string; to: string }

export type BoardAnnotationRequest =
  | ({ kind: "attacks" } & BoardBearingRequest)
  | ({ kind: "defends" } & BoardBearingRequest)
  | ({ kind: "controls" } & BoardBearingRequest)
  | { kind: "multiAttack"; from: string; label: string; targets: string[] }
  | { kind: "square"; label: string; square: string }
  | { kind: "move"; label: string; uci: string }

/** A verified mark, in the two shapes a board can draw. */
export type CoachingBoardMark =
  | { from: Square; kind: "arrow"; label: string; to: Square }
  | { kind: "square"; label: string; square: Square }

export type BoardAnnotationRefusalReason =
  | "moveNotGrounded"
  /** The call was not in the closed mark vocabulary at all — a malformed
   * request, distinct from a well-formed claim the position refuses. */
  | "outsideMarkVocabulary"
  | "relationNotOnBoard"
  | "staleRevision"
  | "tooManyMarks"

export type BoardAnnotationOutcome =
  | { kind: "annotated"; marks: readonly CoachingBoardMark[] }
  | { kind: "refused"; reason: BoardAnnotationRefusalReason }

/** What a Player can read at once. The tool description names this number,
 * so the advertised cap and the enforced one cannot drift. */
export const BOARD_ANNOTATION_MARK_LIMIT = 6

/**
 * Verify every requested mark against the position on the board, or refuse.
 *
 * All or nothing: a set of marks is one claim about one position, so a single
 * relation that is not there refuses the call rather than drawing a partial
 * argument the Player would read as complete.
 *
 * `groundedMoveUcis` are the moves ChenChess has already put on this board —
 * a branch move, the active branch's strongest reply, the shown line's move.
 * A `move` mark may name one of those and nothing else.
 */
export function verifyBoardAnnotation({
  fen,
  groundedMoveUcis,
  requests,
}: {
  fen: string
  groundedMoveUcis: ReadonlySet<string>
  requests: readonly BoardAnnotationRequest[]
}): BoardAnnotationOutcome {
  // The tool schema advertises the square and UCI shapes to the agent, and
  // this settles them again: the schema is documentation the model reads,
  // this is what makes the function total for any caller.
  const position = positionFromFen(fen)
  const marks: CoachingBoardMark[] = []
  for (const request of requests) {
    const verified = verifyOne(position, groundedMoveUcis, request)
    if (verified.kind === "refused") return verified
    marks.push(...verified.marks)
  }
  // Counted after verifying, not before: one multiAttack request draws one
  // arrow per target, and it is drawn marks that crowd the board and ride on
  // every later board-tool snapshot.
  if (marks.length > BOARD_ANNOTATION_MARK_LIMIT) {
    return { kind: "refused", reason: "tooManyMarks" }
  }
  return { kind: "annotated", marks }
}

function verifyOne(
  position: Chess,
  groundedMoveUcis: ReadonlySet<string>,
  request: BoardAnnotationRequest,
): BoardAnnotationOutcome {
  switch (request.kind) {
    case "attacks":
      return relationMark(position, request, "enemy")
    case "defends":
      return relationMark(position, request, "friendly")
    case "controls":
      return controlsMark(position, request)
    case "multiAttack":
      return multiAttackMark(position, request)
    case "square":
      return squareMark(request)
    case "move":
      return moveMark(position, groundedMoveUcis, request)
    default: {
      const _exhaustive: never = request
      return _exhaustive
    }
  }
}

/**
 * One piece bearing on one occupied square.
 *
 * Attack and defence are the same geometry read against a different occupant,
 * so they are the same check with the occupant's colour as the argument.
 */
function relationMark(
  position: Chess,
  request: BoardBearingRequest,
  occupant: "enemy" | "friendly",
): BoardAnnotationOutcome {
  const from = parseSquare(request.from)
  const to = parseSquare(request.to)
  if (from === undefined || to === undefined) {
    return refuseRelation()
  }
  const attacker = position.board.get(from)
  const target = position.board.get(to)
  if (!attacker || !target) return refuseRelation()
  const wanted =
    occupant === "enemy" ? opposite(attacker.color) : attacker.color
  if (target.color !== wanted) return refuseRelation()
  if (!bears(position, attacker, from, to)) return refuseRelation()
  return arrow(request.from, request.to, request.label)
}

/**
 * A slider bearing along a line onto a square it can reach.
 *
 * `attacks` is occupancy-aware — it stops at the first blocker — so reaching
 * the square is the whole check; the target may be empty, which is what
 * distinguishes owning a file from attacking a piece.
 */
function controlsMark(
  position: Chess,
  request: BoardBearingRequest,
): BoardAnnotationOutcome {
  const from = parseSquare(request.from)
  const to = parseSquare(request.to)
  if (from === undefined || to === undefined) return refuseRelation()
  const piece = position.board.get(from)
  if (!piece || !isSlider(piece)) return refuseRelation()
  if (!bears(position, piece, from, to)) return refuseRelation()
  return arrow(request.from, request.to, request.label)
}

/**
 * One piece bearing on two or more enemy pieces at once.
 *
 * Named for what is checked. Geometry proves the piece hits both; it cannot
 * prove the fork is worth having, and the word "fork" therefore belongs in
 * the label, which the constraints block governs like any other prose.
 */
function multiAttackMark(
  position: Chess,
  request: { from: string; label: string; targets: string[] },
): BoardAnnotationOutcome {
  const from = parseSquare(request.from)
  if (from === undefined || request.targets.length < 2) return refuseRelation()
  const attacker = position.board.get(from)
  if (!attacker) return refuseRelation()
  const marks: CoachingBoardMark[] = []
  for (const target of request.targets) {
    const to = parseSquare(target)
    if (to === undefined) return refuseRelation()
    const occupant = position.board.get(to)
    if (!occupant || occupant.color === attacker.color) return refuseRelation()
    if (!bears(position, attacker, from, to)) return refuseRelation()
    marks.push({
      from: fromSquare(request.from),
      kind: "arrow",
      label: request.label,
      to: fromSquare(target),
    })
  }
  return { kind: "annotated", marks }
}

/** A bare highlight. It asserts no chess relation, only that the square is
 * one of the sixty-four. */
function squareMark(request: {
  label: string
  square: string
}): BoardAnnotationOutcome {
  if (parseSquare(request.square) === undefined) return refuseRelation()
  return {
    kind: "annotated",
    marks: [
      {
        kind: "square",
        label: request.label,
        square: fromSquare(request.square),
      },
    ],
  }
}

/**
 * A move arrow. The only mark that asserts a move, so it is the only one
 * grounding rather than geometry decides: the move must be one ChenChess put
 * on this board, and it must still be legal in the position on screen.
 */
function moveMark(
  position: Chess,
  groundedMoveUcis: ReadonlySet<string>,
  request: { label: string; uci: string },
): BoardAnnotationOutcome {
  if (!groundedMoveUcis.has(request.uci)) {
    return { kind: "refused", reason: "moveNotGrounded" }
  }
  const move = parseUci(request.uci)
  if (!move || !position.isLegal(move)) {
    return { kind: "refused", reason: "moveNotGrounded" }
  }
  return arrow(request.uci.slice(0, 2), request.uci.slice(2, 4), request.label)
}

function bears(
  position: Chess,
  piece: Piece,
  from: ChessopsSquare,
  to: ChessopsSquare,
): boolean {
  return attacks(piece, from, position.board.occupied).has(to)
}

function isSlider(piece: Piece): boolean {
  return (
    piece.role === "bishop" || piece.role === "rook" || piece.role === "queen"
  )
}

function opposite(color: Piece["color"]): Piece["color"] {
  return color === "white" ? "black" : "white"
}

function arrow(
  from: string,
  to: string,
  label: string,
): BoardAnnotationOutcome {
  return {
    kind: "annotated",
    marks: [
      { from: fromSquare(from), kind: "arrow", label, to: fromSquare(to) },
    ],
  }
}

function refuseRelation(): BoardAnnotationOutcome {
  return { kind: "refused", reason: "relationNotOnBoard" }
}

function positionFromFen(fen: string): Chess {
  const setup = parseFen(fen)
  if (setup.isErr) throw new TypeError("the board FEN does not parse")
  const position = Chess.fromSetup(setup.value)
  if (position.isErr) throw new TypeError("the board FEN is not a position")
  return position.value
}
