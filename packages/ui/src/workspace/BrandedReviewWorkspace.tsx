import { Heading } from "@astryxdesign/core/Heading"
import { HStack } from "@astryxdesign/core/HStack"
import { Layout } from "@astryxdesign/core/Layout"
import { Section } from "@astryxdesign/core/Section"
import { Text } from "@astryxdesign/core/Text"
import { VStack } from "@astryxdesign/core/VStack"
import type { CSSProperties, ReactNode } from "react"

import { BrandLockup } from "../components/BrandLockup"

import "./review-session.css"
import { shellStyles } from "./BrandedReviewWorkspace.styles"
import { brandWorkspaceAssets } from "./brandWorkspaceAssets"

type BrandedReviewStyle = CSSProperties & {
  "--review-mist": string
}

const brandedReviewStyle: BrandedReviewStyle = {
  "--review-mist": `url("${brandWorkspaceAssets.mountainMist}")`,
}

export type BrandedReviewWorkspaceProps = {
  actions?: ReactNode
  board: ReactNode
  children?: ReactNode
  className?: string
  coaching: ReactNode
  conversation?: ReactNode
  eyebrow?: string
  meta?: ReactNode
  summary?: ReactNode
  title: ReactNode
}

export function BrandedReviewWorkspace({
  actions,
  board,
  children,
  className,
  coaching,
  conversation,
  eyebrow = "Game review",
  meta,
  summary,
  title,
}: BrandedReviewWorkspaceProps) {
  return (
    <VStack
      as="main"
      className={["chen-branded-review", className].filter(Boolean).join(" ")}
      data-has-conversation={conversation ? "true" : undefined}
      gap={4}
      style={brandedReviewStyle}
      xstyle={shellStyles.root}
    >
      <HStack gap={4} vAlign="start" wrap="wrap">
        <HStack gap={3} vAlign="center">
          <BrandLockup size="workspace" />
          <VStack gap={1} hAlign="start">
            <Text color="secondary" type="label">
              {eyebrow}
            </Text>
            <Heading level={1}>{title}</Heading>
          </VStack>
        </HStack>
        {meta || actions ? (
          <HStack gap={2} wrap="wrap">
            {meta}
            {actions}
          </HStack>
        ) : null}
      </HStack>

      {summary}

      <Layout
        content={board}
        end={conversation}
        height="auto"
        start={
          <Section
            aria-label="Coaching review"
            padding={0}
            variant="transparent"
          >
            {coaching}
          </Section>
        }
      />

      {children}
    </VStack>
  )
}

export type ReviewFocusCardProps = {
  description: ReactNode
  eyebrow?: string
  moveLabel?: ReactNode
  title: ReactNode
  tone?: "critical" | "positive" | "selected"
}

export function ReviewFocusCard({
  description,
  eyebrow = "Critical moment",
  moveLabel,
  title,
  tone = "selected",
}: ReviewFocusCardProps) {
  return (
    <Section data-tone={tone} padding={4}>
      <VStack gap={2} hAlign="start">
        <Text color="secondary" type="label">
          {eyebrow}
        </Text>
        <Heading level={2}>{title}</Heading>
        {moveLabel ? (
          <Text type="body" weight="semibold">
            {moveLabel}
          </Text>
        ) : null}
        <Text as="p" display="block" type="body">
          {description}
        </Text>
      </VStack>
    </Section>
  )
}
