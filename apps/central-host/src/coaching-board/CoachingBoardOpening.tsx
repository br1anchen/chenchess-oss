import { useEffect, useMemo, useState } from "react"
import {
  CoachEngineClient,
  resolveOpeningLine,
  type GameImportId,
} from "@chenchess/coach-engine-sdk"
import { PLAYER_VISIBLE_MOVE_FALLBACK } from "@chenchess/review-projection"
import { WatercolorNotice } from "@chenchess/ui"

import { writeClipboardText } from "@/clipboard"
import { BoardWorkspace } from "@/review-session/BoardWorkspace"
import type { FetchAccessToken } from "@/review-session/client"
import { moveLabel } from "@/review-session/model"

import { CoachingBoardOpeningStudy } from "./CoachingBoardOpeningStudy"
import { CoachingBoardShell } from "./CoachingBoardShell"
import type { CoachingBoardTargetHost } from "./coachingBoardTargetSwitch"
import { lobbyResult } from "./coachingBoardConstraints"
import type { CoachingBoardPage } from "./coachingBoardPage"
import {
  driveCurrentBoardPosition,
  openingBoardDrive,
} from "./coachingBoardDrive"
import { coachMarkOverlay } from "./coachingBoardMarks"
import {
  coachingBoardGamePath,
  coachingBoardOpeningPath,
} from "./coachingBoardRoute"
import { openingCatalogRow, type OpeningCatalogRow } from "./openingLineCatalog"
import { openingLineTitle, type OpeningLineRef } from "./openingLineRef"
import {
  recallOpeningExploration,
  retainOpeningExploration,
} from "./openingExplorationRetention"
import {
  evaluateOpeningContinuationOnBoard,
  type OpeningContinuationInput,
} from "./openingContinuationTool"
import { openingLineMoves, openingLineViewedPly } from "./openingMoves"
import { openingStudyWorld } from "./openingStudyWorld"
import { useCoachingBoardDrive } from "./useCoachingBoardDrive"
import { useCoachingBoardTools } from "./useCoachingBoardTools"

export function CoachingBoardOpening({
  authorizedPlayerId,
  fetchAccessToken,
  initialViewedPly,
  openingLineRef,
  page,
  targetHost,
}: {
  authorizedPlayerId: string | null
  fetchAccessToken?: FetchAccessToken
  initialViewedPly?: number
  openingLineRef: OpeningLineRef
  page: CoachingBoardPage
  targetHost?: CoachingBoardTargetHost
}) {
  const study = openingCatalogRow(openingLineRef)
  if (study) {
    return (
      <GroundedOpeningBoard
        authorizedPlayerId={authorizedPlayerId}
        fetchAccessToken={fetchAccessToken}
        initialViewedPly={initialViewedPly}
        line={study}
        openingLineRef={openingLineRef}
        page={page}
        study={study}
        targetHost={targetHost}
      />
    )
  }
  return (
    <ResolvedOpeningBoard
      authorizedPlayerId={authorizedPlayerId}
      fetchAccessToken={fetchAccessToken}
      initialViewedPly={initialViewedPly}
      openingLineRef={openingLineRef}
      page={page}
      targetHost={targetHost}
    />
  )
}

/**
 * A line outside the local study catalog grounds through the engine's
 * resolve read: the same pinned catalog the find corpus uses, addressed by
 * the same constructor.
 */
function ResolvedOpeningBoard({
  authorizedPlayerId,
  fetchAccessToken,
  initialViewedPly,
  openingLineRef,
  page,
  targetHost,
}: {
  authorizedPlayerId: string | null
  fetchAccessToken?: FetchAccessToken
  initialViewedPly?: number
  openingLineRef: OpeningLineRef
  page: CoachingBoardPage
  targetHost?: CoachingBoardTargetHost
}) {
  const [resolved, setResolved] = useState<
    | { kind: "loading" }
    | { kind: "unavailable" }
    | { kind: "unknown" }
    | { kind: "resolved"; line: OpeningLineFacts }
  >({ kind: "loading" })

  useEffect(() => {
    let active = true
    resolveOpeningLine(openingLineRef).then(
      (outcome) => {
        if (!active) return
        setResolved(
          outcome.outcome === "resolved"
            ? { kind: "resolved", line: outcome.line }
            : { kind: "unknown" },
        )
      },
      () => {
        // A failed read says nothing about the address; only the engine's
        // typed outcome may claim the line does not exist.
        if (active) setResolved({ kind: "unavailable" })
      },
    )
    return () => {
      active = false
    }
  }, [openingLineRef])

  if (resolved.kind === "resolved") {
    return (
      <GroundedOpeningBoard
        authorizedPlayerId={authorizedPlayerId}
        fetchAccessToken={fetchAccessToken}
        initialViewedPly={initialViewedPly}
        line={resolved.line}
        openingLineRef={openingLineRef}
        page={page}
        study={null}
        targetHost={targetHost}
      />
    )
  }
  return (
    <CoachingBoardShell
      board={
        resolved.kind === "unknown" ? (
          <WatercolorNotice glyph="…" heading="Coaching">
            This address names no Opening Line in the pinned catalog. Find an
            opening to get an address that opens.
          </WatercolorNotice>
        ) : resolved.kind === "unavailable" ? (
          <WatercolorNotice glyph="!" heading="Coaching" tone="vermilion">
            This Opening Line could not be read right now. Try again in a
            moment.
          </WatercolorNotice>
        ) : null
      }
      session={null}
      target={openingLineTitle(openingLineRef)}
      targetHost={targetHost}
    />
  )
}

