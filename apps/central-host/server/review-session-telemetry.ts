import { AsyncLocalStorage } from "node:async_hooks"
import { randomUUID } from "node:crypto"
import { gzipSync } from "node:zlib"

import type { NextFunction, Request, Response } from "express"
import { z } from "zod"
import type { CallToolResult } from "@modelcontextprotocol/server"
import {
  parseJsonObject,
  readJsonObject,
  type JsonObject,
  jsonObjectSchema,
  type OperationId,
  type RequestId,
  type ReviewSessionCommand,
} from "@chenchess/coach-engine-sdk"
import type {
  CoachAppPerformanceMode,
  CoachAppPerformanceSurface,
} from "@chenchess/coach-app/app-performance-contract"

import type {
  CoachCallerKind,
  CoachToolTarget,
} from "./coach-app/tool-surface.js"
import type { DeploymentEnvironment } from "./deployment.js"
import { coachModernProtocolVersion } from "./coach-protocol-revision.js"
import { isCoachTraceId } from "./coach-trace-id.js"
import * as v from "valibot"

export { isCoachTraceId } from "./coach-trace-id.js"

export const coachCallerMetaKey = "chenchess/caller"
export const coachTelemetryMetaKey = "chenchess/telemetry"
export const coachTraceHeader = "x-chenchess-trace-id"

export type { CoachCallerKind } from "./coach-app/tool-surface.js"

type CoachToolCatalogMetrics = {
  descriptionBytes: number
  inputSchemaBytes: number
  instructionsBytes: number
  outputSchemaBytes: number
  registeredToolCount: number
  schemaBytes: number
  toolCount: number
}

type CoachResourceMetrics = {
  gzipBytes: number
  rawBytes: number
}

type CoachCompletionStatus = "failed" | "succeeded"

export type CoachAppResourceKind =
  | "critical_moment_app"
  | "move_sequence_app"
  | "review_session_app"

export type CoachResourceKind =
  | CoachAppResourceKind
  | "game_review_snapshot"
  | "move_sequence"
  | "review_moment"
  | "review_moment_explanation"
  | "unknown"

type CoachAppTelemetryPolicy = {
  appPerformanceMode: CoachAppPerformanceMode
  deploymentEnvironment: DeploymentEnvironment
}

type CoachEngineCallMetrics = {
  callerKind: CoachCallerKind
  command: string
  firstByteMilliseconds?: number
  operationId: string
  requestBytes: number
  requestId: string
  responseBytes?: number
  retry: boolean
  gameImportId?: string
  status: CoachCompletionStatus
  terminalEventMilliseconds?: number
  totalMilliseconds: number
}

type CoachToolResultMetrics = {
  contentBytes: number
  metaBytes: number
  structuredContentBytes: number
}

type CoachModelContextResultMetrics = {
  contentTextBytes: number
  observationCount: number
  structuredContentBytes: number
  totalBytes: number
}

type CoachMcpToolTrace = {
  authenticationMilliseconds: number
  callerKind: Exclude<CoachCallerKind, "server-compound">
  engineCalls: CoachEngineCallMetrics[]
  failureKind?: string
  handlerSelectedMilliseconds?: number
  modelContextResult?: CoachModelContextResultMetrics
  protocolEra: "modern" | "unsupported"
  protocolVersion: string | null
  resource?: CoachResourceMetrics
  resourceKind: CoachResourceKind | null
  result?: CoachToolResultMetrics
  resultStatus?: CoachCompletionStatus
  startedAt: number
  tool: string
  toolRequestReceivedMilliseconds: number
  traceId: string
}

type CoachEngineCall = {
  fail(failureKind: string, responseBytes?: number): void
  firstByte(responseBytes: number): void
  succeed(responseBytes: number): void
  terminal(responseBytes: number): void
}

type CoachTraceRequest = {
  body?: unknown
  headers: Record<string, string | string[] | undefined>
}

