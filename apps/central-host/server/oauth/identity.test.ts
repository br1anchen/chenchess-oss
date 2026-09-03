import { describe, expect, test } from "vitest"

import {
  mcpConformanceAccessTokenClaims,
  parseVerifiedFirebaseIdentity,
} from "./identity"

const conformancePlayerId =
  "benchmark-issue-335-mcp-conformance:42f02064-8489-4d11-9f2f-696bbb20a1f2"

describe("Coach OAuth identity purpose", () => {
  test("parses an ordinary verified Player without a conformance claim", () => {
    expect(
      parseVerifiedFirebaseIdentity({
        authorizationKind: "player",
        playerId: "firebase-player-a",
      }),
    ).toEqual({
      authorizationKind: "player",
      playerId: "firebase-player-a",
    })
    expect(mcpConformanceAccessTokenClaims("firebase-player-a")).toBeUndefined()
  })

  test("carries a strict conformance identity into the Coach access token", () => {
    expect(
      parseVerifiedFirebaseIdentity({
        authorizationKind: "mcpConformance",
        playerId: conformancePlayerId,
      }),
    ).toEqual({
      authorizationKind: "mcpConformance",
      playerId: conformancePlayerId,
    })
    expect(mcpConformanceAccessTokenClaims(conformancePlayerId)).toEqual({
      chenchessMcpConformance: true,
    })
  })

  test.each([
    {
      authorizationKind: "player",
      name: "reserved subject presented as an ordinary Player",
      playerId: conformancePlayerId,
    },
    {
      authorizationKind: "mcpConformance",
      name: "ordinary subject presented as conformance",
      playerId: "firebase-player-a",
    },
    {
      authorizationKind: "mcpConformance",
      name: "non-v4 reserved subject",
      playerId:
        "benchmark-issue-335-mcp-conformance:42f02064-8489-1d11-9f2f-696bbb20a1f2",
    },
    {
      authorizationKind: "mcpConformance",
      name: "uppercase reserved subject",
      playerId:
        "benchmark-issue-335-mcp-conformance:42F02064-8489-4D11-9F2F-696BBB20A1F2",
    },
  ])("rejects $name", ({ authorizationKind, playerId }) => {
    expect(() =>
      parseVerifiedFirebaseIdentity({ authorizationKind, playerId }),
    ).toThrow("playerId")
  })

  test.each([
    "firebase-player-a",
    "benchmark-issue-335-mcp-conformance:42f02064-8489-1d11-9f2f-696bbb20a1f2",
    "benchmark-issue-335-mcp-conformance:42F02064-8489-4D11-9F2F-696BBB20A1F2",
  ])("does not derive purpose for non-conformance subject %s", (playerId) => {
    expect(mcpConformanceAccessTokenClaims(playerId)).toBeUndefined()
  })
})
