import type { Meta, StoryObj } from "@storybook/react-vite"

import { Card, HStack, Text, VStack } from "../src/astryx"
import { DigestCard } from "../src/components/DigestCard"
import { WatercolorNotice } from "../src/components/watercolor"
import { catalogStyles } from "./support/catalog.styles"

/**
 * The morning digest recipes. `Connected Daily Coaching` renders the plain
 * featured card from the layout fixture; these two are the branded variants
 * the catalog studied — the 陳 corner stamp, and the quiet day when no Game
 * was eligible.
 */
const meta = {
  title: "Watercolor/Digest",
} satisfies Meta
export default meta

type Story = StoryObj

export const SealedFeatured: Story = {
  render: () => (
    <VStack gap={4} hAlign="stretch" xstyle={catalogStyles.stage}>
      <HStack
        gap={3}
        vAlign="start"
        wrap="wrap"
        xstyle={catalogStyles.cardGrid}
      >
        <DigestCard
          appearance="featured"
          eyebrow="From your archive"
          gameCount={4}
          seal
          ideas={[
            {
              purpose: "improvement",
              resources: [
                {
                  href: "https://lichess.org/practice/discovered-attacks",
                  label: "Discovered Attacks",
                  role: "learn",
                },
                {
                  href: "https://lichess.org/training/discoveredAttack",
                  label: "Discovered-attack puzzles",
                  role: "drill",
                },
              ],
              title: "Discovered Attacks",
            },
            {
              purpose: "reinforcement",
              resources: [
                {
                  href: "https://lichess.org/training/xRayAttack",
                  label: "X-Ray puzzles",
                  role: "drill",
                },
              ],
              title: "X-Ray",
            },
          ]}
          source="Published Aug 16, 2026, 5:15 AM"
          title="Saturday, August 15, 2026"
        >
          <Text as="p" display="block" type="body">
            4 Games in this digest · 26 grounded learning paths
          </Text>
        </DigestCard>
        <Card padding={4} variant="default">
          <VStack gap={3} hAlign="start">
            <Text color="secondary" type="label">
              No eligible Games yesterday
            </Text>
            <WatercolorNotice
              detail="There were no eligible Games in the latest daily window, so no empty digest was created."
              glyph="✓"
              heading="You’re all caught up."
              tone="bamboo"
            />
            <Text as="p" color="secondary" display="block" type="supporting">
              Sunday, August 16, 2026
            </Text>
          </VStack>
        </Card>
      </HStack>
    </VStack>
  ),
}
