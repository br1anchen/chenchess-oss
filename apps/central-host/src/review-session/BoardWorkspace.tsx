import { Icon } from "@chenchess/ui/astryx"
import { useEffect, useRef, type ReactNode, type RefObject } from "react"
import type {
  EngineEvaluation,
  ImportedGame,
  PositionSnapshot,
  Square,
} from "@chenchess/coach-engine-sdk"
import type { PlayerVisibleSan } from "@chenchess/review-projection"
import { whiteEvaluationShare } from "@chenchess/ui/review/navigation-presentation"

import {
  Button,
  Card,
  Heading,
  HStack,
  Text,
  VStack,
  WatercolorBadge,
  WatercolorEvaluationGraph,
  WatercolorMoveNav,
  type BoardArrow,
  type BoardOrientation,
  type BoardSquareMark,
} from "@chenchess/ui"

import { useCompactLayout } from "@/useCompactLayout"

import {
  BranchPathStrip,
  ReviewBranchControls,
  type BoardWorkspaceBranch,
  type BoardWorkspaceBranchAffordances,
  type ExploredBranchLabel,
} from "./BoardBranchControls"

export {
  ReviewBranchControls,
  type BoardWorkspaceBranch,
  type BoardWorkspaceBranchAffordances,
  type ExploredBranchLabel,
}

import { AskTheCoach } from "./AskTheCoach"
import {
  boardPositionReferent,
  type ShownPosition,
} from "./boardPositionReferent"
import { ChessBoard } from "./ChessBoard"
import { reviewSessionShellStyles } from "./ReviewSessionShell.styles"

import {
  type EvaluationPoint,
  formatEvaluation,
  moveLabel,
  type PromotionRole,
} from "./model"
import type { ReviewMomentMarker } from "./reviewMoments"

type PromotionMove = {
  from: Square
  to: Square
}

type BoardWorkspaceProps = {
  importedGame?: ImportedGame
  moves?: ImportedGame["game"]["moves"]
  position: Pick<PositionSnapshot, "occupied" | "fen" | "sideToMove">
  /** The side the board is drawn from. The Coaching Board's drive owns it, so
   * it is passed in rather than read off the Game (#529). */
  orientation: BoardOrientation
  /** Engine/Maia comparison arrows for the shown position. */
  arrows?: readonly BoardArrow[]
  /** Squares the coach singled out about this position (ADR 0059). */
  marks?: readonly BoardSquareMark[]
  evaluation: EngineEvaluation | null
  evaluationPoints: EvaluationPoint[]
  momentMarkers: readonly ReviewMomentMarker[]
  criticalPly: number
  viewedPly: number
  selectedSquare: Square | null
  destinations: Square[]
  promotion: PromotionMove | null
  branch: BoardWorkspaceBranch | null
  heading: PlayerVisibleSan
  shownLineLabel?: string | null
  shownLineMove?: string | null
  navigationDisabled: boolean
  interactionDisabled: boolean
  alternativeBusy: boolean
  /** The move list + ply nav; the Review Session renders them in the session
   * column instead (ReviewMoveControls), other surfaces keep them here. */
  showMoveControls?: boolean
  /** The move list alone inside the controls block; the coaching board hides
   * it while keeping the ply nav. */
  showMoveList?: boolean
  /** The board's own move/eval caption; the Review Session board pane turns
   * it on, the coaching board keeps its stripped surface. */
  showPositionCaption?: boolean
  /** The line the board is walking, if it is showing one. */
  linePlayback?: BoardWorkspaceLinePlayback | null
  /** Branch navigation drawn in the board column. All of it or none: the
   * Review Session renders the same affordances in its session column and
   * passes none. */
  branchAffordances?: BoardWorkspaceBranchAffordances
  /** Turns on "Ask about this position" (#530): the surface whose chat lives
   * in another window hands over its clipboard write, and the board puts a
   * referent for the shown position on it. The Review Session's chat is on
   * the page, so it passes nothing. */
  copyPositionReferent?: (referent: string) => Promise<void>
  /** Whether the heading's move is already on the board — true while the
   * board shows the refutation of the played move, whose line roots after it.
   * A branch is read off `branch`; everything else stands before its move. */
  headingPlayed?: boolean
  onNavigate: (ply: number) => void
  onSquare: (square: Square) => void
  onPromote: (role: PromotionRole) => void
  onExitBranch: () => void
  onCancel?: () => void
}

