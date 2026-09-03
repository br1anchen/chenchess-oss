import { createHash } from "node:crypto"
import { existsSync } from "node:fs"
import { mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { delimiter, join, resolve } from "node:path"
import { createInterface } from "node:readline"
import { pathToFileURL } from "node:url"

import {
  pinnedMcpJamInspectorBundleSha256,
  pinnedMcpJamInspectorPackage,
  pinnedMcpJamInspectorVersion,
  workerCommandSchema,
  type JsonObject,
  type PinnedMcpJamAppToolCallObservation,
  type PinnedMcpJamDomObservation,
  type PinnedMcpJamInspection,
  type PinnedMcpJamRenderInput,
  type PinnedMcpJamRenderObservation,
  type PinnedMcpJamRenderStatus,
  type PinnedMcpJamScreenshotObservation,
  type PinnedMcpJamUpstreamIdentity,
  type WorkerCommand,
  type WorkerMessage,
} from "./upstream-browser-harness-protocol"
import {
  createInFlightCommandGate,
  disposeAndExitDecision,
} from "./upstream-browser-harness-worker-lifecycle"
import {
  parseIsFunction,
  parseIsObject,
  type HostFunction,
  type JsonValue,
} from "./json-value"
const visibleTextLimit = 4_096
const semanticStateLimit = 32
const semanticStateCharacterLimit = 128
const outboundMethodLimit = 64
const outboundMethodCharacterLimit = 160
const activityQuietMilliseconds = 120
const activityWaitLimitMilliseconds = 2_000
const captureStateKey = "__chenchessMcpJamOutboundCapture"

type UpstreamRenderInput = Omit<PinnedMcpJamRenderInput, "screenshotPath"> & {
  readonly hostContext?: JsonObject
  readonly keepMounted: true
}

type UpstreamRenderObservation = {
  readonly blockedRequests?: readonly string[]
  readonly bridgeInitialized?: boolean
  readonly consoleErrors?: readonly string[]
  readonly elapsedMs: number
  readonly screenshotBase64?: string
  readonly status: PinnedMcpJamRenderStatus
}

type BrowserLocator = {
  count(): Promise<number>
  evaluateAll<T>(
    callback: (elements: readonly Element[]) => T,
  ): Promise<Awaited<T>>
  innerText(): Promise<string>
}

type BrowserFrameLocator = {
  getByRole(
    role: "button" | "heading" | "region",
    options: { readonly exact: true; readonly name: string },
  ): BrowserLocator
  locator(selector: string): BrowserLocator
}

type BrowserPage = {
  evaluate<T, Argument>(
    callback: (argument: Argument) => T,
    argument: Argument,
  ): Promise<Awaited<T>>
  waitForTimeout(milliseconds: number): Promise<void>
}

type UpstreamHarness = {
  readonly page: BrowserPage | null
  dismissWidget(toolCallId: string): Promise<void>
  dispose(): Promise<void>
  ensureLaunched(): Promise<void>
  getMountedWidgetId(): string | null
  renderWidget(input: UpstreamRenderInput): Promise<UpstreamRenderObservation>
  widgetFrame(): BrowserFrameLocator
}

type ParentToolCallResult = JsonValue | undefined

type UpstreamHarnessConstructor = new (options: {
  readonly budgets: {
    readonly paintTimeoutMs: number
    readonly renderTimeoutMs: number
    readonly settleTimeoutMs: number
  }
  readonly callTool: (
    serverId: string,
    name: string,
    args: JsonObject,
  ) => Promise<ParentToolCallResult>
}) => UpstreamHarness

type PendingParentToolCall = {
  readonly reject: (error: Error) => void
  readonly resolve: (result: ParentToolCallResult) => void
}

const pendingParentToolCalls = new Map<string, PendingParentToolCall>()
let parentToolCallSequence = 0
let activeToolCalls: PinnedMcpJamAppToolCallObservation[] = []
let activeToolCallCount = 0

const upstreamModule = await loadPinnedUpstreamHarness()
const harness = new upstreamModule.McpAppBrowserHarness({
  budgets: {
    paintTimeoutMs: 8_000,
    renderTimeoutMs: 5_000,
    settleTimeoutMs: 2_000,
  },
  callTool: callParentTool,
})

send({ kind: "ready", upstream: upstreamModule.identity })

const input = createInterface({ input: process.stdin, crlfDelay: Infinity })
const inFlight = createInFlightCommandGate(send)
let commandQueue = Promise.resolve()
let cleanupPromise: Promise<void> | null = null

input.on("line", (line) => {
  let command: WorkerCommand
  try {
    command = parseWorkerCommand(JSON.parse(line))
  } catch (error) {
    fatal(`The parent sent an invalid command: ${parseErrorMessage(error)}`)
    return
  }

  if (command.kind === "callToolResult") {
    settleParentToolCall(command)
    return
  }

  commandQueue = commandQueue
    .then(async () => handleCommand(command))
    .catch((error: unknown) => {
      if (inFlight.interrupted) return
      send({
        id: command.id,
        kind: "response",
        outcome: { kind: "error", message: parseErrorMessage(error) },
      })
    })
})

input.once("close", () => {
  void disposeAndExit()
})
process.once("SIGINT", () => void disposeAndExit())
process.once("SIGTERM", () => void disposeAndExit())

async function handleCommand(
  command: Exclude<WorkerCommand, { readonly kind: "callToolResult" }>,
): Promise<void> {
  inFlight.begin(command.id)
  try {
    switch (command.kind) {
      case "render": {
        const result = await render(command.input)
        parseRespondSuccess(command.id, result)
        return
      }
      case "inspect": {
        const result = await inspectMountedWidget()
        parseRespondSuccess(command.id, result)
        return
      }
      case "dismiss": {
        await harness.dismissWidget(command.toolCallId)
        await restoreOutboundMethodCapture()
        parseRespondSuccess(command.id)
        return
      }
      case "dispose": {
        await disposeWorkerResources()
        parseRespondSuccess(command.id)
        inFlight.end()
        input.close()
        process.stdin.pause()
        return
      }
      default: {
        const unhandled: never = command
        throw new Error(`Unhandled worker command: ${String(unhandled)}`)
      }
    }
  } finally {
    inFlight.end()
  }
}

function parseRespondSuccess(id: string, result?: unknown): void {
  if (inFlight.interrupted) return
  send({
    id,
    kind: "response",
    outcome:
      result === undefined ? { kind: "success" } : { kind: "success", result },
  })
}

async function render(
  renderInput: PinnedMcpJamRenderInput,
): Promise<PinnedMcpJamRenderObservation> {
  await harness.ensureLaunched()
  await restoreOutboundMethodCapture()
  activeToolCalls = []
  activeToolCallCount = 0

  const { screenshotPath, toolInfo, ...upstreamInput } = renderInput
  const observation = await harness.renderWidget({
    ...upstreamInput,
    ...(toolInfo === undefined ? undefined : { hostContext: { toolInfo } }),
    html: injectOutboundMethodCapture(renderInput.html),
    keepMounted: true,
  })
  await waitForAppActivityToSettle()

  const dom =
    observation.status === "rendered" ? await observeMountedDom() : null
  const observedOutboundMethods = await readOutboundMethods()
  const screenshot = await observeScreenshot(
    observation.screenshotBase64,
    screenshotPath,
  )

  if (renderInput.keepMounted !== true) {
    await harness.dismissWidget(renderInput.toolCallId)
    await restoreOutboundMethodCapture()
  }

  return {
    appToolCalls: [...activeToolCalls],
    blockedRequestCount: observation.blockedRequests?.length ?? 0,
    ...(observation.bridgeInitialized === undefined
      ? undefined
      : { bridgeInitialized: observation.bridgeInitialized }),
    consoleErrorCount: observation.consoleErrors?.length ?? 0,
    dom,
    elapsedMilliseconds: observation.elapsedMs,
    observedOutboundMethods: [...observedOutboundMethods],
    screenshot,
    status: observation.status,
  }
}

async function inspectMountedWidget(): Promise<PinnedMcpJamInspection> {
  const toolCallId = harness.getMountedWidgetId()
  if (toolCallId === null) {
    throw new Error("The pinned MCPJam harness has no mounted widget.")
  }
  return {
    dom: await observeMountedDom(),
    observedOutboundMethods: [...(await readOutboundMethods())],
    toolCallId,
  }
}

async function observeMountedDom(): Promise<PinnedMcpJamDomObservation> {
  const frame = harness.widgetFrame()
  const visibleText = await frame.locator("body").innerText()
  const gameReviewHeadingObserved =
    (await frame
      .getByRole("heading", { exact: true, name: "Game Review" })
      .count()) > 0
  const criticalMomentsRegionObserved =
    (await frame
      .getByRole("region", { exact: true, name: "Critical moments" })
      .count()) > 0
  const openMomentControls = frame.getByRole("button", {
    exact: true,
    name: "Open this moment",
  })
  const openMomentCandidateCount = Math.min(
    await openMomentControls.count(),
    100,
  )
  const openMomentControlCount = await openMomentControls.evaluateAll(
    (elements) =>
      elements.slice(0, 100).filter((element) => {
        if (!(element instanceof HTMLButtonElement) || element.disabled) {
          return false
        }
        const style = getComputedStyle(element)
        const rect = element.getBoundingClientRect()
        return (
          style.display !== "none" &&
          style.visibility !== "hidden" &&
          rect.width > 0 &&
          rect.height > 0
        )
      }).length,
  )
  if (openMomentControlCount > openMomentCandidateCount) {
    throw new Error("The Open this moment control count exceeded its bound.")
  }
  const semanticStates = await frame
    .locator("[data-semantic-state]")
    .evaluateAll((elements) =>
      elements
        .map((element) => element.getAttribute("data-semantic-state"))
        .filter((value): value is string => value !== null),
    )

  return {
    criticalMomentsRegionObserved,
    gameReviewHeadingObserved,
    openMomentControlCount,
    semanticStates: [
      ...new Set(
        semanticStates
          .slice(0, semanticStateLimit)
          .map((state) => state.slice(0, semanticStateCharacterLimit)),
      ),
    ],
    visibleText: visibleText.slice(0, visibleTextLimit),
    visibleTextCharacters: visibleText.length,
    visibleTextSha256: sha256(visibleText),
    visibleTextTruncated: visibleText.length > visibleTextLimit,
  }
}

async function observeScreenshot(
  screenshotBase64: string | undefined,
  screenshotPath: string | undefined,
): Promise<PinnedMcpJamScreenshotObservation | null> {
  if (screenshotBase64 === undefined) return null
  const bytes = Buffer.from(screenshotBase64, "base64")
  if (screenshotPath !== undefined) await writeFile(screenshotPath, bytes)
  return {
    byteLength: bytes.byteLength,
    ...(screenshotPath === undefined ? undefined : { path: screenshotPath }),
    sha256: sha256(bytes),
  }
}

async function restoreOutboundMethodCapture(): Promise<void> {
  const page = requireHarnessPage()
  await page.evaluate((key) => {
    type HostFunction = (...args: never[]) => void
    type CapturedOutbound = {
      marker: string
      originalPostMessage: HostFunction
    }
    function parseIsHostFunction(value: unknown): value is HostFunction {
      return typeof value === "function"
    }
    function parseIsCapturedOutbound(
      value: unknown,
    ): value is CapturedOutbound {
      if (typeof value !== "object" || value === null) return false
      if (!("marker" in value) || typeof value.marker !== "string") return false
      if (!("originalPostMessage" in value)) return false
      return parseIsHostFunction(value.originalPostMessage)
    }
    function parseOwnCapturedOutbound(
      target: typeof globalThis,
      name: string,
    ): CapturedOutbound | undefined {
      const descriptor = Object.getOwnPropertyDescriptor(target, name)
      const value = descriptor === undefined ? undefined : descriptor.value
      return parseIsCapturedOutbound(value) ? value : undefined
    }
    function forgetOwnProperty(target: typeof globalThis, name: string): void {
      const descriptor = Object.getOwnPropertyDescriptor(target, name)
      if (descriptor?.configurable !== true) return
      Object.defineProperty(target, name, {
        configurable: true,
        enumerable: false,
        value: undefined,
        writable: true,
      })
    }
    const existing = parseOwnCapturedOutbound(globalThis, key)
    if (existing?.marker === "chenchess-mcpjam-outbound-v1") {
      Object.defineProperty(globalThis, "postMessage", {
        configurable: true,
        value: existing.originalPostMessage,
        writable: true,
      })
      forgetOwnProperty(globalThis, key)
    }
  }, captureStateKey)
}

function injectOutboundMethodCapture(html: string): string {
  const script = `<script>
(() => {
  const parentWindow = window.parent;
  const key = ${JSON.stringify(captureStateKey)};
  const originalPostMessage = parentWindow.postMessage;
  const capture = {
    marker: "chenchess-mcpjam-outbound-v1",
    methods: [],
    originalPostMessage,
  };
  Object.defineProperty(parentWindow, key, {
    configurable: true,
    enumerable: false,
    value: capture,
    writable: false,
  });
  Object.defineProperty(parentWindow, "postMessage", {
    configurable: true,
    value(message, ...rest) {
      function parseIsObject(value) {
        return typeof value === "object" && value !== null && !Array.isArray(value);
      }
      function parseIsString(value) {
        return typeof value === "string";
      }
      if (
        capture.methods.length < ${outboundMethodLimit} &&
        message &&
        parseIsObject(message) &&
        message.jsonrpc === "2.0" &&
        parseIsString(message.method)
      ) {
        capture.methods.push(message.method.slice(0, ${outboundMethodCharacterLimit}));
      }
      return Reflect.apply(originalPostMessage, parentWindow, [message, ...rest]);
    },
    writable: true,
  });
})();
</script>`
  const head = /<head\b[^>]*>/i.exec(html)
  if (head === null) return `${script}${html}`
  const insertionIndex = head.index + head[0].length
  return `${html.slice(0, insertionIndex)}${script}${html.slice(insertionIndex)}`
}

async function readOutboundMethods(): Promise<readonly string[]> {
  const page = requireHarnessPage()
  const value = await page.evaluate((key) => {
    function parseIsString(value: unknown): value is string {
      return typeof value === "string"
    }
    function parseIsMethodsCapture(
      value: unknown,
    ): value is { methods: unknown[] } {
      return (
        typeof value === "object" &&
        value !== null &&
        "methods" in value &&
        Array.isArray(value.methods)
      )
    }
    function parseOwnMethodsCapture(
      target: typeof globalThis,
      name: string,
    ): { methods: unknown[] } | undefined {
      const descriptor = Object.getOwnPropertyDescriptor(target, name)
      const value = descriptor === undefined ? undefined : descriptor.value
      return parseIsMethodsCapture(value) ? value : undefined
    }
    const capture = parseOwnMethodsCapture(globalThis, key)
    if (capture === undefined) return []
    return capture.methods.filter((method): method is string =>
      parseIsString(method),
    )
  }, captureStateKey)
  return value.slice(0, outboundMethodLimit)
}

function requireHarnessPage(): BrowserPage {
  if (harness.page === null) {
    throw new Error("The pinned MCPJam browser page is not available.")
  }
  return harness.page
}

async function waitForAppActivityToSettle(): Promise<void> {
  const page = requireHarnessPage()
  const deadline = Date.now() + activityWaitLimitMilliseconds
  let previousCallCount = -1
  let stableSince = Date.now()

  while (Date.now() < deadline) {
    if (activeToolCallCount !== previousCallCount) {
      previousCallCount = activeToolCallCount
      stableSince = Date.now()
    } else if (
      pendingParentToolCalls.size === 0 &&
      Date.now() - stableSince >= activityQuietMilliseconds
    ) {
      return
    }
    await page.waitForTimeout(30)
  }
  if (pendingParentToolCalls.size > 0) {
    throw new Error(
      "An app-to-host tool call did not settle before inspection.",
    )
  }
}

function parseParentToolCallResult(result: unknown): ParentToolCallResult {
  if (result === undefined) return undefined
  // SAFETY: callToolResult carries JSON from the worker protocol.
  return result as JsonValue
}

async function callParentTool(
  serverId: string,
  name: string,
  args: JsonObject,
): Promise<ParentToolCallResult> {
  const callId = `call-${++parentToolCallSequence}`
  const startedAt = Date.now()
  activeToolCallCount += 1

  try {
    const result = await new Promise<ParentToolCallResult>(
      (resolveCall, rejectCall) => {
        pendingParentToolCalls.set(callId, {
          reject: rejectCall,
          resolve: resolveCall,
        })
        send({ args, callId, kind: "callTool", name, serverId })
      },
    )
    activeToolCalls.push({
      elapsedMilliseconds: Date.now() - startedAt,
      name,
      ok: true,
    })
    return result
  } catch (error) {
    activeToolCalls.push({
      elapsedMilliseconds: Date.now() - startedAt,
      name,
      ok: false,
    })
    throw error
  }
}

function settleParentToolCall(
  command: Extract<WorkerCommand, { readonly kind: "callToolResult" }>,
): void {
  const pending = pendingParentToolCalls.get(command.callId)
  if (pending === undefined) {
    fatal(`The parent answered unknown tool call ${command.callId}.`)
    return
  }
  pendingParentToolCalls.delete(command.callId)
  if (command.outcome.kind === "success") {
    pending.resolve(parseParentToolCallResult(command.outcome.result))
  } else {
    pending.reject(new Error(command.outcome.message))
  }
}

async function loadPinnedUpstreamHarness(): Promise<{
  readonly identity: PinnedMcpJamUpstreamIdentity
  readonly McpAppBrowserHarness: UpstreamHarnessConstructor
  readonly patchDirectory: string
}> {
  const packageRoot = await findPinnedInspectorPackageRoot()
  const bundlePath = join(packageRoot, "dist/server/index.js")
  const source = await readFile(bundlePath, "utf8")
  const bundleSha256 = sha256(source)
  if (bundleSha256 !== pinnedMcpJamInspectorBundleSha256) {
    throw new Error(
      `Inspector ${pinnedMcpJamInspectorVersion} server bundle digest changed: ${bundleSha256}.`,
    )
  }

  const classMarker = "var McpAppBrowserHarness = class {"
  const cutoffMarker = "// server/utils/computer-use-tool.ts"
  if (countOccurrences(source, classMarker) !== 1) {
    throw new Error("The pinned Inspector harness class marker drifted.")
  }
  if (countOccurrences(source, cutoffMarker) !== 1) {
    throw new Error("The pinned Inspector harness cutoff marker drifted.")
  }
  const classIndex = source.indexOf(classMarker)
  const cutoffIndex = source.indexOf(cutoffMarker)
  if (cutoffIndex <= classIndex) {
    throw new Error("The pinned Inspector harness markers are out of order.")
  }

  const patchDirectory = await mkdtemp(
    join(tmpdir(), "chenchess-mcpjam-harness-"),
  )
  try {
    await writeFile(join(patchDirectory, "package.json"), '{"type":"module"}\n')
    await symlink(
      resolve(packageRoot, "../.."),
      join(patchDirectory, "node_modules"),
      "dir",
    )
    const harnessSource = source.slice(0, cutoffIndex)
    const pageHostContextMarker = "hostContext: {}\\n    });"
    const pageHostContextReplacement =
      "hostContext: opts.hostContext ?? {}\\n    });"
    const renderInputMarker =
      "          },\n          permissions: input.permissions,"
    const renderInputReplacement =
      "          },\n          hostContext: input.hostContext,\n          permissions: input.permissions,"
    if (
      countOccurrences(harnessSource, pageHostContextMarker) !== 1 ||
      countOccurrences(harnessSource, renderInputMarker) !== 1
    ) {
      throw new Error("The pinned Inspector harness host-context seam drifted.")
    }
    const patchedHarnessSource = harnessSource
      .replace(pageHostContextMarker, pageHostContextReplacement)
      .replace(renderInputMarker, renderInputReplacement)
    const patchedBundle = `${patchedHarnessSource}\nexport { McpAppBrowserHarness };\n`
    const patchedBundlePath = join(patchDirectory, "harness-export.js")
    await writeFile(patchedBundlePath, patchedBundle)
    const loaded: unknown = await import(pathToFileURL(patchedBundlePath).href)
    const constructor = parseHarnessConstructor(loaded)
    return {
      identity: {
        bundleSha256,
        packageName: pinnedMcpJamInspectorPackage,
        version: pinnedMcpJamInspectorVersion,
      },
      McpAppBrowserHarness: constructor,
      patchDirectory,
    }
  } catch (error) {
    await rm(patchDirectory, { force: true, recursive: true })
    throw error
  }
}

async function findPinnedInspectorPackageRoot(): Promise<string> {
  const nodePathValue = process.env.NODE_PATH
  if (nodePathValue !== undefined) {
    for (const entry of nodePathValue.split(delimiter)) {
      if (entry === "") continue
      const matched = await readPinnedInspectorPackageRoot(
        join(entry, pinnedMcpJamInspectorPackage),
      )
      if (matched !== undefined) return matched
    }
  }

  const pathValue = process.env.PATH
  if (pathValue === undefined) {
    throw new Error("PATH is missing in the pinned MCPJam worker.")
  }

  for (const entry of pathValue.split(delimiter)) {
    const matched = await readPinnedInspectorPackageRoot(
      resolve(entry, `../${pinnedMcpJamInspectorPackage}`),
    )
    if (matched !== undefined) return matched
  }

  throw new Error(
    `The pinned inspector package was not found on NODE_PATH or PATH.`,
  )
}

async function readPinnedInspectorPackageRoot(
  packageRoot: string,
): Promise<string | undefined> {
  const packageJsonPath = join(packageRoot, "package.json")
  if (!existsSync(packageJsonPath)) return undefined
  const parsed: unknown = JSON.parse(await readFile(packageJsonPath, "utf8"))
  if (parseIsPinnedInspectorPackage(parsed)) return packageRoot
  return undefined
}

function parseIsPinnedInspectorPackage(
  value: unknown,
): value is { name: string; version: string } {
  return (
    parseIsObject(value) &&
    value.name === pinnedMcpJamInspectorPackage &&
    value.version === pinnedMcpJamInspectorVersion
  )
}

function parseHarnessConstructor(value: unknown): UpstreamHarnessConstructor {
  if (
    typeof value !== "object" ||
    value === null ||
    !("McpAppBrowserHarness" in value) ||
    !parseIsHarnessConstructor(value.McpAppBrowserHarness)
  ) {
    throw new Error("The pinned Inspector harness export had an invalid shape.")
  }
  return value.McpAppBrowserHarness
}

function parseIsHarnessConstructor(
  value: unknown,
): value is UpstreamHarnessConstructor {
  return (
    parseIsFunction(value) &&
    parseIsHarnessPrototype("prototype" in value ? value.prototype : undefined)
  )
}

function parseIsHarnessPrototype(value: unknown): value is {
  dismissWidget: HostFunction
  dispose: HostFunction
  ensureLaunched: HostFunction
  getMountedWidgetId: HostFunction
  renderWidget: HostFunction
  widgetFrame: HostFunction
} {
  return (
    typeof value === "object" &&
    value !== null &&
    "dismissWidget" in value &&
    parseIsFunction(value.dismissWidget) &&
    "dispose" in value &&
    parseIsFunction(value.dispose) &&
    "ensureLaunched" in value &&
    parseIsFunction(value.ensureLaunched) &&
    "getMountedWidgetId" in value &&
    parseIsFunction(value.getMountedWidgetId) &&
    "renderWidget" in value &&
    parseIsFunction(value.renderWidget) &&
    "widgetFrame" in value &&
    parseIsFunction(value.widgetFrame)
  )
}

function parseWorkerCommand(value: unknown): WorkerCommand {
  return workerCommandSchema.parse(value)
}

function countOccurrences(source: string, needle: string): number {
  return source.split(needle).length - 1
}

function sha256(value: string | Uint8Array): string {
  return createHash("sha256").update(value).digest("hex")
}

function send(message: WorkerMessage): void {
  process.stdout.write(`${JSON.stringify(message)}\n`)
}

function fatal(message: string): void {
  send({ kind: "fatal", message })
  process.exitCode = 1
  input.close()
}

function parseErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

let exiting = false

async function disposeAndExit(): Promise<void> {
  if (exiting) return
  exiting = true
  const commandWasInFlight = inFlight.interruptStdinClosed()
  input.close()
  await disposeWorkerResources()
  const decision = disposeAndExitDecision(commandWasInFlight, process.exitCode)
  process.exitCode = decision.exitCode
  if (decision.shouldCallProcessExitZero) {
    process.exit(0)
    return
  }
  process.exit(1)
}

function disposeWorkerResources(): Promise<void> {
  cleanupPromise ??= (async () => {
    for (const pending of pendingParentToolCalls.values()) {
      pending.reject(new Error("The MCPJam harness worker is shutting down."))
    }
    pendingParentToolCalls.clear()
    await harness.dispose().catch(() => undefined)
    await rm(upstreamModule.patchDirectory, { force: true, recursive: true })
  })()
  return cleanupPromise
}
