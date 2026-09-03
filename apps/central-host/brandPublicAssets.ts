import brandManifest from "../../packages/ui/src/assets/brand/manifest.json"

/**
 * The brand files Central Host serves at stable `/brand/` paths, named from
 * `packages/ui`'s asset manifest rather than hand-copied. The document `<head>`
 * and the server-rendered OAuth pages both sit outside the hashed asset graph,
 * so they need names that do not move between builds.
 *
 * The manifest is imported, not read: this module is bundled into the page
 * graph, where `import.meta.url` no longer names its own source directory.
 */
function publicBrandIcon(relativePath: string) {
  return `/brand/${relativePath.slice(relativePath.lastIndexOf("/") + 1)}`
}

export const publicBrandIcons = {
  darkSvg: publicBrandIcon(brandManifest.appIcons.darkSvg),
  light180: publicBrandIcon(brandManifest.appIcons.light180),
  lightSvg: publicBrandIcon(brandManifest.appIcons.lightSvg),
}

/**
 * Paths relative to `packages/ui/src/assets/brand/`, every one of them named by
 * the manifest. The app icons carry the favicons and web manifest; the rest are
 * what the OAuth interaction pages paint, and those are plain HTML outside the
 * Vite graph, so they cannot reference hashed build assets.
 */
export const brandServedRelativePaths = [
  ...Object.values(brandManifest.appIcons),
  brandManifest.illustrations.mountainMist.path,
  brandManifest.motionMasks.watercolorBrushHorizontal,
  brandManifest.motionMasks.watercolorBrushVertical,
  brandManifest.motionMasks.watercolorControlFrame,
  brandManifest.motionMasks.watercolorControlFrameWide,
]

const brandAssetContentTypes = new Map([
  [".png", "image/png"],
  [".svg", "image/svg+xml"],
  [".webp", "image/webp"],
])

export function brandAssetContentType(extension: string) {
  const contentType = brandAssetContentTypes.get(extension)
  if (!contentType) {
    // Serving a brand asset as octet-stream breaks the OAuth pages quietly;
    // a manifest that grows a new extension should stop the build instead.
    throw new Error(`No brand asset content type for ${extension}`)
  }
  return contentType
}