function boardWorkspaceProps(props: BoardWorkspaceProps) {
  return {
    showMoveControls: props.showMoveControls ?? true,
    showMoveList: props.showMoveList ?? true,
    showPositionCaption: props.showPositionCaption ?? false,
    shownLineLabel: props.shownLineLabel ?? null,
    shownLineMove: props.shownLineMove ?? null,
  }
}

/** All markers on one ply keep one chip: the first carries the tone, every
 * label reaches the accessible name. */
function groupedMomentMarkers(momentMarkers: readonly ReviewMomentMarker[]) {
  const grouped = new Map<number, ReviewMomentMarker[]>()
  for (const marker of momentMarkers) {
    const held = grouped.get(marker.ply)
    if (held) held.push(marker)
    else grouped.set(marker.ply, [marker])
  }
  return grouped
}

export function boardMaxPly(
  moves: ImportedGame["game"]["moves"] | undefined,
  evaluationPoints: EvaluationPoint[],
  criticalPly: number,
  viewedPly: number,
) {
  return (
    moves?.at(-1)?.ply ??
    evaluationPoints.at(-1)?.ply ??
    Math.max(criticalPly, viewedPly)
  )
}

function boardLastMove(
  shownLineMove: string | null,
  branch: BoardWorkspaceBranch | null,
  moves: ImportedGame["game"]["moves"] | undefined,
  viewedPly: number,
) {
  return (
    shownLineMove ??
    branch?.moveUci ??
    moves?.find((move) => move.ply === viewedPly - 1)?.uci ??
    null
  )
}

/**
 * What the caption says and the referent repeats. Only off-game lines earn
 * a kind; the real game and a Critical Moment both read as the plain
 * position.
 */
function shownPosition(
  heading: PlayerVisibleSan,
  shownLineLabel: string | null,
  branch: BoardWorkspaceBranch | null,
  linePlayback: BoardWorkspaceLinePlayback | null | undefined,
  headingPlayed: boolean,
): ShownPosition {
  return {
    heading,
    kind: shownLineLabel ?? (branch ? "Alternative branch" : null),
    lineStep: linePlayback?.index ?? 0,
    played: branch !== null || headingPlayed,
  }
}

export function reviewSessionEvaluationGraph({
  activePly,
  disabled,
  evaluationPoints,
  maxPly,
  momentMarkers,
  onSelect,
}: {
  activePly: number
  disabled: boolean
  evaluationPoints: readonly EvaluationPoint[]
  maxPly: number
  momentMarkers: readonly ReviewMomentMarker[]
  onSelect: (ply: number) => void
}) {
  if (evaluationPoints.length === 0) return null
  return (
    <WatercolorEvaluationGraph
      activePly={activePly}
      density="sparkline"
      disabled={disabled}
      maxPly={maxPly}
      moments={momentMarkers}
      onSelect={onSelect}
      points={evaluationPoints}
    />
  )
}

