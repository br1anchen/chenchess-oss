import type { Meta, StoryObj } from "@storybook/react-vite"
import * as stylex from "@stylexjs/stylex"
import { useState } from "react"

import type { CSSProperties } from "react"

import { HStack, Text, VStack } from "../src/astryx"
import { catalogStyles, silhouetteStyles } from "./support/catalog.styles"
import {
  WatercolorButton,
  WatercolorCard,
  WatercolorChatBubble,
  WatercolorDialog,
  WatercolorEyebrow,
  WatercolorTooltip,
} from "../src/components/watercolor"
import { DialogHeader } from "../src/components/dialog"

const meta = {
  title: "Watercolor/Surfaces",
} satisfies Meta
export default meta

type Story = StoryObj

const coachLine =
  "Bxh7+ looks natural, but count the defenders before you trust the pattern."
const playerLine = "I thought the bishop was winning material there."

/**
 * The chat thread with and without the painted patch. The patch is opt-in:
 * every bubble carrying artwork turns a thread into texture noise, so it marks
 * the openers and the moments worth pausing on.
 */
export const ChatBubbleBackdrops: Story = {
  render: () => (
    <VStack gap={4} hAlign="stretch" style={{ padding: "2rem", maxWidth: 620 }}>
      <VStack gap={2} hAlign="start">
        <WatercolorEyebrow>Plain — the working thread</WatercolorEyebrow>
        <WatercolorChatBubble tone="coach">{coachLine}</WatercolorChatBubble>
        <WatercolorChatBubble tone="player">{playerLine}</WatercolorChatBubble>
        <WatercolorChatBubble tone="system">
          Review paused at move 14.
        </WatercolorChatBubble>
      </VStack>

      <VStack gap={3} hAlign="start">
        <WatercolorEyebrow>Painted — the marked moment</WatercolorEyebrow>
        <WatercolorChatBubble backdrop="patch" tone="coach">
          {coachLine}
        </WatercolorChatBubble>
        <WatercolorChatBubble backdrop="patch" tone="player">
          {playerLine}
        </WatercolorChatBubble>
        <WatercolorChatBubble backdrop="patch" tone="system">
          Review paused at move 14.
        </WatercolorChatBubble>
      </VStack>

      <VStack gap={3} hAlign="start">
        <WatercolorEyebrow>Wash — the cloud as pigment</WatercolorEyebrow>
        <WatercolorChatBubble backdrop="wash" tone="coach">
          {coachLine}
        </WatercolorChatBubble>
        <WatercolorChatBubble backdrop="wash" tone="player">
          {playerLine}
        </WatercolorChatBubble>
      </VStack>
    </VStack>
  ),
}

function DialogVariant({
  backdrop,
  label,
}: {
  backdrop: "paper" | "cloud" | "ink"
  label: string
}) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <WatercolorButton onClick={() => setOpen(true)} variant="secondary">
        {label}
      </WatercolorButton>
      <WatercolorDialog
        backdrop={backdrop}
        isOpen={open}
        onOpenChange={(next) => setOpen(next)}
        width={460}
      >
        <DialogHeader
          onOpenChange={(next) => setOpen(next)}
          subtitle="Coaching keeps your reviewed Games so the next digest can build on them. You can turn this off at any time."
          title="Before this review continues"
        />
        <HStack gap={2} wrap="wrap">
          <WatercolorButton
            onClick={() => setOpen(false)}
            size="sm"
            variant="secondary"
          >
            Continue with current choice
          </WatercolorButton>
          <WatercolorButton onClick={() => setOpen(false)} size="sm">
            Turn off and continue
          </WatercolorButton>
        </HStack>
      </WatercolorDialog>
    </>
  )
}

/**
 * Two dialog backdrops. `paper` is the routine confirmation — the same ink
 * frame every card wears. `cloud` floats the painting behind the copy for the
 * standing moments: a first run, a finished review.
 */
export const DialogBackdrops: Story = {
  render: () => (
    <VStack gap={3} hAlign="start" style={{ padding: "2rem" }}>
      <Text as="p" display="block" type="supporting">
        Open each to compare; both paint their ink frame on entry.
      </Text>
      <HStack gap={3} wrap="wrap">
        <DialogVariant backdrop="paper" label="Paper dialog" />
        <DialogVariant backdrop="cloud" label="Cloud dialog" />
        <DialogVariant backdrop="ink" label="Ink dialog" />
      </HStack>
      <WatercolorTooltip content="Sign out of ChenChess">
        <WatercolorButton variant="quiet">
          Hover for the tooltip
        </WatercolorButton>
      </WatercolorTooltip>
    </VStack>
  ),
}

/**
 * The dialog surfaces rendered inline, so the story shows both without a
 * pointer. `isInline` skips the native <dialog> element — previews only.
 */
