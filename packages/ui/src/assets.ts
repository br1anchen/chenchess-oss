import appIconLight from "./assets/brand/app-icons/app-icon-light.svg?url"
import appIconDark from "./assets/brand/app-icons/app-icon-dark.svg?url"
import boardFrame from "./assets/brand/board/watercolor-frame.webp?url"
import boardLightSquare from "./assets/brand/board/watercolor-square-light.webp?url"
import boardDarkSquare from "./assets/brand/board/watercolor-square-dark.webp?url"
import mountainMist from "./assets/brand/illustrations/mountain-mist.webp?url"
import iconSee from "./assets/brand/icons/see.svg?url"
import iconUnderstand from "./assets/brand/icons/understand.svg?url"
import iconImprove from "./assets/brand/icons/improve.svg?url"
import iconEnjoy from "./assets/brand/icons/enjoy.svg?url"
import brushCircle from "./assets/brand/motion/brush-circle.svg?url"
import watercolorControlFrame from "./assets/brand/motion/watercolor-control-frame.svg?url"
import watercolorControlFrameWide from "./assets/brand/motion/watercolor-control-frame-wide.svg?url"
import watercolorOutlineFrame from "./assets/brand/motion/watercolor-outline-frame.svg?url"
import watercolorBrushHorizontal from "./assets/brand/motion/watercolor-brush-h.svg?url"
import watercolorBrushVertical from "./assets/brand/motion/watercolor-brush-v.svg?url"
import watercolorDot from "./assets/brand/motion/watercolor-dot.svg?url"
import pigmentBloom from "./assets/brand/motion/pigment-bloom.svg?url"
import washReveal from "./assets/brand/motion/wash-reveal.svg?url"
import diffusionExit from "./assets/brand/motion/diffusion-exit.svg?url"

/**
 * Asset URLs come from `?url` imports, not `new URL(…, import.meta.url)`:
 * the latter is plain Node code under SSR and resolves to the filesystem, so
 * Central Host's prerendered pages shipped `file://` references. The import
 * form is resolved by the bundler in every graph — client, SSR, and dev.
 */
export const brandAssets = Object.freeze({
  appIcons: {
    primary: appIconLight,
    light: appIconLight,
    dark: appIconDark,
  },
  boardSurfaces: {
    frame: boardFrame,
    lightSquare: boardLightSquare,
    darkSquare: boardDarkSquare,
  },
  illustrations: {
    mountainMist,
  },
  valueIcons: {
    see: iconSee,
    understand: iconUnderstand,
    improve: iconImprove,
    enjoy: iconEnjoy,
  },
  /**
   * Dry-brush artwork is deliberately absent from this manifest. Every brush
   * asset is reached through a `chenTokens.css` custom property, except the
   * swoosh, which `watercolor.tsx` imports as a module. Naming one here as
   * well inlines its payload a second time into the single-file Coach App
   * artifacts, which `assertUniqueImageDataUris` rejects — see
   * `assets/brand/brush/README.md`.
   */
  motionMasks: {
    brushCircle,
    watercolorControlFrame,
    watercolorControlFrameWide,
    watercolorOutlineFrame,
    watercolorBrushHorizontal,
    watercolorBrushVertical,
    watercolorDot,
    pigmentBloom,
    washReveal,
    diffusionExit,
  },
})

export type PieceColor = "white" | "black"
export type PieceRole = "king" | "queen" | "bishop" | "knight" | "rook" | "pawn"