type CoachTraceResponse = {
  statusCode: number
  once(event: "close" | "finish", listener: () => void): void
}

type CoachTelemetryLevel = "error" | "info" | "warn"

const toolCatalog = new Map<
  string,
  {
    descriptionBytes: number
    inputSchemaBytes: number
    outputSchemaBytes: number
    visibility: CoachToolTarget[]
  }
>()
let coachInstructionsBytes = 0
const traceStorage = new AsyncLocalStorage<CoachMcpToolTrace>()
const requestAdmission = new WeakMap<
  object,
  { admittedAt: number; authenticatedAt?: number; traceId: string }
>()
const issuedTraceIds = new Map<string, number>()
const traceLifetimeMilliseconds = 30 * 60 * 1_000
const coachAppResources = new Map<
  string,
  {
    kind: CoachAppResourceKind
    metrics?: CoachResourceMetrics & { source: string }
  }
>()

/** One app's whole artifact history under its slug, by URI prefix. */
const coachAppResourcePrefixes = new Map<string, CoachAppResourceKind>()

export function markCoachMcpRequestAdmission(
  request: Request,
  response: Response,
  next: NextFunction,
) {
  const traceId = issueCoachTraceId()
  requestAdmission.set(request, {
    admittedAt: performance.now(),
    traceId,
  })
  response.setHeader(coachTraceHeader, traceId)
  next()
}

export function issueCoachTraceId() {
  const traceId = `trace:review-session:${randomUUID()}`
  rememberIssuedTraceId(traceId)
  return traceId
}

export function markCoachMcpAuthenticationComplete(
  request: Request,
  _response: Response,
  next: NextFunction,
) {
  const timing = requestAdmission.get(request)
  if (timing) timing.authenticatedAt = performance.now()
  next()
}

export async function runWithCoachMcpTrace<T>(
  request: CoachTraceRequest,
  response: CoachTraceResponse,
  telemetryPolicy: CoachAppTelemetryPolicy,
  operation: () => Promise<T>,
): Promise<T> {
  const tool = parseOperationName(request.body)
  if (!tool) return operation()

  const admission = requestAdmission.get(request)
  const startedAt = admission?.admittedAt ?? performance.now()
  const protocol = parseCoachProtocolIdentity(request)
  const trace: CoachMcpToolTrace = {
    authenticationMilliseconds: roundMilliseconds(
      (admission?.authenticatedAt ?? startedAt) - startedAt,
    ),
    callerKind: parseCallerKind(request.body),
    engineCalls: [],
    protocolEra: protocol.era,
    protocolVersion: protocol.version,
    resourceKind: parseCoachResourceKind(request.body),
    startedAt,
    tool,
    toolRequestReceivedMilliseconds: roundMilliseconds(
      performance.now() - startedAt,
    ),
    traceId: admission?.traceId ?? `trace:review-session:${randomUUID()}`,
  }
  let emitted = false
  const emit = () => {
    if (emitted) return
    emitted = true
    // A tools/call whose arguments the registered schema refuses never reaches
    // the handler: the SDK answers a JSON-RPC error, so nothing sets
    // `resultStatus`, and grading on the transport status alone called a
    // refused call a success. The app-only performance beacon is what made that
    // blind spot expensive — its batch is `.strict()`, one bad field drops the
    // whole thing, and #337's `livePayload…` marks ride in exactly that batch,
    // so a Claude host reported nothing while every span said it landed.
    if (
      trace.failureKind === undefined &&
      trace.handlerSelectedMilliseconds === undefined &&
      toolCatalog.has(trace.tool) &&
      response.statusCode >= 200 &&
      response.statusCode < 400
    ) {
      trace.failureKind = "tool_arguments_rejected"
      parseRecordCoachToolArgumentsRejected(
        trace,
        request.body,
        telemetryPolicy,
      )
    }
    const status =
      !trace.failureKind &&
      trace.resultStatus !== "failed" &&
      response.statusCode >= 200 &&
      response.statusCode < 400
        ? "succeeded"
        : "failed"
    emitCoachTelemetry(
      {
        boundary: "coach-mcp",
        authenticationMilliseconds: trace.authenticationMilliseconds,
        callerKind: trace.callerKind,
        engineCalls: trace.engineCalls,
        event: "coach_mcp_tool_completion",
        failureKind: trace.failureKind,
        handlerSelectedMilliseconds: trace.handlerSelectedMilliseconds ?? null,
        hostModelPlanningMeasured: false,
        hostResponseGenerationMeasured: false,
        // The one fact that distinguishes a transport-level refusal from a
        // handler failure when a read fails with no failureKind (#324's
        // reopen certification hit exactly that blind spot).
        httpStatus: response.statusCode,
        latencyScope: "mcp_request_only",
        messageReceivedMilliseconds: null,
        modelContextResult: trace.modelContextResult,
        // Wall clock, so a journey can claim its own spans out of a log it
        // shares with every other Player. A hosted acceptance run is driven by
        // hand in a real chat and knows only when it started and stopped;
        // `traceId` then joins those spans to the iframe stages that followed
        // them. Every other boundary already emits this — the tool completion
        // was the one span a journey window could not select.
        observedAtUnixMilliseconds: Date.now(),
        protocolEra: trace.protocolEra,
        protocolVersion: trace.protocolVersion,
        resource: trace.resource ?? null,
        resourceKind: trace.resourceKind,
        result: trace.result,
        returnMilliseconds: roundMilliseconds(
          performance.now() - trace.startedAt,
        ),
        schemaVersion: 1,
        status,
        tool: trace.tool,
        toolCatalog: catalogMetrics(),
        toolRequestReceivedMilliseconds: trace.toolRequestReceivedMilliseconds,
        toolSelectedMilliseconds: null,
        traceId: trace.traceId,
      },
      status === "failed" ? "error" : "info",
    )
  }
  response.once("finish", emit)
  response.once("close", emit)
  return traceStorage.run(trace, operation)
}

