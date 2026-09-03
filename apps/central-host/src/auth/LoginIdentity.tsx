import { useState, type FormEvent, type ReactNode } from "react"
import { Icon } from "@chenchess/ui/astryx"

import {
  HStack,
  Text,
  VStack,
  WatercolorBadge,
  WatercolorButton,
  WatercolorCard,
  WatercolorField,
  WatercolorInput,
} from "@chenchess/ui"

import { AuthNotice, type AuthNoticeStatus } from "./AuthNotice"
import { AuthStudio } from "./AuthStudio"
import {
  useFirebaseAuth,
  type FetchAccessToken,
  type FirebaseIdentity,
  type IdentityJourneyResult,
} from "./FirebaseAuthProvider"
import { RouteRedirect, type Navigate } from "./RouteRedirect"
import { sessionStatusOnLogin } from "./sessionStatus"
import { useBetaAuthorization } from "./useBetaAuthorization"
import { useSessionStatusTool } from "./useSessionStatusTool"
import { VerifiedIdentityArrival } from "./VerifiedIdentityArrival"
import type { VerifiedIdentityDestination } from "./verifiedIdentityDestination"
import { withInvitationFragment } from "./verifiedIdentityDestination"

type IdentityFlow = "signIn" | "signUp" | "reset"
type Notice = IdentityJourneyResult | { kind: "resetRequested" }
type SignedInIdentity = Extract<FirebaseIdentity, { kind: "signedIn" }>
export const identityNoticeCopy = {
  resetRequested:
    "If an account can receive a reset email, instructions are on the way.",
  wrongAccount:
    "That sign-in belongs to a different account. No accounts were linked.",
  canceled: "Sign-in was canceled. You can safely try again.",
  invalidCredentials:
    "That email and password combination could not be verified.",
  tryLater: "Too many attempts. Please wait before trying again.",
  unavailable: "Sign-in is temporarily unavailable. Please try again.",
} as const

type NoticePresentation = {
  message: string
  status: AuthNoticeStatus
}

type IdentityFlowCopy = {
  description?: string
  title: string
}

