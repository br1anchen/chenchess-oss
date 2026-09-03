import { isIP } from "node:net"

import type {
  IncomingHttpHeaders,
  IncomingMessage,
  ServerResponse,
} from "node:http"
import * as v from "valibot"

export const COACH_PROTOCOL_ADMISSION_POLICY_VERSION = "v1"
export const COACH_PROTOCOL_ADMISSION_LAYER = "node"
export const COACH_PROTOCOL_ADMISSION_MAX_KEYS = 10_000
export const COACH_PROTOCOL_ADMISSION_EVENT = "coach_protocol_admission"

export const coachProtocolRouteClasses = [
  "metadataJwks",
  "registration",
  "authorization",
  "token",
  "unauthenticatedMcp",
  "authenticatedMcp",
  "health",
] as const

export type CoachProtocolRouteClass = (typeof coachProtocolRouteClasses)[number]

export type CoachProtocolPathClass = Exclude<
  CoachProtocolRouteClass,
  "authenticatedMcp"
>

export type CoachProtocolClassLimit = {
  baseline: number
  burst: number
  ceiling: number
  windowMilliseconds: number
}

export type CoachProtocolAdmissionPolicy = {
  version: string
  maxKeys: number
  classes: Record<CoachProtocolRouteClass, CoachProtocolClassLimit>
}

/**
 * Checked-in public Node OAuth/MCP admission policy.
 *
 * Baseline is a legitimate ChatGPT or Claude connect / reconnect burst on one
 * source: discovery + JWKS, one Dynamic Client Registration, authorization and
 * interaction pages, token exchange plus one refresh, a few unauthenticated
 * `/mcp` challenges, and a hosted Game Review journey's authenticated POSTs.
 * Burst is at least 10× that baseline. Ceiling is the documented absolute max
 * for the current single `central-host` replica; a second replica is not
 * authorized until shared enforcement exists.
 */
export const coachProtocolAdmissionPolicyV1 = {
  version: COACH_PROTOCOL_ADMISSION_POLICY_VERSION,
  maxKeys: COACH_PROTOCOL_ADMISSION_MAX_KEYS,
  classes: {
    metadataJwks: {
      baseline: 8,
      burst: 80,
      ceiling: 240,
      windowMilliseconds: 60_000,
    },
    registration: {
      baseline: 2,
      burst: 20,
      ceiling: 40,
      windowMilliseconds: 60_000,
    },
    authorization: {
      baseline: 12,
      burst: 120,
      ceiling: 240,
      windowMilliseconds: 60_000,
    },
    token: {
      baseline: 4,
      burst: 40,
      ceiling: 80,
      windowMilliseconds: 60_000,
    },
    unauthenticatedMcp: {
      baseline: 6,
      burst: 60,
      ceiling: 120,
      windowMilliseconds: 60_000,
    },
    authenticatedMcp: {
      baseline: 40,
      burst: 400,
      ceiling: 600,
      windowMilliseconds: 60_000,
    },
    health: {
      baseline: 12,
      burst: 120,
      ceiling: 300,
      windowMilliseconds: 60_000,
    },
  },
} as const satisfies CoachProtocolAdmissionPolicy

export type CoachProtocolAdmissionDecision = {
  admitted: boolean
  occupancy: number
  retryAfterSeconds?: number
}

type CoachProtocolReservation = {
  chargedAt: number
  key: string
  routeClass: CoachProtocolRouteClass
  release(): void
}

const coachProtocolRouteClassSchema = v.picklist(coachProtocolRouteClasses)

export type CoachProtocolAdmissionTelemetry = {
  boundedOccupancy: number
  decision: "admitted" | "rejected"
  event: typeof COACH_PROTOCOL_ADMISSION_EVENT
  layer: typeof COACH_PROTOCOL_ADMISSION_LAYER
  occupancy: number
  policyVersion: string
  retryAfterSeconds: number | undefined
  routeClass: CoachProtocolRouteClass
}

export class CoachProtocolAdmission {
  readonly #clock: () => number
  readonly #emit: (event: CoachProtocolAdmissionTelemetry) => void
  readonly policy: CoachProtocolAdmissionPolicy
  readonly #windows = new Map<string, number[]>()

  constructor(
    policy: CoachProtocolAdmissionPolicy = coachProtocolAdmissionPolicyV1,
    clock: () => number = Date.now,
    emit: (
      event: CoachProtocolAdmissionTelemetry,
    ) => void = writeAdmissionEvent,
  ) {
    this.policy = validateCoachProtocolAdmissionPolicy(policy)
    this.#clock = clock
    this.#emit = emit
  }

  admit(
    routeClass: CoachProtocolRouteClass,
    identity: string,
  ): CoachProtocolAdmissionDecision {
    return this.#decide(routeClass, identity)
  }

