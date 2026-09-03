import { describe, expect, test } from "bun:test"
import { readFile } from "node:fs/promises"

import {
  findAppOwnedDownloadCalls,
  pinnedMcpJamDownloadMatches,
  pinnedMcpJamInfersDownload,
  resourceUiMetadata,
  shellOnlySemanticState,
} from "./repro-contract"
import { buildReproWidgetArtifacts } from "./widget-artifacts"

describe("MCPJam compatibility reproductions", () => {
  test("an inert bundled dependency string triggers download inference without permission or an app call", async () => {
    const source = await readFile(
      new URL("./false-download-widget.ts", import.meta.url),
      "utf8",
    )
    const artifacts = await buildReproWidgetArtifacts()

    expect(pinnedMcpJamInfersDownload(artifacts.falseDownload.html)).toBe(true)
    expect(pinnedMcpJamDownloadMatches(artifacts.falseDownload.html)).toContain(
      "ui/download-file",
    )
    expect(Object.hasOwn(resourceUiMetadata, "permissions")).toBe(false)
    expect(
      findAppOwnedDownloadCalls(source, "false-download-widget.ts"),
    ).toEqual([])
  })

  test("the painted shell records a non-meaningful No game state", async () => {
    const artifacts = await buildReproWidgetArtifacts()

    expect(shellOnlySemanticState).toEqual({
      kind: "not-ready",
      reason: "no-game",
      visibleText: "No game loaded",
    })
    expect(artifacts.shellOnly.html).toContain(
      'data-semantic-state="no-game">No game loaded',
    )
  })
})
