import express from "express"
import { originValidation } from "@modelcontextprotocol/express"
import { toNodeHandler } from "@modelcontextprotocol/node"
import { createMcpHandler, McpServer } from "@modelcontextprotocol/server"

import { mcpAppResourceMimeType, resourceUiMetadata } from "./repro-contract"
import {
  buildReproWidgetArtifacts,
  type WidgetArtifact,
} from "./widget-artifacts"

const port = parsePort(process.env.MCPJAM_REPRO_PORT)
const origin = new URL(`http://127.0.0.1:${port}`)
const artifacts = await buildReproWidgetArtifacts()

const handler = toNodeHandler(
  createMcpHandler(() => {
    const server = new McpServer({
      name: "mcpjam-compatibility-repro",
      version: "1.0.0",
    })
    registerReproTool({
      artifact: artifacts.falseDownload,
      description:
        "Render a ready control widget whose bundle contains an inert dependency method name.",
      meaningfulContentPresent: true,
      name: "show_false_download_control",
      resultText: "The control is ready. It requests no file operation.",
      server,
      title: "False download inference control",
    })
    registerReproTool({
      artifact: artifacts.shellOnly,
      description:
        "Render a bridge-initialized shell that remains at No game loaded.",
      meaningfulContentPresent: false,
      name: "show_shell_only",
      resultText: "No game address or meaningful content is available.",
      server,
      title: "Shell-only render control",
    })
    return server
  }),
)

const app = express()
app.disable("x-powered-by")
app.get("/health", (_request, response) => {
  response.json({ ok: true, service: "mcpjam-compatibility-repro" })
})
app.use(
  "/mcp",
  originValidation(["127.0.0.1", "localhost"]),
  express.json({ limit: "64kb" }),
  (request, response) => handler(request, response, request.body),
)

const listener = app.listen(port, "127.0.0.1", () => {
  process.stdout.write(
    `${JSON.stringify({
      boundary: "mcpjam-compatibility-repro",
      event: "listening",
      url: `${origin.origin}/mcp`,
    })}\n`,
  )
})

let stopping = false
const stop = (): void => {
  if (stopping) return
  stopping = true
  listener.close((error) => process.exit(error ? 1 : 0))
}
process.once("SIGINT", stop)
process.once("SIGTERM", stop)

type ReproTool = {
  readonly artifact: WidgetArtifact
  readonly description: string
  readonly meaningfulContentPresent: boolean
  readonly name: string
  readonly resultText: string
  readonly server: McpServer
  readonly title: string
}

function registerReproTool(input: ReproTool): void {
  input.server.registerTool(
    input.name,
    {
      annotations: {
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
        readOnlyHint: true,
      },
      description: input.description,
      title: input.title,
      _meta: {
        "ui/resourceUri": input.artifact.resourceUri,
        ui: {
          resourceUri: input.artifact.resourceUri,
          visibility: ["model", "app"],
        },
      },
    },
    async () => ({
      content: [{ text: input.resultText, type: "text" }],
      structuredContent: {
        meaningfulContentPresent: input.meaningfulContentPresent,
        resourceUri: input.artifact.resourceUri,
      },
    }),
  )
  input.server.registerResource(
    input.title,
    input.artifact.resourceUri,
    {
      description: input.description,
      mimeType: mcpAppResourceMimeType,
      _meta: { ui: resourceUiMetadata },
    },
    async () => ({
      contents: [
        {
          _meta: { ui: resourceUiMetadata },
          mimeType: mcpAppResourceMimeType,
          text: input.artifact.html,
          uri: input.artifact.resourceUri,
        },
      ],
    }),
  )
}

function parsePort(value: string | undefined): number {
  if (value === undefined) return 5175
  const parsed = Number(value)
  if (!Number.isInteger(parsed) || parsed < 1 || parsed > 65_535) {
    throw new Error(
      "MCPJAM_REPRO_PORT must be an integer from 1 through 65535.",
    )
  }
  return parsed
}