  reserve(
    routeClass: CoachProtocolRouteClass,
    identity: string,
  ): CoachProtocolAdmissionDecision & {
    reservation?: CoachProtocolReservation
  } {
    const decision = this.#decide(routeClass, identity)
    if (!decision.admitted) return decision
    const key = windowKey(routeClass, identity)
    const chargedAt = this.#windows.get(key)?.at(-1)
    if (chargedAt === undefined) return decision
    return {
      ...decision,
      reservation: {
        chargedAt,
        key,
        routeClass,
        release: () => this.release(routeClass, identity, chargedAt),
      },
    }
  }

  release(
    routeClass: CoachProtocolRouteClass,
    identity: string,
    chargedAt: number,
  ) {
    const key = windowKey(routeClass, identity)
    const stamps = this.#windows.get(key)
    if (!stamps) return
    const index = stamps.lastIndexOf(chargedAt)
    if (index < 0) return
    stamps.splice(index, 1)
    if (stamps.length === 0) this.#windows.delete(key)
  }

  #decide(
    routeClass: CoachProtocolRouteClass,
    identity: string,
  ): CoachProtocolAdmissionDecision {
    const now = this.#clock()
    const limit = this.policy.classes[routeClass]
    this.#expire(now)
    const key = windowKey(routeClass, identity)
    const existing = this.#windows.get(key)
    if (!existing && this.#windows.size >= this.policy.maxKeys) {
      const retryAfterSeconds = cardinalityRetryAfter(
        this.#windows,
        now,
        this.policy,
      )
      this.#emitDecision(routeClass, "rejected", retryAfterSeconds, 0)
      return { admitted: false, occupancy: 0, retryAfterSeconds }
    }
    const stamps = existing ?? []
    expireStamps(stamps, now, limit.windowMilliseconds)
    if (stamps.length >= limit.burst) {
      const retryAfterSeconds = retryAfterSecondsFrom(
        stamps[0] ?? now,
        limit.windowMilliseconds,
        now,
      )
      this.#windows.set(key, stamps)
      this.#emitDecision(
        routeClass,
        "rejected",
        retryAfterSeconds,
        stamps.length,
      )
      return { admitted: false, occupancy: stamps.length, retryAfterSeconds }
    }
    stamps.push(now)
    this.#windows.set(key, stamps)
    this.#emitDecision(routeClass, "admitted", undefined, stamps.length)
    return { admitted: true, occupancy: stamps.length }
  }

  #expire(now: number) {
    for (const [key, stamps] of this.#windows) {
      const routeClass = routeClassFromKey(key)
      expireStamps(
        stamps,
        now,
        this.policy.classes[routeClass].windowMilliseconds,
      )
      if (stamps.length === 0) this.#windows.delete(key)
    }
  }

  #emitDecision(
    routeClass: CoachProtocolRouteClass,
    decision: "admitted" | "rejected",
    retryAfterSeconds: number | undefined,
    occupancy: number,
  ) {
    this.#emit({
      boundedOccupancy: this.#windows.size,
      decision,
      event: COACH_PROTOCOL_ADMISSION_EVENT,
      layer: COACH_PROTOCOL_ADMISSION_LAYER,
      occupancy,
      policyVersion: this.policy.version,
      retryAfterSeconds,
      routeClass,
    })
  }
}

export function createCoachProtocolAdmission(
  policy: CoachProtocolAdmissionPolicy = coachProtocolAdmissionPolicyV1,
  clock: () => number = Date.now,
) {
  return new CoachProtocolAdmission(policy, clock)
}

export function validateCoachProtocolAdmissionPolicy(
  policy: CoachProtocolAdmissionPolicy,
): CoachProtocolAdmissionPolicy {
  if (policy.version.trim().length === 0) {
    throw new Error("Coach protocol admission policy version is required")
  }
  if (!Number.isInteger(policy.maxKeys) || policy.maxKeys <= 0) {
    throw new Error(
      "Coach protocol admission maxKeys must be a positive integer",
    )
  }
  for (const routeClass of coachProtocolRouteClasses) {
    const limit = policy.classes[routeClass]
    if (!limit) {
      throw new Error(`Coach protocol admission missing class ${routeClass}`)
    }
    if (
      !isPositiveInteger(limit.baseline) ||
      !isPositiveInteger(limit.burst) ||
      !isPositiveInteger(limit.ceiling) ||
      !isPositiveInteger(limit.windowMilliseconds)
    ) {
      throw new Error(
        `Coach protocol admission ${routeClass} requires positive integers`,
      )
    }
    if (limit.burst < limit.baseline * 10) {
      throw new Error(
        `Coach protocol admission ${routeClass} burst must be at least 10× baseline`,
      )
    }
    if (limit.ceiling < limit.burst) {
      throw new Error(
        `Coach protocol admission ${routeClass} ceiling must be at least burst`,
      )
    }
  }
  return policy
}

