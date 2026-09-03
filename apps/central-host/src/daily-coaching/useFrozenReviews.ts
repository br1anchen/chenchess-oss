import type { GameImportId, GameReview } from "@chenchess/coach-engine-sdk"
import { useCallback, useRef, useState } from "react"

import type { FrozenReviewState } from "./ReviewedGameCard"

/**
 * One frozen Game Review fetch per Game, shared by the digest, Imported
 * Games, and Recent Games surfaces: requests dedupe while in flight, and
 * `reset` invalidates every load started before it so a stale response can
 * never land in a fresh map.
 */
export function useFrozenReviews(
  read: (gameImportId: GameImportId) => Promise<GameReview>,
) {
  const [reviews, setReviews] = useState<
    ReadonlyMap<GameImportId, FrozenReviewState>
  >(new Map())
  const requested = useRef(new Set<GameImportId>())
  const generation = useRef(0)

  const reset = useCallback(() => {
    generation.current += 1
    requested.current.clear()
    setReviews(new Map())
  }, [])

  const loadReview = useCallback(
    async (gameImportId: GameImportId) => {
      if (requested.current.has(gameImportId)) return
      const request = generation.current
      requested.current.add(gameImportId)
      setReviews((current) =>
        new Map(current).set(gameImportId, { kind: "loading" }),
      )
      try {
        const review = await read(gameImportId)
        if (request !== generation.current) return
        setReviews((current) =>
          new Map(current).set(gameImportId, { kind: "ready", review }),
        )
      } catch (caught) {
        requested.current.delete(gameImportId)
        if (request !== generation.current) return
        setReviews((current) =>
          new Map(current).set(gameImportId, {
            kind: "failed",
            message:
              caught instanceof Error
                ? caught.message
                : "This review could not be opened.",
          }),
        )
      }
    },
    [read],
  )

  return { loadReview, reset, reviews }
}
