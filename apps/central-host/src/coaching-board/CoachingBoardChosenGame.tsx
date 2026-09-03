import { useEffect, useMemo } from "react"
import type {
  AlternativeMoveId,
  GameImportId,
  GameReview,
  ImportedGame,
} from "@chenchess/coach-engine-sdk"
import {
  engineMoveArrow,
  PLAYER_VISIBLE_MOVE_FALLBACK,
  playerVisibleSanFromLegalUci,
  playerVisibleStrongestReply,
} from "@chenchess/review-projection"
import { WatercolorNotice } from "@chenchess/ui"

import { writeClipboardText } from "@/clipboard"
import { boardArrowsFrom } from "@/review-session/boardArrows"
import {
  BoardWorkspace,
  type BoardWorkspaceBranchAffordances,
  type ExploredBranchLabel,
} from "@/review-session/BoardWorkspace"
import { evaluationPoint, moveLabel } from "@/review-session/model"
import {
  frozenReviewMomentMarkers,
  learningPathsForReviewMoment,
} from "@/review-session/reviewMoments"
import { shownLineLabel } from "@/review-session/thread-state"

import type { FetchAccessToken } from "@/review-session/client"

import {
  coachingBoardCommandExecute,
  evaluatePlayerLineOnBoard,
  listCriticalMomentsOnBoard,
  openReviewMomentInPlaceOnBoard,
} from "./coachingBoardCoachTools"
import { CoachingBoardSession } from "./CoachingBoardSession"
import { CoachingBoardShell } from "./CoachingBoardShell"
import type { CoachingBoardTargetHost } from "./coachingBoardTargetSwitch"
import { lobbyResult } from "./coachingBoardConstraints"
import {
  coachingBoardGamePath,
  coachingBoardOpeningPath,
} from "./coachingBoardRoute"
import type { OpeningLineRef } from "./openingLineRef"
import {
  activeExploredBranch,
  branchSourceFen,
  driveCurrentBoardPosition,
  drivePlayback,
  driveRefusal,
  engineArrowUci,
  explorationLineUcis,
  gameBoardDrive,
  shownLineMoveUci,
  type CoachingBoardDriveState,
} from "./coachingBoardDrive"
import { boardMomentCommentary } from "./boardMomentCommentary"
import { coachMarkOverlay } from "./coachingBoardMarks"
import {
  explorationBranchPath,
  type CoachingBoardExplorationBranch,
} from "./coachingBoardSnapshot"
import { useBoardExploration } from "./useBoardExploration"
import { useCoachingBoardDrive } from "./useCoachingBoardDrive"
import { useCoachingBoardTools } from "./useCoachingBoardTools"
import { useGameLineExploration } from "./useGameLineExploration"

