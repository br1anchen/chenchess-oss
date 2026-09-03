import type { ReactNode } from "react"

import {
  HStack,
  Text,
  VStack,
  WatercolorButton,
  WatercolorCard,
} from "@chenchess/ui"

import type {
  FetchAccessToken,
  FirebaseIdentity,
} from "@/auth/FirebaseAuthProvider"
import { AuthStudio } from "./AuthStudio"
import { RouteRedirect, type Navigate } from "./RouteRedirect"
import { SignOutControl } from "./SignOutControl"
import { useBetaAuthorization } from "./useBetaAuthorization"
import type { VerifiedIdentityDestination } from "./verifiedIdentityDestination"

type SignedInIdentity = Extract<FirebaseIdentity, { kind: "signedIn" }>

export function BetaAccessBoundary({
  children,
  destination,
  fetchAccessToken,
  identity,
  navigate,
  signOut,
}: {
  children: (authorizedPlayerId: string) => ReactNode
  destination: VerifiedIdentityDestination
  fetchAccessToken: FetchAccessToken
  identity: SignedInIdentity
  navigate: Navigate
  signOut: () => Promise<void>
}) {
  const { authorization, refreshAuthorization } = useBetaAuthorization(
    fetchAccessToken,
    identity,
  )
  if (authorization.kind === "granted") {
    return children(authorization.playerId)
  }
  if (authorization.kind === "required") {
    return (
      <RouteRedirect
        description="This account needs an invite before this page can open."
        href={destination.joinHref}
        navigate={navigate}
        title="Opening ChenChess"
      />
    )
  }
  if (authorization.kind === "authenticationRequired") {
    return (
      <RouteRedirect
        description="Please sign in again."
        href={destination.loginHref}
        navigate={navigate}
        title="Opening sign-in"
      />
    )
  }

  const copy = accessCheckCopy(authorization.kind)

  return (
    <AuthStudio legal={false}>
      <WatercolorCard headingLevel={2} title={copy.title}>
        <VStack gap={4} hAlign="stretch">
          <Text as="p" display="block" type="body">
            {copy.description}
          </Text>
          <HStack gap={2} wrap="wrap">
            {authorization.kind === "unavailable" ? (
              <WatercolorButton
                onClick={refreshAuthorization}
                type="button"
                variant="primary"
              >
                Check again
              </WatercolorButton>
            ) : null}
            <SignOutControl signOut={signOut} />
          </HStack>
        </VStack>
      </WatercolorCard>
    </AuthStudio>
  )
}

function accessCheckCopy(kind: "checking" | "unavailable") {
  switch (kind) {
    case "checking":
      return {
        description: "Checking your access.",
        title: "Checking your access",
      }
    case "unavailable":
      return {
        description:
          "We could not check your access, so the page was not opened.",
        title: "Access check unavailable",
      }
    default: {
      const _exhaustive: never = kind
      return _exhaustive
    }
  }
}
