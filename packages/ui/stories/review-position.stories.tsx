import type { Meta, StoryObj } from "@storybook/react-vite"
import { useState } from "react"

import { Card, Heading, HStack, Text, VStack } from "../src/astryx"
import {
  WatercolorChessboard,
  WatercolorEvaluationBar,
  WatercolorEvaluationGraph,
  WatercolorMoveNav,
} from "../src/components/watercolor"
import { workspaceFixture } from "../src/fixtures"
import { ReviewMomentCarousel } from "../src/review/ReviewContextNavigation"
import { catalogStyles } from "./support/catalog.styles"

/**
 * Board, evaluation and moment navigation as one composition — the piece of
 * the retired catalog no single-primitive story covers. Selecting a marked
 * moment keeps the graph, the bar and the board caption in lockstep.
 */
const meta = {
  title: "Watercolor/Position",
} satisfies Meta
export default meta

type Story = StoryObj

const evaluationPoints = [
  { label: "+0.18", ply: 0, value: 18 },
  { label: "+0.42", ply: 4, value: 42 },
  { label: "+0.86", ply: 6, value: 86 },
  { label: "+0.31", ply: 10, value: 31 },
  { label: "−0.24", ply: 12, value: -24 },
  { label: "+0.55", ply: 17, value: 55 },
  { label: "+0.12", ply: 23, value: 12 },
] as const

const evaluationMoments = [
  {
    glyph: "!",
    label: "Queen exposed",
    moveLabel: "3… Qxd5",
    ply: 6,
    summary: "White develops with tempo.",
    tone: "improvement",
  },
  {
    glyph: "✓",
    label: "Useful pin",
    moveLabel: "6… Bg4",
    ply: 12,
    summary: "The position briefly favors Black.",
    tone: "positive",
  },
  {
    glyph: "◆",
    label: "Player selected",
    moveLabel: "9. h3",
    ply: 17,
    summary: "Inspect the bishop before deciding.",
    tone: "selected",
  },
] as const

const boardArrows = [
  { from: "d4", label: "Candidate advance", to: "d5", tone: "candidate" },
] as const

export const BoardAndEvaluation: Story = {
  render: function BoardAndEvaluationStory() {
    const [activePly, setActivePly] = useState<number>(evaluationMoments[0].ply)
    const [previewPly, setPreviewPly] = useState(14)
    const evaluation =
      evaluationPoints.find((point) => point.ply === activePly) ??
      evaluationPoints[0]
    return (
      <VStack gap={4} hAlign="stretch" xstyle={catalogStyles.stage}>
        <HStack
          gap={4}
          vAlign="center"
          wrap="wrap"
          xstyle={catalogStyles.positionShowcase}
        >
          <Card
            padding={4}
            variant="default"
            xstyle={catalogStyles.boardSample}
          >
            <VStack gap={3} hAlign="stretch">
              <VStack gap={1} hAlign="start">
                <Text color="secondary" type="label">
                  White perspective
                </Text>
                <Heading level={3}>Position evaluation</Heading>
                <Text aria-live="polite" role="status">
                  {evaluation.label}
                </Text>
              </VStack>
              <HStack
                gap={2}
                vAlign="stretch"
                xstyle={catalogStyles.boardWithEvaluation}
              >
                <WatercolorEvaluationBar
                  valueLabel={evaluation.label}
                  whiteShare={50 + evaluation.value / 12}
                  xstyle={catalogStyles.evaluationBar}
                />
                <WatercolorChessboard
                  arrows={boardArrows}
                  board={workspaceFixture.board}
                />
              </HStack>
            </VStack>
          </Card>

          <VStack gap={2} hAlign="stretch" xstyle={catalogStyles.graphSample}>
            <WatercolorEvaluationGraph
              activePly={activePly}
              disabled={false}
              maxPly={23}
              moments={evaluationMoments}
              onSelect={setActivePly}
              points={evaluationPoints}
            />
            <Text
              as="p"
              color="secondary"
              display="block"
              type="supporting"
              xstyle={catalogStyles.graphCaption}
            >
              Select a marked moment to keep the graph and evaluation bar in
              lockstep.
            </Text>
            <VStack xstyle={catalogStyles.moveNav}>
              <WatercolorMoveNav
                aria-label="Move sequence"
                maxPly={107}
                onNavigate={setPreviewPly}
                ply={previewPly}
              />
            </VStack>
          </VStack>
        </HStack>
      </VStack>
    )
  },
}

/** The exact swipeable Critical Moment navigator shared with the Coach App. */
export const MomentSelector: Story = {
  render: function MomentSelectorStory() {
    const [activePly, setActivePly] = useState<number>(evaluationMoments[0].ply)
    return (
      <VStack gap={4} hAlign="stretch" xstyle={catalogStyles.stage}>
        <VStack xstyle={catalogStyles.momentCarouselSample}>
          <ReviewMomentCarousel
            activePly={activePly}
            ariaLabel="Review moment selector example"
            disabled={false}
            moments={evaluationMoments}
            onSelect={setActivePly}
          />
        </VStack>
      </VStack>
    )
  },
}
