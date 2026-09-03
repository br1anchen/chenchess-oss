import type { Meta, StoryObj } from "@storybook/react-vite"
import { Icon } from "../src/icons"
import { useState } from "react"

import {
  Badge,
  Banner,
  Button as AstryxButton,
  Card as AstryxCard,
  CheckboxInput,
  Heading,
  ProgressBar,
  Selector,
  Text,
  TextArea,
  TextInput,
  Token,
  VStack,
} from "../src/astryx"
import { workspaceFixture } from "../src/fixtures"
import { WatercolorBadge } from "../src/components/watercolor"
import { HeaderSurface } from "./critical-moment-selector-header.stories"
import {
  WatercolorChessboard,
  WatercolorEvaluationBar,
  WatercolorEvaluationGraph,
  WatercolorMomentCard,
  WatercolorMoveNav,
} from "../src/components/watercolor"

const evaluationPoints = [
  { label: "+0.18", ply: 0, value: 18 },
  { label: "+0.86", ply: 6, value: 86 },
  { label: "−0.24", ply: 12, value: -24 },
] as const

const evaluationMoments = [
  {
    glyph: "!",
    label: "Queen exposed",
    moveLabel: "3… Qxd5",
    ply: 6,
    summary: "White develops with tempo.",
    tone: "improvement" as const,
  },
]

function noopSelect(): void {
  return
}

function noopNavigate(): void {
  return
}

const meta = {
  title: "Watercolor",
} satisfies Meta
export default meta

type Story = StoryObj

export const Button: Story = {
  render: () => (
    <AstryxButton label="Join the private beta" variant="primary" />
  ),
}

export const Card: Story = {
  render: () => (
    <AstryxCard padding={4}>
      <VStack gap={3} hAlign="start">
        <Text color="secondary" type="label">
          Daily digest
        </Text>
        <Heading level={3}>A morning recipe</Heading>
        <Text type="supporting">28 April 2026</Text>
        <Heading level={4}>Today’s priorities</Heading>
        <Text as="p" display="block" type="supporting">
          Two findings stay attached to their Games.
        </Text>
        <Text as="p" display="block" type="body">
          Fork awareness in the Saragossa Opening middlegame.
        </Text>
        <Text as="p" display="block" type="supporting">
          Grounded in the engine review.
        </Text>
      </VStack>
    </AstryxCard>
  ),
}

export const Field: Story = {
  render: function FieldStory() {
    const [profile, setProfile] = useState(
      "https://lichess.org/@/synthetic-white",
    )
    return (
      <TextInput
        description="Public Lichess or Chess.com URL"
        label="Playing profile"
        onChange={setProfile}
        value={profile}
      />
    )
  },
}

export const Input: Story = {
  render: function InputStory() {
    const [username, setUsername] = useState("synthetic-white")
    return (
      <TextInput
        isLabelHidden
        label="Username"
        onChange={setUsername}
        value={username}
      />
    )
  },
}

export const Textarea: Story = {
  render: function TextareaStory() {
    const [question, setQuestion] = useState("Why is Bxh7+ tempting?")
    return <TextArea label="Question" onChange={setQuestion} value={question} />
  },
}

export const Select: Story = {
  render: function SelectStory() {
    const [side, setSide] = useState("black")
    return (
      <Selector
        label="Review side"
        onChange={setSide}
        options={[
          { label: "White", value: "white" },
          { label: "Black", value: "black" },
        ]}
        value={side}
      />
    )
  },
}

export const Checkbox: Story = {
  render: function CheckboxStory() {
    const [kept, setKept] = useState(true)
    return (
      <CheckboxInput
        label="Keep review available"
        onChange={setKept}
        value={kept}
      />
    )
  },
}

export const Chip: Story = {
  render: () => <Token color="green" label="Win" />,
}

export const Symbol: Story = {
  render: () => (
    <Token
      color="green"
      icon={<Icon icon="sparkles" size="sm" />}
      label="Insight"
    />
  ),
}

export const Notice: Story = {
  render: () => (
    <Banner
      description="Select a Critical Moment in the Review Session widget."
      icon={<Icon icon="sparkles" size="sm" />}
      status="info"
      title="No Critical Moment selected"
    />
  ),
}

export const Eyebrow: Story = {
  render: () => (
    <Text color="secondary" type="label">
      Critical Moment
    </Text>
  ),
}

export const Studio: Story = {
  render: () => (
    <VStack gap={3} hAlign="start">
      <Text as="p" display="block" type="body">
        Rice-paper studio for dashboard chrome.
      </Text>
    </VStack>
  ),
}

export const Chessboard: Story = {
  render: () => (
    <WatercolorChessboard
      board={workspaceFixture.board}
      style={{ width: "min(22rem, 100%)" }}
    />
  ),
}

export const EvaluationBar: Story = {
  render: () => <WatercolorEvaluationBar valueLabel="+0.86" whiteShare={58} />,
}

export const EvaluationGraph: Story = {
  render: () => (
    <WatercolorEvaluationGraph
      activePly={6}
      disabled={false}
      maxPly={12}
      moments={evaluationMoments}
      onSelect={noopSelect}
      points={evaluationPoints}
    />
  ),
}

export const MomentCard: Story = {
  render: () => (
    <WatercolorMomentCard
      current
      detail="White develops with tempo."
      glyph="!"
      label="Queen exposed"
      moveLabel="3… Qxd5"
      tone="improvement"
    />
  ),
}

export const Progress: Story = {
  render: () => (
    <ProgressBar isLabelHidden label="Review progress" max={100} value={42} />
  ),
}

export const BadgeShort: Story = {
  render: () => <Badge label="Saragossa Opening" variant="info" />,
}

const SIXTY_CHARACTER_BADGE_LABEL = `${"60-char badge: "}${"x".repeat(45)}`

/**
 * The longest badge the widget header must survive, in the header the widget
 * actually ships — same composition as `Critical Moment Selector/Header`.
 */
export const BadgeSixtyCharacter: Story = {
  render: () => (
    <HeaderSurface layoutName="sixty-character-badge">
      <WatercolorBadge data-layout-name="sixty-character" tone="info">
        {SIXTY_CHARACTER_BADGE_LABEL}
      </WatercolorBadge>
    </HeaderSurface>
  ),
}

export const MoveNav: Story = {
  render: () => (
    <WatercolorMoveNav
      aria-label="Move sequence"
      data-layout-name="move-nav"
      data-layout-single-row=""
      maxPly={107}
      onNavigate={noopNavigate}
      ply={14}
    />
  ),
}
