import * as v from "valibot"

import type { AlternativeMoveId } from "./AlternativeMoveId.js"
import type { ArtifactDigest } from "./ArtifactDigest.js"
import type { BranchParent } from "./BranchParent.js"
import type { BranchRef } from "./BranchRef.js"
import type { CoachTurnId } from "./CoachTurnId.js"
import type { CriticalMomentId } from "./CriticalMomentId.js"
import type { EloRating } from "./EloRating.js"
import type { ExplanationPathRef } from "./ExplanationPathRef.js"
import type { GameImportId } from "./GameImportId.js"
import type { IdempotencyKey } from "./IdempotencyKey.js"
import type { LearningPathFeedbackState } from "./LearningPathFeedbackState.js"
import type { LearningPathRef } from "./LearningPathRef.js"
import type { LearningResourceId } from "./LearningResourceId.js"
import type { MoveInput } from "./MoveInput.js"
import type { MoveSequenceRef } from "./MoveSequenceRef.js"
import type { OperationId } from "./OperationId.js"
import type { PositionRef } from "./PositionRef.js"
import type { ReviewContentDigest } from "./ReviewContentDigest.js"
import type { RequestId } from "./RequestId.js"
import type { Square } from "./Square.js"

export type JsonPrimitive = string | number | boolean | null

export type JsonValue = JsonPrimitive | JsonObject | JsonValue[]

export type JsonObject = { readonly [key: string]: JsonValue }

export const jsonValueSchema: v.GenericSchema<JsonValue> = v.lazy(() =>
  v.union([
    v.string(),
    v.number(),
    v.boolean(),
    v.null(),
    v.array(jsonValueSchema),
    jsonObjectSchema,
  ]),
)

export const jsonObjectSchema: v.GenericSchema<JsonObject> = v.lazy(() =>
  v.record(v.string(), jsonValueSchema),
)

export function parseJsonValue(value: unknown): JsonValue {
  return v.parse(jsonValueSchema, dropUndefined(value))
}

export function readJsonValue(value: unknown): JsonValue | undefined {
  try {
    const parsed = v.safeParse(jsonValueSchema, dropUndefined(value))
    return parsed.success ? parsed.output : undefined
  } catch {
    return undefined
  }
}

export function fromJsonValue(value: JsonValue): JsonValue {
  return parseJsonValue(value)
}

export function parseJsonObject(value: unknown): JsonObject {
  return v.parse(jsonObjectSchema, dropUndefined(value))
}

export function readJsonObject(value: unknown): JsonObject | undefined {
  try {
    const parsed = v.safeParse(jsonObjectSchema, dropUndefined(value))
    return parsed.success ? parsed.output : undefined
  } catch {
    return undefined
  }
}

export function fromJsonObject(value: JsonObject): JsonObject {
  return parseJsonObject(value)
}

const gameImportId = brandedId<GameImportId>(
  "GameImportId",
  prefixedId("game-import:"),
)
export const parseGameImportId = gameImportId.parse
export const fromGameImportId = gameImportId.from
export const readGameImportId = gameImportId.read
export const parseIsGameImportId = gameImportId.parseIs

const criticalMomentId = brandedId<CriticalMomentId>(
  "CriticalMomentId",
  prefixedIds(["review-moment:", "critical-moment:"]),
)
export const parseCriticalMomentId = criticalMomentId.parse
export const fromCriticalMomentId = criticalMomentId.from
export const readCriticalMomentId = criticalMomentId.read
export const parseIsCriticalMomentId = criticalMomentId.parseIs

const operationId = brandedId<OperationId>(
  "OperationId",
  prefixedId("operation:"),
)
export const parseOperationId = operationId.parse
export const fromOperationId = operationId.from
export const readOperationId = operationId.read
export const parseIsOperationId = operationId.parseIs

const requestId = brandedId<RequestId>("RequestId", prefixedId("request:"))
export const parseRequestId = requestId.parse
export const fromRequestId = requestId.from
export const readRequestId = requestId.read
export const parseIsRequestId = requestId.parseIs

const idempotencyKey = brandedId<IdempotencyKey>(
  "IdempotencyKey",
  prefixedIds(["idempotency-key:", "idempotency:"]),
)
export const parseIdempotencyKey = idempotencyKey.parse
export const fromIdempotencyKey = idempotencyKey.from
export const readIdempotencyKey = idempotencyKey.read
export const parseIsIdempotencyKey = idempotencyKey.parseIs

const learningPathRef = brandedId<LearningPathRef>(
  "LearningPathRef",
  prefixedId("learning-path:"),
)
export const parseLearningPathRef = learningPathRef.parse
export const fromLearningPathRef = learningPathRef.from
export const readLearningPathRef = learningPathRef.read
export const parseIsLearningPathRef = learningPathRef.parseIs