export function observeCoachToolDefinition(
  name: string,
  description: string | undefined,
  inputSchema: z.ZodType,
  outputSchema: z.ZodType,
  visibility: CoachToolTarget[],
) {
  toolCatalog.set(name, {
    descriptionBytes: parseByteLength(description),
    inputSchemaBytes: parseByteLength(z.toJSONSchema(inputSchema)),
    outputSchemaBytes: parseByteLength(z.toJSONSchema(outputSchema)),
    visibility,
  })
}

/** Records the static instruction cost once per server construction. */
export function observeCoachMcpInstructions(instructions: string) {
  coachInstructionsBytes = Buffer.byteLength(instructions)
}

export function registerCoachAppResourceTelemetry(
  uri: string,
  kind: CoachAppResourceKind,
) {
  const registered = coachAppResources.get(uri)
  if (registered && registered.kind !== kind) {
    throw new Error(`Coach App resource ${uri} has conflicting kinds`)
  }
  if (!registered) coachAppResources.set(uri, { kind })
}

/**
 * Classifies every artifact of one app's slug, whichever build minted it.
 *
 * A persisted card requests the content-hashed template URI of its own build,
 * so the exact-URI registry above only ever knows the current build's three
 * hashes and a reopened older card logged `resourceKind="unknown"` — which is
 * how #324's orphaned-template reopens hid among genuinely foreign URIs.
 */
export function registerCoachAppResourcePrefixTelemetry(
  prefix: string,
  kind: CoachAppResourceKind,
) {
  const registered = coachAppResourcePrefixes.get(prefix)
  if (registered && registered !== kind) {
    throw new Error(`Coach App resource prefix ${prefix} has conflicting kinds`)
  }
  if (!registered) coachAppResourcePrefixes.set(prefix, kind)
}