export function LoginIdentity({
  initialInvitationCode,
  navigate,
  verifiedDestination,
}: {
  initialInvitationCode: string | null
  navigate: Navigate
  verifiedDestination: VerifiedIdentityDestination
}) {
  const auth = useFirebaseAuth()
  useSessionStatusTool(sessionStatusOnLogin(auth.identity, verifiedDestination))
  const [flow, setFlow] = useState<IdentityFlow>("signIn")
  const [notice, setNotice] = useState<Notice | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)

  async function perform(action: () => Promise<IdentityJourneyResult>) {
    setNotice(null)
    setIsSubmitting(true)
    try {
      setNotice(await action())
    } finally {
      setIsSubmitting(false)
    }
  }

  async function performSessionAction(action: () => Promise<void>) {
    setNotice(null)
    setIsSubmitting(true)
    try {
      await action()
    } catch {
      setNotice({ kind: "unavailable" })
    } finally {
      setIsSubmitting(false)
    }
  }

  async function requestPasswordReset(email: string) {
    setNotice(null)
    setIsSubmitting(true)
    try {
      await auth.requestPasswordReset(email)
    } finally {
      setNotice({ kind: "resetRequested" })
      setIsSubmitting(false)
    }
  }

  function selectFlow(nextFlow: IdentityFlow) {
    setNotice(null)
    setFlow(nextFlow)
  }

  if (auth.providerLink.kind === "required") {
    return (
      <IdentityShell>
        <WatercolorCard
          eyebrow={
            <WatercolorBadge tone="warning">Account protection</WatercolorBadge>
          }
          headingLevel={2}
          title="Confirm before linking"
          tone="vermilion"
        >
          <VStack gap={4} hAlign="stretch">
            <Text as="p" display="block" type="body">
              Sign in to the existing account for {auth.providerLink.email},
              then explicitly link this sign-in method. Matching email text
              alone is never enough.
            </Text>
            <PasswordForm
              email={auth.providerLink.email}
              isSubmitting={isSubmitting}
              key={auth.providerLink.email}
              notice={notice}
              onSubmit={(submittedEmail, password) =>
                perform(() => auth.signInWithPassword(submittedEmail, password))
              }
              submitLabel="Sign in and link accounts"
            />
            <VStack gap={2} hAlign="stretch">
              <WatercolorButton
                block
                disabled={isSubmitting}
                onClick={() => perform(auth.signInWithGoogle)}
                type="button"
                variant="secondary"
              >
                Continue with Google and link
              </WatercolorButton>
              <WatercolorButton
                block
                disabled={isSubmitting}
                onClick={() =>
                  void performSessionAction(auth.cancelProviderLink)
                }
                type="button"
                variant="quiet"
              >
                Cancel linking
              </WatercolorButton>
            </VStack>
          </VStack>
        </WatercolorCard>
      </IdentityShell>
    )
  }

  if (auth.identity.kind === "loading") {
    return (
      <IdentityShell>
        <WatercolorCard headingLevel={2} title="Signing you in">
          <Text as="p" display="block" type="body">
            Checking your sign-in.
          </Text>
        </WatercolorCard>
      </IdentityShell>
    )
  }

  if (auth.identity.kind === "signedIn") {
    if (!auth.identity.emailVerified) {
      return (
        <IdentityShell>
          <WatercolorCard
            headingLevel={2}
            meta={<Icon icon="mail" size="sm" />}
            title="Verify your email"
            tone="mist"
          >
            <VStack gap={4} hAlign="stretch">
              <Text as="p" display="block" type="body">
                We sent a verification link to
                {auth.identity.email
                  ? ` ${auth.identity.email}`
                  : " your email"}
                . You can redeem an invite once the address is confirmed.
              </Text>
              <NoticeMessage notice={notice} />
              <VStack gap={2} hAlign="stretch">
                <WatercolorButton
                  block
                  disabled={isSubmitting}
                  onClick={() => perform(auth.refreshIdentity)}
                  type="button"
                  variant="primary"
                >
                  I verified—check again
                </WatercolorButton>
                <WatercolorButton
                  block
                  disabled={isSubmitting}
                  onClick={() => perform(auth.sendVerification)}
                  type="button"
                  variant="secondary"
                >
                  Send another verification email
                </WatercolorButton>
                <WatercolorButton
                  block
                  disabled={isSubmitting}
                  onClick={() => void performSessionAction(auth.signOut)}
                  type="button"
                  variant="quiet"
                >
                  Log out
                </WatercolorButton>
              </VStack>
            </VStack>
          </WatercolorCard>
        </IdentityShell>
      )
    }

    return (
      <VerifiedIdentityRoute
        fetchAccessToken={auth.fetchAccessToken}
        identity={auth.identity}
        initialInvitationCode={initialInvitationCode}
        navigate={navigate}
        onSignOut={() => performSessionAction(auth.signOut)}
        verifiedDestination={verifiedDestination}
      />
    )
  }

  const copy = identityFlowCopy(flow)

  return (
    <IdentityShell>
      <WatercolorCard
        headingLevel={2}
        meta={identityFlowIcon(flow)}
        title={copy.title}
      >
        <VStack gap={4} hAlign="stretch">
          {copy.description ? (
            <Text as="p" display="block" type="body">
              {copy.description}
            </Text>
          ) : null}
          {flow === "reset" ? (
            <ResetForm
              isSubmitting={isSubmitting}
              notice={notice}
              onSubmit={requestPasswordReset}
            />
          ) : (
            <PasswordForm
              isSubmitting={isSubmitting}
              notice={notice}
              onSubmit={(email, password) =>
                perform(() =>
                  flow === "signIn"
                    ? auth.signInWithPassword(email, password)
                    : auth.signUpWithPassword(email, password),
                )
              }
              submitLabel={flow === "signIn" ? "Sign in" : "Create account"}
            />
          )}
          <VStack gap={2} hAlign="stretch">
            {flow !== "reset" ? (
              <WatercolorButton
                block
                disabled={isSubmitting}
                onClick={() => perform(auth.signInWithGoogle)}
                type="button"
                variant="secondary"
              >
                Continue with Google
              </WatercolorButton>
            ) : null}
            <HStack gap={2} wrap="wrap">
              {flow !== "signIn" ? (
                <WatercolorButton
                  onClick={() => selectFlow("signIn")}
                  size="sm"
                  type="button"
                  variant="quiet"
                >
                  Use an existing account
                </WatercolorButton>
              ) : null}
              {flow !== "signUp" ? (
                <WatercolorButton
                  onClick={() => selectFlow("signUp")}
                  size="sm"
                  type="button"
                  variant="quiet"
                >
                  Create account
                </WatercolorButton>
              ) : null}
              {flow !== "reset" ? (
                <WatercolorButton
                  onClick={() => selectFlow("reset")}
                  size="sm"
                  type="button"
                  variant="quiet"
                >
                  Forgot password?
                </WatercolorButton>
              ) : null}
            </HStack>
          </VStack>
        </VStack>
      </WatercolorCard>
    </IdentityShell>
  )
}

