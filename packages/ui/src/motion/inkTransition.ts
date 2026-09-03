import inkSequence from "../assets/brand/motion/ink-sequence.webp?url"

/**
 * The landing's scroll-driven ink transition: a 25-frame ink-spread sprite
 * (frames side by side), shown one frame at a time by stepping mask-position
 * across a mask sized to 2500%. The frame count lives in the stylesheet's
 * `maskSize` and `steps(24)` and in `generate-ink-sequence.sh`; StyleX cannot
 * read a value from here, so exporting one would only be a fourth place to
 * forget. Generated from this repo's cleared
 * brush scans by `assets/brand/brush/generate-ink-sequence.sh`. Imported only
 * from Central Host's landing, which is never part of the single-file Coach
 * App graph the brush README's one-path rule protects.
 */
export const inkTransitionSequence = { url: inkSequence }