export function BoardWorkspace(props: BoardWorkspaceProps) {
  const {
    importedGame,
    orientation,
    position,
    arrows,
    evaluation,
    evaluationPoints,
    momentMarkers,
    criticalPly,
    viewedPly,
    selectedSquare,
    destinations,
    promotion,
    branch,
    heading,
    marks,
    navigationDisabled,
    interactionDisabled,
    alternativeBusy,
    branchAffordances,
    copyPositionReferent,
    headingPlayed = false,
    linePlayback,
    onNavigate,
    onSquare,
    onPromote,
    onExitBranch,
    onCancel,
  } = props
  const {
    showMoveControls,
    showMoveList,
    showPositionCaption,
    shownLineLabel,
    shownLineMove,
  } = boardWorkspaceProps(props)
  const leftoverFill = !useCompactLayout()
  const game = importedGame?.game
  const listedMoves = game?.moves ?? props.moves
  const maxPly = boardMaxPly(
    listedMoves,
    evaluationPoints,
    criticalPly,
    viewedPly,
  )
  const momentByPly = groupedMomentMarkers(momentMarkers)
  const cancelButton =
    alternativeBusy && onCancel ? (
      <Button
        label="Cancel"
        onClick={onCancel}
        size="sm"
        type="button"
        variant="destructive"
      />
    ) : null
  const lastMove = boardLastMove(shownLineMove, branch, listedMoves, viewedPly)
  const shown = shownPosition(
    heading,
    shownLineLabel,
    branch,
    linePlayback,
    headingPlayed,
  )
  const referent = boardPositionReferent(shown)

  return (
    /* The board column sizes from leftover height, so every ancestor between
       the shell and the board has to stay a flex column. Astryx `Section`
       wraps its children in a `display: block` inner div that `xstyle` never
       reaches, which severs the chain and collapses the board to zero. */
    <VStack
      aria-label="Game and board"
      as="section"
      className="chen-review-board-column"
      gap={3}
      hAlign="stretch"
      xstyle={reviewSessionShellStyles.boardColumn}
    >
      {showMoveControls ? (
        <>
          {showMoveList ? (
            <GameMoveList
              branch={branch}
              moves={listedMoves}
              momentByPly={momentByPly}
              navigationDisabled={navigationDisabled}
              onNavigate={onNavigate}
              viewedPly={viewedPly}
            />
          ) : null}
          {branchAffordances ? (
            <BranchPathStrip
              branch={branch}
              branchPath={branchAffordances.path}
              // Walking to a branch already explored is navigation, not a
              // move the engine has to answer.
              interactionDisabled={navigationDisabled}
              onSelectBranch={branchAffordances.onSelectBranch}
            />
          ) : null}
          {linePlayback ? (
            <LinePlaybackControls playback={linePlayback} />
          ) : null}
          <BoardNavControls
            branch={branch}
            cancelButton={cancelButton}
            maxPly={maxPly}
            navigationDisabled={navigationDisabled}
            onExitBranch={onExitBranch}
            onNavigate={onNavigate}
            onStrongestReply={branchAffordances?.onStrongestReply}
            strongestReplyLabel={branchAffordances?.strongestReplyLabel}
            viewedPly={viewedPly}
          />
        </>
      ) : null}

      <VStack
        className="chen-review-board-fill"
        gap={0}
        hAlign="stretch"
        xstyle={[
          reviewSessionShellStyles.boardLift,
          reviewSessionShellStyles.boardFill,
        ]}
      >
        <VStack
          className="chen-review-board-square"
          gap={2}
          hAlign="stretch"
          xstyle={reviewSessionShellStyles.boardSquare}
        >
          <VStack
            gap={0}
            hAlign="stretch"
            xstyle={[
              reviewSessionShellStyles.boardFillChild,
              reviewSessionShellStyles.boardAssembly,
              leftoverFill && reviewSessionShellStyles.boardAssemblyFill,
            ]}
          >
            <ChessBoard
              arrows={arrows}
              destinations={destinations}
              marks={marks}
              disabled={interactionDisabled}
              evaluationPercent={
                evaluation ? whiteEvaluationShare(evaluation) : undefined
              }
              fill={leftoverFill}
              lastMove={lastMove}
              onSquare={onSquare}
              orientation={orientation}
              position={position}
              selectedSquare={selectedSquare}
            />
          </VStack>
        </VStack>
      </VStack>
      <VStack
        className="chen-review-board-meta"
        gap={3}
        hAlign="stretch"
        xstyle={[
          reviewSessionShellStyles.boardMeta,
          reviewSessionShellStyles.boardMetaSnug,
        ]}
      >
        <PromotionChooser
          disabled={interactionDisabled}
          onPromote={onPromote}
          promotion={promotion}
        />

        {showPositionCaption ? (
          <BoardPositionCaption
            evaluation={evaluation}
            heading={heading}
            kind={shown.kind}
          />
        ) : null}

        {copyPositionReferent ? (
          // Keyed by the referent: a new position is a new affordance, so
          // "Copied" never describes a position the board has left.
          <AskTheCoach
            label="Ask about this position"
            copyReferent={copyPositionReferent}
            key={referent}
            referent={referent}
          />
        ) : null}

        <BoardAnnotationLegend arrows={arrows} marks={marks} />

        {branchAffordances ? (
          <ReviewBranchControls
            branch={branch}
            exploredBranches={branchAffordances.exploredBranches}
            interactionDisabled={navigationDisabled}
            onSelectBranch={branchAffordances.onSelectBranch}
          />
        ) : null}
      </VStack>
    </VStack>
  )
}

