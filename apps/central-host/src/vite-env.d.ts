/// <reference types="vite/client" />

/**
 * The browser tool host the Coaching Board registers with.
 *
 * Authored here instead of depending on a types-only package that tracks a
 * moving 0.1.x WebMCP spec. Only the calls this origin makes are declared.
 */
interface ModelContextToolAnnotations {
  idempotentHint?: boolean
  openWorldHint?: boolean
  readOnlyHint?: boolean
}

interface ModelContextToolResult {
  content?: ReadonlyArray<{ text: string; type: "text" }>
  structuredContent?: object
}

interface ModelContextToolArgs {}

interface ModelContextToolDefinition {
  annotations?: ModelContextToolAnnotations
  description: string
  execute: (
    args: ModelContextToolArgs,
  ) => ModelContextToolResult | Promise<ModelContextToolResult>
  inputSchema?: object
  name: string
}

interface ModelContext {
  registerTool(
    tool: ModelContextToolDefinition,
    options?: { signal?: AbortSignal },
  ): void
}

interface Document {
  modelContext?: ModelContext
}

interface ImportMetaEnv {
  readonly VITE_FIREBASE_API_KEY?: string
  readonly VITE_FIREBASE_APP_ID?: string
  readonly VITE_FIREBASE_AUTH_DOMAIN?: string
  readonly VITE_FIREBASE_AUTH_EMULATOR_URL?: string
  readonly VITE_FIREBASE_PROJECT_ID?: string
}
