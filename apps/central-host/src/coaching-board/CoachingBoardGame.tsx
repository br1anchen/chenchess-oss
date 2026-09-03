import { useEffect, useRef, useState } from "react"
import type {
  GameImportId,
  GameReview,
  ImportedGame,
} from "@chenchess/coach-engine-sdk"
import { WatercolorNotice } from "@chenchess/ui"

import type { FetchAccessToken } from "@/review-session/client"

import { useReviewSessionCommands } from "@/review-session/useReviewSessionCommands"

import { CoachingBoardChosenGame } from "./CoachingBoardChosenGame"
import { CoachingBoardShell } from "./CoachingBoardShell"
import type { CoachingBoardTargetHost } from "./coachingBoardTargetSwitch"

export function CoachingBoardGame({
  authorizedPlayerId,
  fetchAccessToken,
  gameImportId,
  targetHost,
}: {
  authorizedPlayerId: string | null
  fetchAccessToken: FetchAccessToken
  gameImportId: GameImportId
  targetHost?: CoachingBoardTargetHost
}) {
  const { failure, run } = useReviewSessionCommands(fetchAccessToken)
  const [importedGame, setImportedGame] = useState<ImportedGame | null>(null)
  const [review, setReview] = useState<GameReview | null>(null)
  const opened = useRef<GameImportId | null>(null)

  useEffect(() => {
    if (!authorizedPlayerId) return
    if (opened.current === gameImportId) return
    opened.current = gameImportId
    void run(
      "import",
      { gameImportId, kind: "readGameReviewSnapshot" },
      "Open",
    ).then((read) => {
      if (read?.kind !== "gameReviewSnapshotRead") return
      setImportedGame(read.importedGame)
      setReview(read.review)
    })
  }, [authorizedPlayerId, gameImportId, run])

  if (!authorizedPlayerId) {
    return (
      <CoachingBoardShell
        board={
          <WatercolorNotice glyph="…" heading="Coaching">
            Sign in with Beta Access to open this Game Import.
          </WatercolorNotice>
        }
        session={null}
        target={gameImportId}
        targetHost={targetHost}
      />
    )
  }

  if (!importedGame) {
    return (
      <CoachingBoardShell
        board={
          failure ? (
            <WatercolorNotice glyph="!" heading="Coaching" tone="vermilion">
              {failure}
            </WatercolorNotice>
          ) : null
        }
        session={null}
        target={gameImportId}
        targetHost={targetHost}
      />
    )
  }

  return (
    <CoachingBoardChosenGame
      authorizedPlayerId={authorizedPlayerId}
      fetchAccessToken={fetchAccessToken}
      gameImportId={gameImportId}
      importedGame={importedGame}
      review={review}
      targetHost={targetHost}
    />
  )
}