/**
 * Attaches static artifact size only to the resources/read request that served
 * that exact allowlisted URI. The source text remains process-local and is
 * used only to invalidate the tiny metrics cache when a compatibility alias is
 * registered against new content in a test or development process.
 */
export function observeCoachAppResource(uri: string, resource: string) {
  const trace = traceStorage.getStore()
  const registered = coachAppResources.get(uri)
  if (
    !trace ||
    trace.tool !== "resources/read" ||
    trace.resourceKind !== registered?.kind
  ) {
    return
  }
  const cached = registered.metrics
  const metrics =
    cached?.source === resource
      ? cached
      : {
          gzipBytes: gzipSync(resource).byteLength,
          rawBytes: Buffer.byteLength(resource),
          source: resource,
        }
  registered.metrics = metrics
  trace.resource = {
    gzipBytes: metrics.gzipBytes,
    rawBytes: metrics.rawBytes,
  }
}

/**
 * Attaches the answered payload's size to a dynamic resource read.
 *
 * The static-artifact observer above is registry-gated and request-scoped; the
 * addressed snapshot and moment reads matched no registry entry, so their
 * completions logged `resource=null` and #324's central question — how many
 * bytes the host was asked to relay into the widget — was unanswerable from
 * production telemetry. Sizes only, never the URI: the address embeds the
 * Game Import ID, and opaque handles never belong in a log line.
 */
export function observeCoachDynamicResource(resource: string) {
  const trace = traceStorage.getStore()
  if (!trace || trace.tool !== "resources/read") return
  trace.resource = {
    gzipBytes: gzipSync(resource).byteLength,
    rawBytes: Buffer.byteLength(resource),
  }
}

export function observeCoachCacheLookup(
  cache: string,
  cacheHit: boolean,
  entryCount: number,
) {
  emitCoachTelemetry({
    boundary: "coach-mcp",
    cache,
    cacheHit,
    entryCount,
    event: "coach_process_cache_lookup",
    schemaVersion: 1,
    traceId: currentCoachTraceId() ?? null,
  })
}

export function observeMalformedGameImportId(tool: string, form: JsonObject) {
  emitCoachTelemetry(
    {
      boundary: "coach-mcp",
      event: "coach_malformed_game_import_id",
      schemaVersion: 1,
      form,
      tool,
      traceId: currentCoachTraceId() ?? null,
    },
    "error",
  )
}

export function markCoachToolHandlerSelected() {
  const trace = traceStorage.getStore()
  if (trace && trace.handlerSelectedMilliseconds === undefined) {
    trace.handlerSelectedMilliseconds = roundMilliseconds(
      performance.now() - trace.startedAt,
    )
  }
}

export function attachCoachTelemetry(result: CallToolResult): CallToolResult {
  const trace = traceStorage.getStore()
  if (!trace) return result
  const withTelemetry = {
    ...result,
    _meta: {
      ...result._meta,
      [coachTelemetryMetaKey]: { traceId: trace.traceId },
    },
  }
  trace.result = {
    contentBytes: parseByteLength(withTelemetry.content),
    metaBytes: parseByteLength(withTelemetry._meta),
    structuredContentBytes:
      trace.modelContextResult?.structuredContentBytes ??
      parseByteLength(withTelemetry.structuredContent),
  }
  trace.resultStatus = result.isError === true ? "failed" : "succeeded"
  return withTelemetry
}

/**
 * Measures only the parts of a terminal tool result that can enter model
 * context. The result itself stays process-local and unchanged.
 */
export function observeCoachToolResult(name: string, result: CallToolResult) {
  const trace = traceStorage.getStore()
  if (!trace || trace.tool !== name) return
  if (trace.modelContextResult) {
    trace.modelContextResult.observationCount += 1
    return
  }
  const contentTextBytes = result.content.reduce(
    (total, block) =>
      block.type === "text" ? total + Buffer.byteLength(block.text) : total,
    0,
  )
  const structuredContentBytes = parseByteLength(result.structuredContent)
  trace.modelContextResult = {
    contentTextBytes,
    observationCount: 1,
    structuredContentBytes,
    totalBytes: contentTextBytes + structuredContentBytes,
  }
}

