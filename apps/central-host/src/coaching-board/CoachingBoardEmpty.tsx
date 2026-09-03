import { PLAYER_VISIBLE_MOVE_FALLBACK } from "@chenchess/review-projection"

import { BoardWorkspace } from "@/review-session/BoardWorkspace"

import { CoachingBoardShell } from "./CoachingBoardShell"
import type { CoachingBoardTargetPane } from "./CoachingBoardTargetDialog"
import type { CoachingBoardTargetHost } from "./coachingBoardTargetSwitch"
import { startingBoardPosition } from "./openingMoves"

export function CoachingBoardEmpty({
  initialTargetPane = "import",
  targetHost,
}: {
  initialTargetPane?: CoachingBoardTargetPane
  targetHost: CoachingBoardTargetHost
}) {
  return (
    <CoachingBoardShell
      board={
        <BoardWorkspace
          alternativeBusy={false}
          branch={null}
          criticalPly={1}
          destinations={[]}
          evaluation={null}
          evaluationPoints={[]}
          heading={PLAYER_VISIBLE_MOVE_FALLBACK}
          interactionDisabled
          momentMarkers={[]}
          moves={[]}
          navigationDisabled
          onExitBranch={() => undefined}
          onNavigate={() => undefined}
          onPromote={() => undefined}
          onSquare={() => undefined}
          orientation="white"
          position={startingBoardPosition()}
          promotion={null}
          selectedSquare={null}
          viewedPly={1}
        />
      }
      initialTargetPane={initialTargetPane}
      registerTargetTools
      session={null}
      targetHost={targetHost}
    />
  )
}
