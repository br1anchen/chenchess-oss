import { useEffect, useMemo, useState } from "react"
import {
  CoachEngineClient,
  type PlayedOpeningAggregate,
  type ReviewedGameSearchRequest,
} from "@chenchess/coach-engine-sdk"
import {
  Heading,
  Section,
  Text,
  VStack,
  retentionDisclosureDescription,
} from "@chenchess/ui"

import { checkBetaAuthorization } from "@/auth/betaAuthorization"
import {
  useFirebaseAuth,
  type FetchAccessToken,
  type FirebaseIdentity,
} from "@/auth/FirebaseAuthProvider"
import type { Navigate } from "@/auth/RouteRedirect"
import { parseImportGameRequest } from "@/daily-coaching/importGameRequest"
import { useRecentReviewedGames } from "@/daily-coaching/useRecentReviewedGames"
import { useReviewRetentionPreference } from "@/review-session/useReviewRetentionPreference"
import { useReviewSessionCommands } from "@/review-session/useReviewSessionCommands"

import { CoachingBoardEmpty } from "./CoachingBoardEmpty"
import { CoachingBoardGame } from "./CoachingBoardGame"
import { CoachingBoardOpening } from "./CoachingBoardOpening"
import {
  useCoachingBoardPage,
  type CoachingBoardPage,
} from "./coachingBoardPage"
import type { CoachingBoardRoute } from "./coachingBoardRoute"
import { coachingBoardGamePath } from "./coachingBoardRoute"
import {
  latestPlayingProfileGameFromRead,
  type CoachingBoardTargetHost,
  type LatestPlayingProfileGame,
} from "./coachingBoardTargetSwitch"
import { lobbyConstraints } from "./coachingBoardConstraints"
import { useOpeningExplorationBoundary } from "./useOpeningExplorationBoundary"
import { readOpeningLinesFromCatalog } from "./openingLineFind"
import type { GameImportFields } from "./stagedGameImport"

export function CoachingBoardMount({
  navigate,
  route,
}: {
  navigate: Navigate
  route: Exclude<CoachingBoardRoute, { kind: "none" } | { kind: "invalid" }>
}) {
  const { fetchAccessToken, identity } = useFirebaseAuth()
  const authorizedPlayerId = useOptionalAuthorizedPlayer(
    fetchAccessToken,
    identity,
  )
  // Above the keyed children, so the page revision outlives the board it is
  // counting for — see `coachingBoardPage`.
  const page = useCoachingBoardPage(navigate)
  const targetHost = useCoachingBoardTargetHost({
    authorizedPlayerId,
    fetchAccessToken,
    page,
  })
  useOpeningExplorationBoundary(authorizedPlayerId)
  // Keyed by the target so navigating between two games or two Opening
  // Lines rebuilds the drive state instead of leaving the board and the
  // agent snapshot frozen on the previous target.
  if (route.kind === "game") {
    return (
      <CoachingBoardGame
        key={`${authorizedPlayerId ?? "anonymous"}:${route.gameImportId}`}
        authorizedPlayerId={authorizedPlayerId}
        fetchAccessToken={fetchAccessToken}
        gameImportId={route.gameImportId}
        targetHost={targetHost}
      />
    )
  }
  if (route.kind === "opening") {
    return (
      <CoachingBoardOpening
        key={`${authorizedPlayerId ?? "anonymous"}:${route.openingLineRef}`}
        authorizedPlayerId={authorizedPlayerId}
        fetchAccessToken={fetchAccessToken}
        openingLineRef={route.openingLineRef}
        page={page}
        targetHost={targetHost}
      />
    )
  }
  return <CoachingBoardEmpty targetHost={targetHost} />
}

