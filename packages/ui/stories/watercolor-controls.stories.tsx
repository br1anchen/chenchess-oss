import type { Meta, StoryObj } from "@storybook/react-vite"
import { useState } from "react"

import * as stylex from "@stylexjs/stylex"

import { Heading, HStack, Text, VStack } from "../src/astryx"
import { brushworkStyles, catalogStyles } from "./support/catalog.styles"
import { Icon } from "../src/icons"
import {
  WatercolorBadge,
  WatercolorButton,
  WatercolorButtonLink,
  WatercolorCard,
  WatercolorCheckbox,
  WatercolorChip,
  WatercolorField,
  WatercolorInkStroke,
  WatercolorInput,
  WatercolorMoveNav,
  WatercolorNotice,
  WatercolorPlaque,
  WatercolorSelect,
  WatercolorSymbol,
  WatercolorTextarea,
} from "../src/components/watercolor"

const meta = {
  title: "Watercolor/Controls",
} satisfies Meta
export default meta

type Story = StoryObj

export const Buttons: Story = {
  render: () => (
    <VStack gap={3} hAlign="start" xstyle={catalogStyles.stage}>
      <HStack gap={2} wrap="wrap">
        <WatercolorButton>Primary</WatercolorButton>
        <WatercolorButton variant="secondary">Secondary</WatercolorButton>
        <WatercolorButton variant="quiet">Quiet</WatercolorButton>
        <WatercolorButton variant="danger">Danger</WatercolorButton>
        <WatercolorButton disabled>Disabled</WatercolorButton>
        <WatercolorButton loading>Loading</WatercolorButton>
      </HStack>
      <HStack gap={2} wrap="wrap">
        <WatercolorButton size="sm">Small</WatercolorButton>
        <WatercolorButton size="md">Medium</WatercolorButton>
        <WatercolorButton size="lg">Large</WatercolorButton>
        <WatercolorButtonLink href="#nowhere" variant="secondary">
          Button link
        </WatercolorButtonLink>
      </HStack>
      <WatercolorButton block>Block primary</WatercolorButton>
    </VStack>
  ),
}

export const CardTones: Story = {
  render: () => (
    <VStack gap={3} hAlign="stretch" xstyle={catalogStyles.stage}>
      <HStack gap={3} vAlign="start" wrap="wrap">
        {(["paper", "mist", "bamboo", "vermilion", "watercolor"] as const).map(
          (tone) => (
            <WatercolorCard
              eyebrow="Tone"
              key={tone}
              style={{ width: "16rem" }}
              title={tone}
              tone={tone}
            >
              Quiet paper, one ink frame, title on the plaque.
            </WatercolorCard>
          ),
        )}
      </HStack>
      <WatercolorCard title="Framed parent" seal>
        <WatercolorCard frame={false} title="Frameless child">
          Nested cards drop the second ink border — two stacked frames read as a
          rendering fault.
        </WatercolorCard>
      </WatercolorCard>
    </VStack>
  ),
}

/**
 * The splash reading is reserved for the surfaces that matter: a featured
 * prompt, a marked moment, a standing message. The routine inline card keeps
 * its framed paper; `splash` becomes a lobed drop of the tone's own pigment,
 * filled with the watercolor painting, no frame. Coloured tones only — on
 * white paper (`tone="paper"`) the flag is ignored and the frame stays.
 */
export const SplashCards: Story = {
  render: () => (
    <VStack gap={3} hAlign="stretch" xstyle={catalogStyles.stage}>
      <HStack gap={3} vAlign="start" wrap="wrap">
        {(["mist", "bamboo", "vermilion", "watercolor"] as const).map(
          (tone) => (
            <WatercolorCard
              eyebrow="Splash"
              key={tone}
              splash
              style={{ width: "16rem" }}
              title={tone}
              tone={tone}
            >
              The paper spreads like a drop of pigment; the frame retires.
            </WatercolorCard>
          ),
        )}
      </HStack>
      <WatercolorCard splash title="A standing moment" seal tone="watercolor">
        Wide splash panels carry the announcements — the framed card beside them
        stays quiet.
      </WatercolorCard>
      <HStack gap={3} vAlign="start" wrap="wrap">
        <WatercolorCard
          padding="compact"
          splash
          style={{ width: "15rem" }}
          title="Compact splash"
          tone="bamboo"
        >
          Small containers wear the calm silhouette — fewer, gentler waves.
        </WatercolorCard>
        <WatercolorCard
          padding="compact"
          splash
          style={{ width: "15rem" }}
          title="Compact ink"
          tone="watercolor"
        >
          The same calm edge on the ink pigment.
        </WatercolorCard>
      </HStack>
    </VStack>
  ),
}

function FormControls() {
  const [confirmed, setConfirmed] = useState(true)
  return (
    <VStack gap={3} hAlign="stretch" xstyle={catalogStyles.stageNarrow}>
      <WatercolorField
        hint="The hint describes the control; it never joins its name."
        label="Game source"
      >
        <WatercolorInput placeholder="https://lichess.org/…" />
      </WatercolorField>
      <WatercolorField error="Enter a complete Lichess game URL." label="Email">
        <WatercolorInput defaultValue="not-a-url" type="email" />
      </WatercolorField>
      <WatercolorField hint="0/280 characters" label="Message to Coach">
        <WatercolorTextarea rows={3} />
      </WatercolorField>
      <WatercolorField label="Review side">
        <WatercolorSelect defaultValue="black">
          <option value="white">White</option>
          <option value="black">Black</option>
        </WatercolorSelect>
      </WatercolorField>
      <WatercolorCheckbox
        checked={confirmed}
        label="Help improve coaching"
        onChange={(event) => setConfirmed(event.target.checked)}
      />
    </VStack>
  )
}

