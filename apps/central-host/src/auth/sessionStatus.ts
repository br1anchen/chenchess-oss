import { sharedGroundingSentences } from "@chenchess/shared-assets"

import type { FirebaseIdentity } from "./FirebaseAuthProvider"
import type { BetaAuthorizationState } from "./useBetaAuthorization"
import type { VerifiedIdentityDestination } from "./verifiedIdentityDestination"

export type SessionStatusStage =
  | "emailUnverified"
  | "noBetaAccess"
  | "signedOut"

export type SessionStatus = {
  href: string
  stage: SessionStatusStage
}

export type SessionStatusResult = SessionStatus & {
  constraints: {
    kind: "constraints"
    sentences: string[]
  }
  kind: "sessionStatus"
}

const sessionConstraintSentences = [
  "This page has no board tools. The visitor is not a signed-in Player with Beta Access.",
  "Name only the returned stage and href. Do not invent board state, a Game Review, or coaching facts.",
  "Send the Player to the returned href to resolve this stage.",
] as const

export const readSessionStatusDescription = [
  "Read why this page has no board tools. Reports the visitor's locked stage — signed out, email unverified, or no Beta Access — and the href that resolves it.",
  "This is a cheap read. It writes nothing, spends no Engine compute, and never registers a board tool.",
  ...sharedGroundingSentences,
  ...sessionConstraintSentences,
].join(" ")

export function sessionStatusOnLogin(
  identity: FirebaseIdentity,
  destination: Pick<VerifiedIdentityDestination, "loginHref">,
): SessionStatus | null {
  if (identity.kind === "signedOut") {
    return { href: destination.loginHref, stage: "signedOut" }
  }
  if (identity.kind === "signedIn" && !identity.emailVerified) {
    return { href: destination.loginHref, stage: "emailUnverified" }
  }
  return null
}

export function sessionStatusOnJoin(
  authorization: BetaAuthorizationState,
  destination: Pick<VerifiedIdentityDestination, "joinHref">,
): SessionStatus | null {
  if (authorization.kind !== "required") return null
  return { href: destination.joinHref, stage: "noBetaAccess" }
}

export function sessionStatusResult(
  status: SessionStatus,
): SessionStatusResult {
  return {
    constraints: {
      kind: "constraints",
      sentences: [...sharedGroundingSentences, ...sessionConstraintSentences],
    },
    href: status.href,
    kind: "sessionStatus",
    stage: status.stage,
  }
}
