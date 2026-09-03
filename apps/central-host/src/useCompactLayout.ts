import { useSyncExternalStore } from "react"

const compactLayoutQuery = "(max-width: 64rem)"

/** True on the one-column stack, where the conversation rides below the board
 * instead of the side column. Pinned to the foundation 64rem cut every board
 * and shell media query uses — a divergent pixel value here leaves a band
 * where fill sizing and the stacked CSS disagree. */
export function useCompactLayout() {
  return useSyncExternalStore(
    (onChange) => {
      const media = window.matchMedia(compactLayoutQuery)
      media.addEventListener("change", onChange)
      return () => media.removeEventListener("change", onChange)
    },
    () => window.matchMedia(compactLayoutQuery).matches,
    () => false,
  )
}
