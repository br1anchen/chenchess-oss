import { useEffect } from "react"

import { clearOpeningExplorationRetention } from "./openingExplorationRetention"

/**
 * Retained opening exploration is per-tab view state, but it must not
 * survive a Player change: the next identity on this tab starts with no
 * inherited exploration. Reachability is already settled by the retention
 * itself, which is owned by a Player and answers no recall from another —
 * this clear drops the previous identity's exploration promptly rather than
 * leaving it in memory until the next Player retains something.
 */
export function useOpeningExplorationBoundary(
  authorizedPlayerId: string | null,
) {
  useEffect(() => clearOpeningExplorationRetention, [authorizedPlayerId])
}