export function CoachingBoardChosenGame({
  authorizedPlayerId,
  fetchAccessToken,
  gameImportId,
  importedGame,
  review,
  targetHost,
}: {
  authorizedPlayerId: string
  fetchAccessToken?: FetchAccessToken
  gameImportId: GameImportId
  importedGame: ImportedGame
  review: GameReview | null
  targetHost?: CoachingBoardTargetHost
}) {
  // A board rendered with no target host has no page above it: it opens on
  // the initial revision, and there is nowhere for it to navigate.
  const page = targetHost?.page
  const {
    agent,
    beginPendingMove,
    followExploredLine,
    host: driveHost,
    player,
    selectPly,
    state,
  } = useCoachingBoardDrive(
    () =>
      gameBoardDrive({
        gameImportId,
        importedGame,
        pageRevision: page?.readRevision(),
        review,
      }),
    page,
  )
  const view = chosenGameView(importedGame, review, state.viewedPly)
  const branch = activeExploredBranch(state) ?? null
  const position = driveCurrentBoardPosition(state)
  const execute = fetchAccessToken
    ? coachingBoardCommandExecute(fetchAccessToken)
    : null
  const teardown = useMemo(() => new AbortController(), [])
  useEffect(() => () => teardown.abort(), [teardown])

  const exploration = useGameLineExploration({
    activeBranch: branch,
    applyBranches: player.applyBranches,
    beginPendingMove,
    execute,
    followExploredLine,
    gameImportId,
    line: explorationLineUcis(state),
    playerId: authorizedPlayerId,
    signal: teardown.signal,
    viewedPly: state.viewedPly,
  })
  const selection = useBoardExploration({
    explore: (uci) => void exploration.explore(uci),
    exploring: exploration.busy,
    position,
  })

  function browseToPly(ply: number) {
    selection.clearSelection()
    exploration.clearNotice()
    selectPly(ply)
  }

  function selectBranch(alternativeMoveId: AlternativeMoveId) {
    selection.clearSelection()
    exploration.clearNotice()
    player.setBoardPosition({ alternativeMoveId, kind: "alternativeMove" })
  }

  const path = explorationBranchPath(state.branches, state.activeBranchId)
  const onPath = new Set(path.map((step) => step.alternativeMoveId))
  const coachDrew = coachMarkOverlay(state.marks)
  const playback = drivePlayback(state)
  const linePlayback = playback
    ? {
        index: playback.index,
        onStep: (target: number) => {
          selection.clearSelection()
          exploration.clearNotice()
          player.stepLine(target)
        },
        steps: playback.steps,
      }
    : null

  /**
   * When a drag on the board becomes an Alternative Move.
   *
   * Three ways it does not. Without a token the engine is out of reach, so
   * the board browses rather than accepting moves it cannot evaluate. While a
   * move is in flight the engine admits one evaluation at a time and the
   * board has not moved yet, so a second drag would have nothing to attach
   * to. And mid-walk the board is showing a position off the Game's own line,
   * where a click would be evaluated at the ply the walk started from — a
   * move in a position it was never played in. Stepping back to the line's
   * start hands the board back.
   */
  const acceptsMoves =
    execute !== null && !exploration.busy && (linePlayback?.index ?? 0) === 0

  const branchAffordances: BoardWorkspaceBranchAffordances = {
    // Only while the board is inside a branch. The line on screen already has
    // its own strip, so this lists the other lines tried from this origin
    // rather than repeating the active one. Back on the Game's own line there
    // is no line to sit beside, and every branch ever explored would pile up
    // under the board and shove it as the Player walks the Game.
    exploredBranches: branch
      ? branchLabels(
          state,
          state.branches.filter(
            (explored) => !onPath.has(explored.alternativeMoveId),
          ),
        )
      : [],
    onSelectBranch: selectBranch,
    onStrongestReply: (uci) => void exploration.explore(uci),
    path: branchLabels(state, path),
    strongestReplyLabel:
      branch?.strongestReply?.kind === "offered"
        ? playerVisibleStrongestReply(
            branch.strongestReply,
            branch.resultingPosition.fen,
          )
        : null,
  }

  const host = {
    ...driveHost,
    openOpeningLine: (openingLineRef: OpeningLineRef) => {
      if (!page) {
        return driveRefusal("unreachablePosition", driveHost.readSnapshot())
      }
      page.navigateAsAgent(coachingBoardOpeningPath(openingLineRef))
      return { ...lobbyResult(), openingLineRef, outcome: "opened" }
    },
    openReviewedGame: (requestedId: GameImportId) => {
      if (!page) {
        return driveRefusal("unreachablePosition", driveHost.readSnapshot())
      }
      page.navigateAsAgent(coachingBoardGamePath(requestedId))
      return { ...lobbyResult(), gameImportId: requestedId, outcome: "opened" }
    },
    evaluatePlayerLine: (
      input: Parameters<typeof driveHost.evaluatePlayerLine>[0],
    ) =>
      execute
        ? evaluatePlayerLineOnBoard({
            applyBranches: agent.applyBranches,
            execute,
            input,
            playerId: authorizedPlayerId,
            signal: teardown.signal,
            snapshot: driveHost.readSnapshot(),
          })
        : driveHost.evaluatePlayerLine(input),
    listCriticalMoments: (requestedId: string) =>
      execute
        ? listCriticalMomentsOnBoard({
            execute,
            gameImportId: requestedId,
            snapshot: driveHost.readSnapshot(),
          })
        : driveHost.listCriticalMoments(requestedId),
    openReviewMomentInPlace: (
      input: Parameters<typeof driveHost.openReviewMomentInPlace>[0],
    ) =>
      execute
        ? openReviewMomentInPlaceOnBoard({
            execute,
            gameImportId: input.gameImportId,
            moment: input.moment,
            onOpened: (detail) => {
              if (detail.gameImportId !== gameImportId) {
                return driveHost.readSnapshot()
              }
              const moved = driveHost.setBoardPosition({
                kind: "ply",
                ply: detail.ply,
              })
              return "origin" in moved ? moved : driveHost.readSnapshot()
            },
            snapshot: driveHost.readSnapshot(),
          })
        : driveHost.openReviewMomentInPlace(input),
  }

  useCoachingBoardTools({
    authorizedPlayerId,
    host,
    surface: "board",
  })

  return (
    <CoachingBoardShell
      board={
        <>
          {exploration.notice ? (
            <WatercolorNotice glyph="!" heading="Exploring" tone="vermilion">
              {exploration.notice}
            </WatercolorNotice>
          ) : exploration.progress ? (
            // The engine already streams which stage a move is in, including
            // the wait for an engine lease. Saying so beats a board that sits
            // still with no reason given.
            <WatercolorNotice glyph="…" heading="Exploring">
              {exploration.progress}
            </WatercolorNotice>
          ) : null}
          <BoardWorkspace
            alternativeBusy={exploration.busy}
            arrows={[
              ...boardArrowsFrom([engineMoveArrow(engineArrowUci(state))]),
              ...coachDrew.arrows,
            ]}
            branch={branch}
            branchAffordances={branchAffordances}
            copyPositionReferent={writeClipboardText}
            criticalPly={view.criticalPly}
            destinations={selection.destinations}
            evaluation={branch?.evaluation.selectedMove ?? view.evaluation}
            evaluationPoints={view.evaluationPoints}
            heading={view.heading}
            // The refutation answers the played move, so its line roots after
            // it; every other view of the Game stands before the caption's move.
            headingPlayed={state.shownLine?.kind === "playedMoveRefutation"}
            importedGame={importedGame}
            interactionDisabled={!acceptsMoves}
            linePlayback={linePlayback}
            marks={coachDrew.squares}
            momentMarkers={view.momentMarkers}
            // Browsing is not exploring. Waiting on Stockfish is no reason to
            // stop the Player reading the rest of their own Game.
            navigationDisabled={false}
            onCancel={exploration.cancel}
            onExitBranch={() => browseToPly(state.viewedPly)}
            onNavigate={browseToPly}
            onPromote={selection.promote}
            onSquare={selection.selectSquare}
            orientation={state.orientation}
            position={position}
            promotion={selection.promotion}
            selectedSquare={selection.selectedSquare}
            shownLineLabel={
              state.shownLine ? shownLineLabel(state.shownLine) : null
            }
            shownLineMove={shownLineMoveUci(state)}
            viewedPly={state.viewedPly}
          />
        </>
      }
      session={
        <CoachingBoardSession
          commentary={boardMomentCommentary(state)}
          evaluationPoints={view.evaluationPoints}
          learningPaths={view.learningPaths}
          maxPly={view.maxPly}
          momentMarkers={view.momentMarkers}
          onSelect={browseToPly}
          viewedPly={state.viewedPly}
        />
      }
      target={view.target}
      targetHost={targetHost}
    />
  )
}

