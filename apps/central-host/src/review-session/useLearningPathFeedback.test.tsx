// @vitest-environment jsdom

import { act, renderHook, waitFor } from "@testing-library/react"
import { StrictMode } from "react"
import { expect, test, vi } from "vitest"

import {
  fromGameImportId,
  fromLearningPathRef,
  type LearningPathRef,
} from "@chenchess/coach-engine-sdk"
import { LEARNING_PLAN_FEEDBACK_UNCONFIRMED } from "@chenchess/ui/review/learning-path-feedback"

import type { RunIndependentReviewSessionCommand } from "./useReviewSessionCommands"
import { useLearningPathFeedback } from "./useLearningPathFeedback"

test("records one idempotent exposure and preserves cross-surface vote state", async () => {
  const gameImportId = fromGameImportId("game-import:feedback")
  const learningPathRef = fromLearningPathRef("learning-path:feedback")
  const run = vi.fn<RunIndependentReviewSessionCommand>().mockResolvedValue({
    kind: "learningPathFeedbackRecorded",
    feedback: {
      currentVote: "thumbsDown",
      exposedSurfaces: ["coachApp", "web"],
      learningPathRef,
    },
  })
  const refs = [learningPathRef]

  const { result, rerender } = renderHook(
    () => useLearningPathFeedback(gameImportId, refs, run),
    { wrapper: StrictMode },
  )

  await waitFor(() =>
    expect(result.current.feedback[learningPathRef]?.currentVote).toBe(
      "thumbsDown",
    ),
  )
  expect(run).toHaveBeenCalledWith(
    {
      kind: "recordLearningPathExposure",
      learningPathRef,
      gameImportId,
    },
    "Recording Learning Path delivery…",
  )
  rerender()
  expect(run).toHaveBeenCalledTimes(1)
})

test("updates and removes the current structured vote", async () => {
  const gameImportId = fromGameImportId("game-import:vote")
  const learningPathRef = fromLearningPathRef("learning-path:vote")
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockResolvedValueOnce({
      kind: "learningPathFeedbackRecorded",
      feedback: {
        currentVote: null,
        exposedSurfaces: ["web"],
        learningPathRef,
      },
    })
    .mockResolvedValueOnce({
      kind: "learningPathFeedbackRecorded",
      feedback: {
        currentVote: "thumbsUp",
        exposedSurfaces: ["web"],
        learningPathRef,
      },
    })
    .mockResolvedValueOnce({
      kind: "learningPathFeedbackRecorded",
      feedback: {
        currentVote: null,
        exposedSurfaces: ["web"],
        learningPathRef,
      },
    })
  const { result } = renderHook(() =>
    useLearningPathFeedback(gameImportId, [learningPathRef], run),
  )
  await waitFor(() => expect(run).toHaveBeenCalledTimes(1))

  await act(() => result.current.updateVote(learningPathRef, "thumbsUp"))
  expect(result.current.feedback[learningPathRef]?.currentVote).toBe("thumbsUp")
  await act(() => result.current.updateVote(learningPathRef, null))
  expect(result.current.feedback[learningPathRef]?.currentVote).toBeNull()
})

test("queues a vote until exposure has been acknowledged", async () => {
  const gameImportId = fromGameImportId("game-import:queued-vote")
  const learningPathRef = fromLearningPathRef("learning-path:queued-vote")
  const exposure =
    deferred<Awaited<ReturnType<RunIndependentReviewSessionCommand>>>()
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockImplementationOnce(() => exposure.promise)
    .mockResolvedValueOnce(feedbackCompletion(learningPathRef, "thumbsUp"))
  const { result } = renderHook(() =>
    useLearningPathFeedback(gameImportId, [learningPathRef], run),
  )

  expect(result.current.pending.has(learningPathRef)).toBe(true)
  let voteCompletion: Promise<void> | undefined
  act(() => {
    voteCompletion = result.current.updateVote(learningPathRef, "thumbsUp")
  })
  expect(run).toHaveBeenCalledTimes(1)

  await act(async () => {
    exposure.resolve(feedbackCompletion(learningPathRef, null))
    await voteCompletion
  })
  expect(run).toHaveBeenNthCalledWith(
    2,
    {
      kind: "updateLearningPathVote",
      learningPathRef,
      gameImportId,
      vote: "thumbsUp",
    },
    "Saving Learning Path feedback…",
  )
  expect(result.current.feedback[learningPathRef]?.currentVote).toBe("thumbsUp")
})

