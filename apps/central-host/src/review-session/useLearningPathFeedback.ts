import { useCallback } from "react"

import type {
  LearningPathRef,
  LearningPathVote,
  GameImportId,
} from "@chenchess/coach-engine-sdk"
import { useAcknowledgedLearningPathFeedback } from "@chenchess/ui/review/learning-path-feedback"

import type { RunIndependentReviewSessionCommand } from "./useReviewSessionCommands"

export function useLearningPathFeedback(
  gameImportId: GameImportId | null,
  learningPathRefs: readonly LearningPathRef[],
  run: RunIndependentReviewSessionCommand,
) {
  const recordExposure = useCallback(
    async (
      requestedSessionId: GameImportId,
      learningPathRef: LearningPathRef,
    ) => {
      const completion = await run(
        {
          kind: "recordLearningPathExposure",
          gameImportId: requestedSessionId,
          learningPathRef,
        },
        "Recording Learning Path delivery…",
      )
      return completion?.kind === "learningPathFeedbackRecorded"
        ? completion.feedback
        : undefined
    },
    [run],
  )
  const saveVote = useCallback(
    async (
      requestedSessionId: GameImportId,
      learningPathRef: LearningPathRef,
      vote: LearningPathVote | null,
    ) => {
      const completion = await run(
        {
          kind: "updateLearningPathVote",
          gameImportId: requestedSessionId,
          learningPathRef,
          vote,
        },
        "Saving Learning Path feedback…",
      )
      return completion?.kind === "learningPathFeedbackRecorded"
        ? completion.feedback
        : undefined
    },
    [run],
  )
  return useAcknowledgedLearningPathFeedback({
    learningPathRefs,
    recordExposure,
    saveVote,
    gameImportId,
  })
}
