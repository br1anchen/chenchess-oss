import {
  type ArtifactRetentionPreference,
  CoachEngineClient,
} from "@chenchess/coach-engine-sdk"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"

type FetchAccessToken = (options: {
  forceRefreshToken: boolean
}) => Promise<string | null>

export type ReviewRetentionPreference = ArtifactRetentionPreference

const unavailablePreference: ReviewRetentionPreference = {
  available: false,
  deletedReviewSnapshots: 0,
  enabled: false,
  disclosureRequired: false,
}

export function useReviewRetentionPreference(
  fetchAccessToken: FetchAccessToken,
  { reportsInitialRead = true }: { reportsInitialRead?: boolean } = {},
) {
  const client = useMemo(
    () =>
      new CoachEngineClient({
        credential: async () => {
          const token = await fetchAccessToken({ forceRefreshToken: true })
          if (!token) throw new Error("Authentication expired. Sign in again.")
          return token
        },
      }),
    [fetchAccessToken],
  )
  const [preference, setPreference] = useState(unavailablePreference)
  const [failure, setFailure] = useState<string | null>(null)
  const [resolving, setResolving] = useState(false)
  const preferenceRef = useRef<ReviewRetentionPreference | null>(null)
  const loadingRef = useRef<Promise<ReviewRetentionPreference> | null>(null)

  const load = useCallback((): Promise<ReviewRetentionPreference> => {
    if (preferenceRef.current) return Promise.resolve(preferenceRef.current)
    if (loadingRef.current) return loadingRef.current
    const pending = client
      .artifactRetentionPreference()
      .then((loaded) => {
        preferenceRef.current = loaded
        setPreference(loaded)
        setFailure(null)
        return loaded
      })
      .finally(() => {
        loadingRef.current = null
      })
    loadingRef.current = pending
    return pending
  }, [client])

  /* Account Settings mounts this hook because the Player opened the preference,
     so its first read is one they asked for and its failure is theirs to see.
     The Coaching Board mounts it only to learn whether a disclosure is needed;
     a failure there — including a token that has not resolved yet — paints a
     false "Authentication expired" over a page nobody acted on. The
     Player-initiated paths below always report, and both retry the load. */
  useEffect(() => {
    let active = true
    void load().catch((caught) => {
      if (active && reportsInitialRead) setFailure(parseErrorMessage(caught))
    })
    return () => {
      active = false
    }
  }, [load, reportsInitialRead])

  const resolveBeforeReview = useCallback(async (): Promise<boolean> => {
    setResolving(true)
    try {
      const current = await load()
      if (!current.available || !current.disclosureRequired) return true
      const resolved = await client.setArtifactRetentionPreference(
        current.enabled,
      )
      preferenceRef.current = resolved
      setPreference(resolved)
      setFailure(null)
      return true
    } catch (caught) {
      setFailure(parseErrorMessage(caught))
      return false
    } finally {
      setResolving(false)
    }
  }, [client, load])

  const updateEnabled = useCallback(
    async (enabled: boolean): Promise<boolean> => {
      setResolving(true)
      try {
        const resolved = await client.setArtifactRetentionPreference(enabled)
        preferenceRef.current = resolved
        setPreference(resolved)
        setFailure(null)
        return true
      } catch (caught) {
        setFailure(parseErrorMessage(caught))
        return false
      } finally {
        setResolving(false)
      }
    },
    [client],
  )

  return {
    ...preference,
    failure,
    resolving,
    resolveBeforeReview,
    updateEnabled,
  }
}

function parseErrorMessage(error: unknown): string {
  return error instanceof Error
    ? error.message
    : "Review retention preference is unavailable."
}
