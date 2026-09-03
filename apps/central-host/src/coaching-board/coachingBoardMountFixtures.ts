import type { FirebaseIdentity } from "@/auth/FirebaseAuthProvider"
import { reviewSessionResponder } from "@/review-session/reviewSessionStreamFixtures"

/**
 * What `CoachingBoardMount` needs before it renders a board or registers a
 * single tool: a verified identity, and a beta authorization behind it.
 */
export const MOUNTED_PLAYER_ID = "firebase-player-test"

export function verifiedIdentity(): FirebaseIdentity {
  return {
    email: "player@example.test",
    emailVerified: true,
    kind: "signedIn",
    playerId: MOUNTED_PLAYER_ID,
  }
}

/** Beta authorization in front of the fixture responder. */
export function betaAuthorizedResponder() {
  const responder = reviewSessionResponder({ retentionAvailable: true })
  return async (input: RequestInfo | URL, init?: RequestInit) => {
    if (String(input).endsWith("/api/v1/beta-access/authorization")) {
      return Response.json({ playerId: MOUNTED_PLAYER_ID })
    }
    return responder(input, init)
  }
}
