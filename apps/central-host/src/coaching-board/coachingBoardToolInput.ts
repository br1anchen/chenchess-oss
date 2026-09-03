import * as v from "valibot"
import {
  fromAlternativeMoveId,
  parseIsAlternativeMoveId,
  parseIsGameImportId,
  type GameImportId,
} from "@chenchess/coach-engine-sdk"

import { BOARD_ANNOTATION_MARK_LIMIT } from "./boardAnnotation"
import type { CoachingBoardPositionTarget } from "./coachingBoardDrive"
import { COACHING_BOARD_STEP_DIRECTIONS } from "./coachingBoardLinePlayback"
import {
  COACHING_BOARD_ORIENTATIONS,
  type CoachingBoardOrientation,
} from "./coachingBoardSnapshot"
import { parseOpeningLineRef, type OpeningLineRef } from "./openingLineRef"

/**
 * The wire boundary for every Coaching Board tool call.
 *
 * Agent-supplied arguments arrive as `unknown` and become typed drive inputs
 * exactly here, once. Keeping the schemas out of the drive leaves that module
 * a state machine over already-trusted values — it no longer imports valibot
 * at all — and gives the shapes the tools advertise a single home to be read
 * in (`useCoachingBoardTools` builds the published JSON Schema from these).
 */

const alternativeMoveIdSchema = v.pipe(
  v.string(),
  v.check((value) => parseIsAlternativeMoveId(value)),
  v.transform((value) => fromAlternativeMoveId(value)),
)

export const gameImportIdSchema = v.pipe(
  v.string(),
  v.check((value) => parseIsGameImportId(value)),
  // SAFETY: the check above established the identifier shape; the brand
  // marks the same string.
  v.transform((value) => value as GameImportId),
)

export const showLineSchema = v.variant("kind", [
  v.strictObject({ kind: v.literal("engineBest") }),
  v.strictObject({ kind: v.literal("playedMoveRefutation") }),
  v.strictObject({
    alternativeMoveId: alternativeMoveIdSchema,
    kind: v.literal("alternativeMove"),
  }),
])

export const setPositionSchema = v.variant("kind", [
  v.strictObject({
    kind: v.literal("ply"),
    ply: v.pipe(v.number(), v.integer(), v.minValue(1)),
  }),
  v.strictObject({
    alternativeMoveId: alternativeMoveIdSchema,
    kind: v.literal("alternativeMove"),
  }),
  v.strictObject({
    kind: v.literal("orientation"),
    orientation: v.picklist(COACHING_BOARD_ORIENTATIONS),
  }),
  v.strictObject({
    gameImportId: gameImportIdSchema,
    kind: v.literal("game"),
  }),
  v.strictObject({
    kind: v.literal("openingLine"),
    openingLineRef: v.pipe(
      v.string(),
      v.check((value) => parseOpeningLineRef(value) !== undefined),
      // SAFETY: the check above established the address pattern; the brand
      // marks the same string.
      v.transform((value) => value as OpeningLineRef),
    ),
  }),
])

const boardSquareSchema = v.pipe(v.string(), v.regex(/^[a-h][1-8]$/))
const markLabelSchema = v.pipe(v.string(), v.minLength(1), v.maxLength(24))
const bearingSquares = {
  from: boardSquareSchema,
  label: markLabelSchema,
  to: boardSquareSchema,
}

export const annotateBoardSchema = v.strictObject({
  // The request cap. One multiAttack request still draws several marks, so
  // the verifier caps the drawn marks by the same constant afterwards.
  marks: v.pipe(
    v.array(
      v.variant("kind", [
        v.strictObject({ ...bearingSquares, kind: v.literal("attacks") }),
        v.strictObject({ ...bearingSquares, kind: v.literal("defends") }),
        v.strictObject({ ...bearingSquares, kind: v.literal("controls") }),
        v.strictObject({
          from: boardSquareSchema,
          kind: v.literal("multiAttack"),
          label: markLabelSchema,
          targets: v.pipe(
            v.array(boardSquareSchema),
            v.minLength(2),
            v.maxLength(4),
          ),
        }),
        v.strictObject({
          kind: v.literal("square"),
          label: markLabelSchema,
          square: boardSquareSchema,
        }),
        v.strictObject({
          kind: v.literal("move"),
          label: markLabelSchema,
          uci: v.pipe(v.string(), v.regex(/^[a-h][1-8][a-h][1-8][qrbn]?$/)),
        }),
      ]),
    ),
    v.minLength(1),
    v.maxLength(BOARD_ANNOTATION_MARK_LIMIT),
  ),
  revision: v.pipe(v.number(), v.integer(), v.minValue(1)),
})

export function parseAnnotateBoard(args: unknown) {
  const parsed = v.safeParse(annotateBoardSchema, args)
  return parsed.success
    ? ({
        kind: "ok",
        request: {
          requests: parsed.output.marks,
          revision: parsed.output.revision,
        },
      } as const)
    : ({ kind: "refused", reason: "outsideMarkVocabulary" } as const)
}

export const stepLineSchema = v.strictObject({
  to: v.union([
    v.pipe(v.number(), v.integer(), v.minValue(0)),
    v.picklist(COACHING_BOARD_STEP_DIRECTIONS),
  ]),
})

export function parseStepLine(args: unknown) {
  const parsed = v.safeParse(stepLineSchema, args)
  return parsed.success
    ? ({ kind: "ok", target: parsed.output.to } as const)
    : ({ kind: "refused", reason: "outsideStepVocabulary" } as const)
}

export function parseShowLine(args: unknown) {
  const parsed = v.safeParse(showLineSchema, args)
  return parsed.success
    ? ({ kind: "ok", line: parsed.output } as const)
    : ({ kind: "refused", reason: "outsideClosedLineUnion" } as const)
}

/**
 * Everything `set_board_position` accepts, which is more than the drive's
 * positions: an Opening Line and a reviewed Game are navigation to another
 * board, and an orientation is not a position at all. The tool layer dispatches on this and
 * hands the drive only what it owns.
 */
export type CoachingBoardToolTarget =
  | CoachingBoardPositionTarget
  | { kind: "game"; gameImportId: GameImportId }
  | { kind: "openingLine"; openingLineRef: OpeningLineRef }
  | { kind: "orientation"; orientation: CoachingBoardOrientation }

/**
 * A call the schema rejects is refused for its shape, not for its position:
 * `outsideTargetVocabulary` tells the agent to fix the call, where
 * `unreachablePosition` would tell it the ply does not exist — a claim the
 * board never checked.
 */
export function parseSetPosition(
  args: unknown,
):
  | { kind: "ok"; target: CoachingBoardToolTarget }
  | { kind: "refused"; reason: "outsideTargetVocabulary" } {
  const parsed = v.safeParse(setPositionSchema, args)
  return parsed.success
    ? { kind: "ok", target: parsed.output }
    : { kind: "refused", reason: "outsideTargetVocabulary" }
}

/**
 * A transition that moves the board.
 *
 * It advances the page revision and drops the marks with it: a mark belongs
 * to the position it was drawn on, so nothing the coach drew can survive onto
 * a different one (ADR 0059). Every mover goes through here so the next one
 * added cannot forget.
 */
