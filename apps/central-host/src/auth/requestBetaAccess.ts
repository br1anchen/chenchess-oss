import { z } from "zod"

import type { FetchAccessToken } from "./FirebaseAuthProvider"

const responseSchema = z
  .object({
    message: z.string().min(1).max(500),
  })
  .strict()

export type BetaAccessRequestResult =
  | { kind: "accepted"; message: string }
  | { kind: "session"; message: string }
  | { kind: "unavailable"; message: string }

export const betaAccessRequestAcceptedMessage = "Thanks — we have your request."

export const betaAccessRequestUnavailableMessage =
  "Invite requests are temporarily unavailable. Please try again later."

export const betaAccessRequestSessionMessage =
  "Please sign out and sign in again, then ask for an invite."

export async function requestBetaAccess({
  fetchAccessToken,
  endpoint = "/api/v1/beta-access/requests",
}: {
  fetchAccessToken: FetchAccessToken
  endpoint?: string
}): Promise<BetaAccessRequestResult> {
  let token: string | null
  try {
    // Coach Engine admits the request from email_verified and provider claims.
    // A cached token can still have email_verified=false after the verification
    // continue URL updates the User record. The id-token event this emits does
    // not remount the form: settledIdentity returns the same object when email,
    // emailVerified, and playerId are unchanged.
    token = await fetchAccessToken({ forceRefreshToken: true })
  } catch {
    return sessionResult(betaAccessRequestSessionMessage)
  }
  if (!token) {
    return sessionResult(betaAccessRequestSessionMessage)
  }

  try {
    const response = await fetch(endpoint, {
      headers: {
        Accept: "application/json",
        Authorization: `Bearer ${token}`,
      },
      method: "POST",
    })
    const parsed = responseSchema.safeParse(
      await response.json().catch(() => null),
    )
    if (response.ok && parsed.success) {
      return { kind: "accepted", message: parsed.data.message }
    }
    if (response.status === 401) {
      return sessionResult(
        parsed.success ? parsed.data.message : betaAccessRequestSessionMessage,
      )
    }
    if (response.status === 403) {
      // Coach Engine's inadmissible-identity body always includes `message`.
      // A 403 without it is origin protection or another edge reject.
      return parsed.success
        ? sessionResult(parsed.data.message)
        : {
            kind: "unavailable",
            message: betaAccessRequestUnavailableMessage,
          }
    }
    return {
      kind: "unavailable",
      message: parsed.success
        ? parsed.data.message
        : betaAccessRequestUnavailableMessage,
    }
  } catch {
    return {
      kind: "unavailable",
      message: betaAccessRequestUnavailableMessage,
    }
  }
}

function sessionResult(message: string): BetaAccessRequestResult {
  return { kind: "session", message }
}
