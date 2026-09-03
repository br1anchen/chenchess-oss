import type { Meta, StoryObj } from "@storybook/react-vite"
import { useState } from "react"

import { HStack, Text, VStack } from "../src/astryx"
import { TrailingActionRow } from "../src/components/TrailingActionRow"
import { WatercolorBadge, WatercolorCard } from "../src/components/watercolor"

const meta = {
  title: "Watercolor/Trailing action row",
} satisfies Meta
export default meta

type Story = StoryObj

type Row = {
  id: string
  opening: string
  opponent: string
  outcome: "win" | "loss"
  side: "black" | "white"
}

const rows: Row[] = [
  {
    id: "ada",
    opening: "B01 · Scandinavian Defense",
    opponent: "Ada",
    outcome: "win",
    side: "black",
  },
  {
    id: "cleo",
    opening: "A00 · Saragossa Opening",
    opponent: "Cleo",
    outcome: "loss",
    side: "white",
  },
  {
    id: "bea",
    opening: "A05 · King's Indian Attack",
    opponent: "Bea",
    outcome: "loss",
    side: "white",
  },
]

/**
 * The Imported Games list row. On a mouse the delete sits at the trailing end
 * of the row and appears when the row is hovered or focused, and the row
 * stays put; on a touchscreen the row drags left to uncover it, and a throw
 * past the middle runs it. Every row on this list is a Game the Player
 * imported themselves, so every row offers the delete.
 */
export const ImportedGamesList: Story = {
  render: () => <ImportedGamesListDemo />,
}

function ImportedGamesListDemo() {
  const [asked, setAsked] = useState<string | null>(null)
  return (
    /* The list that holds these rows clips its own horizontal overflow, so
         a row sliding open disappears under the list edge rather than into the
         page gutter. */
    <div style={{ maxWidth: 560, overflowX: "clip", padding: "2rem" }}>
      <VStack gap={2} hAlign="stretch">
        <Text type="supporting">
          {asked === null ? "Nothing asked for yet." : `Confirming: ${asked}`}
        </Text>
        {rows.map((row) => (
          <TrailingActionRow
            action={{
              accessibleLabel: `Delete the ${row.side} Game against ${row.opponent}`,
              label: "Delete",
              onAction: () => setAsked(row.opponent),
            }}
            key={row.id}
          >
            <WatercolorCard padding="compact">
              <HStack gap={2} vAlign="center">
                <WatercolorBadge
                  tone={row.outcome === "win" ? "success" : "danger"}
                >
                  {row.outcome}
                </WatercolorBadge>
                <VStack gap={1} hAlign="start">
                  <Text type="body" weight="semibold">
                    vs. {row.opponent}
                  </Text>
                  <Text type="supporting">
                    {row.side} · {row.opening}
                  </Text>
                </VStack>
              </HStack>
            </WatercolorCard>
          </TrailingActionRow>
        ))}
      </VStack>
    </div>
  )
}
