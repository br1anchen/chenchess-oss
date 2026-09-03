import { z } from "zod"

export const pinnedMcpJamInspectorPackage = "@mcpjam/inspector"
export const pinnedMcpJamInspectorVersion = "2.35.0"
export const pinnedMcpJamInspectorBundleSha256 =
  "4b6b08327b8df7a90fcce7b128a56330af9e296db608946e5be37972b36874ec"

export const pinnedMcpJamRenderStatuses = [
  "rendered",
  "no_ui_resource",
  "resource_read_failed",
  "mount_failed",
  "bridge_timeout",
  "render_error",
  "blank_screenshot",
  "screenshot_failed",
  "browser_unavailable",
] as const

const jsonObjectSchema = z.record(z.string(), z.unknown())
const toolDefinitionSchema = z
  .object({
    inputSchema: jsonObjectSchema,
    name: z.string().min(1),
  })
  .passthrough()
const toolInfoSchema = z
  .object({
    tool: toolDefinitionSchema,
  })
  .strict()
const sha256Schema = z.string().regex(/^[0-9a-f]{64}$/)
const successOutcomeSchema = z
  .object({ kind: z.literal("success"), result: z.unknown().optional() })
  .strict()
const errorOutcomeSchema = z
  .object({ kind: z.literal("error"), message: z.string() })
  .strict()
const workerOutcomeSchema = z.discriminatedUnion("kind", [
  successOutcomeSchema,
  errorOutcomeSchema,
])

const pinnedMcpJamDomObservationSchema = z
  .object({
    criticalMomentsRegionObserved: z.boolean(),
    gameReviewHeadingObserved: z.boolean(),
    openMomentControlCount: z.number().int().nonnegative(),
    semanticStates: z.array(z.string()),
    visibleText: z.string(),
    visibleTextCharacters: z.number().int().nonnegative(),
    visibleTextSha256: sha256Schema,
    visibleTextTruncated: z.boolean(),
  })
  .strict()

const pinnedMcpJamScreenshotObservationSchema = z
  .object({
    byteLength: z.number().int().nonnegative(),
    path: z.string().optional(),
    sha256: sha256Schema,
  })
  .strict()

const pinnedMcpJamAppToolCallObservationSchema = z
  .object({
    elapsedMilliseconds: z.number().finite().nonnegative(),
    name: z.string(),
    ok: z.boolean(),
  })
  .strict()

export const pinnedMcpJamRenderInputSchema = z
  .object({
    allowFeatures: z.record(z.string(), z.string()).optional(),
    cspMeta: z
      .object({
        connect_domains: z.array(z.string()).optional(),
        frame_domains: z.array(z.string()).optional(),
        resource_domains: z.array(z.string()).optional(),
      })
      .strict()
      .optional(),
    html: z.string(),
    keepMounted: z.boolean().optional(),
    permissions: jsonObjectSchema.optional(),
    resourceUri: z.string().optional(),
    sandboxAttrs: z.array(z.string()).optional(),
    screenshotPath: z.string().optional(),
    serverId: z.string(),
    toolCallId: z.string(),
    toolInput: jsonObjectSchema.optional(),
    toolInfo: toolInfoSchema.optional(),
    toolName: z.string(),
    toolOutput: z.unknown().optional(),
  })
  .strict()

export const pinnedMcpJamRenderObservationSchema = z
  .object({
    appToolCalls: z.array(pinnedMcpJamAppToolCallObservationSchema),
    blockedRequestCount: z.number().int().nonnegative(),
    bridgeInitialized: z.boolean().optional(),
    consoleErrorCount: z.number().int().nonnegative(),
    dom: pinnedMcpJamDomObservationSchema.nullable(),
    elapsedMilliseconds: z.number().finite().nonnegative(),
    observedOutboundMethods: z.array(z.string()),
    screenshot: pinnedMcpJamScreenshotObservationSchema.nullable(),
    status: z.enum(pinnedMcpJamRenderStatuses),
  })
  .strict()

export const pinnedMcpJamInspectionSchema = z
  .object({
    dom: pinnedMcpJamDomObservationSchema,
    observedOutboundMethods: z.array(z.string()),
    toolCallId: z.string(),
  })
  .strict()

export const pinnedMcpJamUpstreamIdentitySchema = z
  .object({
    bundleSha256: sha256Schema,
    packageName: z.literal(pinnedMcpJamInspectorPackage),
    version: z.literal(pinnedMcpJamInspectorVersion),
  })
  .strict()

export const workerCommandSchema = z.discriminatedUnion("kind", [
  z
    .object({
      id: z.string(),
      input: pinnedMcpJamRenderInputSchema,
      kind: z.literal("render"),
    })
    .strict(),
  z.object({ id: z.string(), kind: z.literal("inspect") }).strict(),
  z
    .object({
      id: z.string(),
      kind: z.literal("dismiss"),
      toolCallId: z.string(),
    })
    .strict(),
  z.object({ id: z.string(), kind: z.literal("dispose") }).strict(),
  z
    .object({
      callId: z.string(),
      kind: z.literal("callToolResult"),
      outcome: workerOutcomeSchema,
    })
    .strict(),
])

export const workerMessageSchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("ready"),
      upstream: pinnedMcpJamUpstreamIdentitySchema,
    })
    .strict(),
  z
    .object({
      id: z.string(),
      kind: z.literal("response"),
      outcome: workerOutcomeSchema,
    })
    .strict(),
  z
    .object({
      args: jsonObjectSchema,
      callId: z.string(),
      kind: z.literal("callTool"),
      name: z.string(),
      serverId: z.string(),
    })
    .strict(),
  z.object({ kind: z.literal("fatal"), message: z.string() }).strict(),
])

export type JsonObject = Readonly<z.infer<typeof jsonObjectSchema>>
export type PinnedMcpJamRenderStatus = z.infer<
  typeof pinnedMcpJamRenderObservationSchema
>["status"]
export type PinnedMcpJamRenderInput = Readonly<
  z.infer<typeof pinnedMcpJamRenderInputSchema>
>
export type PinnedMcpJamDomObservation = Readonly<
  z.infer<typeof pinnedMcpJamDomObservationSchema>
>
export type PinnedMcpJamScreenshotObservation = Readonly<
  z.infer<typeof pinnedMcpJamScreenshotObservationSchema>
>
export type PinnedMcpJamAppToolCallObservation = Readonly<
  z.infer<typeof pinnedMcpJamAppToolCallObservationSchema>
>
export type PinnedMcpJamRenderObservation = Readonly<
  z.infer<typeof pinnedMcpJamRenderObservationSchema>
>
export type PinnedMcpJamInspection = Readonly<
  z.infer<typeof pinnedMcpJamInspectionSchema>
>
export type PinnedMcpJamUpstreamIdentity = Readonly<
  z.infer<typeof pinnedMcpJamUpstreamIdentitySchema>
>
export type WorkerCommand = z.infer<typeof workerCommandSchema>
export type WorkerMessage = z.infer<typeof workerMessageSchema>
