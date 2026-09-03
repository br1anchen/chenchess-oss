import { z } from "zod"

import type { FetchAccessToken } from "./FirebaseAuthProvider"

const authorizedPlayerSchema = z
  .object({
    playerId: z.string().regex(/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/),
  })
  .strict()

export type BetaAuthorization =
  | { kind: "granted"; playerId: string }
  | { kind: "required" }
  | { kind: "authenticationRequired" }
  | { kind: "unavailable" }

export async function checkBetaAuthorization(
  fetchAccessToken: FetchAccessToken,
  expectedPlayerId: string,
): Promise<BetaAuthorization> {
  let token: string | null
  try {
    // GET /authorization does not judge email_verified or the sign-in provider.
    // A cached token still authenticates, and a Player without Beta Access
    // correctly receives 403 Required. Minting is reserved for write paths that
    // admit from those claims. A forced refresh would not remount this check —
    // settledIdentity reuses the object when email, emailVerified, and playerId
    // are unchanged — but it would still cost a token round-trip on every load.
    token = await fetchAccessToken({ forceRefreshToken: false })
  } catch {
    return { kind: "authenticationRequired" }
  }
  if (!token) return { kind: "authenticationRequired" }

  let response: Response
  try {
    response = await fetch("/api/v1/beta-access/authorization", {
      headers: { Authorization: `Bearer ${token}` },
      method: "GET",
    })
  } catch {
    return { kind: "unavailable" }
  }
  if (response.status === 401) return { kind: "authenticationRequired" }
  if (response.status === 403) return { kind: "required" }
  if (!response.ok) return { kind: "unavailable" }

  const parsed = authorizedPlayerSchema.safeParse(
    await response.json().catch(() => null),
  )
  if (!parsed.success || parsed.data.playerId !== expectedPlayerId) {
    return { kind: "unavailable" }
  }
  return { kind: "granted", playerId: parsed.data.playerId }
}