/**
 * The session column's move sequence: the full game move list (Critical
 * Moments carry their tone) and the ply nav. Selecting any ply routes through
 * one navigate handler — a Moment ply opens its coaching, any other ply walks
 * the board (#518).
 */
export function ReviewMoveControls({
  alternativeBusy,
  branch,
  maxPly,
  momentMarkers,
  moves,
  navigationDisabled,
  onCancel,
  onExitBranch,
  onNavigate,
  onStrongestReply,
  strongestReplyLabel,
  viewedPly,
}: {
  alternativeBusy: boolean
  branch: BoardWorkspaceBranch | null
  maxPly: number
  momentMarkers: readonly ReviewMomentMarker[]
  moves: ImportedGame["game"]["moves"] | undefined
  navigationDisabled: boolean
  onCancel?: () => void
  onExitBranch: () => void
  onNavigate: (ply: number) => void
  /** Previews the engine's best move from the explored branch position;
   * rendered beside Exit branch so the pair shares one line. */
  onStrongestReply?: (uci: string) => void
  strongestReplyLabel?: PlayerVisibleSan | null
  viewedPly: number
}) {
  const momentByPly = groupedMomentMarkers(momentMarkers)
  const cancelButton =
    alternativeBusy && onCancel ? (
      <Button
        label="Cancel"
        onClick={onCancel}
        size="sm"
        type="button"
        variant="destructive"
      />
    ) : null
  return (
    <VStack
      aria-label="Move sequence"
      className="chen-review-move-controls"
      gap={2}
      hAlign="stretch"
    >
      <GameMoveList
        branch={branch}
        moves={moves}
        momentByPly={momentByPly}
        navigationDisabled={navigationDisabled}
        onNavigate={onNavigate}
        viewedPly={viewedPly}
      />
      <BoardNavControls
        branch={branch}
        cancelButton={cancelButton}
        maxPly={maxPly}
        navigationDisabled={navigationDisabled}
        onExitBranch={onExitBranch}
        onNavigate={onNavigate}
        onStrongestReply={onStrongestReply}
        strongestReplyLabel={strongestReplyLabel}
        viewedPly={viewedPly}
      />
    </VStack>
  )
}

/**
 * The board's own state line, under the board: the viewed move, the engine
 * number, and — when the board is off the real game — which line it is
 * showing. Game identity lives in the page header (#518).
 */
function BoardPositionCaption({
  evaluation,
  heading,
  kind,
}: {
  evaluation: EngineEvaluation | null
  heading: PlayerVisibleSan
  kind: string | null
}) {
  return (
    <HStack aria-label="Position" gap={2} vAlign="center" wrap="wrap">
      {kind === null ? null : (
        <WatercolorBadge tone="info">{kind}</WatercolorBadge>
      )}
      <Heading level={2}>{heading}</Heading>
      <Text type="body" weight="semibold">
        {formatEvaluation(evaluation)}
      </Text>
    </HStack>
  )
}

/**
 * Walking a line the board is already showing.
 *
 * The control steps by index, which is what a transport does; the named
 * directions stay the agent's vocabulary. Kept structural so this file does
 * not reach into the coaching board for a type.
 */
export type BoardWorkspaceLinePlayback = {
  index: number
  onStep: (index: number) => void
  steps: readonly { san: string }[]
}

/**
 * What the coach drew, said in words under the board.
 *
 * Coloured lines in one window and prose in another leaves the Player to fuse
 * them; the legend is what makes a drawn claim readable (ADR 0059). Each entry
 * carries the ink it explains, because a label that does not point at its own
 * arrow is a weak mapping. Only coach-toned ink is listed — the engine's own
 * arrow is already named by the position caption.
 */
