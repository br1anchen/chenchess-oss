import { RouteRedirect, type Navigate } from "./RouteRedirect"
import type { VerifiedIdentityDestination } from "./verifiedIdentityDestination"

/**
 * Where a verified identity goes once nothing is left to check.
 *
 * Every destination this snapshot serves is a page, so arriving is a plain
 * navigation.
 */
export function VerifiedIdentityArrival({
  description,
  destination,
  navigate,
  title,
}: {
  description: string
  destination: VerifiedIdentityDestination
  navigate: Navigate
  title: string
}) {
  return (
    <RouteRedirect
      description={description}
      href={destination.href}
      navigate={navigate}
      title={title}
    />
  )
}
