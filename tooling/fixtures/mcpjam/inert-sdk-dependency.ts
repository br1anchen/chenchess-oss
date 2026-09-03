/**
 * A stand-in for an SDK module bundled into an app.
 *
 * The string is data, not a request. The widget reads it only to prove that
 * MCPJam's source scanner cannot distinguish dependency definitions from an
 * app-owned call site.
 */
const inertWireMethod = "ui/download-file"

export function inertDependencyMethodName(): string {
  return inertWireMethod
}
