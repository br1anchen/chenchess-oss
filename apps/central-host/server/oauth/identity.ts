import { z } from "zod"

const playerId = z.string().regex(/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/)
const authorizedPlayer = z.object({ playerId }).strict()
const mcpConformancePlayerIdPattern =
  /^benchmark-issue-335-mcp-conformance:[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/
const verifiedIdentity = z.discriminatedUnion("authorizationKind", [
  z
    .object({
      authorizationKind: z.literal("player"),
      playerId: playerId.refine(
        (value) => !value.startsWith("benchmark-issue-335-mcp-conformance:"),
      ),
    })
    .strict(),
  z
    .object({
      authorizationKind: z.literal("mcpConformance"),
      playerId: playerId.regex(mcpConformancePlayerIdPattern),
    })
    .strict(),
])

export type VerifiedFirebaseIdentity = z.infer<typeof verifiedIdentity>

export function parseVerifiedFirebaseIdentity(value: unknown) {
  return verifiedIdentity.parse(value)
}

export function isMcpConformancePlayerId(value: string) {
  return mcpConformancePlayerIdPattern.test(value)
}

export function mcpConformanceAccessTokenClaims(subject: string | undefined) {
  return subject && isMcpConformancePlayerId(subject)
    ? { chenchessMcpConformance: true }
    : undefined
}

export async function verifyFirebasePlayer(
  coachEngineBaseUrl: string,
  firebaseIdToken: string,
) {
  if (!firebaseIdToken.trim() || firebaseIdToken.length > 16 * 1024) {
    throw new Error("Firebase ID token is invalid")
  }
  const response = await fetch(
    `${coachEngineBaseUrl}/internal/v1/oauth/firebase-identity`,
    {
      body: JSON.stringify({ firebaseIdToken }),
      headers: { "Content-Type": "application/json" },
      method: "POST",
      signal: AbortSignal.timeout(10_000),
    },
  )
  return readVerifiedIdentity(
    response,
    "Coach Engine rejected the Firebase identity",
  )
}

export async function verifyCoachPlayer(
  coachEngineBaseUrl: string,
  coachAccessToken: string,
) {
  const response = await fetch(
    `${coachEngineBaseUrl}/api/v1/beta-access/authorization`,
    {
      headers: { Authorization: `Bearer ${coachAccessToken}` },
      method: "GET",
      signal: AbortSignal.timeout(10_000),
    },
  )
  return readVerifiedPlayerId(
    response,
    "Coach Engine rejected current Beta Access",
  )
}

async function readVerifiedIdentity(response: Response, rejection: string) {
  if (!response.ok) throw new Error(rejection)
  return parseVerifiedFirebaseIdentity(await response.json())
}

async function readVerifiedPlayerId(response: Response, rejection: string) {
  if (!response.ok) throw new Error(rejection)
  return authorizedPlayer.parse(await response.json()).playerId
}
