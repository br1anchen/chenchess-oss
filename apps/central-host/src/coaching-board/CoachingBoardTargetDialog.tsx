import { useMemo, useRef, useState, type ReactNode } from "react"
import type {
  GameImportId,
  GameReview,
  PlayedOpeningAggregate,
  ReviewedGameSearchCard,
  ReviewedGameSearchRequest,
  ReviewSide,
} from "@chenchess/coach-engine-sdk"
import {
  List,
  ListItem,
  Text,
  VStack,
  WatercolorButton,
  WatercolorField,
  WatercolorInput,
  WatercolorNotice,
  WatercolorSelect,
} from "@chenchess/ui"

import {
  parseImportGameRequest,
  preselectedReviewSide,
} from "@/daily-coaching/importGameRequest"
import { RecentGamesCarousel } from "@/daily-coaching/RecentGamesCarousel"

import {
  consumeAnonymousAllowance,
  localAnonymousAttemptStore,
  type AnonymousAttemptStore,
} from "./anonymousRateLimit"
import { coachingBoardStyles } from "./coachingBoard.styles"
import {
  lobbyConstraints,
  lobbyResult,
  unavailableLobbyResult,
} from "./coachingBoardConstraints"
import { unavailableBoardCoachResult } from "./coachingBoardCoachTools"
import { driveRefusal } from "./coachingBoardDrive"
import type { CoachingBoardPage } from "./coachingBoardPage"
import type { PlayedOpening } from "./openingLineCatalog"
import {
  findOpeningLines,
  readOpeningLinesFromCatalog,
  type OpeningFindMatch,
  type OpeningLineFindResult,
  type OpeningLineLookup,
} from "./openingLineFind"
import {
  coachingBoardGamePath,
  coachingBoardOpeningPath,
} from "./coachingBoardRoute"
import { parseOpeningLineRef, type OpeningLineRef } from "./openingLineRef"
import {
  applyStagedGameImport,
  emptyGameImportFields,
  gameImportFieldsEdited,
  type GameImportFields,
} from "./stagedGameImport"
import {
  latestGameControlVisible,
  recentGamesCarouselVisible,
  type CoachingBoardTargetHost,
  type LatestPlayingProfileGame,
} from "./coachingBoardTargetSwitch"
import { useCoachingBoardTools } from "./useCoachingBoardTools"

const reviewSideOptions = [
  { label: "White", value: "white" },
  { label: "Black", value: "black" },
  { label: "Both sides (pasted PGN)", value: "both" },
]

export type CoachingBoardTargetPane = "chooser" | "import" | "find"

