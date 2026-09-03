import {
  type IdempotencyKey,
  type OperationCompletion,
  type ReviewSessionPresentation,
  type ReviewSessionPresentationAddition,
} from "@chenchess/coach-engine-sdk"
import {
  projectReviewSessionPresentation as computeReviewSessionPresentation,
  projectReviewSessionPresentationAddition as computeReviewSessionPresentationAddition,
} from "@chenchess/review-projection"
import { observeCoachCacheLookup } from "./review-session-telemetry.js"

type SessionCompletion = Extract<
  OperationCompletion,
  { kind: "reviewSessionStarted" }
>

type MomentCompletion = Extract<
  OperationCompletion,
  { kind: "reviewMomentOpened" }
>

export function projectReviewSessionPresentation(
  result: SessionCompletion,
  idempotencyKey: IdempotencyKey,
): ReviewSessionPresentation {
  const key = JSON.stringify([result.gameImportId, result.sessionRevision])
  const cached = presentationCache.get(key)
  observeCoachCacheLookup(
    "review_session_presentation",
    cached !== undefined,
    presentationCache.size,
  )
  if (cached) return bindIdempotencyKey(cached, idempotencyKey)

  const presentation = computeReviewSessionPresentation(result, idempotencyKey)
  presentationCache.set(key, presentation)
  return bindIdempotencyKey(presentation, idempotencyKey)
}

export function projectReviewSessionPresentationAddition(
  result: MomentCompletion,
  idempotencyKey: IdempotencyKey,
): ReviewSessionPresentationAddition {
  const key = JSON.stringify([
    result.gameImportId,
    result.sessionRevision,
    result.reviewMoment.reviewMoment.momentId,
  ])
  const cached = presentationAdditionCache.get(key)
  observeCoachCacheLookup(
    "review_session_presentation_addition",
    cached !== undefined,
    presentationAdditionCache.size,
  )
  if (cached) return bindAdditionIdempotencyKey(cached, idempotencyKey)

  const addition = computeReviewSessionPresentationAddition(
    result,
    idempotencyKey,
  )
  presentationAdditionCache.set(key, addition)
  return bindAdditionIdempotencyKey(addition, idempotencyKey)
}

function bindIdempotencyKey(
  cached: ReviewSessionPresentation,
  idempotencyKey: IdempotencyKey,
) {
  const presentation = structuredClone(cached)
  for (const moment of presentation.moments) {
    moment.handoff.idempotencyKey = idempotencyKey
  }
  return presentation
}

function bindAdditionIdempotencyKey(
  cached: ReviewSessionPresentationAddition,
  idempotencyKey: IdempotencyKey,
) {
  const addition = structuredClone(cached)
  addition.moment.handoff.idempotencyKey = idempotencyKey
  return addition
}

class BoundedCache<T> {
  readonly #entries = new Map<string, T>()

  constructor(readonly maximumEntries: number) {}

  get size() {
    return this.#entries.size
  }

  get(key: string) {
    const value = this.#entries.get(key)
    if (value === undefined) return undefined
    this.#entries.delete(key)
    this.#entries.set(key, value)
    return value
  }

  set(key: string, value: T) {
    this.#entries.delete(key)
    this.#entries.set(key, value)
    while (this.#entries.size > this.maximumEntries) {
      const oldest = this.#entries.keys().next().value
      if (oldest === undefined) break
      this.#entries.delete(oldest)
    }
  }

  clear() {
    this.#entries.clear()
  }
}

const presentationCache = new BoundedCache<ReviewSessionPresentation>(128)
const presentationAdditionCache =
  new BoundedCache<ReviewSessionPresentationAddition>(256)

/** @internal Test isolation for process-level projection caches. */
export function resetReviewSessionPresentationCaches() {
  presentationCache.clear()
  presentationAdditionCache.clear()
}
