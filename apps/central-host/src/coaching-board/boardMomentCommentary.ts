import { frozenMomentText } from "@/review-session/reviewMoments"

import {
  viewedMoment,
  type CoachingBoardDriveState,
} from "./coachingBoardDrive"

/**
 * What the coach already said about the ply the board is showing.
 *
 * The Coaching Board is not a thread: this is the frozen Review's own
 * commentary, read beside the position. A ply the Review said nothing about
 * has no commentary, and the Player is not offered an empty card.
 */
export function boardMomentCommentary(
  state: CoachingBoardDriveState,
): string | null {
  const moment = viewedMoment(state)
  return moment ? frozenMomentText(moment) : null
}
