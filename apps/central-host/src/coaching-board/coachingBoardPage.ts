import { useRef } from "react"

import type { Navigate } from "@/auth/RouteRedirect"

import {
  advancedPageRevision,
  initialPageRevision,
  type CoachingBoardActor,
  type CoachingBoardPageRevision,
} from "./coachingBoardSnapshot"

/**
 * The page a Coaching Board is mounted on.
 *
 * `CoachingBoardMount` keys its children on the target, so changing origin
 * rebuilds the whole drive — which is what correctly rebuilds board state, and
 * what would otherwise restart a counter the spec calls monotonic for the life
 * of the page, across moments, lines and origins (decision 7). The page sits
 * above that subtree: it holds the revision the board showing has reached and
 * hands it to the board mounting next. A full reload is a new page and
 * legitimately starts over; a remount is not a reload.
 *
 * Navigating is a change to the board like any other, so it advances the
 * revision and names who did it. The two navigations are bound here, once, for
 * the same reason the drive binds its own: which one a call site reaches for
 * is the whole answer to who moved the board, so no call site names an actor
 * and none can name the wrong one.
 */
export type CoachingBoardPage = {
  /** `open_opening_line` and an Opening Line `set_board_position` target. */
  navigateAsAgent: Navigate
  /** The board's own affordances: the target dialog, the study's next moves,
   * and the Game the Player just imported. */
  navigateAsPlayer: Navigate
  /** Where a board mounting now picks up. */
  readRevision: () => CoachingBoardPageRevision
  /** Where the board showing has come to, so the next one starts there. */
  reachedRevision: (reached: CoachingBoardPageRevision) => void
}

/** One page for as long as the mount lives, which is the whole point of it. */
export function useCoachingBoardPage(navigate: Navigate): CoachingBoardPage {
  // The route may hand down a fresh navigate; the page must not be rebuilt for
  // that, so it calls through the latest one rather than the one it was built
  // with.
  const latest = useRef(navigate)
  latest.current = navigate
  return useRef(coachingBoardPage((href) => latest.current(href))).current
}

/** The page itself: one revision, advanced by whichever navigation is used. */
export function coachingBoardPage(navigate: Navigate): CoachingBoardPage {
  let revision = initialPageRevision

  function navigatedBy(by: CoachingBoardActor): Navigate {
    return (href) => {
      revision = advancedPageRevision(revision, by)
      navigate(href)
    }
  }

  return {
    navigateAsAgent: navigatedBy("agent"),
    navigateAsPlayer: navigatedBy("player"),
    readRevision: () => revision,
    reachedRevision: (reached) => {
      revision = reached
    },
  }
}
