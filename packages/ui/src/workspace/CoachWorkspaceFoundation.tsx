import { AppShell } from "@astryxdesign/core/AppShell"
import { Icon } from "../icons"
import { Badge } from "@astryxdesign/core/Badge"
import { Button } from "@astryxdesign/core/Button"
import { Card } from "@astryxdesign/core/Card"
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput"
import { Heading } from "@astryxdesign/core/Heading"
import { HStack } from "@astryxdesign/core/HStack"
import { IconButton } from "@astryxdesign/core/IconButton"
import { Layout } from "@astryxdesign/core/Layout"
import { List, ListItem } from "@astryxdesign/core/List"
import { Section } from "@astryxdesign/core/Section"
import { Selector } from "@astryxdesign/core/Selector"
import { Text } from "@astryxdesign/core/Text"
import { TopNav } from "@astryxdesign/core/TopNav"
import { VStack } from "@astryxdesign/core/VStack"

import { InteractiveChessboardGrid } from "../board"
import { BrandLockup } from "../components/BrandLockup"
import { DialogHeader } from "../components/dialog"
import { WatercolorDialog, WatercolorTooltip } from "../components/watercolor"
import { DryBrushCircle, WatercolorWashPanel } from "../motion"
import type {
  BoardPresentation,
  BoardSquare,
  ImportSetupPresentation,
  ReviewMomentPresentation,
  WorkspaceAction,
  WorkspaceActionHandler,
  WorkspacePresentation,
} from "../contracts"
import { retentionDisclosureDescription } from "../retention"
import { AlternativeMoves } from "./AlternativeMoves"

export type CoachWorkspaceFoundationProps = {
  model: WorkspacePresentation
  onAction: WorkspaceActionHandler
  showImportSetup?: boolean
}

const importSourceOptions = [
  { label: "Chess.com game link", value: "chessCom" },
  { label: "Lichess game link", value: "lichess" },
  { label: "Pasted PGN", value: "pgn" },
] as const

