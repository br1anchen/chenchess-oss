import { useSyncExternalStore } from "react"

const stackLayoutQuery = "(max-width: 64rem)"

/** True when Coaching Board columns stack — same 64rem cut as the board
 * / thread StyleX. Server and jsdom stay on the desktop header. */
export function useStackLayout() {
  return useSyncExternalStore(
    (onChange) => {
      const media = window.matchMedia(stackLayoutQuery)
      media.addEventListener("change", onChange)
      return () => media.removeEventListener("change", onChange)
    },
    () => window.matchMedia(stackLayoutQuery).matches,
    () => false,
  )
}