export function classifyCoachProtocolPath(
  pathname: string,
): CoachProtocolPathClass | undefined {
  if (pathname === "/health") return "health"
  if (
    pathname === "/.well-known/oauth-authorization-server" ||
    pathname === "/.well-known/oauth-protected-resource" ||
    pathname === "/.well-known/openid-configuration" ||
    pathname === "/jwks" ||
    pathname.startsWith("/.well-known/oauth-protected-resource/")
  ) {
    return "metadataJwks"
  }
  if (pathname === "/reg") return "registration"
  if (pathname === "/token" || pathname === "/token/revocation") return "token"
  if (pathname === "/mcp" || pathname.startsWith("/mcp/")) {
    return "unauthenticatedMcp"
  }
  if (
    pathname === "/auth" ||
    pathname.startsWith("/auth/") ||
    pathname.startsWith("/interaction/") ||
    pathname.startsWith("/session/")
  ) {
    return "authorization"
  }
  return undefined
}

export function trustedAnonymousSource(request: IncomingMessage) {
  const railwaySourceIp = headerValue(request.headers, "x-real-ip")
  if (railwaySourceIp !== undefined) {
    return isIP(railwaySourceIp) !== 0 ? railwaySourceIp : "unattested"
  }
  const socketSourceIp = request.socket.remoteAddress
  return socketSourceIp && isIP(socketSourceIp) !== 0
    ? socketSourceIp
    : "unattested"
}

export function admitHealthRequest(
  admission: CoachProtocolAdmission,
  request: IncomingMessage,
  response: ServerResponse,
) {
  const decision = admission.admit("health", trustedAnonymousSource(request))
  if (decision.admitted) return true
  sendAdmissionRejection(response, "health", decision, request)
  return false
}

export function sendAdmissionRejection(
  response: ServerResponse,
  routeClass: CoachProtocolRouteClass,
  decision: CoachProtocolAdmissionDecision,
  request?: IncomingMessage,
) {
  request?.resume()
  const retryAfterSeconds = decision.retryAfterSeconds ?? 1
  const body = admissionRejectionBody(routeClass)
  if (response.headersSent) return
  response.writeHead(429, {
    "Cache-Control": "no-store",
    "Content-Length": Buffer.byteLength(body),
    "Content-Type": "application/json; charset=utf-8",
    "Retry-After": String(retryAfterSeconds),
  })
  response.end(body)
}

export function admissionRejectionBody(routeClass: CoachProtocolRouteClass) {
  switch (routeClass) {
    case "registration":
    case "authorization":
    case "token":
      return `${JSON.stringify({
        error: "temporarily_unavailable",
        error_description: "Coach protocol traffic limit reached",
      })}\n`
    case "metadataJwks":
    case "unauthenticatedMcp":
    case "authenticatedMcp":
    case "health":
      return `${JSON.stringify({ error: "rate_limited" })}\n`
    default: {
      const exhaustive: never = routeClass
      return exhaustive
    }
  }
}

export const coachProtocolAdmissionTelemetryFields = [
  "event",
  "layer",
  "routeClass",
  "policyVersion",
  "decision",
  "retryAfterSeconds",
  "occupancy",
  "boundedOccupancy",
] as const

function headerValue(headers: IncomingHttpHeaders, name: string) {
  const value = headers[name]
  return parseIsString(value) ? value : undefined
}

function windowKey(routeClass: CoachProtocolRouteClass, identity: string) {
  return `${routeClass}\u0000${identity}`
}

function routeClassFromKey(key: string): CoachProtocolRouteClass {
  return v.parse(
    coachProtocolRouteClassSchema,
    key.slice(0, key.indexOf("\u0000")),
  )
}

function expireStamps(
  stamps: number[],
  now: number,
  windowMilliseconds: number,
) {
  while (stamps[0] !== undefined && now - stamps[0] >= windowMilliseconds) {
    stamps.shift()
  }
}

function retryAfterSecondsFrom(
  oldest: number,
  windowMilliseconds: number,
  now: number,
) {
  return secondsUntil(oldest + windowMilliseconds - now)
}

function cardinalityRetryAfter(
  windows: Map<string, number[]>,
  now: number,
  policy: CoachProtocolAdmissionPolicy,
) {
  let earliest = Number.POSITIVE_INFINITY
  for (const [key, stamps] of windows) {
    const oldest = stamps[0]
    if (oldest === undefined) continue
    const routeClass = routeClassFromKey(key)
    const readyAt = oldest + policy.classes[routeClass].windowMilliseconds
    if (readyAt < earliest) earliest = readyAt
  }
  return secondsUntil(Number.isFinite(earliest) ? earliest - now : 1_000)
}

function secondsUntil(remainingMilliseconds: number) {
  return Math.max(1, Math.ceil(remainingMilliseconds / 1_000))
}

function isPositiveInteger(value: number) {
  return Number.isInteger(value) && value > 0
}

function writeAdmissionEvent(event: CoachProtocolAdmissionTelemetry) {
  process.stdout.write(`${JSON.stringify(event)}\n`)
}

function parseIsString(value: unknown): value is string {
  return v.is(v.string(), value)
}