export function CoachWorkspaceFoundation({
  model,
  onAction,
  showImportSetup = true,
}: CoachWorkspaceFoundationProps) {
  const activeMoment =
    model.moments.find((moment) => moment.id === model.activeMomentId) ??
    model.moments[0]

  return (
    <AppShell
      contentPadding={4}
      height="auto"
      topNav={
        <TopNav
          endContent={
            <HStack gap={2} vAlign="center">
              <Text type="supporting">Reviewing as {model.playerName}</Text>
              <WatercolorTooltip content="Sign out">
                <IconButton
                  icon={<Icon icon="logOut" size="sm" />}
                  label="Sign out"
                  onClick={() => onAction({ type: "signOutRequested" })}
                  variant="ghost"
                />
              </WatercolorTooltip>
            </HStack>
          }
          heading={<BrandLockup href="#coach-workspace" size="workspace" />}
          label="Workspace"
        />
      }
    >
      <VStack gap={4} hAlign="stretch" id="coach-workspace">
        {showImportSetup ? (
          <Section aria-labelledby="setup-heading">
            <VStack gap={3} hAlign="start">
              <VStack gap={1} hAlign="start">
                <Text color="secondary" type="label">
                  Review setup
                </Text>
                <Heading id="setup-heading" level={1}>
                  Review the game, not just the score
                </Heading>
                <Text as="p" display="block" type="body">
                  {model.sessionLabel}
                </Text>
              </VStack>
              <Selector
                description={model.importSetup.sourceLabel}
                label="Game source"
                onChange={(source) =>
                  onAction({
                    type: "importSourceChanged",
                    source: parseImportSource(source),
                  })
                }
                options={[...importSourceOptions]}
                value={model.importSetup.source}
                width="100%"
              />
              <Button
                endContent={<Icon icon="arrowRight" size="sm" />}
                isDisabled={model.importSetup.status === "importing"}
                isLoading={model.importSetup.status === "importing"}
                label={
                  model.importSetup.status === "importing"
                    ? "Importing…"
                    : "Import game"
                }
                onClick={() => onAction({ type: "importRequested" })}
              />
            </VStack>
          </Section>
        ) : null}

        <Layout
          content={
            <Section aria-labelledby="board-heading" padding={3}>
              <VStack gap={3} hAlign="stretch">
                <HStack gap={2} vAlign="start" wrap="wrap">
                  <VStack gap={1} hAlign="start">
                    <Text color="secondary" type="label">
                      Position snapshot
                    </Text>
                    <Heading id="board-heading" level={2}>
                      {activeMoment?.moveLabel ?? "Review position"}
                    </Heading>
                  </VStack>
                  <Badge
                    label={`${model.board.orientation} below`}
                    variant="info"
                  />
                </HStack>
                <DryBrushCircle />
                <InteractiveChessboardGrid
                  destinations={model.board.legalDestinations}
                  disabled={model.board.disabled}
                  lastMove={model.board.lastMove}
                  onSquare={(square) =>
                    onAction(boardSquareAction(model.board, square))
                  }
                  orientation={model.board.orientation}
                  pieces={model.board.pieces}
                  selectedSquare={model.board.selectedSquare}
                />
                {model.board.promotion ? (
                  <PromotionChoices
                    onAction={onAction}
                    promotion={model.board.promotion}
                  />
                ) : null}
                <Text aria-live="polite" className="sr-only" type="supporting">
                  {model.board.announcement}
                </Text>
              </VStack>
            </Section>
          }
          end={
            <Section
              aria-label="Coaching rail"
              padding={0}
              variant="transparent"
            >
              <VStack gap={3} hAlign="stretch">
                <WatercolorWashPanel motionKey={model.activeMomentId}>
                  <Card>
                    <VStack gap={2} hAlign="start">
                      <HStack gap={2} vAlign="start" wrap="wrap">
                        <VStack gap={1} hAlign="start">
                          <Text color="secondary" type="label">
                            {model.comment.eyebrow}
                          </Text>
                          <Heading level={3}>{model.comment.heading}</Heading>
                        </VStack>
                        <Badge
                          label={model.comment.status}
                          variant={
                            model.comment.status === "admitted"
                              ? "success"
                              : "neutral"
                          }
                        />
                      </HStack>
                      <Text color="secondary" type="supporting">
                        {activeMoment?.moveLabel}
                      </Text>
                      <Text as="p" display="block" type="body">
                        {model.comment.body}
                      </Text>
                    </VStack>
                  </Card>
                </WatercolorWashPanel>
                <AlternativeMoves
                  alternatives={model.alternatives}
                  onAction={onAction}
                />
              </VStack>
            </Section>
          }
          height="auto"
        />

        <Section aria-label="Review moments">
          <VStack gap={3} hAlign="stretch">
            <HStack gap={2} vAlign="center">
              <VStack gap={1} hAlign="start">
                <Text color="secondary" type="label">
                  Chronological review
                </Text>
                <Heading level={2}>Moments</Heading>
              </VStack>
              <Icon icon="search" size="sm" />
            </HStack>
            <List>
              {model.moments.map((moment) => (
                <MomentItem
                  active={moment.id === model.activeMomentId}
                  key={moment.id}
                  moment={moment}
                  onSelect={() =>
                    onAction({ type: "momentSelected", momentId: moment.id })
                  }
                />
              ))}
            </List>
          </VStack>
        </Section>

        {model.retention.available ? (
          <Section aria-labelledby="retention-heading">
            <HStack gap={3} vAlign="start">
              <Icon icon="sparkles" size="sm" />
              <VStack gap={2} hAlign="start">
                <Heading id="retention-heading" level={2}>
                  Help improve coaching
                </Heading>
                <Text as="p" display="block" type="body">
                  {model.retention.description}
                </Text>
                <CheckboxInput
                  description={
                    model.retention.resolving
                      ? "Saving…"
                      : model.retention.enabled
                        ? "Enabled"
                        : "Disabled"
                  }
                  isDisabled={model.retention.resolving}
                  label="Help improve coaching"
                  onChange={(enabled) =>
                    onAction({
                      type: "retentionChanged",
                      enabled,
                    })
                  }
                  value={model.retention.enabled}
                />
              </VStack>
            </HStack>
          </Section>
        ) : null}

        <Text aria-atomic="true" aria-live="polite" type="supporting">
          {model.statusMessage}
        </Text>
      </VStack>

      <WatercolorDialog
        isOpen={model.retention.disclosureRequired}
        onOpenChange={(open) => {
          if (!open && !model.retention.resolving) {
            onAction({ type: "retentionDisclosureAcknowledged" })
          }
        }}
        purpose="form"
      >
        <DialogHeader
          subtitle={retentionDisclosureDescription}
          title="Before this review continues"
        />
        <Button
          isDisabled={model.retention.resolving}
          label="Continue with current choice"
          onClick={() => onAction({ type: "retentionDisclosureAcknowledged" })}
          variant="secondary"
        />
        <Button
          isDisabled={model.retention.resolving}
          label="Turn off and continue"
          onClick={() => onAction({ type: "retentionChanged", enabled: false })}
        />
      </WatercolorDialog>
    </AppShell>
  )
}

function MomentItem({
  active,
  moment,
  onSelect,
}: {
  active: boolean
  moment: ReviewMomentPresentation
  onSelect: () => void
}) {
  const kind = moment.kind === "automatic" ? "Key moment" : "Your pick"
  return (
    <ListItem
      description={moment.summary}
      isSelected={active}
      label={`${moment.moveLabel}, ${kind}: ${moment.title}. ${moment.summary}`}
      onClick={onSelect}
    />
  )
}

function boardSquareAction(
  board: BoardPresentation,
  square: BoardSquare,
): WorkspaceAction {
  if (board.selectedSquare && board.legalDestinations.includes(square)) {
    return {
      type: "boardMoveRequested",
      move: { from: board.selectedSquare, to: square },
    }
  }
  return { type: "boardSquareSelected", square }
}

function PromotionChoices({
  onAction,
  promotion,
}: {
  onAction: WorkspaceActionHandler
  promotion: NonNullable<BoardPresentation["promotion"]>
}) {
  return (
    <HStack
      aria-label="Choose promotion piece"
      gap={2}
      role="group"
      wrap="wrap"
    >
      {promotion.choices.map((role) => (
        <Button
          key={role}
          label={role}
          onClick={() =>
            onAction({
              type: "promotionRequested",
              move: promotion.move,
              role,
            })
          }
          size="sm"
          type="button"
          variant="secondary"
        />
      ))}
    </HStack>
  )
}

function parseImportSource(value: string): ImportSetupPresentation["source"] {
  switch (value) {
    case "chessCom":
    case "lichess":
    case "pgn":
      return value
    default:
      throw new TypeError("invalid import source")
  }
}
