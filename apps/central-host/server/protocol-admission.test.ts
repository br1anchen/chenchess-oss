import { describe, expect, test } from "vitest"
import {
  CoachProtocolAdmission,
  admissionRejectionBody,
  coachProtocolAdmissionPolicyV1,
  coachProtocolAdmissionTelemetryFields,
  createCoachProtocolAdmission,
  validateCoachProtocolAdmissionPolicy,
  type CoachProtocolAdmissionPolicy,
  type CoachProtocolAdmissionTelemetry,
  type CoachProtocolClassLimit,
} from "./protocol-admission"

const validClass: CoachProtocolClassLimit = {
  baseline: 1,
  burst: 10,
  ceiling: 20,
  windowMilliseconds: 60_000,
}

function testPolicy(
  overrides: Partial<CoachProtocolAdmissionPolicy> = {},
): CoachProtocolAdmissionPolicy {
  return {
    version: "v1-test",
    maxKeys: 8,
    classes: {
      authenticatedMcp: validClass,
      authorization: validClass,
      health: validClass,
      metadataJwks: validClass,
      registration: validClass,
      token: validClass,
      unauthenticatedMcp: validClass,
    },
    ...overrides,
  }
}

describe("Coach protocol admission policy", () => {
  test("accepts the checked-in v1 table", () => {
    expect(
      validateCoachProtocolAdmissionPolicy(coachProtocolAdmissionPolicyV1),
    ).toBe(coachProtocolAdmissionPolicyV1)
    expect(createCoachProtocolAdmission().policy.version).toBe("v1")
  })

  test("rejects missing, zero, negative, contradictory, or unsafe values", () => {
    expect(() =>
      validateCoachProtocolAdmissionPolicy({
        ...testPolicy(),
        version: "   ",
      }),
    ).toThrow(/version/i)
    expect(() =>
      validateCoachProtocolAdmissionPolicy({
        ...testPolicy(),
        maxKeys: 0,
      }),
    ).toThrow(/maxKeys/)
    expect(() =>
      validateCoachProtocolAdmissionPolicy({
        ...testPolicy(),
        classes: {
          ...testPolicy().classes,
          token: { ...validClass, burst: 0 },
        },
      }),
    ).toThrow(/positive/)
    expect(() =>
      validateCoachProtocolAdmissionPolicy({
        ...testPolicy(),
        classes: {
          ...testPolicy().classes,
          registration: { ...validClass, burst: 9 },
        },
      }),
    ).toThrow(/10×/)
    expect(() =>
      validateCoachProtocolAdmissionPolicy({
        ...testPolicy(),
        classes: {
          ...testPolicy().classes,
          health: { ...validClass, ceiling: 9 },
        },
      }),
    ).toThrow(/ceiling/)
  })

  test("admits exactly the burst, recovers after the window, and isolates classes", () => {
    let now = 1_000_000
    const events: CoachProtocolAdmissionTelemetry[] = []
    const admission = new CoachProtocolAdmission(
      testPolicy(),
      () => now,
      (event) => {
        events.push(event)
      },
    )

    for (let index = 0; index < 10; index += 1) {
      expect(admission.admit("registration", "203.0.113.1").admitted).toBe(true)
    }
    const rejected = admission.admit("registration", "203.0.113.1")
    expect(rejected).toEqual({
      admitted: false,
      occupancy: 10,
      retryAfterSeconds: 60,
    })
    expect(admission.admit("token", "203.0.113.1").admitted).toBe(true)
    expect(admission.admit("registration", "203.0.113.2").admitted).toBe(true)

    now += 60_000
    expect(admission.admit("registration", "203.0.113.1").admitted).toBe(true)
    expect(
      events.every((event) => event.event === "coach_protocol_admission"),
    ).toBe(true)
    expect(events[0]).toEqual({
      boundedOccupancy: expect.any(Number),
      decision: "admitted",
      event: "coach_protocol_admission",
      layer: "node",
      occupancy: 1,
      policyVersion: "v1-test",
      retryAfterSeconds: undefined,
      routeClass: "registration",
    })
    expect(Object.keys(events[0] ?? {}).sort()).toEqual(
      [...coachProtocolAdmissionTelemetryFields].sort(),
    )
    expect(JSON.stringify(events)).not.toContain("203.0.113")
  })

  test("rejects a new unique source when cardinality is full", () => {
    let now = 5_000
    const admission = new CoachProtocolAdmission(
      testPolicy({ maxKeys: 2 }),
      () => now,
    )
    expect(admission.admit("token", "1").admitted).toBe(true)
    expect(admission.admit("token", "2").admitted).toBe(true)
    const rejected = admission.admit("token", "3")
    expect(rejected.admitted).toBe(false)
    expect(rejected.retryAfterSeconds).toBe(60)
    now += 60_000
    expect(admission.admit("token", "3").admitted).toBe(true)
  })

  test("releases a reserved unauthenticated slot after verification succeeds", () => {
    const admission = new CoachProtocolAdmission(testPolicy(), () => 10_000)
    const reserved = admission.reserve("unauthenticatedMcp", "203.0.113.9")
    expect(reserved.admitted).toBe(true)
    reserved.reservation?.release()
    for (let index = 0; index < 10; index += 1) {
      expect(
        admission.admit("unauthenticatedMcp", "203.0.113.9").admitted,
      ).toBe(true)
    }
    expect(admission.admit("unauthenticatedMcp", "203.0.113.9").admitted).toBe(
      false,
    )
  })

  test("uses route-appropriate OAuth error JSON where OAuth defines one", () => {
    expect(JSON.parse(admissionRejectionBody("token"))).toEqual({
      error: "temporarily_unavailable",
      error_description: "Coach protocol traffic limit reached",
    })
    expect(JSON.parse(admissionRejectionBody("registration"))).toEqual({
      error: "temporarily_unavailable",
      error_description: "Coach protocol traffic limit reached",
    })
    expect(JSON.parse(admissionRejectionBody("unauthenticatedMcp"))).toEqual({
      error: "rate_limited",
    })
    expect(
      JSON.parse(admissionRejectionBody("authenticatedMcp")),
    ).not.toHaveProperty("result")
  })

  test("does not log raw source, credentials, or request bodies", () => {
    const events: CoachProtocolAdmissionTelemetry[] = []
    new CoachProtocolAdmission(testPolicy(), Date.now, (event) => {
      events.push(event)
    }).admit("token", "198.51.100.10")
    expect(JSON.stringify(events)).toContain("coach_protocol_admission")
    expect(JSON.stringify(events)).not.toContain("198.51.100.10")
    expect(JSON.stringify(events)).not.toContain("Bearer")
    expect(JSON.stringify(events)).not.toContain("pgn")
  })
})
