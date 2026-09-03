import { useState, type ReactNode } from "react"

import {
  HStack,
  Text,
  VStack,
  WatercolorButton,
  WatercolorCard,
} from "@chenchess/ui"

import { AuthStudio } from "./AuthStudio"
import { BetaAccessRequestForm } from "./BetaAccessRequestForm"
import {
  useFirebaseAuth,
  type FetchAccessToken,
  type FirebaseIdentity,
} from "./FirebaseAuthProvider"
import { InvitationRedemption } from "./InvitationRedemption"
import { RouteRedirect, type Navigate } from "./RouteRedirect"
import { sessionStatusOnJoin } from "./sessionStatus"
import { SignOutControl } from "./SignOutControl"
import { useBetaAuthorization } from "./useBetaAuthorization"
import { useSessionStatusTool } from "./useSessionStatusTool"
import { VerifiedIdentityArrival } from "./VerifiedIdentityArrival"
import {
  type VerifiedIdentityDestination,
  withInvitationFragment,
} from "./verifiedIdentityDestination"

type SignedInIdentity = Extract<FirebaseIdentity, { kind: "signedIn" }>

export function JoinIdentity({
  initialInvitationCode,
  navigate,
  verifiedDestination,
}: {
  initialInvitationCode: string | null
  navigate: Navigate
  verifiedDestination: VerifiedIdentityDestination
}) {
  const auth = useFirebaseAuth()

  if (auth.identity.kind === "loading") {
    return (
      <JoinShell>
        <AdmissionNotice
          description="Checking your sign-in."
          title="Loading identity"
        />
      </JoinShell>
    )
  }
  if (
    auth.identity.kind === "signedOut" ||
    !auth.identity.emailVerified ||
    auth.providerLink.kind === "required"
  ) {
    return (
      <RouteRedirect
        description="Sign in and verify your email before asking for an invite or redeeming a code."
        href={withInvitationFragment(
          verifiedDestination.loginHref,
          initialInvitationCode,
        )}
        navigate={navigate}
        title="Opening sign-in"
      />
    )
  }

  return (
    <VerifiedAdmission
      fetchAccessToken={auth.fetchAccessToken}
      identity={auth.identity}
      initialInvitationCode={initialInvitationCode}
      navigate={navigate}
      signOut={auth.signOut}
      verifiedDestination={verifiedDestination}
    />
  )
}

function VerifiedAdmission({
  fetchAccessToken,
  identity,
  initialInvitationCode,
  navigate,
  signOut,
  verifiedDestination,
}: {
  fetchAccessToken: FetchAccessToken
  identity: SignedInIdentity
  initialInvitationCode: string | null
  navigate: Navigate
  signOut: () => Promise<void>
  verifiedDestination: VerifiedIdentityDestination
}) {
  const { authorization, refreshAuthorization } = useBetaAuthorization(
    fetchAccessToken,
    identity,
  )
  // Redemption grants Beta Access server-side, so arrival no longer waits on
  // another authorization round trip to say what this page just did.
  const [redeemed, setRedeemed] = useState(false)
  // Redeemed or granted stages render an arrival shell that opens the app;
  // the locked-stage read must retract rather than keep reporting
  // noBetaAccess.
  useSessionStatusTool(
    redeemed ? null : sessionStatusOnJoin(authorization, verifiedDestination),
  )

  if (authorization.kind === "granted" || redeemed) {
    return (
      <VerifiedIdentityArrival
        description="This account already has access."
        destination={verifiedDestination}
        navigate={navigate}
        title="Opening ChenChess"
      />
    )
  }
  if (authorization.kind === "authenticationRequired") {
    return (
      <RouteRedirect
        description="Please sign in again."
        href={withInvitationFragment(
          verifiedDestination.loginHref,
          initialInvitationCode,
        )}
        navigate={navigate}
        title="Opening sign-in"
      />
    )
  }
  if (authorization.kind === "checking") {
    return (
      <JoinShell>
        <AdmissionNotice
          action={<SignOutControl signOut={signOut} />}
          description="Checking whether this account already has access."
          title="Checking your access"
        />
      </JoinShell>
    )
  }
  if (authorization.kind === "unavailable") {
    return (
      <JoinShell>
        <AdmissionNotice
          action={
            <HStack gap={2} wrap="wrap">
              <WatercolorButton
                onClick={refreshAuthorization}
                type="button"
                variant="primary"
              >
                Check again
              </WatercolorButton>
              <SignOutControl signOut={signOut} />
            </HStack>
          }
          description="We could not check your access, so nothing was redeemed."
          title="Access check unavailable"
        />
      </JoinShell>
    )
  }

  return (
    <JoinShell wide>
      <VStack gap={4} hAlign="stretch">
        <Text as="p" display="block" type="body">
          This account does not have access yet. Ask for an invite, or redeem
          the code you were sent.
        </Text>
        <BetaAccessRequestForm
          email={identity.email}
          fetchAccessToken={fetchAccessToken}
        />
        <WatercolorCard headingLevel={2} title="Redeem an invite">
          <InvitationRedemption
            fetchAccessToken={fetchAccessToken}
            initialCode={initialInvitationCode}
            onRedeemed={() => setRedeemed(true)}
          />
        </WatercolorCard>
        <SignOutControl signOut={signOut} />
      </VStack>
    </JoinShell>
  )
}

function JoinShell({
  children,
  wide = false,
}: {
  children: ReactNode
  wide?: boolean
}) {
  return <AuthStudio wide={wide}>{children}</AuthStudio>
}

function AdmissionNotice({
  action,
  description,
  title,
}: {
  action?: ReactNode
  description: string
  title: string
}) {
  return (
    <WatercolorCard headingLevel={2} title={title}>
      <VStack gap={3} hAlign="start">
        <Text as="p" display="block" type="body">
          {description}
        </Text>
        {action}
      </VStack>
    </WatercolorCard>
  )
}
