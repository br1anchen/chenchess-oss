import { z } from "zod"

import type { FetchAccessToken } from "./FirebaseAuthProvider"

const redemptionResponseSchema = z
  .object({
    outcome: z.enum([
      "granted",
      "wrongAccount",
      "verificationRequired",
      "revoked",
      "invalid",
      "alreadyHandled",
      "tryLater",
    ]),
  })
  .strict()

export type InvitationRedemptionResult =
  | { kind: z.infer<typeof redemptionResponseSchema>["outcome"] }
  | { kind: "unavailable" }

export async function redeemInvitation(
  fetchAccessToken: FetchAccessToken,
  code: string,
): Promise<InvitationRedemptionResult> {
  try {
    const token = await fetchAccessToken({ forceRefreshToken: true })
    if (!token) return { kind: "unavailable" }
    const response = await fetch("/api/v1/beta-access/invitations/redeem", {
      body: JSON.stringify({ code }),
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      method: "POST",
    })
    if (!response.ok) return { kind: "unavailable" }
    const parsed = redemptionResponseSchema.safeParse(await response.json())
    return parsed.success
      ? { kind: parsed.data.outcome }
      : { kind: "unavailable" }
  } catch {
    return { kind: "unavailable" }
  }
}