const learningResourceId = brandedId<LearningResourceId>(
  "LearningResourceId",
  prefixedId("lichess:"),
)
export const parseLearningResourceId = learningResourceId.parse
export const fromLearningResourceId = learningResourceId.from
export const readLearningResourceId = learningResourceId.read
export const parseIsLearningResourceId = learningResourceId.parseIs

const explanationPathRef = brandedId<ExplanationPathRef>(
  "ExplanationPathRef",
  prefixedId("sha256:"),
)
export const parseExplanationPathRef = explanationPathRef.parse
export const fromExplanationPathRef = explanationPathRef.from
export const readExplanationPathRef = explanationPathRef.read
export const parseIsExplanationPathRef = explanationPathRef.parseIs

const branchRef = brandedId<BranchRef>("BranchRef", prefixedId("branch:"))
export const parseBranchRef = branchRef.parse
export const fromBranchRef = branchRef.from
export const readBranchRef = branchRef.read
export const parseIsBranchRef = branchRef.parseIs

const positionRef = brandedId<PositionRef>(
  "PositionRef",
  v.pipe(v.string(), v.minLength(1)),
)
export const parsePositionRef = positionRef.parse
export const fromPositionRef = positionRef.from
export const readPositionRef = positionRef.read
export const parseIsPositionRef = positionRef.parseIs

const reviewContentDigest = brandedId<ReviewContentDigest>(
  "ReviewContentDigest",
  v.pipe(v.string(), v.regex(/^sha256:[0-9a-f]{64}$/u)),
)
export const parseReviewContentDigest = reviewContentDigest.parse
export const fromReviewContentDigest = reviewContentDigest.from
export const readReviewContentDigest = reviewContentDigest.read
export const parseIsReviewContentDigest = reviewContentDigest.parseIs

const alternativeMoveId = brandedId<AlternativeMoveId>(
  "AlternativeMoveId",
  prefixedId("alternative-move:"),
)
export const parseAlternativeMoveId = alternativeMoveId.parse
export const fromAlternativeMoveId = alternativeMoveId.from
export const readAlternativeMoveId = alternativeMoveId.read
export const parseIsAlternativeMoveId = alternativeMoveId.parseIs

const moveSequenceRef = brandedId<MoveSequenceRef>(
  "MoveSequenceRef",
  v.pipe(
    v.string(),
    v.minLength(1),
    v.maxLength(128),
    v.regex(/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/),
  ),
)
export const parseMoveSequenceRef = moveSequenceRef.parse
export const fromMoveSequenceRef = moveSequenceRef.from
export const readMoveSequenceRef = moveSequenceRef.read
export const parseIsMoveSequenceRef = moveSequenceRef.parseIs

const square = brandedId<Square>(
  "Square",
  v.pipe(v.string(), v.regex(/^[a-h][1-8]$/)),
)
export const parseSquare = square.parse
export const fromSquare = square.from
export const readSquare = square.read
export const parseIsSquare = square.parseIs

const eloRatingSchema = v.pipe(v.number(), v.integer(), v.minValue(100))

export function parseIsEloRating(value: unknown): value is EloRating {
  return v.is(eloRatingSchema, value)
}

export function parseEloRating(value: unknown): EloRating {
  if (!parseIsEloRating(value)) {
    throw new TypeError("invalid EloRating")
  }
  return value
}

export function fromEloRating(value: number): EloRating {
  return parseEloRating(value)
}

export function readEloRating(value: unknown): EloRating | undefined {
  return parseIsEloRating(value) ? value : undefined
}

const coachTurnId = brandedId<CoachTurnId>(
  "CoachTurnId",
  prefixedId("coach-turn:"),
)
export const parseCoachTurnId = coachTurnId.parse
export const fromCoachTurnId = coachTurnId.from
export const readCoachTurnId = coachTurnId.read
export const parseIsCoachTurnId = coachTurnId.parseIs

export function mintCoachTurnId(
  surface: string,
  identity: string,
): CoachTurnId {
  return fromCoachTurnId(`coach-turn:${surface}:${identity}`)
}

const artifactDigest = brandedId<ArtifactDigest>(
  "ArtifactDigest",
  prefixedId("sha256:"),
)
export const parseArtifactDigest = artifactDigest.parse
export const fromArtifactDigest = artifactDigest.from
export const readArtifactDigest = artifactDigest.read
export const parseIsArtifactDigest = artifactDigest.parseIs

export function mintOperationId(
  surface: string,
  identity: string,
): OperationId {
  return fromOperationId(`operation:${surface}:${identity}`)
}

export function mintRequestId(surface: string, identity: string): RequestId {
  return fromRequestId(`request:${surface}:${identity}`)
}

export function mintIdempotencyKey(
  surface: string,
  identity: string,
): IdempotencyKey {
  return fromIdempotencyKey(`idempotency-key:${surface}:${identity}`)
}

