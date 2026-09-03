import type { ReactNode } from "react"
import { WatercolorNotice } from "@chenchess/ui"

import {
  ConversationPanel,
  type ConversationPanelProps,
} from "./ConversationPanel"
import { composerConversationBindings } from "./composerState"
import { ReviewGraphFrame } from "./ReviewGraphFrame"
import {
  ReviewSessionShell,
  type ReviewSessionHeaderProps,
} from "./ReviewSessionShell"
import type { ReviewMomentMarker } from "./reviewMoments"
import type { ComposerState } from "./thread-state"

export type ReviewSessionConversationProps = Omit<
  ConversationPanelProps,
  "busyLabel" | "inputDisabled"
> & {
  composer: ComposerState
  composerLocked?: boolean
  pendingLabel?: string | null
}

export type ReviewSessionViewProps = ReviewSessionHeaderProps & {
  board: ReactNode
  conversation: ReviewSessionConversationProps
  evaluationGraph?: ReactNode
  gameInfo?: ReactNode
  conversationKey?: string
  eyebrow?: string
  failure?: string | null
  momentMarkers: readonly ReviewMomentMarker[]
  momentNavigationDisabled?: boolean
  /** The merged move list + ply nav for the session column (#518). */
  moveControls?: ReactNode
  onSelectMoment?: (ply: number) => void
  sessionPly?: number | null
  title?: string
  viewedPly: number | null
}

/**
 * Props-driven Review Session. Fetching and HostTurn dispatch stay in the
 * workspace owner; this file only lays out #433 thread state on the #235
 * two-column shell. The Critical Moment selector is the move sequence itself:
 * toned chips in the move list plus the moment stepper on the eval graph.
 */
export function ReviewSessionView({
  board,
  conversation,
  conversationKey,
  evaluationGraph,
  extra,
  eyebrow,
  failure,
  gameInfo,
  meta,
  momentMarkers,
  momentNavigationDisabled = false,
  moveControls,
  onAccountSettings,
  onSelectMoment,
  sessionPly = null,
  signOut,
  title,
  viewedPly,
}: ReviewSessionViewProps) {
  const coachingOpen = sessionPly !== null
  const { composer, composerLocked, pendingLabel, ...panel } = conversation
  const composerBindings = composerConversationBindings(
    composer,
    composerLocked ?? false,
    pendingLabel ?? null,
  )
  const currentPly = viewedPly ?? sessionPly ?? momentMarkers[0]?.ply ?? 0
  const graph = evaluationGraph ? (
    <ReviewGraphFrame
      currentPly={currentPly}
      disabled={momentNavigationDisabled}
      graph={evaluationGraph}
      markers={momentMarkers}
      onSelect={onSelectMoment}
    />
  ) : (
    evaluationGraph
  )
  return (
    <ReviewSessionShell
      board={board}
      evaluationGraph={graph}
      extra={extra}
      eyebrow={eyebrow}
      failure={null}
      meta={meta}
      onAccountSettings={onAccountSettings}
      signOut={signOut}
      title={title}
      session={
        <>
          {moveControls}
          {gameInfo}
          {momentMarkers.length === 0 ? (
            <WatercolorNotice glyph="…" heading="No moments yet">
              Import a game or discuss this position to start a Review Session.
            </WatercolorNotice>
          ) : null}
          {coachingOpen ? (
            <ConversationPanel
              key={
                conversationKey ??
                `${viewedPly ?? "none"}:${panel.openingText ?? ""}`
              }
              {...panel}
              {...composerBindings}
              failure={panel.failure ?? failure ?? null}
            />
          ) : null}
        </>
      }
    />
  )
}
