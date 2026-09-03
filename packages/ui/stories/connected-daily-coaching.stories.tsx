import type { Meta, StoryObj } from "@storybook/react-vite"

import { Button, Card, Heading, VStack } from "../src/astryx"
import { DigestCard, type DigestCardIdea } from "../src/components/DigestCard"
import fixture from "./connected-daily-coaching.fixture.json"

const meta = {
  title: "Connected Daily Coaching",
} satisfies Meta
export default meta

type Story = StoryObj

export const Fixture: Story = {
  render: () => (
    <VStack
      data-layout-name="connected-daily-coaching"
      data-story-ready=""
      gap={4}
      hAlign="stretch"
    >
      <DigestCard
        appearance="featured"
        eyebrow={fixture.eyebrow}
        gameCount={fixture.gameCount}
        ideas={readDigestCardIdeas(fixture.ideas)}
        source={fixture.source}
        title={fixture.title}
      />
      <Card padding={2}>
        <VStack gap={3} hAlign="start">
          <Heading level={3}>Daily Coaching</Heading>
          <Button label="Open your Coaching Digest" type="button" />
        </VStack>
      </Card>
    </VStack>
  ),
}

function readDigestCardIdeas(
  ideas: readonly {
    purpose: string
    resources: readonly { href: string; label: string; role?: string }[]
    title: string
  }[],
): readonly DigestCardIdea[] {
  return ideas.map((idea) => {
    if (idea.purpose !== "improvement" && idea.purpose !== "reinforcement") {
      throw new Error(
        "Daily Coaching fixture purpose must be improvement or reinforcement",
      )
    }
    return {
      purpose: idea.purpose,
      resources: idea.resources.map((resource) => ({
        href: resource.href,
        label: resource.label,
        role: readResourceRole(resource.role),
      })),
      title: idea.title,
    }
  })
}

function readResourceRole(role: string | undefined): "drill" | "learn" {
  if (role === "drill") return "drill"
  if (role === "learn" || role === undefined) return "learn"
  throw new Error("Daily Coaching fixture resource role must be learn or drill")
}
