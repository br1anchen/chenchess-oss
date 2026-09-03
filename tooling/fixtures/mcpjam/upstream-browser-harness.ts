import { build } from "esbuild"
import {
  execFile,
  spawn,
  type ChildProcessWithoutNullStreams,
} from "node:child_process"
import { promisify } from "node:util"
import { mkdtemp, readFile, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { createInterface, type Interface } from "node:readline"

import {
  pinnedMcpJamInspectionSchema,
  pinnedMcpJamInspectorPackage,
  pinnedMcpJamInspectorVersion,
  pinnedMcpJamRenderObservationSchema,
  workerMessageSchema,
  type JsonObject,
  type PinnedMcpJamInspection,
  type PinnedMcpJamRenderInput,
  type PinnedMcpJamRenderObservation,
  type PinnedMcpJamUpstreamIdentity,
  type WorkerCommand,
  type WorkerMessage,
} from "./upstream-browser-harness-protocol"
import { parseIsObject, parseIsString, type JsonValue } from "./json-value"
export type {
  PinnedMcpJamAppToolCallObservation,
  PinnedMcpJamDomObservation,
  PinnedMcpJamInspection,
  PinnedMcpJamRenderInput,
  PinnedMcpJamRenderObservation,
  PinnedMcpJamRenderStatus,
  PinnedMcpJamScreenshotObservation,
  PinnedMcpJamUpstreamIdentity,
} from "./upstream-browser-harness-protocol"

const execFileAsync = promisify(execFile)
const startupTimeoutMilliseconds = 60_000
const commandTimeoutMilliseconds = 60_000
const shutdownTimeoutMilliseconds = 5_000
const stderrLimit = 16_384

type WorkerCommandResult = JsonValue | undefined

type PinnedMcpJamCallTool = (input: {
  readonly args: JsonObject
  readonly name: string
  readonly serverId: string
}) => Promise<WorkerCommandResult>

type PendingCommand = {
  readonly reject: (error: Error) => void
  readonly resolve: (result: WorkerCommandResult) => void
  readonly timeout: ReturnType<typeof setTimeout>
}

type StderrBuffer = { text: string }

export function formatWorkerExitError(
  phase: string,
  code: number | null,
  signal: NodeJS.Signals | null,
  stderr: string,
): string {
  const trimmed = stderr.trim()
  return `The MCPJam worker exited ${phase} (code ${String(code)}, signal ${String(signal)}).${trimmed === "" ? "" : ` ${trimmed}`}`
}

export function isWorkerProtocolLine(line: string): boolean {
  return line.trim().startsWith("{")
}

/**
 * How a missed graceful shutdown reads in the run's stderr capture.
 *
 * Named so the note cannot be mistaken for the cause of a failure: the run
 * that prints it has already forced the worker down and cleaned up after it.
 */
export function gracefulShutdownNote(error: unknown): string {
  return `The MCPJam worker did not shut down gracefully and was forced down: ${parseErrorMessage(error)}`
}

export function appendWorkerProtocolNoise(
  stderr: { text: string },
  line: string,
  limit = stderrLimit,
): boolean {
  const trimmed = line.trim()
  if (trimmed === "" || isWorkerProtocolLine(trimmed)) return false
  if (stderr.text.length >= limit) return true
  const prefix = stderr.text === "" ? "" : "\n"
  stderr.text += `${prefix}${trimmed}`.slice(0, limit - stderr.text.length)
  return true
}

class PinnedMcpJamBrowserHarness {
  readonly upstream: PinnedMcpJamUpstreamIdentity

  private commandSequence = 0
  private disposed = false
  private readonly pendingCommands = new Map<string, PendingCommand>()

  private constructor(
    private readonly child: ChildProcessWithoutNullStreams,
    private readonly callTool: PinnedMcpJamCallTool,
    private readonly stderr: StderrBuffer,
    private readonly temporaryDirectory: string,
    upstream: PinnedMcpJamUpstreamIdentity,
  ) {
    this.upstream = upstream
  }

  static async open(
    callTool: PinnedMcpJamCallTool,
  ): Promise<PinnedMcpJamBrowserHarness> {
    const temporaryDirectory = await mkdtemp(
      join(tmpdir(), "chenchess-mcpjam-worker-"),
    )
    const workerPath = join(temporaryDirectory, "worker.mjs")
    try {
      await build({
        bundle: true,
        entryPoints: [
          new URL("./upstream-browser-harness-worker.ts", import.meta.url)
            .pathname,
        ],
        format: "esm",
        legalComments: "none",
        outfile: workerPath,
        platform: "node",
        target: "node22",
      })

      const inspectorNodePath = await resolvePinnedInspectorNodePath()
      const stderr: StderrBuffer = { text: "" }
      const child = spawn("node", [workerPath], {
        env: { ...process.env, NODE_PATH: inspectorNodePath },
        stdio: ["pipe", "pipe", "pipe"],
      })
      child.stderr.on("data", (chunk: Buffer) => {
        if (stderr.text.length >= stderrLimit) return
        stderr.text += chunk
          .toString("utf8")
          .slice(0, stderrLimit - stderr.text.length)
      })

      return await awaitWorkerReady({
        callTool,
        child,
        stderr,
        temporaryDirectory,
      })
    } catch (error) {
      await rm(temporaryDirectory, { force: true, recursive: true })
      throw error
    }
  }

  /** @internal Attaches the validated worker after its ready handshake. */
  static attachWorker(input: {
    readonly callTool: PinnedMcpJamCallTool
    readonly child: ChildProcessWithoutNullStreams
    readonly stderr: StderrBuffer
    readonly temporaryDirectory: string
    readonly upstream: PinnedMcpJamUpstreamIdentity
  }): PinnedMcpJamBrowserHarness {
    return new PinnedMcpJamBrowserHarness(
      input.child,
      input.callTool,
      input.stderr,
      input.temporaryDirectory,
      input.upstream,
    )
  }

  async render(
    input: PinnedMcpJamRenderInput,
  ): Promise<PinnedMcpJamRenderObservation> {
    const result = await this.request({
      id: this.nextCommandId(),
      input,
      kind: "render",
    })
    return parseRenderObservation(result)
  }

  async inspect(): Promise<PinnedMcpJamInspection> {
    const result = await this.request({
      id: this.nextCommandId(),
      kind: "inspect",
    })
    return parseInspection(result)
  }

  async dismiss(toolCallId: string): Promise<void> {
    await this.request({
      id: this.nextCommandId(),
      kind: "dismiss",
      toolCallId,
    })
  }

  async dispose(): Promise<void> {
    if (this.disposed) return
    this.disposed = true
    try {
      if (this.child.exitCode === null && this.child.signalCode === null) {
        await this.request(
          { id: this.nextCommandId(), kind: "dispose" },
          shutdownTimeoutMilliseconds,
        )
        this.child.stdin.end()
        await waitForExit(this.child, shutdownTimeoutMilliseconds)
      }
    } catch (error) {
      /* Saying goodbye is a courtesy, not the contract: the worker is closing
         a real browser and can miss the window on a loaded machine. The block
         below owns termination, so a missed farewell is a note on the run
         rather than its verdict — failing here would fail a run whose every
         assertion already passed. */
      appendWorkerProtocolNoise(this.stderr, gracefulShutdownNote(error))
    } finally {
      if (this.child.exitCode === null && this.child.signalCode === null) {
        this.child.kill("SIGTERM")
        try {
          await waitForExit(this.child, shutdownTimeoutMilliseconds)
        } catch {
          this.child.kill("SIGKILL")
          await waitForExit(this.child, shutdownTimeoutMilliseconds)
        }
      }
      this.failPending(new Error("The pinned MCPJam harness was disposed."))
      await rm(this.temporaryDirectory, { force: true, recursive: true })
    }
  }

  attachMessageLoop(lines: Interface): void {
    lines.on("line", (line) => {
      if (!isWorkerProtocolLine(line)) {
        appendWorkerProtocolNoise(this.stderr, line)
        return
      }
      const trimmed = line.trim()
      try {
        void this.handleWorkerMessage(
          parseWorkerMessage(JSON.parse(trimmed)),
        ).catch((error: unknown) => {
          this.failPending(
            new Error(
              `MCPJam worker handling failed: ${parseErrorMessage(error)}`,
            ),
          )
          this.child.kill("SIGTERM")
        })
      } catch (error) {
        this.failPending(
          new Error(
            `Invalid MCPJam worker message: ${parseErrorMessage(error)}`,
          ),
        )
        this.child.kill("SIGTERM")
      }
    })
    this.child.once("exit", (code, signal) => {
      if (this.pendingCommands.size === 0) return
      this.failPending(
        new Error(
          formatWorkerExitError(
            "before answering",
            code,
            signal,
            this.stderr.text,
          ),
        ),
      )
    })
    this.child.once("error", (error) => this.failPending(error))
  }

  private async handleWorkerMessage(message: WorkerMessage): Promise<void> {
    switch (message.kind) {
      case "response": {
        const pending = this.pendingCommands.get(message.id)
        if (pending === undefined) {
          return
        }
        this.pendingCommands.delete(message.id)
        clearTimeout(pending.timeout)
        if (message.outcome.kind === "success") {
          pending.resolve(parseWorkerCommandResult(message.outcome.result))
        } else {
          pending.reject(new Error(message.outcome.message))
        }
        return
      }
      case "callTool": {
        try {
          const result = await this.callTool({
            args: message.args,
            name: message.name,
            serverId: message.serverId,
          })
          this.write({
            callId: message.callId,
            kind: "callToolResult",
            outcome: { kind: "success", result },
          })
        } catch (error) {
          this.write({
            callId: message.callId,
            kind: "callToolResult",
            outcome: { kind: "error", message: parseErrorMessage(error) },
          })
        }
        return
      }
      case "fatal":
        this.failPending(new Error(message.message))
        this.child.kill("SIGTERM")
        return
      case "ready":
        throw new Error("The MCPJam worker sent a second ready message.")
      default: {
        const unhandled: never = message
        throw new Error(`Unhandled worker message: ${String(unhandled)}`)
      }
    }
  }

  private request(
    command: Exclude<WorkerCommand, { readonly kind: "callToolResult" }>,
    timeoutMilliseconds = commandTimeoutMilliseconds,
  ) {
    if (this.disposed && command.kind !== "dispose") {
      return Promise.reject(
        new Error("The pinned MCPJam harness is already disposed."),
      )
    }
    return new Promise((resolveCommand, rejectCommand) => {
      const timeout = setTimeout(() => {
        this.pendingCommands.delete(command.id)
        rejectCommand(new Error(`MCPJam command ${command.kind} timed out.`))
        this.child.kill("SIGTERM")
      }, timeoutMilliseconds)
      this.pendingCommands.set(command.id, {
        reject: rejectCommand,
        resolve: resolveCommand,
        timeout,
      })
      this.write(command)
    })
  }

  private write(command: WorkerCommand): void {
    this.child.stdin.write(`${JSON.stringify(command)}\n`)
  }

  private nextCommandId(): string {
    return `command-${++this.commandSequence}`
  }

  private failPending(error: Error): void {
    for (const pending of this.pendingCommands.values()) {
      clearTimeout(pending.timeout)
      pending.reject(error)
    }
    this.pendingCommands.clear()
  }
}

export async function openPinnedMcpJamBrowserHarness(options: {
  readonly callTool: PinnedMcpJamCallTool
}): Promise<PinnedMcpJamBrowserHarness> {
  return PinnedMcpJamBrowserHarness.open(options.callTool)
}

async function awaitWorkerReady(input: {
  readonly callTool: PinnedMcpJamCallTool
  readonly child: ChildProcessWithoutNullStreams
  readonly stderr: StderrBuffer
  readonly temporaryDirectory: string
}): Promise<PinnedMcpJamBrowserHarness> {
  return await new Promise((resolveReady, rejectReady) => {
    const lines = createInterface({
      input: input.child.stdout,
      crlfDelay: Infinity,
    })
    const timeout = setTimeout(() => {
      input.child.kill("SIGTERM")
      rejectReady(new Error("The pinned MCPJam worker did not become ready."))
    }, startupTimeoutMilliseconds)

    const rejectExit = (code: number | null, signal: NodeJS.Signals | null) => {
      clearTimeout(timeout)
      rejectReady(
        new Error(
          formatWorkerExitError(
            "during startup",
            code,
            signal,
            input.stderr.text,
          ),
        ),
      )
    }
    const rejectError = (error: Error) => {
      clearTimeout(timeout)
      rejectReady(error)
    }
    input.child.once("exit", rejectExit)
    input.child.once("error", rejectError)

    lines.once("line", (line) => {
      clearTimeout(timeout)
      input.child.off("exit", rejectExit)
      input.child.off("error", rejectError)
      try {
        const message = parseWorkerMessage(JSON.parse(line))
        if (message.kind !== "ready") {
          throw new Error(`Expected ready, received ${message.kind}.`)
        }
        const harness = PinnedMcpJamBrowserHarness.attachWorker({
          callTool: input.callTool,
          child: input.child,
          stderr: input.stderr,
          temporaryDirectory: input.temporaryDirectory,
          upstream: message.upstream,
        })
        harness.attachMessageLoop(lines)
        resolveReady(harness)
      } catch (error) {
        input.child.kill("SIGTERM")
        rejectReady(
          new Error(
            `Invalid MCPJam worker startup: ${parseErrorMessage(error)}`,
          ),
        )
      }
    })
  })
}

async function resolvePinnedInspectorNodePath(): Promise<string> {
  const packageSpecifier = `${pinnedMcpJamInspectorPackage}@${pinnedMcpJamInspectorVersion}`
  let stdout: string
  let stderr: string
  const packageManager = "npm"
  try {
    const result = await execFileAsync(
      packageManager,
      [
        "exec",
        "--yes",
        "--package",
        packageSpecifier,
        "--",
        "node",
        "-e",
        [
          'const fs = require("node:fs");',
          'const path = require("node:path");',
          "const name = process.env.CHENCHESS_MCPJAM_PACKAGE;",
          "const version = process.env.CHENCHESS_MCPJAM_VERSION;",
          'for (const entry of (process.env.PATH ?? "").split(path.delimiter)) {',
          '  const packageRoot = path.resolve(entry, "../" + name);',
          '  const packageJsonPath = path.join(packageRoot, "package.json");',
          "  if (!fs.existsSync(packageJsonPath)) continue;",
          '  const parsed = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));',
          "  if (parsed.name === name && parsed.version === version) {",
          "    console.log(packageRoot);",
          "    process.exit(0);",
          "  }",
          "}",
          "process.exit(1);",
        ].join("\n"),
      ],
      {
        env: {
          ...process.env,
          CHENCHESS_MCPJAM_PACKAGE: pinnedMcpJamInspectorPackage,
          CHENCHESS_MCPJAM_VERSION: pinnedMcpJamInspectorVersion,
          npm_config_loglevel: "error",
        },
        maxBuffer: 1_048_576,
        timeout: startupTimeoutMilliseconds,
      },
    )
    stdout = result.stdout
    stderr = result.stderr
  } catch (error) {
    const details =
      parseIsObject(error) &&
      error !== null &&
      "stderr" in error &&
      parseIsString(error.stderr) &&
      error.stderr.trim() !== ""
        ? error.stderr
        : parseErrorMessage(error)
    throw new Error(
      `Failed to resolve ${packageSpecifier}.${details.trim() === "" ? "" : ` ${details.trim()}`}`,
    )
  }

  const printed = stdout
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "")
  const packageRoot = printed[printed.length - 1]
  if (packageRoot === undefined) {
    throw new Error(
      `The one-shot inspector resolve printed no path.${stderr.trim() === "" ? "" : ` ${stderr.trim()}`}`,
    )
  }
  if (!packageRoot.endsWith(pinnedMcpJamInspectorPackage)) {
    throw new Error(
      `Resolved ${packageRoot} is not ${pinnedMcpJamInspectorPackage}.`,
    )
  }
  const inspectorNodePath = packageRoot.slice(
    0,
    -(pinnedMcpJamInspectorPackage.length + 1),
  )

  const parsed: unknown = JSON.parse(
    await readFile(
      join(inspectorNodePath, pinnedMcpJamInspectorPackage, "package.json"),
      "utf8",
    ),
  )
  if (
    !parseIsPackageIdentity(parsed) ||
    parsed.name !== pinnedMcpJamInspectorPackage ||
    parsed.version !== pinnedMcpJamInspectorVersion
  ) {
    throw new Error(`Resolved ${inspectorNodePath} is not ${packageSpecifier}.`)
  }

  return inspectorNodePath
}

