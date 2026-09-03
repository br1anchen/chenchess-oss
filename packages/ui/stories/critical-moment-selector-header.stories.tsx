import type { Meta, StoryObj } from "@storybook/react-vite"

import { HStack, VStack } from "../src/astryx"
import { WatercolorBadge } from "../src/components/watercolor"
import { ReviewMomentCarousel } from "../src/review/ReviewContextNavigation"

/** The width the Coach App widget renders its header at. */
const WIDGET_HEADER_WIDTH_PX = 343

/**
 * The Game Review's context above its moment picker, at the Coach App widget's
 * own width. Composed the way `CoachReviewContext` composes it: the opening
 * and Elo stamps belong to the review, and the picker below them carries no
 * header of its own.
 */
const meta = {
  title: "Critical Moment Selector/Header",
} satisfies Meta
export default meta

type Story = StoryObj

const moments = [
  {
    glyph: "!",
    label: "Queen exposed",
    moveLabel: "3… Qxd5",
    ply: 6,
    summary: "White develops with tempo.",
    tone: "improvement",
  },
] as const

function noopSelect(): void {
  return
}

function HeaderSurface({
  children,
  layoutName,
}: {
  children: React.ReactNode
  layoutName: string
}) {
  return (
    <VStack
      data-layout-name={layoutName}
      gap={2}
      hAlign="stretch"
      style={{ maxWidth: "100%", width: WIDGET_HEADER_WIDTH_PX }}
    >
      <HStack
        className="coach-review-header-meta"
        gap={1}
        vAlign="center"
        wrap="wrap"
      >
        {children}
      </HStack>
      <ReviewMomentCarousel
        activePly={moments[0].ply}
        ariaLabel="Critical moments"
        density="compact"
        disabled={false}
        moments={moments}
        onSelect={noopSelect}
      />
    </VStack>
  )
}

export const LongOpening: Story = {
  render: () => (
    <HeaderSurface layoutName="critical-moment-selector-header">
      <WatercolorBadge tone="info">
        King&apos;s Indian Attack: Yugoslav Variation
      </WatercolorBadge>
      <WatercolorBadge tone="neutral">Black · Elo 1450</WatercolorBadge>
    </HeaderSurface>
  ),
}

export { HeaderSurface }
