# Dry-brush artwork

Real brush scans, cropped to their ink and reduced to what a mask needs. The
procedural `motion/watercolor-*.svg` masks (feTurbulence) stay for the fine
frame strokes; these carry the heavy shapes — plaques, filled block controls,
stamps, and the chat/dialog backdrops.

| File                       | Role                                                                                                                                                                              |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `brush-banner-slab.svg`    | Straight slab, ragged both ends. `WatercolorPlaque`.                                                                                                                              |
| `brush-stroke-wide.svg`    | Tapered stroke. Block buttons and the Next-move control, masked to the stroke's dense body.                                                                                       |
| `brush-swoosh.svg`         | Curved sweep, flat right end by design — meant to bleed off its container. `WatercolorInkStroke`.                                                                                 |
| `ink-blot.webp`            | Soft round blot, alpha only. `WatercolorSymbol silhouette="soft"`.                                                                                                                |
| `ink-square.webp`          | Squarer blot with dry top edge, alpha only. Chat bubble and dialog backdrops.                                                                                                     |
| `cloud-wash.webp`          | Teal/stone cloud painting, full color. Dialog and featured-surface backdrop.                                                                                                      |
| `generate-ink-sequence.sh` | Builds `../motion/ink-sequence.webp`, the landing's 25-frame ink-spread sprite, from `ink-blot.webp` + `ink-square.webp`. Deterministic; rerun by hand, the sprite is checked in. |

## Provenance and pipeline

Free-to-download scans supplied by the project owner, cleared for use here.
The originals are not vendored (they run 2–15 MB). Each SVG here was produced
by:

1. Render the source 1:1 in Chromium, measure the ink bounding box, and set
   that box as the `viewBox` with `preserveAspectRatio="none"` so the mask
   stretches to its host.
2. For the slab and the wide stroke, union the path with a 180°-rotated,
   half-clipped copy of itself, so both ends read as brush ends rather than
   one ragged end and one crop. (The swoosh is left single-sided: rotating a
   curve makes an S with a visible seam.)
3. `svgo --config svgo.config.mjs` — `floatPrecision: 0`. The masks scale
   non-uniformly, so sub-user-unit coordinates in a 1000-unit box are noise:
   dropping them takes the swoosh from 262 KB to 21 KB with no visible change
   at render size.

The rasters are alpha-only (luminance inverted into the alpha channel) so the
tone comes from `background-color`/`currentColor` at the call site, except
`cloud-wash.webp`, which keeps its own pigment.
