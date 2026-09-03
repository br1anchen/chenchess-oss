import type { ReactNode } from "react"
import {
  Heading,
  ReviewMomentCarousel,
  Section,
  Text,
  VStack,
  WatercolorEvaluationGraph,
} from "@chenchess/ui"
import { LearningPathCards } from "@chenchess/ui/review/learning-paths"

import type { EvaluationPoint } from "@/review-session/model"
import type {
  MomentLearningPath,
  ReviewMomentMarker,
} from "@/review-session/reviewMoments"
import { reviewSessionShellStyles } from "@/review-session/ReviewSessionShell.styles"

import { coachingBoardStyles } from "./coachingBoard.styles"

export function CoachingBoardSession({
  commentary,
  evaluationPoints,
  learningPaths,
  maxPly,
  momentMarkers,
  onSelect,
  viewedPly,
}: {
  commentary: string | null
  evaluationPoints: readonly EvaluationPoint[]
  learningPaths: readonly MomentLearningPath[]
  maxPly: number
  momentMarkers: readonly ReviewMomentMarker[]
  onSelect: (ply: number) => void
  viewedPly: number
}): ReactNode {
  return (
    <VStack gap={3} hAlign="stretch">
      <WatercolorEvaluationGraph
        activePly={viewedPly}
        density="sparkline"
        disabled={false}
        maxPly={maxPly}
        moments={momentMarkers}
        onSelect={onSelect}
        points={evaluationPoints}
      />
      <CoachingBoardMomentPicker
        momentMarkers={momentMarkers}
        onSelect={onSelect}
        viewedPly={viewedPly}
      />
      <CoachingBoardCommentary commentary={commentary} />
      <LearningPathCards ariaLabel="Learning plans" paths={learningPaths} />
    </VStack>
  )
}

/**
 * What the coach already said about this position — prose only.
 *
 * The Coaching Board carries no thread: talking happens with the host agent,
 * so this reads back the frozen Review's commentary and offers no reply.
 */
function CoachingBoardCommentary({
  commentary,
}: {
  commentary: string | null
}) {
  if (!commentary) return null
  return (
    <Section aria-label="Coach commentary" padding={0} variant="transparent">
      <VStack gap={2} hAlign="stretch">
        <Heading level={2}>Coach commentary</Heading>
        {commentary.split(/\n{2,}/).map((paragraph) => (
          <Text as="p" display="block" key={paragraph} type="body">
            {paragraph}
          </Text>
        ))}
      </VStack>
    </Section>
  )
}

function CoachingBoardMomentPicker({
  momentMarkers,
  onSelect,
  viewedPly,
}: {
  momentMarkers: readonly ReviewMomentMarker[]
  onSelect: (ply: number) => void
  viewedPly: number
}) {
  if (momentMarkers.length > 0) {
    return (
      <ReviewMomentCarousel
        activePly={viewedPly}
        ariaLabel="Critical moments"
        density="compact"
        disabled={false}
        moments={momentMarkers}
        onSelect={onSelect}
        title="Critical moments"
        xstyle={reviewSessionShellStyles.pickerFlush}
      />
    )
  }
  return (
    <Section
      aria-label="Critical moments"
      className="chen-review-moment-picker"
      padding={0}
      variant="transparent"
      xstyle={coachingBoardStyles.momentPicker}
    >
      <Heading level={2}>Critical moments</Heading>
    </Section>
  )
}