function BoardAnnotationLegend({
  arrows,
  marks,
}: {
  arrows: readonly BoardArrow[] | undefined
  marks: readonly BoardSquareMark[] | undefined
}) {
  const labels = [
    ...new Set(
      [...(arrows ?? []), ...(marks ?? [])]
        .filter((drawn) => drawn.tone === "coach")
        .map((drawn) => drawn.label),
    ),
  ]
  if (labels.length === 0) return null
  return (
    <HStack
      aria-label="What the coach drew"
      gap={2}
      vAlign="center"
      wrap="wrap"
    >
      {labels.map((label) => (
        <HStack gap={1} key={label} vAlign="center" wrap="nowrap">
          <span
            aria-hidden="true"
            className="chen-coach-mark-swatch"
            style={{
              background: "var(--color-board-arrow-coach)",
              borderRadius: "0.15rem",
              display: "inline-block",
              height: "0.65rem",
              width: "0.65rem",
            }}
          />
          <Text type="supporting">{label}</Text>
        </HStack>
      ))}
    </HStack>
  )
}

/**
 * The line's own transport: step through what the board is showing.
 *
 * It is the ply nav's component, not a lookalike — a second transport in its
 * own visual language, stacked directly above the real one, would say the two
 * behave differently when they do the same thing to different lines. Compact
 * density and the leading token are what keep them told apart.
 *
 * A Player who was told to look at a line should not have to ask the coach to
 * advance it, so the same walk the agent drives is a control here.
 */
function LinePlaybackControls({
  playback,
}: {
  playback: BoardWorkspaceLinePlayback
}) {
  const { index, onStep, steps } = playback
  const next = steps[index]
  return (
    <WatercolorMoveNav
      aria-label="Line playback"
      density="compact"
      disabled={false}
      firstAriaLabel="Line start"
      lastAriaLabel="Line end"
      maxPly={steps.length}
      minPly={0}
      onNavigate={onStep}
      ply={index}
      // The row names itself rather than carrying a token beside it: a
      // separate label is one more thing to squeeze, and it was the first
      // thing to truncate in a side panel.
      plyLabel={
        next
          ? `Line ${index} of ${steps.length} · ${next.san}`
          : `Line ${index} of ${steps.length}`
      }
    />
  )
}

function PromotionChooser({
  disabled,
  onPromote,
  promotion,
}: {
  disabled: boolean
  onPromote: (role: PromotionRole) => void
  promotion: PromotionMove | null
}) {
  if (!promotion) return null
  return (
    <Card aria-label="Choose promotion piece" padding={2} role="group">
      <HStack gap={2} wrap="wrap">
        <Text type="body">
          Promote {promotion.from}–{promotion.to} to
        </Text>
        {(["queen", "rook", "bishop", "knight"] as const).map((role) => (
          <Button
            isDisabled={disabled}
            key={role}
            label={`${role[0]!.toUpperCase()}${role.slice(1)}`}
            onClick={() => onPromote(role)}
            size="sm"
            type="button"
            variant="secondary"
          />
        ))}
      </HStack>
    </Card>
  )
}

function BoardNavControls({
  branch,
  cancelButton,
  maxPly,
  navigationDisabled,
  onExitBranch,
  onNavigate,
  onStrongestReply,
  strongestReplyLabel,
  viewedPly,
}: {
  branch: BoardWorkspaceBranch | null
  cancelButton: ReactNode
  maxPly: number
  navigationDisabled: boolean
  onExitBranch: () => void
  onNavigate: (ply: number) => void
  onStrongestReply?: (uci: string) => void
  strongestReplyLabel?: PlayerVisibleSan | null
  viewedPly: number
}) {
  if (branch) {
    const strongestReply =
      branch.strongestReply?.kind === "offered" ? branch.strongestReply : null
    return (
      <HStack
        aria-label="Position navigation"
        gap={2}
        role="group"
        wrap="nowrap"
      >
        <Button
          icon={<Icon icon="close" size="sm" />}
          label="Exit branch"
          onClick={onExitBranch}
          size="sm"
          type="button"
          variant="secondary"
        />
        {strongestReply && strongestReplyLabel && onStrongestReply ? (
          <Button
            icon={<Icon icon="bot" size="sm" />}
            isDisabled={navigationDisabled}
            label={`Best move: ${strongestReplyLabel}`}
            onClick={() => onStrongestReply(strongestReply.uci)}
            size="sm"
            type="button"
            variant="secondary"
          />
        ) : null}
        {cancelButton}
      </HStack>
    )
  }
  return (
    <WatercolorMoveNav
      aria-label="Position navigation"
      disabled={navigationDisabled}
      maxPly={maxPly}
      onNavigate={onNavigate}
      ply={viewedPly}
    >
      {cancelButton}
    </WatercolorMoveNav>
  )
}