test("discards an exposure completion from a previous session", async () => {
  const firstSession = fromGameImportId("game-import:first")
  const secondSession = fromGameImportId("game-import:second")
  const learningPathRef = fromLearningPathRef("learning-path:shared")
  const first =
    deferred<Awaited<ReturnType<RunIndependentReviewSessionCommand>>>()
  const second =
    deferred<Awaited<ReturnType<RunIndependentReviewSessionCommand>>>()
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockImplementationOnce(() => first.promise)
    .mockImplementationOnce(() => second.promise)
  const { result, rerender } = renderHook(
    ({ gameImportId }) =>
      useLearningPathFeedback(gameImportId, [learningPathRef], run),
    { initialProps: { gameImportId: firstSession } },
  )
  await waitFor(() => expect(run).toHaveBeenCalledTimes(1))

  rerender({ gameImportId: secondSession })
  await waitFor(() => expect(run).toHaveBeenCalledTimes(2))
  await act(async () => {
    first.resolve(feedbackCompletion(learningPathRef, "thumbsDown"))
    await first.promise
  })
  expect(result.current.feedback[learningPathRef]).toBeUndefined()

  await act(async () => {
    second.resolve(feedbackCompletion(learningPathRef, "thumbsUp"))
    await second.promise
  })
  expect(result.current.feedback[learningPathRef]?.currentVote).toBe("thumbsUp")
})

test("records every rendered path without waiting for an earlier exposure", async () => {
  const gameImportId = fromGameImportId("game-import:parallel-exposures")
  const firstRef = fromLearningPathRef("learning-path:first")
  const secondRef = fromLearningPathRef("learning-path:second")
  const first =
    deferred<Awaited<ReturnType<RunIndependentReviewSessionCommand>>>()
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockImplementationOnce(() => first.promise)
    .mockResolvedValueOnce(feedbackCompletion(secondRef, null))

  const { result } = renderHook(() =>
    useLearningPathFeedback(gameImportId, [firstRef, secondRef], run),
  )
  expect(run).toHaveBeenCalledTimes(2)
  expect(result.current.pending).toEqual(new Set([firstRef, secondRef]))
  expect(run).toHaveBeenNthCalledWith(
    2,
    {
      kind: "recordLearningPathExposure",
      learningPathRef: secondRef,
      gameImportId,
    },
    "Recording Learning Path delivery…",
  )

  first.resolve(feedbackCompletion(firstRef, null))
  await first.promise
})

test("serializes votes per path and keeps the newest acknowledged vote", async () => {
  const gameImportId = fromGameImportId("game-import:serial-votes")
  const learningPathRef = fromLearningPathRef("learning-path:serial-votes")
  const firstVote =
    deferred<Awaited<ReturnType<RunIndependentReviewSessionCommand>>>()
  const secondVote =
    deferred<Awaited<ReturnType<RunIndependentReviewSessionCommand>>>()
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockResolvedValueOnce(feedbackCompletion(learningPathRef, null))
    .mockImplementationOnce(() => firstVote.promise)
    .mockImplementationOnce(() => secondVote.promise)
  const { result } = renderHook(() =>
    useLearningPathFeedback(gameImportId, [learningPathRef], run),
  )
  await waitFor(() => expect(run).toHaveBeenCalledTimes(1))

  let older: Promise<void> | undefined
  let newer: Promise<void> | undefined
  act(() => {
    older = result.current.updateVote(learningPathRef, "thumbsDown")
    newer = result.current.updateVote(learningPathRef, "thumbsUp")
  })
  await waitFor(() => expect(run).toHaveBeenCalledTimes(2))
  expect(result.current.pending.has(learningPathRef)).toBe(true)

  await act(async () => {
    firstVote.resolve(feedbackCompletion(learningPathRef, "thumbsDown"))
    await older
  })
  await waitFor(() => expect(run).toHaveBeenCalledTimes(3))
  expect(result.current.pending.has(learningPathRef)).toBe(true)
  await act(async () => {
    secondVote.resolve(feedbackCompletion(learningPathRef, "thumbsUp"))
    await newer
  })

  expect(result.current.feedback[learningPathRef]?.currentVote).toBe("thumbsUp")
  expect(result.current.pending.has(learningPathRef)).toBe(false)
})

test("cancels a queued vote when its session is no longer active", async () => {
  const firstSession = fromGameImportId("game-import:queued-first")
  const secondSession = fromGameImportId("game-import:queued-second")
  const learningPathRef = fromLearningPathRef("learning-path:queued-session")
  const firstExposure =
    deferred<Awaited<ReturnType<RunIndependentReviewSessionCommand>>>()
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockImplementationOnce(() => firstExposure.promise)
    .mockResolvedValueOnce(feedbackCompletion(learningPathRef, null))
  const { result, rerender } = renderHook(
    ({ gameImportId }) =>
      useLearningPathFeedback(gameImportId, [learningPathRef], run),
    { initialProps: { gameImportId: firstSession } },
  )
  await waitFor(() => expect(run).toHaveBeenCalledTimes(1))
  let queuedVote: Promise<void> | undefined
  act(() => {
    queuedVote = result.current.updateVote(learningPathRef, "thumbsUp")
  })

  rerender({ gameImportId: secondSession })
  await waitFor(() => expect(run).toHaveBeenCalledTimes(2))
  await act(async () => {
    firstExposure.resolve(feedbackCompletion(learningPathRef, null))
    await queuedVote
  })

  expect(
    run.mock.calls.filter(
      ([command]) =>
        command.kind === "updateLearningPathVote" &&
        command.gameImportId === firstSession,
    ),
  ).toHaveLength(0)
})

