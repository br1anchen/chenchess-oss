import type {
  AlternativeMoveId,
  AlternativeMoveResult,
} from "@chenchess/coach-engine-sdk"
import type { PlayerVisibleSan } from "@chenchess/review-projection"
import { Button, HStack, Text, VStack } from "@chenchess/ui"

import { formatEvaluation } from "./model"
import { reviewSessionShellStyles } from "./ReviewSessionShell.styles"

/**
 * What the board renders from an explored branch.
 *
 * A Review Session branch is a full `AlternativeMoveResult`. A game Coaching
 * Board branch is one too, so its strongest reply reaches the board. An
 * opening Coaching Board branch is built in the page from stateless opening
 * analysis, which returns no StrongestReply, and the control that reads one
 * stays inert there.
 */
export type BoardWorkspaceBranch = Pick<
  AlternativeMoveResult,
  "alternativeMoveId" | "evaluation" | "moveUci"
> & { strongestReply?: AlternativeMoveResult["strongestReply"] }

export type ExploredBranchLabel = {
  alternativeMoveId: AlternativeMoveId
  label: PlayerVisibleSan
  selectedMove: AlternativeMoveResult["evaluation"]["selectedMove"]
}

/**
 * What a surface needs to walk its own branches from the board column.
 *
 * `path` is the line the board is inside, origin first; `exploredBranches`
 * are the other lines tried from this origin.
 */
export type BoardWorkspaceBranchAffordances = {
  exploredBranches: readonly ExploredBranchLabel[]
  onSelectBranch: (alternativeMoveId: AlternativeMoveId) => void
  onStrongestReply: (uci: string) => void
  path: readonly ExploredBranchLabel[]
  strongestReplyLabel: PlayerVisibleSan | null
}

/**
 * The session column's branch affordances: revisit explored branches. The
 * strongest-reply preview moved beside Exit branch (one line on the stack),
 * so this renders nothing until a branch has been explored.
 *
 * One scrollable row, like the move strips above the board: a tall column of
 * branches pushed the thread off a phone screen once a Player had tried a
 * handful.
 */
export function ReviewBranchControls({
  branch,
  exploredBranches,
  interactionDisabled,
  onSelectBranch,
}: {
  branch: BoardWorkspaceBranch | null
  exploredBranches: readonly ExploredBranchLabel[]
  interactionDisabled: boolean
  onSelectBranch: (alternativeMoveId: AlternativeMoveId) => void
}) {
  if (exploredBranches.length === 0) return null
  return (
    <VStack aria-label="Explored alternatives" gap={1} hAlign="stretch">
      <Text type="supporting">Explored branches</Text>
      <BranchButtonRow
        aria-label="Alternative branches"
        branch={branch}
        interactionDisabled={interactionDisabled}
        onSelectBranch={onSelectBranch}
        steps={exploredBranches}
      />
    </VStack>
  )
}

/**
 * The line the board is inside, origin first.
 *
 * The game move list keeps describing the Game, so without this a Player in a
 * branch reads a strip that does not contain the position on screen. Each step
 * is selectable, which is how the board walks back up its own branch.
 */
export function BranchPathStrip({
  branch,
  branchPath,
  interactionDisabled,
  onSelectBranch,
}: {
  branch: BoardWorkspaceBranch | null
  branchPath: readonly ExploredBranchLabel[]
  interactionDisabled: boolean
  onSelectBranch: (alternativeMoveId: AlternativeMoveId) => void
}) {
  if (branchPath.length === 0) return null
  return (
    <BranchButtonRow
      aria-label="Branch line"
      branch={branch}
      interactionDisabled={interactionDisabled}
      onSelectBranch={onSelectBranch}
      steps={branchPath}
    />
  )
}

/** One nowrap, snap-scrolling row of branch chips; the board's branch strips
 * differ only in which branches they list. */
function BranchButtonRow({
  "aria-label": ariaLabel,
  branch,
  interactionDisabled,
  onSelectBranch,
  steps,
}: {
  "aria-label": string
  branch: BoardWorkspaceBranch | null
  interactionDisabled: boolean
  onSelectBranch: (alternativeMoveId: AlternativeMoveId) => void
  steps: readonly ExploredBranchLabel[]
}) {
  return (
    <HStack
      aria-label={ariaLabel}
      gap={1}
      wrap="nowrap"
      xstyle={[
        reviewSessionShellStyles.moveListLine,
        reviewSessionShellStyles.branchCarousel,
      ]}
    >
      {steps.map((step) => (
        <ExploredBranchButton
          branch={branch}
          candidate={step}
          interactionDisabled={interactionDisabled}
          key={step.alternativeMoveId}
          onSelectBranch={onSelectBranch}
        />
      ))}
    </HStack>
  )
}

function ExploredBranchButton({
  branch,
  candidate,
  interactionDisabled,
  onSelectBranch,
}: {
  branch: BoardWorkspaceBranch | null
  candidate: ExploredBranchLabel
  interactionDisabled: boolean
  onSelectBranch: (alternativeMoveId: AlternativeMoveId) => void
}) {
  const selected = candidate.alternativeMoveId === branch?.alternativeMoveId
  return (
    <Button
      aria-current={selected ? "step" : undefined}
      isDisabled={interactionDisabled}
      label={`${candidate.label} · ${formatEvaluation(candidate.selectedMove)}`}
      onClick={() => onSelectBranch(candidate.alternativeMoveId)}
      size="sm"
      type="button"
      variant={selected ? "secondary" : "ghost"}
      xstyle={reviewSessionShellStyles.branchCarouselItem}
    />
  )
}