export const recordCoachToolFailure = parseRecordCoachToolFailure

export function parseRecordCoachToolFailure(error: unknown) {
  const trace = traceStorage.getStore()
  if (trace) trace.failureKind = parseNormalizedFailure(error)
}

export function startCoachEngineCall(
  tool: string,
  command: ReviewSessionCommand,
  requestId: RequestId,
  operationId: OperationId,
  requestBytes: number,
): CoachEngineCall {
  const trace = traceStorage.getStore()
  const startedAt = performance.now()
  const commandKind = command.kind
  const retry =
    trace?.engineCalls.some((call) => call.command === commandKind) ?? false
  const callerKind: CoachCallerKind =
    trace && trace.engineCalls.length > 0
      ? "server-compound"
      : (trace?.callerKind ?? "model")
  let firstByteMilliseconds: number | undefined
  let terminalEventMilliseconds: number | undefined
  let completed = false

  const finish = (
    status: CoachEngineCallMetrics["status"],
    responseBytes?: number,
    failureKind?: string,
  ) => {
    if (completed) return
    completed = true
    const metrics: CoachEngineCallMetrics = {
      callerKind,
      command: commandKind,
      firstByteMilliseconds,
      operationId,
      requestBytes,
      requestId,
      responseBytes,
      retry,
      gameImportId:
        "gameImportId" in command ? command.gameImportId : undefined,
      status,
      terminalEventMilliseconds,
      totalMilliseconds: roundMilliseconds(performance.now() - startedAt),
    }
    trace?.engineCalls.push(metrics)
    if (failureKind && trace) trace.failureKind = failureKind
    emitCoachTelemetry(
      {
        boundary: "coach-mcp-to-engine",
        event: "coach_engine_call_completion",
        observedAtUnixMilliseconds: Date.now(),
        schemaVersion: 1,
        tool,
        traceId: trace?.traceId ?? null,
        ...metrics,
      },
      status === "failed" ? "error" : "info",
    )
  }

  return {
    fail(failureKind, responseBytes) {
      finish("failed", responseBytes, failureKind)
    },
    firstByte() {
      firstByteMilliseconds ??= roundMilliseconds(performance.now() - startedAt)
    },
    succeed(responseBytes) {
      finish("succeeded", responseBytes)
    },
    terminal() {
      terminalEventMilliseconds ??= roundMilliseconds(
        performance.now() - startedAt,
      )
    },
  }
}

export type CoachProtocolIdentity = {
  era: "modern" | "unsupported"
  /** The claimed revision, but only when it is one this server recognizes. */
  version: string | null
}

export const coachProtocolIdentity = parseCoachProtocolIdentity
export const coachResourceKind = parseCoachResourceKind

export function parseCoachProtocolIdentity(request: {
  body?: unknown
  headers: Record<string, string | string[] | undefined>
}): CoachProtocolIdentity {
  const body = readJsonObject(request.body)
  const params = readJsonObject(body?.params)
  const meta = readJsonObject(params?._meta)
  const header = singleHeader(request.headers["mcp-protocol-version"])
  const initializeVersion =
    body?.method === "initialize" && parseIsString(params?.protocolVersion)
      ? params.protocolVersion
      : undefined
  const claimedVersion =
    typeof meta?.["io.modelcontextprotocol/protocolVersion"] === "string"
      ? meta["io.modelcontextprotocol/protocolVersion"]
      : undefined
  const version = claimedVersion ?? header ?? initializeVersion
  return {
    era: version === coachModernProtocolVersion ? "modern" : "unsupported",
    version: version === coachModernProtocolVersion ? version : null,
  }
}

