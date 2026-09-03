/**
 * Templates, fixtures, and constants that more than one deployable consumes.
 *
 * `apps/` and `services/` import from here. They do not reach into each
 * other for JSON, PGN, or shared numbers. See `CODING_STANDARDS.md`.
 *
 * Path helpers that need `node:path` live in `./paths`.
 */

import * as v from "valibot"

import groundingSentencesJson from "../grounding/sentences.json"
import limitsJson from "../limits.json"

const groundingSentencesSchema = v.array(v.pipe(v.string(), v.minLength(1)))

const sharedLimitsSchema = v.object({
  commentAuthoringDeadlineSeconds: v.pipe(
    v.number(),
    v.integer(),
    v.minValue(1),
  ),
  hostTurnMaxPriorTurns: v.pipe(v.number(), v.integer(), v.minValue(1)),
})

export type SharedLimits = v.InferOutput<typeof sharedLimitsSchema>

export function parseGroundingSentences(raw: unknown): string[] {
  return v.parse(groundingSentencesSchema, raw)
}

export function parseSharedLimits(raw: unknown): SharedLimits {
  return v.parse(sharedLimitsSchema, raw)
}

export const sharedGroundingSentences = parseGroundingSentences(
  groundingSentencesJson,
)

export const sharedLimits = parseSharedLimits(limitsJson)
