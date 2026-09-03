export const spaSurfaceRoots = ["/app", "/join", "/login"] as const

export const staticPublicPages = ["privacy", "support", "terms"] as const

export const marketingPagePaths = [
  "/",
  "/privacy/",
  "/support/",
  "/terms/",
] as const

/**
 * The surface a path belongs to, or `undefined` when the path names a file or
 * sits outside every surface. `server.ts` resolves it to that surface's built
 * shell and `dev.ts` to its Astro route, so both answer a deep link the same
 * way.
 */
export function spaSurfaceRootFor(pathname: string) {
  const lastSegment = pathname.slice(pathname.lastIndexOf("/") + 1)
  if (lastSegment.includes(".")) return undefined
  return spaSurfaceRoots.find(
    (candidate) =>
      pathname === candidate || pathname.startsWith(`${candidate}/`),
  )
}

export function surfaceRouteUrl(url: URL) {
  const root = spaSurfaceRootFor(url.pathname)
  return root ? `${root}/${url.search}` : undefined
}