function VerifiedIdentityRoute({
  fetchAccessToken,
  identity,
  initialInvitationCode,
  navigate,
  onSignOut,
  verifiedDestination,
}: {
  fetchAccessToken: FetchAccessToken
  identity: SignedInIdentity
  initialInvitationCode: string | null
  navigate: Navigate
  onSignOut: () => Promise<void>
  verifiedDestination: VerifiedIdentityDestination
}) {
  if (!verifiedDestination.requiresBetaAccess) {
    return (
      <VerifiedIdentityArrival
        description="You are signed in."
        destination={verifiedDestination}
        navigate={navigate}
        title="Opening your destination"
      />
    )
  }
  return (
    <BetaAccessLoginRoute
      fetchAccessToken={fetchAccessToken}
      identity={identity}
      initialInvitationCode={initialInvitationCode}
      navigate={navigate}
      onSignOut={onSignOut}
      verifiedDestination={verifiedDestination}
    />
  )
}

function BetaAccessLoginRoute({
  fetchAccessToken,
  identity,
  initialInvitationCode,
  navigate,
  onSignOut,
  verifiedDestination,
}: {
  fetchAccessToken: FetchAccessToken
  identity: SignedInIdentity
  initialInvitationCode: string | null
  navigate: Navigate
  onSignOut: () => Promise<void>
  verifiedDestination: VerifiedIdentityDestination
}) {
  const { authorization, refreshAuthorization } = useBetaAuthorization(
    fetchAccessToken,
    identity,
  )

  if (authorization.kind === "granted") {
    return (
      <VerifiedIdentityArrival
        description="You are signed in and your access is confirmed."
        destination={verifiedDestination}
        navigate={navigate}
        title="Opening ChenChess"
      />
    )
  }
  if (authorization.kind === "required") {
    return (
      <RouteRedirect
        description="Your email is verified. Next, ask for an invite or redeem your code."
        href={withInvitationFragment(
          verifiedDestination.joinHref,
          initialInvitationCode,
        )}
        navigate={navigate}
        title="Opening ChenChess"
      />
    )
  }

  const copy = accessCheckCopy(authorization.kind)

  return (
    <IdentityShell>
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
            <WatercolorButton
              onClick={() => void onSignOut()}
              type="button"
              variant="secondary"
            >
              Log out
            </WatercolorButton>
          </HStack>
        </VStack>
      </WatercolorCard>
    </IdentityShell>
  )
}

function IdentityShell({ children }: { children: ReactNode }) {
  return <AuthStudio>{children}</AuthStudio>
}

