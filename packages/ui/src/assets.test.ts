import { existsSync, readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { describe, expect, test } from "vitest"

type ChessPiecePaths = {
  king: string
  queen: string
  bishop: string
  knight: string
  rook: string
  pawn: string
}

type IllustrationAsset = {
  path: string
}

type AssetManifest = {
  appIcons: {
    dark180: string
    dark512: string
    darkSvg: string
    light180: string
    light512: string
    lightSvg: string
  }
  boardSurfaces: {
    frame: string
    lightSquare: string
    darkSquare: string
  }
  chessPieces: {
    viewBox: string
    white: ChessPiecePaths
    black: ChessPiecePaths
  }
  valueIcons: {
    see: string
    understand: string
    improve: string
    enjoy: string
  }
  illustrations: {
    reviewCompanions: IllustrationAsset
    mountainMist: IllustrationAsset
  }
  motionMasks: {
    brushCircle: string
    watercolorControlFrame: string
    watercolorControlFrameWide: string
    watercolorOutlineFrame: string
    pigmentBloom: string
    washReveal: string
    diffusionExit: string
    watercolorBrushHorizontal: string
    watercolorBrushVertical: string
    watercolorDot: string
  }
  brushStrokes: {
    bannerSlab: string
    strokeWide: string
    swoosh: string
    inkBlot: string
    inkSquare: string
    cloudWash: string
  }
}

const assetRoot = fileURLToPath(new URL("./assets/brand/", import.meta.url))
const watercolorTokenStyles = readFileSync(
  new URL("./theme/chenTokens.css", import.meta.url),
  "utf8",
)
const surfaceTokenStyles = readFileSync(
  new URL("./theme/surfaces.css", import.meta.url),
  "utf8",
)
const watercolorCraftSource = readFileSync(
  new URL("./components/watercolor.styles.ts", import.meta.url),
  "utf8",
)
const manifest = parseAssetManifest(
  JSON.parse(
    readFileSync(
      new URL("./assets/brand/manifest.json", import.meta.url),
      "utf8",
    ),
  ) as unknown,
)

function parseAssetManifest(value: unknown): AssetManifest {
  const manifest = parseManifestObject(value)
  const appIcons = parseManifestObject(manifest.appIcons)
  const boardSurfaces = parseManifestObject(manifest.boardSurfaces)
  const chessPieces = parseManifestObject(manifest.chessPieces)
  const valueIcons = parseManifestObject(manifest.valueIcons)
  const illustrations = parseManifestObject(manifest.illustrations)
  const motionMasks = parseManifestObject(manifest.motionMasks)
  const brushStrokes = parseManifestObject(manifest.brushStrokes)
  return {
    appIcons: {
      dark180: parseString(appIcons.dark180),
      dark512: parseString(appIcons.dark512),
      darkSvg: parseString(appIcons.darkSvg),
      light180: parseString(appIcons.light180),
      light512: parseString(appIcons.light512),
      lightSvg: parseString(appIcons.lightSvg),
    },
    boardSurfaces: {
      frame: parseString(boardSurfaces.frame),
      lightSquare: parseString(boardSurfaces.lightSquare),
      darkSquare: parseString(boardSurfaces.darkSquare),
    },
    chessPieces: {
      viewBox: parseString(chessPieces.viewBox),
      white: parseChessPiecePaths(parseManifestObject(chessPieces.white)),
      black: parseChessPiecePaths(parseManifestObject(chessPieces.black)),
    },
    valueIcons: {
      see: parseString(valueIcons.see),
      understand: parseString(valueIcons.understand),
      improve: parseString(valueIcons.improve),
      enjoy: parseString(valueIcons.enjoy),
    },
    illustrations: {
      reviewCompanions: parseIllustration(illustrations.reviewCompanions),
      mountainMist: parseIllustration(illustrations.mountainMist),
    },
    motionMasks: {
      brushCircle: parseString(motionMasks.brushCircle),
      watercolorControlFrame: parseString(motionMasks.watercolorControlFrame),
      watercolorControlFrameWide: parseString(
        motionMasks.watercolorControlFrameWide,
      ),
      watercolorOutlineFrame: parseString(motionMasks.watercolorOutlineFrame),
      pigmentBloom: parseString(motionMasks.pigmentBloom),
      washReveal: parseString(motionMasks.washReveal),
      diffusionExit: parseString(motionMasks.diffusionExit),
      watercolorBrushHorizontal: parseString(
        motionMasks.watercolorBrushHorizontal,
      ),
      watercolorBrushVertical: parseString(motionMasks.watercolorBrushVertical),
      watercolorDot: parseString(motionMasks.watercolorDot),
    },
    brushStrokes: {
      bannerSlab: parseString(brushStrokes.bannerSlab),
      strokeWide: parseString(brushStrokes.strokeWide),
      swoosh: parseString(brushStrokes.swoosh),
      inkBlot: parseString(brushStrokes.inkBlot),
      inkSquare: parseString(brushStrokes.inkSquare),
      cloudWash: parseString(brushStrokes.cloudWash),
    },
  }
}

type ManifestObject = {
  readonly identity?: ManifestValue
  readonly appIcons?: ManifestValue
  readonly boardSurfaces?: ManifestValue
  readonly chessPieces?: ManifestValue
  readonly valueIcons?: ManifestValue
  readonly illustrations?: ManifestValue
  readonly motionMasks?: ManifestValue
  readonly brushStrokes?: ManifestValue
  readonly viewBox?: ManifestValue
  readonly white?: ManifestValue
  readonly black?: ManifestValue
  readonly path?: ManifestValue
  readonly king?: ManifestValue
  readonly queen?: ManifestValue
  readonly bishop?: ManifestValue
  readonly knight?: ManifestValue
  readonly rook?: ManifestValue
  readonly pawn?: ManifestValue
  readonly dark180?: ManifestValue
  readonly dark512?: ManifestValue
  readonly darkSvg?: ManifestValue
  readonly light180?: ManifestValue
  readonly light512?: ManifestValue
  readonly lightSvg?: ManifestValue
  readonly frame?: ManifestValue
  readonly lightSquare?: ManifestValue
  readonly darkSquare?: ManifestValue
  readonly see?: ManifestValue
  readonly understand?: ManifestValue
  readonly improve?: ManifestValue
  readonly enjoy?: ManifestValue
  readonly reviewCompanions?: ManifestValue
  readonly mountainMist?: ManifestValue
  readonly brushCircle?: ManifestValue
  readonly watercolorControlFrame?: ManifestValue
  readonly watercolorControlFrameWide?: ManifestValue
  readonly watercolorOutlineFrame?: ManifestValue
  readonly pigmentBloom?: ManifestValue
  readonly washReveal?: ManifestValue
  readonly diffusionExit?: ManifestValue
  readonly watercolorBrushHorizontal?: ManifestValue
  readonly watercolorBrushVertical?: ManifestValue
  readonly watercolorDot?: ManifestValue
  readonly bannerSlab?: ManifestValue
  readonly strokeWide?: ManifestValue
  readonly swoosh?: ManifestValue
  readonly inkBlot?: ManifestValue
  readonly inkSquare?: ManifestValue
  readonly cloudWash?: ManifestValue
}

type ManifestValue = string | ManifestObject

function parseChessPiecePaths(value: ManifestObject): ChessPiecePaths {
  return {
    king: parseString(value.king),
    queen: parseString(value.queen),
    bishop: parseString(value.bishop),
    knight: parseString(value.knight),
    rook: parseString(value.rook),
    pawn: parseString(value.pawn),
  }
}

function parseIllustration(value: unknown): IllustrationAsset {
  return { path: parseString(parseManifestObject(value).path) }
}

function parseIsManifestObject(value: unknown): value is ManifestObject {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function parseManifestObject(value: unknown): ManifestObject {
  if (!parseIsManifestObject(value)) {
    throw new TypeError("asset manifest field must be an object")
  }
  return value
}

function parseString(value: unknown): string {
  if (typeof value !== "string") throw new TypeError("expected a string")
  return value
}

function manifestPaths() {
  return [
    ...Object.values(manifest.appIcons),
    ...Object.values(manifest.boardSurfaces),
    ...Object.values(manifest.chessPieces.white),
    ...Object.values(manifest.chessPieces.black),
    ...Object.values(manifest.valueIcons),
    ...Object.values(manifest.illustrations).map((asset) => asset.path),
    ...Object.values(manifest.motionMasks),
    ...Object.values(manifest.brushStrokes),
  ]
}

describe("production brand assets", () => {
  test("the manifest resolves only locally bundled assets", () => {
    const paths = manifestPaths()
    expect(paths.length).toBeGreaterThan(0)
    expect(paths.every((path) => !/^(?:https?:)?\/\//.test(path))).toBe(true)
    expect(paths.every((path) => existsSync(`${assetRoot}/${path}`))).toBe(true)
  })

  test("all twelve chess pieces share the controlled view box", () => {
    const pieces = [
      ...Object.values(manifest.chessPieces.white),
      ...Object.values(manifest.chessPieces.black),
    ]
    expect(pieces).toHaveLength(12)
    for (const piece of pieces) {
      const source = readFileSync(`${assetRoot}/${piece}`, "utf8")
      expect(source).toContain(`viewBox="${manifest.chessPieces.viewBox}"`)
      expect(source).toContain('role="img"')
      expect(source).toMatch(/<title>[A-Z][a-z]+ [a-z]+<\/title>/)
      expect(source).not.toContain("aria-labelledby")
      expect(source).not.toContain('id="title"')
      expect(source).toMatch(/<path\b/)
      expect(source).not.toContain("transform=")
      expect(source).not.toContain("data:image/webp")
      expect(source).not.toMatch(
        /<text|font-family|@font-face|(?:href\s*=|url\()\s*["']?https?:\/\//,
      )
    }
    const sprite = readFileSync(`${assetRoot}/chess-pieces/sprite.svg`, "utf8")
    expect(sprite).toContain('viewBox="0 0 600 200"')
    expect(sprite).toMatch(/<path\b/)
    expect(sprite).not.toContain("transform=")
    expect(sprite).not.toContain("data:image/webp")
  })

  /* The 陳 surname seal, the marks derived from it, and the wordmark logos are
     reserved by TRADEMARKS.md and are not part of this snapshot. They were
     removed rather than left unreferenced, so this asserts they stay gone
     instead of asserting how they were built. */
  test("no shipped asset carries the reserved seal or wordmark", () => {
    const paths = manifestPaths()

    expect(existsSync(`${assetRoot}/logos`)).toBe(false)
    for (const path of paths) {
      const source = readFileSync(`${assetRoot}/${path}`)
      const text = source.toString("utf8")
      expect(text).not.toContain("data-seal-codepoint")
      expect(text).not.toContain("陳")
      expect(text).not.toContain("陈")
    }
  })

  test("watercolor component masks stay local, vector, and color-agnostic", () => {
    expect(manifest.motionMasks).toMatchObject({
      watercolorControlFrame: "motion/watercolor-control-frame.svg",
      watercolorControlFrameWide: "motion/watercolor-control-frame-wide.svg",
      watercolorOutlineFrame: "motion/watercolor-outline-frame.svg",
      watercolorBrushHorizontal: "motion/watercolor-brush-h.svg",
      watercolorBrushVertical: "motion/watercolor-brush-v.svg",
      watercolorDot: "motion/watercolor-dot.svg",
    })
    expect(manifest.motionMasks).not.toHaveProperty("dryBrushFrame")
    expect(manifest.motionMasks).not.toHaveProperty("dryBrushStroke")

    for (const path of [
      manifest.motionMasks.watercolorControlFrame,
      manifest.motionMasks.watercolorControlFrameWide,
      manifest.motionMasks.watercolorOutlineFrame,
      manifest.motionMasks.watercolorBrushHorizontal,
      manifest.motionMasks.watercolorBrushVertical,
      manifest.motionMasks.watercolorDot,
    ]) {
      const source = readFileSync(`${assetRoot}/${path}`, "utf8")
      expect(source).toContain('preserveAspectRatio="none"')
      expect(source).not.toMatch(
        /<text|font-family|@font-face|(?:href\s*=|url\()\s*["']?https?:\/\//,
      )
    }

    const brushFrameToken = watercolorTokenStyles.slice(
      watercolorTokenStyles.indexOf("--watercolor-brush-frame:"),
      watercolorTokenStyles.indexOf("--watercolor-brush-sizes:"),
    )
    expect(watercolorTokenStyles).toContain(
      '--watercolor-brush-h: url("../assets/brand/motion/watercolor-brush-h.svg")',
    )
    expect(watercolorTokenStyles).toContain(
      '--watercolor-brush-v: url("../assets/brand/motion/watercolor-brush-v.svg")',
    )
    expect(brushFrameToken).toContain("var(--watercolor-brush-h)")
    expect(brushFrameToken).toContain("var(--watercolor-brush-v)")
    expect(brushFrameToken).not.toContain("url(")
    // The StyleX craft paints its frames through the composed token, never by
    // re-declaring an asset URL of its own.
    expect(watercolorCraftSource).toContain("var(--watercolor-brush-frame)")
    expect(watercolorCraftSource).not.toMatch(/url\(["']/)
  })

  test("dry-brush stroke masks stay local, stretchable, and color-agnostic", () => {
    expect(manifest.brushStrokes).toMatchObject({
      bannerSlab: "brush/brush-banner-slab.svg",
      strokeWide: "brush/brush-stroke-wide.svg",
      swoosh: "brush/brush-swoosh.svg",
      inkBlot: "brush/ink-blot.webp",
      inkSquare: "brush/ink-square.webp",
      cloudWash: "brush/cloud-wash.webp",
    })

    for (const path of [
      manifest.brushStrokes.bannerSlab,
      manifest.brushStrokes.strokeWide,
      manifest.brushStrokes.swoosh,
    ]) {
      const source = readFileSync(`${assetRoot}/${path}`, "utf8")
      expect(source).toContain('preserveAspectRatio="none"')
      expect(source).not.toMatch(
        /<text|font-family|@font-face|(?:href\s*=|url\()\s*["']?https?:\/\//,
      )
    }

    for (const token of [
      '--watercolor-brush-slab: url("../assets/brand/brush/brush-banner-slab.svg")',
      '--watercolor-brush-stroke-wide: url("../assets/brand/brush/brush-stroke-wide.svg")',
      '--watercolor-ink-blot: url("../assets/brand/brush/ink-blot.webp")',
    ]) {
      expect(watercolorTokenStyles).toContain(token)
    }

    // The two raster backdrops are opt-in: every url() in the global token
    // sheet is inlined into each single-file Coach App artifact, and no widget
    // renders a dialog or a painted bubble.
    for (const token of [
      '--watercolor-ink-square: url("../assets/brand/brush/ink-square.webp")',
      '--watercolor-cloud-wash: url("../assets/brand/brush/cloud-wash.webp")',
    ]) {
      expect(surfaceTokenStyles).toContain(token)
      expect(watercolorTokenStyles).not.toContain(token)
    }
  })

  /**
   * An asset named by both a CSS custom property and an import in
   * `assets.ts` is inlined twice into each single-file Coach App artifact, and
   * `assertUniqueImageDataUris` fails that build. Catch the collision here,
   * where the fix is one line, instead of three packages downstream.
   */
  test("no brush asset is reachable from both the CSS tokens and the JS manifest", () => {
    const assetSource = readFileSync(
      new URL("./assets.ts", import.meta.url),
      "utf8",
    )
    const tokenFiles = [
      ...`${watercolorTokenStyles}${surfaceTokenStyles}`.matchAll(
        /url\("\.\.\/assets\/brand\/(brush\/[^"]+)"\)/g,
      ),
    ].map(([, path]) => path)

    expect(tokenFiles.length).toBeGreaterThan(0)
    for (const path of tokenFiles) {
      expect(assetSource).not.toContain(`./assets/brand/${path ?? ""}`)
    }
    expect(assetSource).not.toContain("brush/brush-swoosh.svg")
  })
})