export function currentCoachTraceId() {
  return traceStorage.getStore()?.traceId
}

/**
 * Reduces a caller-controlled resource URI to an allowlisted category. Raw
 * URIs can contain Player or chess data, so telemetry must never retain them.
 */
export function parseCoachResourceKind(
  value: unknown,
): CoachResourceKind | null {
  const request = v.safeParse(mcpMethodRequestSchema, value)
  if (!request.success || request.output.method !== "resources/read")
    return null
  const params = v.safeParse(resourceReadParamsSchema, request.output.params)
  if (!params.success) return "unknown"
  const uri = params.output.uri
  const registered = coachAppResources.get(uri)
  if (registered) return registered.kind
  for (const [prefix, kind] of coachAppResourcePrefixes) {
    if (uri.startsWith(prefix)) return kind
  }
  return dynamicCoachResourceKind(uri) ?? "unknown"
}

function singleHeader(value: string | string[] | undefined) {
  return parseIsString(value) ? value : undefined
}

export function recordCoachAppPerformance(
  deploymentEnvironment: DeploymentEnvironment,
  report: {
    droppedMeasures: number
    /** Batches this card failed to deliver before the one being recorded. */
    droppedReports: number
    host: "chatgpt" | "claude" | "unknown"
    /** How many marks the card sent, which `marks` is recorded up to. */
    markCount: number
    marks: Array<{ milliseconds: number; stage: string }>
    measureCount: number
    measures: Array<{ milliseconds: number; stage: string }>
    motion: "normal" | "reduced"
    resourceBytes: number
    surface: CoachAppPerformanceSurface
    traceId: string | null
    viewport: "narrow" | "standard" | "wide"
  },
) {
  emitCoachTelemetry({
    boundary: "coach-app",
    deploymentEnvironment,
    disposition: "recorded",
    event: "coach_app_performance",
    // A card whose host withheld `_meta` reports no trace id, because the trace
    // id rides in the `_meta` that was withheld. Wall clock is then the only
    // thing that can attribute its report to a journey — and that report is
    // exactly the one saying the payload never arrived (#337).
    observedAtUnixMilliseconds: Date.now(),
    schemaVersion: 1,
    ...report,
  })
}

export function recordCoachAppPerformanceDisposition(
  deploymentEnvironment: DeploymentEnvironment,
) {
  emitCoachTelemetry({
    boundary: "coach-app",
    deploymentEnvironment,
    disposition: "suppressed",
    event: "coach_app_performance_disposition",
    observedAtUnixMilliseconds: Date.now(),
    schemaVersion: 1,
  })
}

/**
 * Names a batch the boundary refused, without carrying a value out of it.
 *
 * A refused call is invisible from both ends: the caller is told only that the
 * host did not accept it, and the span it left says it succeeded. Argument names
 * are structure rather than content, so every refused call carries them. The
 * enabled app-performance batch carries its stage names and counts too, because
 * an unlisted stage and an out-of-range duration are the two documented ways
 * this tool's report gets dropped, and a stage name is an identifier by
 * construction. Disabled deployments retain only the same privacy-safe
 * disposition emitted by an accepted report.
 */
function parseRecordCoachToolArgumentsRejected(
  trace: CoachMcpToolTrace,
  body: unknown,
  telemetryPolicy: CoachAppTelemetryPolicy,
) {
  if (
    trace.tool === coachPerformanceToolName &&
    telemetryPolicy.appPerformanceMode === "disabled"
  ) {
    recordCoachAppPerformanceDisposition(telemetryPolicy.deploymentEnvironment)
    return
  }
  emitCoachTelemetry(
    {
      boundary: "coach-mcp",
      callerKind: trace.callerKind,
      event: "coach_tool_arguments_rejected",
      schemaVersion: 1,
      tool: trace.tool,
      traceId: trace.traceId,
      ...parseRefusedArgumentForm(trace.tool, body),
    },
    "warn",
  )
}

