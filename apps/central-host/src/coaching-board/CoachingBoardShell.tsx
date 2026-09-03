import { useState, type ReactNode } from "react"
import {
  Heading,
  VStack,
  WatercolorButton,
  WatercolorPlaque,
  WatercolorSessionHeader,
  WatercolorStudio,
} from "@chenchess/ui"

import { reviewSessionShellStyles } from "@/review-session/ReviewSessionShell.styles"

import { coachingBoardStyles } from "./coachingBoard.styles"
import {
  CoachingBoardTargetDialog,
  type CoachingBoardTargetPane,
} from "./CoachingBoardTargetDialog"
import type { CoachingBoardTargetHost } from "./coachingBoardTargetSwitch"
import { useStackLayout } from "./useStackLayout"

export function CoachingBoardShell({
  actions,
  board,
  initialTargetPane = "import",
  registerTargetTools = false,
  session,
  target,
  targetHost,
}: {
  actions?: ReactNode
  board: ReactNode
  initialTargetPane?: CoachingBoardTargetPane
  registerTargetTools?: boolean
  session: ReactNode
  target?: string
  targetHost?: CoachingBoardTargetHost
}) {
  const stacked = useStackLayout()
  const [pickerOpen, setPickerOpen] = useState(false)
  // Once opened, the picker stays mounted (hidden) so typed URL, Elo, and
  // find state survive toggling it closed and open again.
  const [pickerMounted, setPickerMounted] = useState(false)
  const noTarget = session === null && target === undefined
  const showPicker = Boolean(targetHost) && (noTarget || pickerOpen)
  const mountPicker = Boolean(targetHost) && (noTarget || pickerMounted)
  return (
    <WatercolorStudio
      aria-label="Coaching"
      as="main"
      className="chen-review-session"
      data-review-session-layout="columns"
      xstyle={coachingBoardStyles.page}
    >
      <WatercolorSessionHeader
        actions={
          <>
            {actions}
            {noTarget ? null : (
              <WatercolorButton
                aria-controls="coaching-board-target-picker"
                aria-expanded={showPicker}
                onClick={() => {
                  setPickerMounted(true)
                  setPickerOpen((open) => !open)
                }}
                size="sm"
                type="button"
                variant="quiet"
              >
                Game or opening
              </WatercolorButton>
            )}
          </>
        }
        eyebrow={stacked ? undefined : "Coaching"}
      />
      {stacked ? (
        <WatercolorPlaque size="lg" xstyle={coachingBoardStyles.pageTitle}>
          Coaching
        </WatercolorPlaque>
      ) : null}
      <VStack
        className="chen-review-session-columns"
        gap={0}
        hAlign="stretch"
        xstyle={reviewSessionShellStyles.columns}
      >
        <VStack
          className="chen-review-session-board"
          gap={3}
          hAlign="stretch"
          xstyle={reviewSessionShellStyles.board}
        >
          {board}
        </VStack>
        <VStack
          className="chen-review-session-thread"
          gap={3}
          hAlign="stretch"
          xstyle={reviewSessionShellStyles.thread}
        >
          {mountPicker && targetHost ? (
            <VStack
              gap={3}
              hAlign="stretch"
              hidden={!showPicker}
              id="coaching-board-target-picker"
              xstyle={showPicker ? undefined : coachingBoardStyles.hiddenPane}
            >
              <CoachingBoardTargetDialog
                {...targetHost}
                initialPane={initialTargetPane}
                onOpenChange={setPickerOpen}
                registerTools={registerTargetTools}
              />
            </VStack>
          ) : null}
          {showPicker ? null : (
            <>
              {target ? (
                <Heading level={2} xstyle={coachingBoardStyles.target}>
                  {target}
                </Heading>
              ) : null}
              {session}
            </>
          )}
        </VStack>
      </VStack>
    </WatercolorStudio>
  )
}
