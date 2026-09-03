import * as ts from "typescript"

export const falseDownloadResourceUri = "ui://mcpjam-repro/false-download.html"
export const shellOnlyResourceUri = "ui://mcpjam-repro/shell-only.html"
export const mcpAppResourceMimeType = "text/html;profile=mcp-app"

/** No `permissions` key is present by design. */
export const resourceUiMetadata = {
  prefersBorder: true,
} satisfies Readonly<{ prefersBorder: boolean }>

export const shellOnlySemanticState = {
  kind: "not-ready",
  reason: "no-game",
  visibleText: "No game loaded",
} satisfies Readonly<{
  kind: "not-ready"
  reason: "no-game"
  visibleText: "No game loaded"
}>

/**
 * Exact download patterns from MCPJam Inspector 2.35.0's widget scanner.
 *
 * Source pinned in README.md. Keeping the three patterns here makes the
 * reproduction fail when its input stops triggering the upstream heuristic.
 */
const pinnedMcpJamDownloadPatterns = [
  { literal: "ui/download-file", pattern: /ui\/download-file/ },
  { literal: "getFileDownloadUrl", pattern: /\bgetFileDownloadUrl\b/ },
  { literal: "uploadFile", pattern: /\buploadFile\b/ },
] as const satisfies readonly {
  readonly literal: string
  readonly pattern: RegExp
}[]

export type PinnedMcpJamDownloadMatch =
  (typeof pinnedMcpJamDownloadPatterns)[number]["literal"]

export function pinnedMcpJamDownloadMatches(
  source: string,
): readonly PinnedMcpJamDownloadMatch[] {
  return pinnedMcpJamDownloadPatterns.flatMap(({ literal, pattern }) =>
    pattern.test(source) ? [literal] : [],
  )
}

export function pinnedMcpJamInfersDownload(source: string): boolean {
  return pinnedMcpJamDownloadMatches(source).length > 0
}

const appDownloadMethodNames = new Set([
  "downloadFile",
  "getFileDownloadUrl",
  "uploadFile",
])

/**
 * Finds download requests in app-owned source. Dependency files are scanned
 * separately because a protocol constant inside an SDK is the false-positive
 * condition this fixture exists to preserve.
 */
export function findAppOwnedDownloadCalls(
  source: string,
  fileName: string,
): readonly string[] {
  const sourceFile = ts.createSourceFile(
    fileName,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  )
  const calls: string[] = []

  const visit = (node: ts.Node): void => {
    if (ts.isCallExpression(node) && isDownloadCall(node)) {
      const location = sourceFile.getLineAndCharacterOfPosition(node.getStart())
      calls.push(`${fileName}:${location.line + 1}:${location.character + 1}`)
    }
    ts.forEachChild(node, visit)
  }
  visit(sourceFile)
  return calls
}

function isDownloadCall(call: ts.CallExpression): boolean {
  if (
    ts.isPropertyAccessExpression(call.expression) &&
    appDownloadMethodNames.has(call.expression.name.text)
  ) {
    return true
  }
  if (ts.isElementAccessExpression(call.expression)) {
    const key = call.expression.argumentExpression
    if (ts.isStringLiteral(key) && appDownloadMethodNames.has(key.text)) {
      return true
    }
  }
  return call.arguments.some(containsRawDownloadMethod)
}

function containsRawDownloadMethod(node: ts.Node): boolean {
  if (ts.isStringLiteral(node) && node.text === "ui/download-file") return true
  let found = false
  ts.forEachChild(node, (child) => {
    if (!found) found = containsRawDownloadMethod(child)
  })
  return found
}