function PasswordForm({
  email,
  isSubmitting,
  notice,
  onSubmit,
  submitLabel,
}: {
  email?: string
  isSubmitting: boolean
  notice: Notice | null
  onSubmit: (email: string, password: string) => Promise<void>
  submitLabel: string
}) {
  const [enteredEmail, setEnteredEmail] = useState("")
  const [password, setPassword] = useState("")
  const emailValue = email ?? enteredEmail

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    await onSubmit(emailValue, password)
  }

  return (
    <form onSubmit={(event) => void submit(event)}>
      <VStack gap={3} hAlign="stretch">
        <NoticeMessage notice={notice} />
        <WatercolorField label="Email">
          <WatercolorInput
            name="email"
            onChange={(event) => setEnteredEmail(event.target.value)}
            readOnly={email !== undefined}
            type="email"
            value={emailValue}
          />
        </WatercolorField>
        <WatercolorField label="Password">
          <WatercolorInput
            name="password"
            onChange={(event) => setPassword(event.target.value)}
            type="password"
            value={password}
          />
        </WatercolorField>
        <WatercolorButton
          block
          disabled={isSubmitting}
          loading={isSubmitting}
          type="submit"
          variant="primary"
        >
          {isSubmitting ? "Working…" : submitLabel}
        </WatercolorButton>
      </VStack>
    </form>
  )
}

function ResetForm({
  isSubmitting,
  notice,
  onSubmit,
}: {
  isSubmitting: boolean
  notice: Notice | null
  onSubmit: (email: string) => Promise<void>
}) {
  const [email, setEmail] = useState("")

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    await onSubmit(email)
  }

  return (
    <form onSubmit={(event) => void submit(event)}>
      <VStack gap={3} hAlign="stretch">
        <NoticeMessage notice={notice} />
        <WatercolorField label="Email">
          <WatercolorInput
            name="email"
            onChange={(event) => setEmail(event.target.value)}
            type="email"
            value={email}
          />
        </WatercolorField>
        <WatercolorButton
          block
          disabled={isSubmitting}
          loading={isSubmitting}
          type="submit"
          variant="primary"
        >
          {isSubmitting ? "Working…" : "Send reset instructions"}
        </WatercolorButton>
      </VStack>
    </form>
  )
}

function NoticeMessage({ notice }: { notice: Notice | null }) {
  const presentation = noticePresentation(notice)
  return presentation ? (
    <AuthNotice message={presentation.message} status={presentation.status} />
  ) : null
}

function identityFlowCopy(flow: IdentityFlow): IdentityFlowCopy {
  switch (flow) {
    case "signIn":
      return {
        title: "Sign in",
      }
    case "signUp":
      return {
        title: "Create your account",
      }
    case "reset":
      return {
        description:
          "Enter your email. We show the same response whether or not an account exists.",
        title: "Reset your password",
      }
    default: {
      const _exhaustive: never = flow
      return _exhaustive
    }
  }
}

function identityFlowIcon(flow: IdentityFlow) {
  switch (flow) {
    case "signUp":
      return <Icon icon="userPlus" size="sm" />
    case "reset":
      return <Icon icon="keyRound" size="sm" />
    case "signIn":
      return null
    default: {
      const _exhaustive: never = flow
      return _exhaustive
    }
  }
}

function accessCheckCopy(
  kind: "checking" | "authenticationRequired" | "unavailable",
) {
  switch (kind) {
    case "checking":
      return {
        description: "Checking your access.",
        title: "Checking your access",
      }
    case "authenticationRequired":
      return {
        description:
          "We could not confirm your sign-in. Sign out, then sign in again.",
        title: "Sign in again",
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

function noticePresentation(notice: Notice | null): NoticePresentation | null {
  const kind = notice?.kind
  switch (kind) {
    case undefined:
    case "complete":
    case "linkRequired":
      return null
    case "resetRequested":
      return {
        message: identityNoticeCopy.resetRequested,
        status: "success",
      }
    case "wrongAccount":
      return {
        message: identityNoticeCopy.wrongAccount,
        status: "error",
      }
    case "canceled":
      return {
        message: identityNoticeCopy.canceled,
        status: "info",
      }
    case "invalidCredentials":
      return {
        message: identityNoticeCopy.invalidCredentials,
        status: "error",
      }
    case "tryLater":
      return {
        message: identityNoticeCopy.tryLater,
        status: "warning",
      }
    case "unavailable":
      return {
        message: identityNoticeCopy.unavailable,
        status: "error",
      }
    default: {
      const _exhaustive: never = kind
      return _exhaustive
    }
  }
}
