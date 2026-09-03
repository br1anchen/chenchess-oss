import { describe, expect, it } from "bun:test"

import {
  appendWorkerProtocolNoise,
  formatWorkerExitError,
  gracefulShutdownNote,
  isWorkerProtocolLine,
} from "./upstream-browser-harness"
import type { WorkerMessage } from "./upstream-browser-harness-protocol"
import {
  createInFlightCommandGate,
  disposeAndExitDecision,
  workerStdinClosedInFlightMessage,
} from "./upstream-browser-harness-worker-lifecycle"

describe("formatWorkerExitError", () => {
  it("includes trimmed stderr when the worker exits before answering", () => {
    expect(
      formatWorkerExitError(
        "before answering",
        1,
        null,
        "  Cannot find module '@mcpjam/inspector'\n",
      ),
    ).toBe(
      "The MCPJam worker exited before answering (code 1, signal null). Cannot find module '@mcpjam/inspector'",
    )
  })

  it("includes trimmed stderr when the worker exits during startup", () => {
    expect(
      formatWorkerExitError("during startup", null, "SIGTERM", " boom \n"),
    ).toBe(
      "The MCPJam worker exited during startup (code null, signal SIGTERM). boom",
    )
  })

  it("omits a stderr suffix when the capture is empty", () => {
    expect(formatWorkerExitError("before answering", 0, null, "   \n")).toBe(
      "The MCPJam worker exited before answering (code 0, signal null).",
    )
  })
})

describe("MCPJam worker stdin close while a command is in flight", () => {
  it("sends the in-flight error response and does not exit 0", () => {
    const sent: WorkerMessage[] = []
    const inFlight = createInFlightCommandGate((message) => {
      sent.push(message)
    })

    inFlight.begin("command-1")
    expect(inFlight.interruptStdinClosed()).toBe(true)
    expect(sent).toEqual([
      {
        id: "command-1",
        kind: "response",
        outcome: {
          kind: "error",
          message: workerStdinClosedInFlightMessage,
        },
      },
    ])
    expect(workerStdinClosedInFlightMessage).toBe(
      "The MCPJam worker stdin closed while a command was in flight.",
    )
    expect(inFlight.interrupted).toBe(true)
    expect(inFlight.interruptStdinClosed()).toBe(true)
    expect(sent).toHaveLength(1)
    expect(disposeAndExitDecision(true)).toEqual({
      exitCode: 1,
      shouldCallProcessExitZero: false,
    })
  })

  it("disposes idle shutdown with process.exit(0)", () => {
    const sent: WorkerMessage[] = []
    const inFlight = createInFlightCommandGate((message) => {
      sent.push(message)
    })

    expect(inFlight.interruptStdinClosed()).toBe(false)
    expect(sent).toEqual([])
    expect(disposeAndExitDecision(false)).toEqual({
      exitCode: 0,
      shouldCallProcessExitZero: true,
    })
  })

  it("keeps a fatal exitCode 1 through idle dispose", () => {
    expect(disposeAndExitDecision(false, 1)).toEqual({
      exitCode: 1,
      shouldCallProcessExitZero: false,
    })
    expect(disposeAndExitDecision(false, "1")).toEqual({
      exitCode: 1,
      shouldCallProcessExitZero: false,
    })
  })
})

describe("non-JSON worker stdout is protocol noise", () => {
  it("does not treat Playwright Chromium setup text as a protocol line", () => {
    const chromium =
      "[browser-rendering] Chromium missing; setting up Playwright Chromium"
    expect(isWorkerProtocolLine(chromium)).toBe(false)
    expect(isWorkerProtocolLine('  {"kind":"ready"}')).toBe(true)
    const stderr = { text: "" }
    expect(appendWorkerProtocolNoise(stderr, chromium)).toBe(true)
    expect(stderr.text).toBe(chromium)
    expect(appendWorkerProtocolNoise(stderr, '{"kind":"ready"}')).toBe(false)
    expect(stderr.text).toBe(chromium)
  })
})

describe("gracefulShutdownNote", () => {
  it("reads as a forced teardown rather than a failed run", () => {
    expect(
      gracefulShutdownNote(new Error("MCPJam command dispose timed out.")),
    ).toBe(
      "The MCPJam worker did not shut down gracefully and was forced down: MCPJam command dispose timed out.",
    )
  })

  it("survives a thrown non-Error", () => {
    expect(gracefulShutdownNote("stdin closed")).toBe(
      "The MCPJam worker did not shut down gracefully and was forced down: stdin closed",
    )
  })

  it("is captured as stderr noise, not swallowed", () => {
    const stderr = { text: "" }
    expect(
      appendWorkerProtocolNoise(
        stderr,
        gracefulShutdownNote(new Error("dispose timed out")),
      ),
    ).toBe(true)
    expect(stderr.text).toContain("was forced down: dispose timed out")
  })
})
