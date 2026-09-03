import { useCallback, useEffect, useState } from "react"
import type {
  CoachEngineClient,
  GameImportId,
  ReviewedGameSearchCard,
} from "@chenchess/coach-engine-sdk"

import type { FetchAccessToken } from "@/auth/FirebaseAuthProvider"

import { readFrozenGameReview } from "./ReviewedGameCard"

const RECENT_GAMES_LIMIT = 10
const RECENT_GAMES_RETRY_MS = 15_000
const RECENT_GAMES_RETRIES = 2

/**
 * The Player's most recently reviewed Games, and the previews their tiles
 * draw.
 *
 * Both carousels — the dashboard's and the Coaching Board lobby's — read the
 * same list with the same limit, so a transient search failure empties
 * neither for the whole session: it takes two spaced retries, then settles.
 */
export function useRecentReviewedGames({
  client,
  enabled = true,
  fetchAccessToken,
}: {
  client: CoachEngineClient
  enabled?: boolean
  fetchAccessToken: FetchAccessToken
}) {
  const [games, setGames] = useState<readonly ReviewedGameSearchCard[]>([])
  const [attempt, setAttempt] = useState(0)

  const loadPreview = useCallback(
    (gameImportId: GameImportId) =>
      readFrozenGameReview(gameImportId, fetchAccessToken),
    [fetchAccessToken],
  )

  useEffect(() => {
    if (!enabled) {
      setGames([])
      return
    }
    let active = true
    let retry: ReturnType<typeof setTimeout> | undefined
    void client.searchReviewedGames({}).then(
      (result) => {
        if (active) setGames(result.games.slice(0, RECENT_GAMES_LIMIT))
      },
      () => {
        if (!active) return
        setGames([])
        if (attempt < RECENT_GAMES_RETRIES) {
          retry = setTimeout(
            () => setAttempt((current) => current + 1),
            RECENT_GAMES_RETRY_MS,
          )
        }
      },
    )
    return () => {
      active = false
      if (retry !== undefined) clearTimeout(retry)
    }
  }, [attempt, client, enabled])

  return { games, loadPreview }
}
