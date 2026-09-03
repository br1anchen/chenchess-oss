import type { ReactNode } from "react"
import { Icon } from "@chenchess/ui/astryx"
import {
  SessionHeaderLabel,
  VStack,
  WatercolorButton,
  WatercolorNotice,
  WatercolorSessionHeader,
  WatercolorStudio,
} from "@chenchess/ui"

import { SignOutControl } from "@/auth/SignOutControl"

import { reviewSessionShellStyles } from "./ReviewSessionShell.styles"

export type ReviewSessionHeaderProps = {
  extra?: ReactNode
  /** Game context beside the session plaque: players, side, opening. */
  meta?: ReactNode
  onAccountSettings?: () => void
  signOut?: () => Promise<void>
}

export type ReviewSessionShellProps = ReviewSessionHeaderProps & {
  board: ReactNode
  evaluationGraph?: ReactNode
  eyebrow?: string
  failure?: string | null
  hasConversation?: boolean
  session?: ReactNode
  title?: string
}

/**
 * The accepted #235 two-column Review Session chrome: board column left,
 * session column right. On the foundation 64rem stack the board precedes the
 * thread and nothing is sticky. The evaluation graph sits in the session
 * column; the left column keeps the main-branch stack.
 */
export function ReviewSessionShell({
  board,
  evaluationGraph,
  extra,
  eyebrow = "Game review",
  failure,
  hasConversation,
  meta,
  onAccountSettings,
  session,
  signOut,
  title,
}: ReviewSessionShellProps) {
  const columns = session !== undefined && session !== null
  return (
    <WatercolorStudio
      aria-label="Game review"
      as="main"
      className="chen-review-session"
      data-has-conversation={(hasConversation ?? columns) ? "true" : undefined}
      data-review-session-layout={columns ? "columns" : "widget"}
      xstyle={reviewSessionShellStyles.page}
    >
      <WatercolorSessionHeader
        actions={
          <>
            {onAccountSettings ? (
              <WatercolorButton
                aria-label="Account settings"
                onClick={onAccountSettings}
                size="sm"
                type="button"
                variant="quiet"
              >
                <Icon icon="settings" size="sm" />
                <SessionHeaderLabel>Account settings</SessionHeaderLabel>
              </WatercolorButton>
            ) : null}
            {signOut ? (
              <SignOutControl signOut={signOut} size="sm" variant="quiet" />
            ) : null}
            {extra}
          </>
        }
        eyebrow={sessionPlaque(title, eyebrow)}
        meta={meta}
      />
      {failure ? (
        <WatercolorNotice glyph="!" heading="Conversation" tone="vermilion">
          {failure}
        </WatercolorNotice>
      ) : null}
      {columns ? (
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
            gap={0}
            hAlign="stretch"
            xstyle={reviewSessionShellStyles.thread}
          >
            {evaluationGraph ? (
              <VStack
                className="chen-review-evaluation-graph"
                hAlign="stretch"
                xstyle={reviewSessionShellStyles.evaluationGraph}
              >
                {evaluationGraph}
              </VStack>
            ) : null}
            {session}
          </VStack>
        </VStack>
      ) : (
        <VStack
          className="chen-review-session-board"
          gap={3}
          hAlign="stretch"
          xstyle={reviewSessionShellStyles.board}
        >
          {board}
        </VStack>
      )}
    </WatercolorStudio>
  )
}

function sessionPlaque(title: string | undefined, eyebrow: string): string {
  return title && title.length > 0 ? title : eyebrow
}