/** The app-only beacon, spelled here because its module imports this one. */
const coachPerformanceToolName = "report_app_performance"

/**
 * What a stage name may look like before it is repeated into telemetry. Player
 * wording and chess notation carry spaces, digits, and punctuation; an
 * identifier cannot hold either of them.
 */
const stageNameForm = /^[A-Za-z][A-Za-z0-9]{0,63}$/

function parseRefusedArgumentForm(tool: string, body: unknown) {
  const parsed = v.safeParse(refusedToolBodySchema, body)
  const args = parsed.success ? parsed.output.params.arguments : undefined
  const argumentNames = args ? Object.keys(args).sort() : []
  if (!args || tool !== coachPerformanceToolName) return { argumentNames }
  const marks = parseRefusedPerformanceEntries(args.marks)
  const measures = parseRefusedPerformanceEntries(args.measures)
  return {
    argumentNames,
    markCount: marks.count,
    markStages: marks.stages,
    maximumMilliseconds: slowestRefusedEntry(
      marks.maximumMilliseconds,
      measures.maximumMilliseconds,
    ),
    measureCount: measures.count,
    measureStages: measures.stages,
    traceIdForm: parseRefusedTraceIdForm(args.traceId),
  }
}

function parseRefusedPerformanceEntries(value: unknown) {
  const entries = Array.isArray(value) ? value : []
  const stages = new Set<string>()
  let maximumMilliseconds: number | null = null
  for (const entry of entries.slice(0, 40)) {
    const parsed = v.safeParse(performanceEntrySchema, entry)
    const stage = parsed.success ? parsed.output.stage : undefined
    stages.add(
      parseIsString(stage) && stageNameForm.test(stage) ? stage : "unnamed",
    )
    const milliseconds = parsed.success ? parsed.output.milliseconds : undefined
    if (parseIsNumber(milliseconds) && Number.isFinite(milliseconds)) {
      maximumMilliseconds = Math.max(
        maximumMilliseconds ?? milliseconds,
        milliseconds,
      )
    }
  }
  return { count: entries.length, maximumMilliseconds, stages: [...stages] }
}

function slowestRefusedEntry(first: number | null, second: number | null) {
  if (first === null) return second
  return second === null ? first : Math.max(first, second)
}

function parseRefusedTraceIdForm(value: unknown) {
  if (value === undefined) return "absent"
  if (!parseIsString(value) || !isCoachTraceId(value)) return "malformed"
  return isIssuedCoachTraceId(value) ? "issued" : "wellFormed"
}

export function isIssuedCoachTraceId(value: string) {
  const issuedAt = issuedTraceIds.get(value)
  return (
    isCoachTraceId(value) &&
    issuedAt !== undefined &&
    Date.now() - issuedAt <= traceLifetimeMilliseconds
  )
}

function catalogMetrics(): CoachToolCatalogMetrics {
  let descriptionBytes = 0
  let inputSchemaBytes = 0
  let outputSchemaBytes = 0
  let toolCount = 0
  for (const metrics of toolCatalog.values()) {
    if (!metrics.visibility.includes("model")) continue
    descriptionBytes += metrics.descriptionBytes
    inputSchemaBytes += metrics.inputSchemaBytes
    outputSchemaBytes += metrics.outputSchemaBytes
    toolCount += 1
  }
  return {
    descriptionBytes,
    inputSchemaBytes,
    instructionsBytes: coachInstructionsBytes,
    outputSchemaBytes,
    registeredToolCount: toolCatalog.size,
    schemaBytes: inputSchemaBytes + outputSchemaBytes,
    toolCount,
  }
}

function parseOperationName(value: unknown) {
  const request = v.safeParse(mcpMethodRequestSchema, value)
  if (!request.success) return undefined
  if (request.output.method === "tools/call") {
    const params = v.safeParse(toolsCallParamsSchema, request.output.params)
    return params.success ? params.output.name : undefined
  }
  return request.output.method === "tools/list" ||
    request.output.method === "resources/read"
    ? request.output.method
    : undefined
}

