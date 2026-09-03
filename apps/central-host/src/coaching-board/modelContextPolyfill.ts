export type RegisteredModelContextTool = {
  annotations?: ModelContextToolAnnotations
  description: string
  execute: ModelContextToolDefinition["execute"]
  inputSchema?: object
  name: string
}

/**
 * jsdom has no document.modelContext. Tests install this polyfill so
 * registration, teardown, and execute can be asserted without a browser host.
 */
export function installModelContextPolyfill(
  tools: Map<string, RegisteredModelContextTool> = new Map(),
) {
  const modelContext: ModelContext = {
    registerTool(tool, options) {
      tools.set(tool.name, tool)
      options?.signal?.addEventListener("abort", () => {
        tools.delete(tool.name)
      })
    },
  }
  Object.defineProperty(document, "modelContext", {
    configurable: true,
    value: modelContext,
    writable: true,
  })
  return tools
}

export function clearModelContextPolyfill() {
  Reflect.deleteProperty(document, "modelContext")
}