export const DialogSurfacesInline: Story = {
  render: () => (
    <HStack gap={4} vAlign="start" wrap="wrap" style={{ padding: "2rem" }}>
      {(["paper", "cloud", "ink"] as const).map((backdrop) => (
        <WatercolorDialog
          backdrop={backdrop}
          isInline
          isOpen
          key={backdrop}
          onOpenChange={() => undefined}
          width={420}
        >
          <VStack gap={3} hAlign="start">
            <WatercolorEyebrow>{backdrop}</WatercolorEyebrow>
            <Text as="p" display="block" type="body">
              Coaching keeps your reviewed Games so the next digest can build on
              them.
            </Text>
            <WatercolorButton size="sm">Continue</WatercolorButton>
          </VStack>
        </WatercolorDialog>
      ))}
    </HStack>
  ),
}

function HoverRow() {
  return (
    <HStack gap={2} wrap="wrap">
      <WatercolorButton>Primary</WatercolorButton>
      <WatercolorButton variant="secondary">Secondary</WatercolorButton>
      <WatercolorButton variant="outline">Outline</WatercolorButton>
      <WatercolorButton variant="quiet">Quiet</WatercolorButton>
      <WatercolorButton variant="danger">Danger</WatercolorButton>
      <WatercolorButton disabled>Disabled</WatercolorButton>
    </HStack>
  )
}

/**
 * On hover the control repaints itself as a dry-brush stroke in its own ink —
 * the resting fill is clipped away as the stroke arrives, so the slab becomes
 * brushwork rather than gaining a layer. Hover or tab to each.
 *
 * The second group opts into a vermilion wet tip at the leading edge, for
 * comparison; the default is the single-colour stroke above it.
 */
export const ButtonHoverWash: Story = {
  render: () => (
    <VStack gap={4} hAlign="start" style={{ padding: "2rem" }}>
      <VStack gap={2} hAlign="start">
        <WatercolorEyebrow>Its own ink (default)</WatercolorEyebrow>
        <HoverRow />
        <WatercolorButton block>Block primary</WatercolorButton>
      </VStack>

      <VStack
        gap={2}
        hAlign="start"
        // SAFETY: a custom property is a valid inline style; React's
        // CSSProperties type just has no member for one.
        style={
          {
            "--watercolor-hover-tip": "var(--color-error)",
          } as CSSProperties
        }
      >
        <WatercolorEyebrow>Opt-in vermilion wet tip</WatercolorEyebrow>
        <HoverRow />
        <WatercolorButton block>Block primary</WatercolorButton>
      </VStack>

      <Text as="p" display="block" type="supporting">
        Reduced motion keeps the highlight and drops the travel.
      </Text>
    </VStack>
  ),
}

/** The cloud painting on a card: the mist splash fills edge to edge with
 * the watercolor asset — the standing empty-state reading. */
export const CloudCard: Story = {
  render: () => (
    <VStack gap={3} hAlign="stretch" style={{ padding: "2rem", maxWidth: 560 }}>
      <WatercolorCard
        eyebrow="Profile connected"
        splash
        title="Preparing your first digest"
        tone="mist"
      >
        <Text as="p" display="block" type="body">
          We’re reviewing your latest eligible Games in the background.
        </Text>
      </WatercolorCard>
    </VStack>
  ),
}

/**
 * The torn silhouettes on their own: the generated `shape()` slabs and blots
 * from `theme/generated/watercolorShapes.css`. The ink specimen morphs
 * between the paired slabs on hover — the pairs share a command structure,
 * which is what lets `clip-path` interpolate between them.
 */
export const TornSilhouettes: Story = {
  render: function TornSilhouettesStory() {
    const [wet, setWet] = useState(false)
    return (
      <VStack gap={4} hAlign="stretch" xstyle={catalogStyles.stage}>
        <span
          onMouseEnter={() => setWet(true)}
          onMouseLeave={() => setWet(false)}
          {...stylex.props(
            silhouetteStyles.slab,
            wet ? silhouetteStyles.slabB : silhouetteStyles.slabA,
          )}
        >
          <Text color="inherit" type="body">
            The slab pair — hover re-wets the edge.
          </Text>
        </span>
        <span {...stylex.props(silhouetteStyles.slab, silhouetteStyles.panel)}>
          <Text color="inherit" type="body">
            The panel — the notification slab, deepest bites.
          </Text>
        </span>
        <HStack gap={4} hAlign="start">
          <span
            {...stylex.props(silhouetteStyles.blot, silhouetteStyles.blotA)}
          />
          <span
            {...stylex.props(silhouetteStyles.blot, silhouetteStyles.blotB)}
          />
        </HStack>
      </VStack>
    )
  },
}
