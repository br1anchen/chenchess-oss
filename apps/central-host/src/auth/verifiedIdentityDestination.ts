import { addressedGameReviewPath } from "@/game-review/gameReviewRoute"
import {
  type AddressedGameReview,
  isCriticalMomentId,
  isGameImportId,
  isSequenceKind,
} from "@/game-review/reviewAddress"

export type VerifiedIdentityDestination = {
  href: string
  joinHref: string
  loginHref: string
  requiresBetaAccess: boolean
}

/**
 * Where a verified Player lands.
 *
 * This snapshot serves one surface — the Coaching Board — so that is also the
 * address every sign-in returns to. A Game Review address still returns to
 * itself, because the gate is on the Player's identity and not on how deep
 * into a review they were.
 */
export const coachingBoardDestination: VerifiedIdentityDestination = {
  href: "/app/board",
  joinHref: "/join/",
  loginHref: "/login/",
  requiresBetaAccess: true,
}

/**
 * The whole address survives the round trip — the review, the Review Moment,
 * and the continuation — because returning someone to the review when they
 * asked for a moment makes signing in cost them their place.
 */
export function coachAppDestinationForGameReview(
  route: AddressedGameReview,
): VerifiedIdentityDestination {
  const returnSearch = new URLSearchParams({
    return_to: "app",
    game_review: route.gameImportId,
  })
  if (route.kind !== "gameReview") {
    returnSearch.set("review_moment", route.reviewMomentId)
  }
  if (route.kind === "moveSequence") {
    returnSearch.set("sequence", route.sequenceKind)
  }
  return {
    href: addressedGameReviewPath(route),
    joinHref: `/join/?${returnSearch}`,
    loginHref: `/login/?${returnSearch}`,
    requiresBetaAccess: true,
  }
}

export function verifiedIdentityDestination(
  search: string,
): VerifiedIdentityDestination {
  const parameters = new URLSearchParams(search)
  if (parameters.get("return_to") !== "app") return coachingBoardDestination
  /* `return_to=app` carrying a Game Review still lands on that review. The
     bare app address is the Coaching Board. */
  const route = addressedRoute(parameters)
  return route
    ? coachAppDestinationForGameReview(route)
    : coachingBoardDestination
}

function addressedRoute(
  parameters: URLSearchParams,
): AddressedGameReview | null {
  const gameImportId = parameters.get("game_review")
  if (!gameImportId || !isGameImportId(gameImportId)) return null
  const reviewMomentId = parameters.get("review_moment")
  if (!reviewMomentId) return { gameImportId, kind: "gameReview" }
  if (!isCriticalMomentId(reviewMomentId)) return null
  const sequenceKind = parameters.get("sequence")
  if (!sequenceKind) {
    return { gameImportId, kind: "reviewMoment", reviewMomentId }
  }
  if (!isSequenceKind(sequenceKind)) return null
  return { gameImportId, kind: "moveSequence", reviewMomentId, sequenceKind }
}

export function withInvitationFragment(
  href: string,
  invitationCode: string | null,
) {
  return invitationCode
    ? `${href}#invite=${encodeURIComponent(invitationCode)}`
    : href
}