/** A branch reads as the move that made it, named from the position it was
 * played in — the same naming `boardHeading` gives the active one. */
function branchLabels(
  state: CoachingBoardDriveState,
  branches: readonly CoachingBoardExplorationBranch[],
): ExploredBranchLabel[] {
  return branches.map((explored) => ({
    alternativeMoveId: explored.alternativeMoveId,
    label: playerVisibleSanFromLegalUci(
      branchSourceFen(state, explored),
      explored.moveUci,
    ),
    selectedMove: explored.evaluation.selectedMove,
  }))
}

function chosenGameView(
  importedGame: ImportedGame,
  review: GameReview | null,
  viewedPly: number,
) {
  const moves = importedGame.game.moves
  const currentMove = moves.find((move) => move.ply === viewedPly)
  const moment = review?.criticalMoments.find(
    (candidate) => candidate.ply === viewedPly,
  )
  return {
    criticalPly: moment?.ply ?? viewedPly,
    evaluation:
      review?.evaluationTimeline.find((point) => point.ply === viewedPly)
        ?.evaluation ?? null,
    evaluationPoints: (review?.evaluationTimeline ?? []).map((point) =>
      evaluationPoint(point.ply, point.evaluation),
    ),
    heading: currentMove
      ? moveLabel(currentMove)
      : PLAYER_VISIBLE_MOVE_FALLBACK,
    learningPaths: learningPathsForReview(review, viewedPly),
    maxPly: moves.at(-1)?.ply ?? viewedPly,
    momentMarkers: review ? frozenReviewMomentMarkers(review) : [],
    reviewMomentId: moment?.criticalMomentId ?? null,
    target: `${playerName(importedGame, "white")} — ${playerName(importedGame, "black")}`,
  }
}

function learningPathsForReview(review: GameReview | null, viewedPly: number) {
  if (!review) return []
  const current = review.criticalMoments.find(
    (moment) => moment.ply === viewedPly,
  )
  const moments = current
    ? [current, ...review.criticalMoments]
    : review.criticalMoments
  for (const moment of moments) {
    if (moment.learningMaterial.tracks.length === 0) continue
    return learningPathsForReviewMoment(
      moment.learningMaterial,
      moment.criticalMomentId,
    )
  }
  return []
}

function playerName(importedGame: ImportedGame, color: "white" | "black") {
  const name = importedGame.game[color].name
  return name.kind === "present"
    ? name.value
    : color === "white"
      ? "White"
      : "Black"
}