type OpeningLineFacts = {
  eco: string
  name: string
  path: string
}

function GroundedOpeningBoard({
  authorizedPlayerId,
  fetchAccessToken,
  initialViewedPly,
  line,
  openingLineRef,
  page,
  study,
  targetHost,
}: {
  authorizedPlayerId: string | null
  fetchAccessToken?: FetchAccessToken
  initialViewedPly?: number
  line: OpeningLineFacts
  openingLineRef: OpeningLineRef
  page: CoachingBoardPage
  study: OpeningCatalogRow | null
  targetHost?: CoachingBoardTargetHost
}) {
  const moves = openingLineMoves(line.path)
  const {
    agent,
    host: driveHost,
    player,
    selectPly,
    snapshot,
    state,
  } = useCoachingBoardDrive(() => {
    const recalled = recallOpeningExploration(
      authorizedPlayerId,
      openingLineRef,
    )
    return openingBoardDrive({
      activeBranchId: recalled?.activeBranchId,
      branches: recalled?.branches,
      eco: line.eco,
      moves,
      name: line.name,
      openingLineRef,
      pageRevision: page.readRevision(),
      viewedPly:
        initialViewedPly ?? recalled?.viewedPly ?? openingLineViewedPly(moves),
      // The session lives in the drive so the agent reads it from every
      // snapshot; a line with no authored world studies from the prose ideas.
      world: openingStudyWorld(openingLineRef),
    })
  }, page)
  useEffect(() => {
    // Merely visiting a line is not exploration; retaining every visit
    // would let browsing evict real exploration from the five slots.
    if (state.branches.length === 0 && state.activeBranchId === null) return
    retainOpeningExploration(authorizedPlayerId, openingLineRef, {
      activeBranchId: state.activeBranchId,
      branches: state.branches,
      viewedPly: state.viewedPly,
    })
  }, [
    authorizedPlayerId,
    openingLineRef,
    state.activeBranchId,
    state.branches,
    state.viewedPly,
  ])
  // Board tools register only for an authorized Player, so the client this
  // builds is only ever asked for by a caller that has one.
  const client = useMemo(
    () =>
      fetchAccessToken
        ? new CoachEngineClient({
            credential: async () =>
              (await fetchAccessToken({ forceRefreshToken: true })) ?? "",
          })
        : null,
    [fetchAccessToken],
  )
  const position = driveCurrentBoardPosition(state)
  const coachDrew = coachMarkOverlay(state.marks)
  const currentMove = moves.find((move) => move.ply === state.viewedPly)
  const heading = currentMove
    ? moveLabel(currentMove)
    : PLAYER_VISIBLE_MOVE_FALLBACK
  const target = `${line.eco} · ${line.name}`
  const host = {
    ...driveHost,
    evaluateOpeningContinuation: (input: OpeningContinuationInput) =>
      client
        ? evaluateOpeningContinuationOnBoard({
            analyze: (request) => client.analyzeOpeningLine(request),
            applyBranches: agent.applyBranches,
            boardLineRef: openingLineRef,
            input,
            snapshot: driveHost.readSnapshot(),
          })
        : driveHost.evaluateOpeningContinuation(input),
    openOpeningLine: (requestedRef: OpeningLineRef) => {
      page.navigateAsAgent(coachingBoardOpeningPath(requestedRef))
      return {
        ...lobbyResult(),
        openingLineRef: requestedRef,
        outcome: "opened",
      }
    },
    openReviewedGame: (gameImportId: GameImportId) => {
      page.navigateAsAgent(coachingBoardGamePath(gameImportId))
      return { ...lobbyResult(), gameImportId, outcome: "opened" }
    },
  }

  useCoachingBoardTools({
    authorizedPlayerId,
    host,
    surface: "board",
  })

  return (
    <CoachingBoardShell
      board={
        <BoardWorkspace
          alternativeBusy={false}
          arrows={coachDrew.arrows}
          branch={null}
          copyPositionReferent={writeClipboardText}
          criticalPly={state.viewedPly}
          destinations={[]}
          evaluation={null}
          evaluationPoints={[]}
          heading={heading}
          interactionDisabled
          marks={coachDrew.squares}
          momentMarkers={[]}
          moves={moves}
          navigationDisabled={false}
          onExitBranch={() => undefined}
          onNavigate={selectPly}
          onPromote={() => undefined}
          onSquare={() => undefined}
          orientation={state.orientation}
          position={position}
          promotion={null}
          selectedSquare={null}
          showMoveList={study === null}
          viewedPly={state.viewedPly}
        />
      }
      session={
        study ? (
          <CoachingBoardOpeningStudy
            currentRef={openingLineRef}
            moves={moves}
            onOpenLine={(ref) =>
              page.navigateAsPlayer(coachingBoardOpeningPath(ref))
            }
            onSelectPly={selectPly}
            row={study}
            study={
              snapshot.study
                ? {
                    answer: player.answerStudyCard,
                    copyReferent: writeClipboardText,
                    restart: player.restartStudy,
                    study: snapshot.study,
                  }
                : null
            }
            viewedPly={state.viewedPly}
          />
        ) : null
      }
      target={target}
      targetHost={targetHost}
    />
  )
}
