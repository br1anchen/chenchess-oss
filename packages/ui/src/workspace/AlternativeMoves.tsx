import { Badge } from "@astryxdesign/core/Badge"
import { Icon } from "../icons"
import { Button } from "@astryxdesign/core/Button"
import { Card } from "@astryxdesign/core/Card"
import { Heading } from "@astryxdesign/core/Heading"
import { HStack } from "@astryxdesign/core/HStack"
import { List, ListItem } from "@astryxdesign/core/List"
import { ProgressBar } from "@astryxdesign/core/ProgressBar"
import { Section } from "@astryxdesign/core/Section"
import { Text } from "@astryxdesign/core/Text"
import { TextArea } from "@astryxdesign/core/TextArea"
import { VStack } from "@astryxdesign/core/VStack"
import { useRef, useState } from "react"

import type {
  AlternativeMovePresentation,
  WorkspaceActionHandler,
} from "../contracts"

export function AlternativeMoves({
  alternatives,
  onAction,
}: {
  alternatives: readonly AlternativeMovePresentation[]
  onAction: WorkspaceActionHandler
}) {
  const [message, setMessage] = useState("")
  const heading = useRef<HTMLHeadingElement>(null)
  const active = alternatives.some(
    (alternative) => alternative.status === "active",
  )

  return (
    <Card>
      <VStack gap={3} hAlign="stretch">
        <HStack gap={2} vAlign="start">
          <VStack gap={1} hAlign="start">
            <Text color="secondary" type="label">
              Explore
            </Text>
            <Heading level={3} ref={heading} tabIndex={-1}>
              Alternative moves
            </Heading>
          </VStack>
          <Icon icon="compass" size="sm" />
        </HStack>
        {alternatives.length === 0 ? (
          <Text as="p" display="block" type="body">
            Select a piece, then choose a highlighted legal destination to
            evaluate an Alternative Move.
          </Text>
        ) : (
          <List>
            {alternatives.map((alternative) => (
              <ListItem
                description={alternative.detail}
                endContent={
                  <Badge label={alternative.status} variant="neutral" />
                }
                isSelected={alternative.selected}
                key={alternative.id}
                label={`${alternative.san} ${alternative.label}`}
                onClick={() =>
                  onAction({
                    type: "alternativeSelected",
                    alternativeId: alternative.id,
                  })
                }
              />
            ))}
          </List>
        )}
        {alternatives.map((alternative) => (
          <AlternativeDetail
            active={active}
            alternative={alternative}
            key={`${alternative.id}-detail`}
            message={message}
            onAction={onAction}
            onMessageChange={setMessage}
          />
        ))}
        {active ? (
          <Button
            icon={<Icon icon="stopCircle" size="sm" />}
            label="Cancel active work"
            onClick={() => {
              onAction({ type: "activeWorkCancelled" })
              heading.current?.focus()
            }}
            size="sm"
            variant="secondary"
          />
        ) : null}
      </VStack>
    </Card>
  )
}

function AlternativeDetail({
  active,
  alternative,
  message,
  onAction,
  onMessageChange,
}: {
  active: boolean
  alternative: AlternativeMovePresentation
  message: string
  onAction: WorkspaceActionHandler
  onMessageChange: (value: string) => void
}) {
  if (alternative.status === "active") {
    return (
      <ProgressBar
        isLabelHidden
        label="Move evaluation progress"
        max={100}
        value={62}
      />
    )
  }
  if (!alternative.evaluation) return null
  return (
    <VStack gap={2} hAlign="start">
      {alternative.selected ? (
        <Section
          aria-label={`Objective evaluation for ${alternative.san}`}
          padding={2}
          variant="muted"
        >
          <VStack gap={2} hAlign="start">
            <Text type="body" weight="semibold">
              Objective evaluation
            </Text>
            <Text as="p" display="block" type="body">
              {alternative.evaluation}
            </Text>
            {alternative.strongestReply ? (
              <Button
                icon={<Icon icon="bot" size="sm" />}
                isDisabled={active}
                label={`Continue with strongest reply ${alternative.strongestReply}`}
                onClick={() =>
                  onAction({
                    type: "strongestReplySelected",
                    alternativeId: alternative.id,
                  })
                }
                size="sm"
                type="button"
                variant="secondary"
              />
            ) : null}
          </VStack>
        </Section>
      ) : (
        <Section
          aria-label={`Objective evaluation for ${alternative.san}`}
          padding={2}
          variant="muted"
        >
          <VStack gap={2} hAlign="start">
            <Text type="body" weight="semibold">
              Objective evaluation
            </Text>
            <Text as="p" display="block" type="body">
              {alternative.evaluation}
            </Text>
          </VStack>
        </Section>
      )}
      {alternative.selected ? (
        <form
          onSubmit={(event) => {
            event.preventDefault()
            const trimmed = message.trim()
            if (!trimmed) return
            onAction({
              type: "alternativeDiscussionRequested",
              alternativeId: alternative.id,
              message: trimmed,
            })
            onMessageChange("")
          }}
        >
          <VStack gap={2} hAlign="start">
            <TextArea
              label="Ask about this alternative"
              maxLength={4096}
              onChange={onMessageChange}
              value={message}
            />
            <Button
              icon={<Icon icon="send" size="sm" />}
              isDisabled={!message.trim()}
              label="Ask coach"
              size="sm"
              type="submit"
            />
          </VStack>
        </form>
      ) : null}
    </VStack>
  )
}
