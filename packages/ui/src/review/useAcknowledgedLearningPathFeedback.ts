import { useCallback, useLayoutEffect, useRef, useState } from "react"

export const LEARNING_PLAN_FEEDBACK_UNCONFIRMED =
  "This learning plan vote is unconfirmed. Try again."

type SessionValues<SessionId extends string, PathRef extends string, Value> = {
  gameImportId: SessionId
  values: Partial<Record<PathRef, Value>>
}

export function useAcknowledgedLearningPathFeedback<
  SessionId extends string,
  PathRef extends string,
  Vote,
  Feedback extends { currentVote: Vote | null },
>({
  learningPathRefs,
  recordExposure,
  saveVote,
  gameImportId,
}: {
  learningPathRefs: readonly PathRef[]
  recordExposure: (
    gameImportId: SessionId,
    learningPathRef: PathRef,
  ) => Promise<Feedback | undefined>
  saveVote: (
    gameImportId: SessionId,
    learningPathRef: PathRef,
    vote: Vote | null,
  ) => Promise<Feedback | undefined>
  gameImportId: SessionId | null | undefined
}) {
  const activeSessionId = useRef(gameImportId)
  const exposures = useRef(new Map<string, Promise<Feedback | undefined>>())
  const voteTails = useRef(new Map<string, Promise<void>>())
  const [feedbackState, setFeedbackState] = useState<
    SessionValues<SessionId, PathRef, Feedback> | undefined
  >()
  const [failureState, setFailureState] = useState<
    SessionValues<SessionId, PathRef, string> | undefined
  >()
  const [pendingOperations, setPendingOperations] = useState<
    ReadonlyMap<string, number>
  >(() => new Map())
  const serializedLearningPathRefs = JSON.stringify(learningPathRefs)
  const learningPathRefsRef = useRef(learningPathRefs)
  learningPathRefsRef.current = learningPathRefs

  useLayoutEffect(() => {
    activeSessionId.current = gameImportId
  }, [gameImportId])

  const changePendingOperations = useCallback((key: string, delta: 1 | -1) => {
    setPendingOperations((current) => {
      const next = new Map(current)
      const count = (next.get(key) ?? 0) + delta
      if (count > 0) next.set(key, count)
      else next.delete(key)
      return next
    })
  }, [])

  const storeFeedback = useCallback(
    (
      completedSessionId: SessionId,
      learningPathRef: PathRef,
      feedback: Feedback,
    ) => {
      if (activeSessionId.current !== completedSessionId) return
      setFeedbackState((current) => {
        if (
          current?.gameImportId === completedSessionId &&
          current.values[learningPathRef] === feedback
        ) {
          return current
        }
        const values: Partial<Record<PathRef, Feedback>> =
          current?.gameImportId === completedSessionId
            ? { ...current.values }
            : {}
        values[learningPathRef] = feedback
        return {
          gameImportId: completedSessionId,
          values,
        }
      })
    },
    [],
  )

  const storeFailure = useCallback(
    (completedSessionId: SessionId, learningPathRef: PathRef) => {
      if (activeSessionId.current !== completedSessionId) return
      setFailureState((current) => {
        if (
          current?.gameImportId === completedSessionId &&
          current.values[learningPathRef] === LEARNING_PLAN_FEEDBACK_UNCONFIRMED
        ) {
          return current
        }
        const values: Partial<Record<PathRef, string>> =
          current?.gameImportId === completedSessionId
            ? { ...current.values }
            : {}
        values[learningPathRef] = LEARNING_PLAN_FEEDBACK_UNCONFIRMED
        return {
          gameImportId: completedSessionId,
          values,
        }
      })
    },
    [],
  )

  const clearFailure = useCallback(
    (completedSessionId: SessionId, learningPathRef: PathRef) => {
      setFailureState((current) => {
        if (
          current?.gameImportId !== completedSessionId ||
          current.values[learningPathRef] === undefined
        ) {
          return current
        }
        const values = { ...current.values }
        delete values[learningPathRef]
        return {
          gameImportId: completedSessionId,
          values,
        }
      })
    },
    [],
  )

  const ensureExposure = useCallback(
    (requestedSessionId: SessionId, learningPathRef: PathRef) => {
      const exposureKey = operationKey(
        "exposure",
        requestedSessionId,
        learningPathRef,
      )
      const existing = exposures.current.get(exposureKey)
      if (existing) {
        return existing.then((feedback) => {
          if (feedback !== undefined) {
            storeFeedback(requestedSessionId, learningPathRef, feedback)
          }
          return feedback
        })
      }

      changePendingOperations(exposureKey, 1)
      const exposure = recordExposure(requestedSessionId, learningPathRef)
        .then((feedback) => {
          if (feedback === undefined) {
            exposures.current.delete(exposureKey)
            return undefined
          }
          storeFeedback(requestedSessionId, learningPathRef, feedback)
          return feedback
        })
        .catch(() => {
          exposures.current.delete(exposureKey)
          return undefined
        })
        .finally(() => changePendingOperations(exposureKey, -1))
      exposures.current.set(exposureKey, exposure)
      return exposure
    },
    [changePendingOperations, recordExposure, storeFeedback],
  )

  useLayoutEffect(() => {
    if (!gameImportId) return
    for (const learningPathRef of learningPathRefsRef.current) {
      void ensureExposure(gameImportId, learningPathRef)
    }
    // serializedLearningPathRefs owns list identity so a fresh array literal
    // does not replay exposure and overwrite an acknowledged vote.
  }, [ensureExposure, serializedLearningPathRefs, gameImportId])

  const updateVote = useCallback(
    (learningPathRef: PathRef, vote: Vote | null) => {
      if (!gameImportId) return
      clearFailure(gameImportId, learningPathRef)
      const voteKey = operationKey("vote", gameImportId, learningPathRef)
      changePendingOperations(voteKey, 1)
      const previous = voteTails.current.get(voteKey) ?? Promise.resolve()
      const queued = previous
        .catch(() => undefined)
        .then(async () => {
          if (activeSessionId.current !== gameImportId) return
          if (!(await ensureExposure(gameImportId, learningPathRef))) {
            storeFailure(gameImportId, learningPathRef)
            return
          }
          if (activeSessionId.current !== gameImportId) return
          const feedback = await saveVote(gameImportId, learningPathRef, vote)
          if (activeSessionId.current !== gameImportId) return
          if (feedback === undefined) {
            exposures.current.delete(
              operationKey("exposure", gameImportId, learningPathRef),
            )
            const confirmed = await ensureExposure(
              gameImportId,
              learningPathRef,
            )
            if (confirmed !== undefined && confirmed.currentVote === vote) {
              return
            }
            storeFailure(gameImportId, learningPathRef)
            return
          }
          storeFeedback(gameImportId, learningPathRef, feedback)
          clearFailure(gameImportId, learningPathRef)
        })
        .catch(() => {
          storeFailure(gameImportId, learningPathRef)
        })
      const tracked = queued.finally(() => {
        changePendingOperations(voteKey, -1)
        if (voteTails.current.get(voteKey) === tracked) {
          voteTails.current.delete(voteKey)
        }
      })
      voteTails.current.set(voteKey, tracked)
      return tracked
    },
    [
      changePendingOperations,
      clearFailure,
      ensureExposure,
      saveVote,
      gameImportId,
      storeFailure,
      storeFeedback,
    ],
  )

  const feedback: Partial<Record<PathRef, Feedback>> =
    feedbackState && feedbackState.gameImportId === gameImportId
      ? feedbackState.values
      : {}
  const failures: Partial<Record<PathRef, string>> =
    failureState && failureState.gameImportId === gameImportId
      ? failureState.values
      : {}
  const pending = new Set(
    gameImportId
      ? learningPathRefs.filter(
          (learningPathRef) =>
            pendingOperations.has(
              operationKey("exposure", gameImportId, learningPathRef),
            ) ||
            pendingOperations.has(
              operationKey("vote", gameImportId, learningPathRef),
            ),
        )
      : [],
  )
  const votePending = new Set(
    gameImportId
      ? learningPathRefs.filter((learningPathRef) =>
          pendingOperations.has(
            operationKey("vote", gameImportId, learningPathRef),
          ),
        )
      : [],
  )
  return { failures, feedback, pending, updateVote, votePending }
}

function operationKey(
  kind: "exposure" | "vote",
  gameImportId: string,
  pathRef: string,
) {
  return JSON.stringify([kind, gameImportId, pathRef])
}
