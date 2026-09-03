import {
  CoachEngineClient,
  type ReviewFeedbackReason,
} from "@chenchess/coach-engine-sdk"
import { useCallback, useMemo, useRef, useState } from "react"

type FetchAccessToken = (options: {
  forceRefreshToken: boolean
}) => Promise<string | null>

export type ReviewFeedbackVote = "thumbsDown" | "thumbsUp"

export type ReviewFeedbackState = {
  failure: string | null
  pending: boolean
  vote: ReviewFeedbackVote | null
}

const unrated: ReviewFeedbackState = {
  failure: null,
  pending: false,
  vote: null,
}

/**
 * Review Feedback is written per Review Moment, so the recorded vote is keyed
 * by the moment it was cast on. One workspace-wide vote would carry a pressed
 * thumb and a "Recorded" status onto the next moment's Coach comment, which the
 * Player never rated.
 */
export function useReviewFeedback(fetchAccessToken: FetchAccessToken) {
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
  const [byTarget, setByTarget] = useState<
    Readonly<Record<string, ReviewFeedbackState>>
  >({})
  /** The write is what must not double, and `pending` settles a render late. */
  const inFlight = useRef<Set<string>>(new Set())

  const submit = useCallback(
    async (target: string, vote: ReviewFeedbackVote) => {
      if (inFlight.current.has(target)) return
      inFlight.current.add(target)
      setByTarget((current) => ({
        ...current,
        [target]: {
          failure: null,
          pending: true,
          vote: current[target]?.vote ?? null,
        },
      }))
      try {
        await client.recordReviewFeedback(reviewFeedbackReasonCodes(vote))
        setByTarget((current) => ({
          ...current,
          [target]: { failure: null, pending: false, vote },
        }))
      } catch (caught) {
        const failure = parseErrorMessage(caught)
        setByTarget((current) => ({
          ...current,
          [target]: {
            failure,
            pending: false,
            vote: current[target]?.vote ?? null,
          },
        }))
      } finally {
        inFlight.current.delete(target)
      }
    },
    [client],
  )

  const stateFor = useCallback(
    (target: string): ReviewFeedbackState => byTarget[target] ?? unrated,
    [byTarget],
  )

  return { stateFor, submit }
}

function reviewFeedbackReasonCodes(
  vote: ReviewFeedbackVote,
): ReviewFeedbackReason[] {
  switch (vote) {
    case "thumbsDown":
      return ["explanationNotHelpful"]
    case "thumbsUp":
      return ["explanationHelpful"]
    default: {
      const exhaustive: never = vote
      return exhaustive
    }
  }
}

function parseErrorMessage(error: unknown): string {
  return error instanceof Error
    ? error.message
    : "Review feedback is unavailable."
}