function useCoachingBoardTargetHost({
  authorizedPlayerId,
  fetchAccessToken,
  page,
}: {
  authorizedPlayerId: string | null
  fetchAccessToken: FetchAccessToken
  page: CoachingBoardPage
}): CoachingBoardTargetHost {
  const { active, failure, run } = useReviewSessionCommands(fetchAccessToken)
  // The lobby reads retention only to know whether a disclosure is required.
  // A failed background read is not a failed import.
  const retention = useReviewRetentionPreference(fetchAccessToken, {
    reportsInitialRead: false,
  })
  const [playedAggregate, setPlayedAggregate] = useState<
    readonly PlayedOpeningAggregate[]
  >([])
  const [latestGame, setLatestGame] = useState<LatestPlayingProfileGame | null>(
    null,
  )
  const [importFailure, setImportFailure] = useState<string | null>(null)
  const client = useMemo(
    () =>
      new CoachEngineClient({
        credential: async () =>
          (await fetchAccessToken({ forceRefreshToken: true })) ?? "",
      }),
    [fetchAccessToken],
  )
  const recentReviewedGames = useRecentReviewedGames({
    client,
    enabled: Boolean(authorizedPlayerId),
    fetchAccessToken,
  })

  useEffect(() => {
    if (!authorizedPlayerId) {
      setPlayedAggregate([])
      setLatestGame(null)
      return
    }
    let activeLoad = true
    // The aggregate is counted engine-side over every imported Game; a
    // page-derived count would silently cap the corpus at the first twenty.
    void client.playedOpenings().then(
      (result) => {
        if (activeLoad) setPlayedAggregate(result.openings)
      },
      () => {
        if (activeLoad) setPlayedAggregate([])
      },
    )
    void client.recentPlayingProfileGames().then(
      (outcome) => {
        if (!activeLoad) return
        setLatestGame(latestPlayingProfileGameFromRead(outcome))
      },
      () => {
        if (activeLoad) setLatestGame(null)
      },
    )
    return () => {
      activeLoad = false
    }
  }, [authorizedPlayerId, client])

  async function commitImport(fields: GameImportFields) {
    if (!authorizedPlayerId) {
      setImportFailure("Sign in to save this game.")
      return
    }
    const parsed = parseImportGameRequest(fields)
    if (parsed.kind === "invalid") {
      setImportFailure(parsed.message)
      return
    }
    if (!(await retention.resolveBeforeReview())) return
    const imported = await run(
      "import",
      {
        eloProfile: parsed.eloProfile,
        kind: "importGame",
        reviewSide: { kind: "selected", reviewSide: parsed.reviewSide },
        source: parsed.source,
      },
      "Import",
    )
    if (imported?.kind !== "gameImported") return
    page.navigateAsPlayer(coachingBoardGamePath(imported.gameImportId))
  }

  return {
    authorizedPlayerId,
    disclosure:
      authorizedPlayerId &&
      retention.available &&
      retention.disclosureRequired ? (
        <Section aria-labelledby="coaching-board-retention-title">
          <VStack gap={2} hAlign="start">
            <Heading id="coaching-board-retention-title" level={2}>
              Before this Game is kept
            </Heading>
            <Text as="p" display="block" type="body">
              {retentionDisclosureDescription}
            </Text>
          </VStack>
        </Section>
      ) : null,
    importFailure: importFailure ?? failure ?? retention.failure,
    importing: active.import !== undefined || retention.resolving,
    latestGame,
    findOpeningLines: readOpeningLinesFromCatalog,
    listPlayedOpenings: () => listPlayedOpeningsForLobby(client),
    listRecentProfileGames: () => readRecentProfileGamesForLobby(client),
    page,
    playedAggregate,
    recentReviewedGames:
      recentReviewedGames.games.length > 0 ? recentReviewedGames : undefined,
    searchReviewedGames: (request) =>
      searchReviewedGamesForLobby(client, request),
    onCommitImport: (fields) => void commitImport(fields),
    playedOpenings: playedAggregate.map(({ eco, name }) => ({ eco, name })),
  }
}

async function listPlayedOpeningsForLobby(client: CoachEngineClient) {
  const result = await client.playedOpenings()
  return {
    constraints: lobbyConstraints(),
    kind: "lobby" as const,
    openings: result.openings,
  }
}

async function searchReviewedGamesForLobby(
  client: CoachEngineClient,
  request: ReviewedGameSearchRequest,
) {
  const result = await client.searchReviewedGames(request)
  return {
    constraints: lobbyConstraints(),
    kind: "lobby" as const,
    ...result,
  }
}

async function readRecentProfileGamesForLobby(client: CoachEngineClient) {
  const outcome = await client.recentPlayingProfileGames()
  return {
    constraints: lobbyConstraints(),
    kind: "lobby" as const,
    ...outcome,
  }
}

function useOptionalAuthorizedPlayer(
  fetchAccessToken: FetchAccessToken,
  identity: FirebaseIdentity,
) {
  const [playerId, setPlayerId] = useState<string | null>(null)
  useEffect(() => {
    if (identity.kind !== "signedIn" || !identity.emailVerified) {
      setPlayerId(null)
      return
    }
    let active = true
    const expected = identity.playerId
    void checkBetaAuthorization(fetchAccessToken, expected).then((result) => {
      if (!active) return
      setPlayerId(result.kind === "granted" ? result.playerId : null)
    })
    return () => {
      active = false
    }
  }, [fetchAccessToken, identity])
  return playerId
}