function dynamicCoachResourceKind(uri: string): CoachResourceKind | undefined {
  let parsed: URL
  try {
    parsed = new URL(uri)
  } catch {
    return undefined
  }
  if (
    parsed.protocol !== "chenchess:" ||
    parsed.hostname !== "game-review" ||
    parsed.username ||
    parsed.password ||
    parsed.port ||
    parsed.search ||
    parsed.hash
  ) {
    return undefined
  }
  const segments = parsed.pathname.slice(1).split("/")
  if (segments.some((segment) => segment.length === 0)) return undefined
  if (segments.length === 1) return "game_review_snapshot"
  if (segments.length === 3 && segments[1] === "moment") {
    return "review_moment"
  }
  if (
    segments.length === 4 &&
    segments[1] === "moment" &&
    segments[3] === "explanation"
  ) {
    return "review_moment_explanation"
  }
  if (
    segments.length === 5 &&
    segments[1] === "moment" &&
    segments[3] === "sequence"
  ) {
    return "move_sequence"
  }
  return undefined
}

function parseCallerKind(
  value: unknown,
): Exclude<CoachCallerKind, "server-compound"> {
  return v.is(appCallerRequestSchema, value) ? "app" : "model"
}

function parseNormalizedFailure(error: unknown) {
  if (!(error instanceof Error)) return "non_error"
  if (error.name === "AbortError") return "cancelled"
  if (/HTTP \d{3}/.test(error.message)) return "coach_engine_http"
  if (/event stream/i.test(error.message)) return "coach_engine_stream"
  return error.name || "error"
}

function parseByteLength(value: unknown) {
  if (value === undefined) return 0
  return Buffer.byteLength(parseIsString(value) ? value : JSON.stringify(value))
}

function roundMilliseconds(value: number) {
  return Math.round(value * 100) / 100
}

function parseEmitCoachTelemetry(
  event: unknown,
  level: CoachTelemetryLevel = "info",
) {
  const payload = parseJsonObject(event)
  const message = parseIsString(payload.event)
    ? payload.event
    : "coach_telemetry"
  const encoded = `${JSON.stringify({ ...payload, level, message })}\n`
  if (level === "error") {
    process.stderr.write(encoded)
    return
  }
  process.stdout.write(encoded)
}

const emitCoachTelemetry = parseEmitCoachTelemetry

function rememberIssuedTraceId(traceId: string) {
  const now = Date.now()
  issuedTraceIds.set(traceId, now)
  if (issuedTraceIds.size < 10_000) return
  for (const [candidate, issuedAt] of issuedTraceIds) {
    if (now - issuedAt > traceLifetimeMilliseconds) {
      issuedTraceIds.delete(candidate)
    }
  }
  while (issuedTraceIds.size > 10_000) {
    const oldest = issuedTraceIds.keys().next().value
    if (oldest === undefined) break
    issuedTraceIds.delete(oldest)
  }
}

const mcpMethodRequestSchema = v.object({
  method: v.string(),
  params: v.optional(v.unknown()),
})

const resourceReadParamsSchema = v.object({
  uri: v.string(),
})

const toolsCallParamsSchema = v.object({
  name: v.string(),
})

const refusedToolBodySchema = v.object({
  params: v.object({
    arguments: jsonObjectSchema,
  }),
})

const performanceEntrySchema = v.object({
  milliseconds: v.optional(v.number()),
  stage: v.optional(v.string()),
})

const appCallerRequestSchema = v.object({
  params: v.object({
    _meta: v.object({
      [coachCallerMetaKey]: v.literal("app"),
    }),
  }),
})

function parseIsString(value: unknown): value is string {
  return v.is(v.string(), value)
}

function parseIsNumber(value: unknown): value is number {
  return v.is(v.number(), value)
}