const moveInputSchema = v.union([
  v.object({
    kind: v.literal("uci"),
    uci: v.pipe(v.string(), v.minLength(1), v.maxLength(16)),
  }),
  v.object({
    kind: v.literal("san"),
    san: v.pipe(v.string(), v.minLength(1), v.maxLength(32)),
  }),
])

export function parseMoveInput(value: unknown): MoveInput {
  return v.parse(moveInputSchema, value)
}

export function fromMoveInput(value: MoveInput): MoveInput {
  return parseMoveInput(value)
}

export function readMoveInput(value: unknown): MoveInput | undefined {
  const parsed = v.safeParse(moveInputSchema, value)
  return parsed.success ? parsed.output : undefined
}

const branchParentSchema = v.union([
  v.object({
    kind: v.literal("root"),
    positionRef: v.pipe(v.string(), v.minLength(1)),
  }),
  v.object({
    kind: v.literal("move"),
    branchRef: prefixedId("branch:"),
  }),
])

export function parseBranchParent(value: unknown): BranchParent {
  return brandBranchParent(v.parse(branchParentSchema, value))
}

export function fromBranchParent(
  value:
    | { kind: "root"; positionRef: string }
    | { kind: "move"; branchRef: string },
): BranchParent {
  return parseBranchParent(value)
}

export function readBranchParent(value: unknown): BranchParent | undefined {
  const parsed = v.safeParse(branchParentSchema, value)
  return parsed.success ? brandBranchParent(parsed.output) : undefined
}

const learningPathFeedbackStateSchema = v.object({
  currentVote: v.nullable(v.picklist(["thumbsUp", "thumbsDown"] as const)),
  exposedSurfaces: v.array(
    v.picklist(["web", "coachSkill", "coachApp"] as const),
  ),
  learningPathRef: prefixedId("learning-path:"),
})

export function parseLearningPathFeedbackState(
  value: unknown,
): LearningPathFeedbackState {
  const parsed = v.parse(learningPathFeedbackStateSchema, value)
  return {
    currentVote: parsed.currentVote,
    exposedSurfaces: parsed.exposedSurfaces,
    learningPathRef: fromLearningPathRef(parsed.learningPathRef),
  }
}

export function fromLearningPathFeedbackState(
  value: LearningPathFeedbackState,
): LearningPathFeedbackState {
  return parseLearningPathFeedbackState(value)
}

export function readLearningPathFeedbackState(
  value: unknown,
): LearningPathFeedbackState | undefined {
  const parsed = v.safeParse(learningPathFeedbackStateSchema, value)
  return parsed.success
    ? {
        currentVote: parsed.output.currentVote,
        exposedSurfaces: parsed.output.exposedSurfaces,
        learningPathRef: fromLearningPathRef(parsed.output.learningPathRef),
      }
    : undefined
}

function brandBranchParent(
  value:
    | { kind: "root"; positionRef: string }
    | { kind: "move"; branchRef: string },
): BranchParent {
  return value.kind === "root"
    ? { kind: "root", positionRef: fromPositionRef(value.positionRef) }
    : { kind: "move", branchRef: fromBranchRef(value.branchRef) }
}

function prefixedId(prefix: string) {
  return v.pipe(
    v.string(),
    v.minLength(prefix.length + 1),
    v.startsWith(prefix),
  )
}

function prefixedIds(prefixes: readonly [string, ...string[]]) {
  return v.pipe(
    v.string(),
    v.minLength(1),
    v.check((value) => prefixes.some((prefix) => value.startsWith(prefix))),
  )
}

function brandedId<T extends string>(
  name: string,
  schema: v.GenericSchema<string>,
) {
  function parseIs(value: unknown): value is T {
    return v.is(schema, value)
  }
  function parse(value: unknown): T {
    if (!parseIs(value)) {
      throw new TypeError(`invalid ${name}`)
    }
    return value
  }
  function from(value: string): T {
    return parse(value)
  }
  function read(value: unknown): T | undefined {
    return parseIs(value) ? value : undefined
  }
  return { from, parse, parseIs, read, schema }
}

function dropUndefined(value: unknown): unknown {
  if (value === undefined) return undefined
  if (
    typeof value === "function" ||
    typeof value === "symbol" ||
    typeof value === "bigint"
  ) {
    throw new TypeError("JSON value cannot contain function, symbol, or bigint")
  }
  if (value === null || typeof value !== "object") return value
  if (Array.isArray(value)) return value.map(dropUndefined)
  const prototype = Object.getPrototypeOf(value)
  if (prototype !== Object.prototype && prototype !== null) {
    throw new TypeError("JSON object must be a plain object")
  }
  const entries: Array<[string, unknown]> = []
  for (const [key, entry] of Object.entries(value)) {
    const dropped = dropUndefined(entry)
    if (dropped !== undefined) entries.push([key, dropped])
  }
  return Object.fromEntries(entries)
}
