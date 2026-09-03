import { describe, expect, it } from "bun:test"

import {
  pinnedMcpJamRenderObservationSchema,
  workerCommandSchema,
  workerMessageSchema,
} from "./upstream-browser-harness-protocol"

const sha256 = "a".repeat(64)

const renderObservation = {
  appToolCalls: [{ elapsedMilliseconds: 1, name: "report", ok: true }],
  blockedRequestCount: 0,
  bridgeInitialized: true,
  consoleErrorCount: 0,
  dom: {
    criticalMomentsRegionObserved: true,
    gameReviewHeadingObserved: true,
    openMomentControlCount: 1,
    semanticStates: ["ready"],
    visibleText: "Game Review",
    visibleTextCharacters: 11,
    visibleTextSha256: sha256,
    visibleTextTruncated: false,
  },
  elapsedMilliseconds: 1,
  observedOutboundMethods: ["ui/initialize"],
  screenshot: { byteLength: 1, sha256 },
  status: "rendered" as const,
}

describe("pinned MCPJam worker protocol", () => {
  it("accepts a complete render observation", () => {
    expect(
      pinnedMcpJamRenderObservationSchema.parse(renderObservation),
    ).toEqual(renderObservation)
  })

  it.each([
    { ...renderObservation, status: "painted" },
    {
      ...renderObservation,
      dom: { ...renderObservation.dom, openMomentControlCount: -1 },
    },
    {
      ...renderObservation,
      screenshot: { ...renderObservation.screenshot, sha256: "not-a-digest" },
    },
    { ...renderObservation, observedOutboundMethods: [42] },
  ])("rejects an invalid nested render fact", (observation) => {
    expect(() =>
      pinnedMcpJamRenderObservationSchema.parse(observation),
    ).toThrow()
  })

  it("rejects malformed commands and callback messages", () => {
    expect(() =>
      workerCommandSchema.parse({
        id: "render-1",
        input: { html: "<html />", serverId: "server" },
        kind: "render",
      }),
    ).toThrow()
    expect(() =>
      workerMessageSchema.parse({
        args: [],
        callId: "call-1",
        kind: "callTool",
        name: "report_app_performance",
        serverId: "server",
      }),
    ).toThrow()
  })

  it("carries the exact mounting Tool definition into a browser render", () => {
    const tool = {
      _meta: {
        "chenchess/app-performance": {
          mode: "enabled",
          resourceBytes: 123,
          schemaVersion: 1,
        },
        ui: { resourceUri: "ui://chenchess/review.html" },
      },
      inputSchema: { type: "object" },
      name: "list_critical_moments",
    }
    const command = workerCommandSchema.parse({
      id: "render-1",
      input: {
        html: "<html />",
        serverId: "server",
        toolCallId: "call-1",
        toolInfo: { tool },
        toolName: "list_critical_moments",
      },
      kind: "render",
    })
    if (command.kind !== "render") {
      throw new Error("Expected a render command.")
    }

    expect(command).toHaveProperty("input.toolInfo.tool", tool)
    expect(() =>
      workerCommandSchema.parse({
        ...command,
        input: { ...command.input, toolInfo: { tool: {} } },
      }),
    ).toThrow()
  })
})