export const Forms: Story = {
  render: () => <FormControls />,
}

export const StampsAndNotices: Story = {
  render: () => (
    <VStack gap={3} hAlign="start" xstyle={catalogStyles.stage}>
      <HStack gap={2} wrap="wrap">
        {(["neutral", "info", "success", "warning", "danger"] as const).map(
          (tone) => (
            <WatercolorBadge key={tone} tone={tone}>
              {tone}
            </WatercolorBadge>
          ),
        )}
      </HStack>
      <HStack gap={2} wrap="wrap">
        {(["win", "draw", "loss", "reinforced"] as const).map((tone) => (
          <WatercolorChip key={tone} tone={tone}>
            {tone}
          </WatercolorChip>
        ))}
      </HStack>
      <WatercolorNotice
        detail="Import a game and its review will appear here."
        glyph="陳"
        heading="No Imported Games yet"
      />
      <WatercolorNotice
        appearance="featured"
        detail="We’re reviewing your latest eligible Games in the background."
        eyebrow="Profile connected"
        glyph="…"
        heading="Preparing your first digest."
      />
    </VStack>
  ),
}

/**
 * The real dry-brush artwork on every primitive that wears it: the slab
 * plaque with its brush-wipe sweep, the self-drawing swoosh, the wide stroke
 * behind block/next controls, and the ink-blot soft stamp.
 */
export const Brushwork: Story = {
  render: () => (
    <VStack gap={4} hAlign="stretch" xstyle={catalogStyles.stage}>
      <VStack gap={0} hAlign="stretch">
        <Heading level={1} xstyle={brushworkStyles.heading}>
          <WatercolorPlaque size="lg">Critical Moment</WatercolorPlaque>
        </Heading>
        <span {...stylex.props(brushworkStyles.underline)}>
          <WatercolorInkStroke />
        </span>
      </VStack>
      <HStack gap={2} wrap="wrap">
        <WatercolorPlaque size="sm">Plaque sm</WatercolorPlaque>
        <WatercolorPlaque>Plaque md</WatercolorPlaque>
      </HStack>
      <WatercolorButton block>Wide stroke block button</WatercolorButton>
      <WatercolorMoveNav
        aria-label="Move sequence"
        maxPly={107}
        onNavigate={() => undefined}
        ply={14}
      />
      <HStack gap={3} vAlign="center" wrap="wrap">
        {(["watercolor", "slate", "bamboo", "vermilion"] as const).map(
          (tone) => (
            <WatercolorSymbol key={tone} silhouette="soft" tone={tone}>
              陳
            </WatercolorSymbol>
          ),
        )}
        <Text type="supporting">Ink-blot soft stamps</Text>
      </HStack>
      <span {...stylex.props(brushworkStyles.strokeAccent)}>
        <WatercolorInkStroke />
      </span>
    </VStack>
  ),
}

/**
 * The three review surfaces the retired catalog showed side by side: each
 * card carries an eyebrow, a meta slot and its own action row, which is what
 * separates them from the bare tone specimens above.
 */
export const CardsInContext: Story = {
  render: () => (
    <VStack gap={3} hAlign="stretch" xstyle={catalogStyles.stage}>
      <WatercolorCard
        eyebrow="Critical Moment"
        meta={<WatercolorBadge tone="warning">Critical</WatercolorBadge>}
        title="A forcing move deserves a pause"
      >
        <Text as="p" display="block" type="body">
          Bxh7+ looks natural, but the useful question is whether the attack
          survives after the king steps away. Count defenders before trusting
          the pattern.
        </Text>
        <HStack gap={2} wrap="wrap">
          <WatercolorButton size="sm" variant="quiet">
            Show on board
          </WatercolorButton>
          <WatercolorButton size="sm" variant="secondary">
            Explore line
          </WatercolorButton>
        </HStack>
      </WatercolorCard>

      <WatercolorCard
        eyebrow="What was your plan?"
        meta={
          <WatercolorBadge tone="success">
            <Icon icon="check" size="sm" /> Strong
          </WatercolorBadge>
        }
        title="A habit worth keeping"
        tone="bamboo"
      >
        <Text as="p" display="block" type="body">
          Before choosing your own plan, you checked the forcing moves on the
          other side. That habit matters more than the engine number here.
        </Text>
      </WatercolorCard>

      <WatercolorCard
        eyebrow="Explore alternatives"
        meta={
          <WatercolorChip tone="missing">
            <Icon icon="circleAlert" size="sm" /> Needs attention
          </WatercolorChip>
        }
        title="Session needs attention"
        tone="vermilion"
      >
        <Text as="p" display="block" type="body">
          The imported game stopped before the final move.
        </Text>
        <HStack gap={2} wrap="wrap">
          <WatercolorButton size="sm" variant="quiet">
            Cancel
          </WatercolorButton>
          <WatercolorButton size="sm" variant="danger">
            Import again
          </WatercolorButton>
        </HStack>
      </WatercolorCard>
    </VStack>
  ),
}
