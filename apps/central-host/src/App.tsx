import { useCallback, useState } from "react"

import { AuthenticatedLinkUnavailable } from "@/auth/AuthenticatedLinkUnavailable"
import { useFirebaseAuth } from "@/auth/FirebaseAuthProvider"
import { BetaAccessBoundary } from "@/auth/BetaAccessBoundary"
import { RouteRedirect, type Navigate } from "@/auth/RouteRedirect"
import {
  coachAppDestinationForGameReview,
  coachingBoardDestination,
} from "@/auth/verifiedIdentityDestination"
import {
  parseGameReviewRoute,
  parseViewedPly,
  type GameReviewRoute,
} from "@/game-review/gameReviewRoute"
import { CoachingBoardMount } from "@/coaching-board/CoachingBoardMount"
import { parseCoachingBoardRoute } from "@/coaching-board/coachingBoardRoute"
import { useCoachingBoardNavigation } from "@/coaching-board/useCoachingBoardNavigation"
import { ReviewSessionWorkspace } from "@/review-session/ReviewSessionWorkspace"
import type { GameImportId } from "@chenchess/coach-engine-sdk"
import {
  AppShell,
  Card,
  Heading,
  Text,
  VStack,
  WatercolorNotice,
  WatercolorStudio,
} from "@chenchess/ui"

const replaceLocation: Navigate = (href) => window.location.replace(href)

export function App({
  navigate: navigateProp,
  pathname: pathnameProp,
  search = window.location.search,
}: {
  navigate?: Navigate
  pathname?: string
  search?: string
}) {
  // The Coaching Board's own path is rendered in place so the page it holds
  // survives a change of board; every other address replaces the document.
  // A test that hands in both drives the route itself.
  const inPlace = useCoachingBoardNavigation(replaceLocation)
  const navigate = navigateProp ?? inPlace.navigate
  const pathname = pathnameProp ?? inPlace.pathname
  const coachingBoard = parseCoachingBoardRoute(pathname)
  if (coachingBoard.kind === "invalid") {
    return (
      <WatercolorStudio as="main">
        <WatercolorNotice glyph="!" heading="Coaching" tone="vermilion">
          This coaching link is incomplete or has been changed.
        </WatercolorNotice>
      </WatercolorStudio>
    )
  }
  if (coachingBoard.kind !== "none") {
    return <CoachingBoardMount navigate={navigate} route={coachingBoard} />
  }
  return (
    <AuthenticatedApp
      initialPly={parseViewedPly(search)}
      navigate={navigate}
      route={parseGameReviewRoute(pathname)}
    />
  )
}

function AuthenticatedApp({
  initialPly,
  navigate,
  route,
}: {
  initialPly: number | null
  navigate: Navigate
  route: GameReviewRoute
}) {
  const { fetchAccessToken, identity, reauthenticate, signOut } =
    useFirebaseAuth()
  // Signing in returns the Player to the exact address they asked for, moment
  // and continuation included: the gate is the identity, not the depth.
  const destination =
    route.kind === "none" || route.kind === "invalid"
      ? coachingBoardDestination
      : coachAppDestinationForGameReview(route)
  if (identity.kind === "loading") {
    return (
      <AppShell contentPadding={6}>
        <Card maxWidth="28rem">
          <VStack gap={3} hAlign="start">
            <Heading level={2}>Loading session</Heading>
            <Text as="p" display="block" type="body">
              Checking Firebase Authentication state.
            </Text>
          </VStack>
        </Card>
      </AppShell>
    )
  }
  if (identity.kind === "signedOut") {
    return (
      <RouteRedirect
        description="Sign in to continue."
        href={destination.loginHref}
        navigate={navigate}
        title="Opening sign-in"
      />
    )
  }
  if (!identity.emailVerified) {
    return (
      <RouteRedirect
        description="Verify your email to continue."
        href={destination.loginHref}
        navigate={navigate}
        title="Opening email verification"
      />
    )
  }
  return (
    <BetaAccessBoundary
      destination={destination}
      fetchAccessToken={fetchAccessToken}
      identity={identity}
      navigate={navigate}
      signOut={signOut}
    >
      {(authorizedPlayerId) => {
        if (route.kind === "invalid") {
          return (
            <AppShell contentPadding={6}>
              <Card maxWidth="28rem" variant="red">
                <VStack gap={3} hAlign="start">
                  <Heading level={2}>Review link unavailable</Heading>
                  <Text as="p" display="block" type="body">
                    This review link is incomplete or has been changed.
                  </Text>
                </VStack>
              </Card>
            </AppShell>
          )
        }
        // The bare app address opens the Coaching Board lobby, which is where
        // a Game is imported and where every WebMCP tool is registered.
        if (route.kind === "none") {
          return (
            <RouteRedirect
              description="Opening the coaching board."
              href={coachingBoardDestination.href}
              navigate={navigate}
              title="Opening coaching board"
            />
          )
        }
        return (
          <CoachWorkspace
            key={authorizedPlayerId}
            fetchAccessToken={fetchAccessToken}
            initialGameImportId={route.gameImportId}
            initialPly={initialPly}
            reauthenticate={reauthenticate}
            signedInAs={identity.email ?? identity.playerId}
            signOut={signOut}
          />
        )
      }}
    </BetaAccessBoundary>
  )
}

type CoachWorkspaceProps = {
  fetchAccessToken: (options: {
    forceRefreshToken: boolean
  }) => Promise<string | null>
  initialGameImportId: GameImportId
  initialPly: number | null
  reauthenticate: (password: string) => Promise<void>
  signedInAs: string
  signOut: () => Promise<void>
}

export function CoachWorkspace({
  signedInAs,
  signOut,
  ...props
}: CoachWorkspaceProps) {
  const [linkUnavailable, setLinkUnavailable] = useState(false)
  const showLinkUnavailable = useCallback(() => setLinkUnavailable(true), [])

  return linkUnavailable ? (
    <AuthenticatedLinkUnavailable account={signedInAs} signOut={signOut} />
  ) : (
    <ReviewSessionWorkspace
      {...props}
      onUnavailableGameImport={showLinkUnavailable}
      signOut={signOut}
    />
  )
}