export function CoachingBoardTargetDialog({
  anonymousAttemptStore,
  authorizedPlayerId,
  disclosure,
  findOpeningLines: findOpeningLinesLookup = readOpeningLinesFromCatalog,
  importFailure,
  importing,
  initialPane = "import",
  latestGame = null,
  listPlayedOpenings,
  listRecentProfileGames,
  page,
  playedAggregate = [],
  onCommitImport,
  onOpenChange,
  playedOpenings,
  recentReviewedGames,
  registerTools,
  searchReviewedGames,
}: {
  anonymousAttemptStore?: AnonymousAttemptStore
  authorizedPlayerId: string | null
  disclosure?: ReactNode
  findOpeningLines?: OpeningLineLookup
  importFailure: string | null
  importing: boolean
  initialPane?: CoachingBoardTargetPane
  latestGame?: LatestPlayingProfileGame | null
  listPlayedOpenings?: () => object | Promise<object>
  listRecentProfileGames?: () => object | Promise<object>
  page: CoachingBoardPage
  playedAggregate?: readonly PlayedOpeningAggregate[]
  onCommitImport: (fields: GameImportFields) => void
  onOpenChange: (open: boolean) => void
  playedOpenings: readonly PlayedOpening[]
  recentReviewedGames?: {
    games: readonly ReviewedGameSearchCard[]
    loadPreview: (gameImportId: GameImportId) => Promise<GameReview>
  }
  registerTools: boolean
  searchReviewedGames?: (
    request: ReviewedGameSearchRequest,
  ) => object | Promise<object>
}) {
  // The anonymous import-form allowance is spent when the form opens. The
  // form is the default pane now, so the landing render itself is an opening
  // and must pass the same gate the tab click passes.
  const [initialAdmission] = useState(() => {
    if (initialPane !== "import" || authorizedPlayerId) {
      return { pane: initialPane, refused: false }
    }
    const store =
      anonymousAttemptStore ?? localAnonymousAttemptStore(window.localStorage)
    return consumeAnonymousAllowance(store)
      ? { pane: initialPane, refused: false }
      : { pane: "chooser" as const, refused: true }
  })
  const [pane, setPane] = useState<CoachingBoardTargetPane>(
    initialAdmission.pane,
  )
  const [fields, setFields] = useState(emptyGameImportFields)
  const [findQuery, setFindQuery] = useState("")
  const [findMatches, setFindMatches] = useState<OpeningFindMatch[]>([])
  const [findTruncation, setFindTruncation] = useState<
    OpeningLineFindResult["truncation"] | null
  >(null)
  const [invalid, setInvalid] = useState<string | null>(
    initialAdmission.refused ? "Try again later." : null,
  )
  const playerEdited = useRef(false)
  const baseline = useRef(emptyGameImportFields)
  const findRun = useRef(0)

  const host = useMemo(
    () => ({
      findOpeningLine: async (query: string) => {
        const found = await findOpeningLines(
          query,
          playedOpenings,
          findOpeningLinesLookup,
        )
        setFindQuery(query)
        setFindMatches(found.matches)
        setFindTruncation(found.truncation)
        setPane("find")
        onOpenChange(true)
        return {
          constraints: lobbyConstraints(),
          kind: "lobby" as const,
          matches: found.matches.map((match) => ({
            eco: match.eco,
            name: match.name,
            openingLineRef: match.ref,
            path: match.path,
            played: match.played,
          })),
          truncation: found.truncation,
        }
      },
      openOpeningLine: (openingLineRef: OpeningLineRef) => {
        page.navigateAsAgent(coachingBoardOpeningPath(openingLineRef))
        onOpenChange(false)
        return { constraints: lobbyConstraints(), kind: "lobby" as const }
      },
      openReviewedGame: (gameImportId: GameImportId) => {
        page.navigateAsAgent(coachingBoardGamePath(gameImportId))
        onOpenChange(false)
        return { ...lobbyResult(), gameImportId, outcome: "opened" as const }
      },
      evaluateOpeningContinuation: () => unavailableBoardCoachResult(null),
      evaluatePlayerLine: () => unavailableBoardCoachResult(null),
      listCriticalMoments: () => unavailableBoardCoachResult(null),
      listRecentProfileGames: () =>
        listRecentProfileGames
          ? listRecentProfileGames()
          : { ...lobbyResult(), outcome: "noPlayingProfile" as const },
      openReviewMomentInPlace: () => unavailableBoardCoachResult(null),
      readSnapshot: () => null,
      listPlayedOpenings: () =>
        listPlayedOpenings ? listPlayedOpenings() : unavailableLobbyResult(),
      searchReviewedGames: (request: ReviewedGameSearchRequest) =>
        searchReviewedGames
          ? searchReviewedGames(request)
          : unavailableLobbyResult(),
      annotateBoard: () => driveRefusal("staleRevision", null),
      setBoardPosition: () => driveRefusal("unreachablePosition", null),
      showLine: () => driveRefusal("noRenderOption", null),
      stepLine: () => driveRefusal("noLineShown", null),
      turnBoard: () => driveRefusal("unreachablePosition", null),
      stageGameImport: (staged: GameImportFields) => {
        // The structured tool shares the form's validator, so the two paths
        // cannot diverge on what is legal; a bad field comes back as a typed
        // per-field refusal instead of a parse failure.
        const parsed = parseImportGameRequest(staged)
        if (parsed.kind === "invalid") {
          return {
            constraints: lobbyConstraints(),
            kind: "lobby" as const,
            outcome: "refused" as const,
            refusals: { [parsed.field]: parsed.message },
          }
        }
        const next = applyStagedGameImport(fields, staged, playerEdited.current)
        if (next.kind === "applied") {
          setFields(next.fields)
          baseline.current = next.fields
          setPane("import")
          onOpenChange(true)
        }
        return {
          constraints: lobbyConstraints(),
          kind: "lobby" as const,
          outcome: next.kind,
        }
      },
    }),
    [
      fields,
      findOpeningLinesLookup,
      listPlayedOpenings,
      listRecentProfileGames,
      onOpenChange,
      page,
      playedOpenings,
      searchReviewedGames,
    ],
  )

  useCoachingBoardTools({
    authorizedPlayerId: registerTools ? authorizedPlayerId : null,
    host,
    surface: "lobby",
  })

  function changeSource(source: string) {
    const next = { ...fields, source }
    const preselected = preselectedReviewSide(source)
    if (preselected) next.reviewSide = preselected
    setFields(next)
    playerEdited.current = gameImportFieldsEdited(next, baseline.current)
  }

  function changeReviewSide(reviewSide: ReviewSide) {
    const next = { ...fields, reviewSide }
    setFields(next)
    playerEdited.current = gameImportFieldsEdited(next, baseline.current)
  }

  function changeElo(elo: string) {
    const next = { ...fields, elo }
    setFields(next)
    playerEdited.current = gameImportFieldsEdited(next, baseline.current)
  }

  function stageLatestGame() {
    if (!latestGame) return
    // A click on Latest game is the Player's own edit, not an agent proposal:
    // it always applies, even over fields the Player typed earlier.
    const next = {
      elo: "",
      reviewSide: latestGame.reviewSide,
      source: latestGame.source,
    }
    setFields(next)
    baseline.current = next
    playerEdited.current = false
  }

  function openImport() {
    if (pane === "import") return
    if (!authorizedPlayerId) {
      const store =
        anonymousAttemptStore ?? localAnonymousAttemptStore(window.localStorage)
      if (!consumeAnonymousAllowance(store)) {
        setInvalid("Try again later.")
        return
      }
    }
    setInvalid(null)
    setPane("import")
  }

  function commitImport() {
    const parsed = parseImportGameRequest(fields)
    if (parsed.kind === "invalid") {
      setInvalid(parsed.message)
      return
    }
    setInvalid(null)
    onCommitImport(fields)
  }

  function runFind() {
    const run = ++findRun.current
    void findOpeningLines(
      findQuery,
      playedOpenings,
      findOpeningLinesLookup,
    ).then(
      (found) => {
        if (findRun.current !== run) return
        setFindMatches(found.matches)
        setFindTruncation(found.truncation)
      },
      () => {
        if (findRun.current !== run) return
        setFindMatches([])
        setFindTruncation(null)
      },
    )
  }

  function openLine(ref: OpeningLineRef) {
    page.navigateAsPlayer(coachingBoardOpeningPath(ref))
    onOpenChange(false)
  }

  const message = invalid ?? importFailure
  // The search control offers the Player's most-played openings before they
  // type. Empty aggregate means the placeholder alone — no curated fallback,
  // and a row without a resolvable address is not offered as a dead button.
  const topPlayedOpenings = playedAggregate
    .flatMap((opening) => {
      const ref = opening.openingLineRef
        ? parseOpeningLineRef(opening.openingLineRef)
        : undefined
      return ref ? [{ opening, ref }] : []
    })
    .slice(0, 5)

  return (
    <VStack gap={3} hAlign="stretch">
      <VStack gap={2} hAlign="stretch" xstyle={coachingBoardStyles.dialogExits}>
        <WatercolorButton
          aria-pressed={pane === "import"}
          block
          onClick={openImport}
          type="button"
          variant={pane === "import" ? "primary" : "secondary"}
        >
          Import a game
        </WatercolorButton>
        <WatercolorButton
          aria-pressed={pane === "find"}
          block
          onClick={() => {
            setInvalid(null)
            setPane("find")
          }}
          type="button"
          variant={pane === "find" ? "primary" : "secondary"}
        >
          Choose an opening
        </WatercolorButton>
      </VStack>
      {pane === "import" ? (
        <VStack gap={3} hAlign="stretch">
          {disclosure}
          <ReviewedGameEntry
            authorizedPlayerId={authorizedPlayerId}
            latestGame={latestGame}
            onStageLatestGame={stageLatestGame}
            recentReviewedGames={recentReviewedGames}
          />
          <WatercolorField label="Game URL or PGN">
            <WatercolorInput
              name="gameSource"
              onChange={(event) => changeSource(event.target.value)}
              placeholder="Paste a Chess.com or Lichess game URL, or a full PGN…"
              value={fields.source}
            />
          </WatercolorField>
          <VStack
            gap={2}
            hAlign="stretch"
            xstyle={coachingBoardStyles.importMeta}
          >
            <WatercolorField label="Review side">
              <WatercolorSelect
                name="reviewSide"
                onChange={(event) =>
                  changeReviewSide(parseReviewSide(event.target.value))
                }
                value={fields.reviewSide}
              >
                {reviewSideOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </WatercolorSelect>
            </WatercolorField>
            <WatercolorField label="Elo">
              <WatercolorInput
                name="elo"
                onChange={(event) => changeElo(event.target.value)}
                placeholder="From the game"
                value={fields.elo}
              />
            </WatercolorField>
          </VStack>
          <WatercolorButton
            disabled={importing || !fields.source.trim()}
            onClick={commitImport}
            type="button"
          >
            Import
          </WatercolorButton>
        </VStack>
      ) : null}
      {pane === "find" ? (
        <VStack gap={3} hAlign="stretch">
          {findQuery.trim() === "" && topPlayedOpenings.length > 0 ? (
            <List header={<Text type="supporting">Your openings</Text>}>
              {topPlayedOpenings.map(({ opening, ref }) => (
                <ListItem
                  key={`${opening.eco}:${opening.name}`}
                  label={`${opening.eco} · ${opening.name} · ${opening.playCount} played`}
                  onClick={() => openLine(ref)}
                />
              ))}
            </List>
          ) : null}
          <WatercolorField label="Find an opening">
            <WatercolorInput
              name="openingQuery"
              onChange={(event) => setFindQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault()
                  runFind()
                }
              }}
              placeholder="ECO, name, or line…"
              value={findQuery}
            />
          </WatercolorField>
          <WatercolorButton
            disabled={!findQuery.trim()}
            onClick={runFind}
            type="button"
            variant="secondary"
          >
            Find
          </WatercolorButton>
          {findMatches.length > 0 ? (
            <List header={<Text type="supporting">Matches</Text>}>
              {findMatches.map((match) => (
                <ListItem
                  key={match.ref}
                  label={`${match.eco} · ${match.name}`}
                  onClick={() => openLine(match.ref)}
                />
              ))}
            </List>
          ) : null}
          {findTruncation?.kind === "truncated" ? (
            <Text type="supporting">
              {`Showing ${findMatches.length} of ${findTruncation.totalMatchCount} matches. Narrow the query to see the rest.`}
            </Text>
          ) : null}
        </VStack>
      ) : null}
      {message ? (
        <WatercolorNotice glyph="!" heading="Import" tone="vermilion">
          {message}
        </WatercolorNotice>
      ) : null}
    </VStack>
  )
}