/** Keeps the viewed move's chip in sight: the list opens centred on the
 * Critical Moment and follows navigation, like the widget's notation strip. */
function useCenteredViewedMove(
  listRef: RefObject<HTMLElement | null>,
  viewedPly: number,
  branch: BoardWorkspaceBranch | null,
) {
  useEffect(() => {
    if (branch) return
    const list = listRef.current
    const viewed = list?.querySelector<HTMLElement>('[aria-current="step"]')
    if (!list || !viewed) return
    const left = Math.max(
      0,
      viewed.offsetLeft - (list.clientWidth - viewed.offsetWidth) / 2,
    )
    if (parseHasScrollTo(list.scrollTo)) {
      list.scrollTo({ behavior: "auto", left })
    } else {
      list.scrollLeft = left
    }
  }, [branch, listRef, viewedPly])
}

/** jsdom's elements carry no scrollTo; the browser's always do. */
function parseHasScrollTo(
  value: unknown,
): value is (options: ScrollToOptions) => void {
  return typeof value === "function"
}

function GameMoveList({
  branch,
  moves,
  momentByPly,
  navigationDisabled,
  onNavigate,
  viewedPly,
}: {
  branch: BoardWorkspaceBranch | null
  moves: ImportedGame["game"]["moves"] | undefined
  momentByPly: Map<number, ReviewMomentMarker[]>
  navigationDisabled: boolean
  onNavigate: (ply: number) => void
  viewedPly: number
}) {
  const listRef = useRef<HTMLElement>(null)
  useCenteredViewedMove(listRef, viewedPly, branch)
  if (!moves || moves.length === 0) return null
  return (
    <HStack
      aria-label="Full game move list"
      gap={1}
      ref={listRef}
      wrap="nowrap"
      xstyle={reviewSessionShellStyles.moveListLine}
    >
      {moves.map((move) => (
        <GameMoveButton
          key={move.ply}
          markers={momentByPly.get(move.ply)}
          move={move}
          navigationDisabled={navigationDisabled}
          onNavigate={onNavigate}
          viewedPly={viewedPly}
        />
      ))}
    </HStack>
  )
}

function GameMoveButton({
  markers,
  move,
  navigationDisabled,
  onNavigate,
  viewedPly,
}: {
  markers: ReviewMomentMarker[] | undefined
  move: ImportedGame["game"]["moves"][number]
  navigationDisabled: boolean
  onNavigate: (ply: number) => void
  viewedPly: number
}) {
  const marker = markers?.[0]
  // Inside a branch this marks the ply the branch left from, which is what
  // the Game's own strip can honestly say about an off-game position.
  const viewed = viewedPly === move.ply
  return (
    <Button
      aria-current={viewed ? "step" : undefined}
      aria-label={
        markers
          ? [moveLabel(move), ...markers.map((held) => held.label)].join(" · ")
          : undefined
      }
      className={marker ? `chen-review-moment-${marker.tone}` : undefined}
      endContent={
        marker ? (
          <Text aria-label="Coaching moment" type="supporting">
            {marker.glyph}
          </Text>
        ) : undefined
      }
      isDisabled={navigationDisabled}
      label={moveLabel(move)}
      onClick={() => onNavigate(move.ply)}
      size="sm"
      type="button"
      variant={viewed ? "secondary" : "ghost"}
      xstyle={marker ? reviewSessionShellStyles.momentMoveChip : undefined}
    />
  )
}
