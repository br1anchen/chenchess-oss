import { useCallback, useEffect, useState } from "react"

import type { Navigate } from "@/auth/RouteRedirect"

import { parseCoachingBoardRoute } from "./coachingBoardRoute"

/**
 * Navigation that keeps the Coaching Board's page alive.
 *
 * The app has no client-side router: every address is read once from the
 * document and every `navigate` replaced the document. That is right for
 * sign-in and the dashboard, and wrong for the board's own path — the page
 * revision (ADR 0062) lives in memory above the board, and a document load
 * threw it away, so an agent opening a Game from the lobby arrived at a board
 * that read as revision 1 with nobody having navigated.
 *
 * An address on the board's own path is now pushed onto history and rendered
 * in place, so the mount stays mounted and the page it holds carries on. The
 * back and forward buttons read the same way. Everything else still leaves
 * the document, exactly as before.
 */
export function useCoachingBoardNavigation(navigateElsewhere: Navigate) {
  const [pathname, setPathname] = useState(() => window.location.pathname)

  useEffect(() => {
    const onPopState = () => setPathname(window.location.pathname)
    window.addEventListener("popstate", onPopState)
    return () => window.removeEventListener("popstate", onPopState)
  }, [])

  const navigate = useCallback<Navigate>(
    (href) => {
      const url = new URL(href, window.location.href)
      if (url.origin !== window.location.origin || !onOwnPath(url.pathname)) {
        navigateElsewhere(href)
        return
      }
      window.history.pushState(null, "", url)
      setPathname(url.pathname)
    },
    [navigateElsewhere],
  )

  return { navigate, pathname }
}

/** The lobby and the two board addresses; a malformed one is left to the
 * document, which renders the same notice it always did. */
function onOwnPath(pathname: string) {
  const route = parseCoachingBoardRoute(pathname)
  return route.kind !== "none" && route.kind !== "invalid"
}