// The way into a Game the Player already has: their reviewed Games when
// there are any, and otherwise the one profile Game the import form can
// stage for them.
function ReviewedGameEntry({
  authorizedPlayerId,
  latestGame,
  onStageLatestGame,
  recentReviewedGames,
}: {
  authorizedPlayerId: string | null
  latestGame: LatestPlayingProfileGame | null
  onStageLatestGame: () => void
  recentReviewedGames?: CoachingBoardTargetHost["recentReviewedGames"]
}) {
  if (
    recentReviewedGames &&
    recentGamesCarouselVisible({ authorizedPlayerId, recentReviewedGames })
  ) {
    return (
      <RecentGamesCarousel
        games={recentReviewedGames.games}
        linkToGame={coachingBoardGamePath}
        loadPreview={recentReviewedGames.loadPreview}
      />
    )
  }
  if (!latestGameControlVisible({ authorizedPlayerId, latestGame })) return null
  return (
    <WatercolorButton
      block
      onClick={onStageLatestGame}
      type="button"
      variant="secondary"
    >
      Latest game
    </WatercolorButton>
  )
}

function parseReviewSide(value: string): ReviewSide {
  switch (value) {
    case "white":
    case "black":
    case "both":
      return value
    default:
      throw new TypeError("invalid import Review Side")
  }
}