function parseIsPackageIdentity(
  value: unknown,
): value is { readonly name: unknown; readonly version: unknown } {
  return (
    parseIsObject(value) &&
    value !== null &&
    "name" in value &&
    "version" in value
  )
}

function parseWorkerCommandResult(result: unknown): WorkerCommandResult {
  if (result === undefined) return undefined
  // SAFETY: worker success outcomes carry JSON from the protocol.
  return result as JsonValue
}

function parseWorkerMessage(value: unknown): WorkerMessage {
  return workerMessageSchema.parse(value)
}

function parseRenderObservation(value: unknown): PinnedMcpJamRenderObservation {
  return pinnedMcpJamRenderObservationSchema.parse(value)
}

function parseInspection(value: unknown): PinnedMcpJamInspection {
  return pinnedMcpJamInspectionSchema.parse(value)
}

function parseErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function waitForExit(
  child: ChildProcessWithoutNullStreams,
  timeoutMilliseconds: number,
): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve()
  }
  return new Promise((resolveExit, rejectExit) => {
    const timeout = setTimeout(() => {
      child.off("exit", onExit)
      rejectExit(new Error("The MCPJam worker did not exit after disposal."))
    }, timeoutMilliseconds)
    const onExit = (): void => {
      clearTimeout(timeout)
      resolveExit()
    }
    child.once("exit", onExit)
  })
}