test("does not announce Saving… for the automatic exposure write", async () => {
  const gameImportId = fromGameImportId("game-import:exposure-pending")
  const learningPathRef = fromLearningPathRef("learning-path:exposure-pending")
  const exposure =
    deferred<Awaited<ReturnType<RunIndependentReviewSessionCommand>>>()
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockImplementationOnce(() => exposure.promise)
  const { result } = renderHook(() =>
    useLearningPathFeedback(gameImportId, [learningPathRef], run),
  )

  expect(result.current.pending.has(learningPathRef)).toBe(true)
  expect(result.current.votePending.has(learningPathRef)).toBe(false)

  await act(async () => {
    exposure.resolve(feedbackCompletion(learningPathRef, null))
    await exposure.promise
  })
})

test("treats a missing vote completion as unconfirmed unless a re-read agrees", async () => {
  const gameImportId = fromGameImportId("game-import:failed-vote")
  const learningPathRef = fromLearningPathRef("learning-path:failed-vote")
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockResolvedValueOnce(feedbackCompletion(learningPathRef, null))
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce(null)
  const { result } = renderHook(() =>
    useLearningPathFeedback(gameImportId, [learningPathRef], run),
  )
  await waitFor(() => expect(run).toHaveBeenCalledTimes(1))

  await act(() => result.current.updateVote(learningPathRef, "thumbsUp"))
  expect(run).toHaveBeenCalledTimes(3)
  expect(result.current.failures[learningPathRef]).toBe(
    LEARNING_PLAN_FEEDBACK_UNCONFIRMED,
  )
  expect(result.current.feedback[learningPathRef]?.currentVote).toBeNull()
})

test("records a vote when the completion is missing and a re-read agrees", async () => {
  const gameImportId = fromGameImportId("game-import:unconfirmed-reread")
  const learningPathRef = fromLearningPathRef(
    "learning-path:unconfirmed-reread",
  )
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockResolvedValueOnce(feedbackCompletion(learningPathRef, null))
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce(feedbackCompletion(learningPathRef, "thumbsUp"))
  const { result } = renderHook(() =>
    useLearningPathFeedback(gameImportId, [learningPathRef], run),
  )
  await waitFor(() => expect(run).toHaveBeenCalledTimes(1))

  await act(() => result.current.updateVote(learningPathRef, "thumbsUp"))
  expect(result.current.failures[learningPathRef]).toBeUndefined()
  expect(result.current.feedback[learningPathRef]?.currentVote).toBe("thumbsUp")
})

test("restores cached exposure feedback when returning to a session", async () => {
  const firstSession = fromGameImportId("game-import:return-first")
  const secondSession = fromGameImportId("game-import:return-second")
  const learningPathRef = fromLearningPathRef("learning-path:return")
  const refs = [learningPathRef]
  const run = vi
    .fn<RunIndependentReviewSessionCommand>()
    .mockImplementation(async (command) => {
      if (command.kind !== "recordLearningPathExposure") {
        throw new Error(`unexpected command ${command.kind}`)
      }
      return feedbackCompletion(
        learningPathRef,
        command.gameImportId === firstSession ? "thumbsDown" : "thumbsUp",
      )
    })
  const { result, rerender } = renderHook(
    ({ gameImportId }) => useLearningPathFeedback(gameImportId, refs, run),
    { initialProps: { gameImportId: firstSession } },
  )
  await waitFor(() =>
    expect(result.current.feedback[learningPathRef]?.currentVote).toBe(
      "thumbsDown",
    ),
  )

  rerender({ gameImportId: secondSession })
  await waitFor(() =>
    expect(result.current.feedback[learningPathRef]?.currentVote).toBe(
      "thumbsUp",
    ),
  )
  rerender({ gameImportId: firstSession })
  await waitFor(() =>
    expect(result.current.feedback[learningPathRef]?.currentVote).toBe(
      "thumbsDown",
    ),
  )

  expect(run).toHaveBeenCalledTimes(2)
})

function feedbackCompletion(
  learningPathRef: LearningPathRef,
  currentVote: "thumbsDown" | "thumbsUp" | null,
) {
  return {
    kind: "learningPathFeedbackRecorded" as const,
    feedback: {
      currentVote,
      exposedSurfaces: ["web" as const],
      learningPathRef,
    },
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise
  })
  return { promise, resolve }
}
