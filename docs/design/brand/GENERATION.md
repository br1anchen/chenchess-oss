# Brand asset generation record

## Approved-board derivatives

The logo, app icons, value icons, and chess pieces are derived directly from
`chenchess-brand-system-reference.jpg` and
`chenchess-workspace-application-target.jpg`. The deterministic regeneration
script crops the approved watercolor content, removes the paper background from
pieces and value icons, stores identity and value-icon results as local WebP
data inside an SVG wrapper, writes chess-piece crops as WebP sources, and
regenerates the 180 px and 512 px app-icon exports. Transparent value-icon
crops use lossless WebP; opaque identity crops use high-quality WebP to keep
the application bundle compact. Chess-piece WebP sources are vectorized once
by `@chenchess/ui` with VTracer into scalable SVGs, then SVGO minifies path
commands without dropping stacked layers. `verify:piece-sprite` checks
`chess-pieces/vectorize.lock.json` against the source WebP hashes and the
committed VTracer/SVGO parameters so a stale trace cannot pass. The trace stays at the native
100×120 crop (`color_precision` 6, `gradient_step` 16, `filter_speckle` 3,
`path_precision` 2) so stacked watercolor washes keep their pigment blooms
without the halo cracks that 2×/3× upscales add.

```sh
node docs/design/brand/regenerate-assets.mjs
bun run --cwd packages/ui vectorize:chess-pieces
```

The script requires ImageMagick's `magick` command. Set
`CHEN_CHESS_MAGICK=/path/to/magick` only when the executable is not on `PATH`.
The motion masks remain code-authored SVG because they carry behavior rather
than approved image content. The layered knight mark is a painted-WebP hybrid
cropped from `logos/mark.svg` (420×420 brand-board mark, the highest-res knight
on that board). `knight-mark-knight.webp` is the knight only; `knight-mark-circle.webp`
is the still brush ring. Compose wraps those rasters:

```sh
bun run --cwd packages/ui compose:knight-mark
```

## Generated illustrations

Mode: built-in image generation tool.

The approved boards were supplied as project visual and content references.
Both production outputs are original, text-free raster illustrations.

## `review-companions.webp`

```text
Use case: precise-object-edit
Asset type: responsive Coach App workspace illustration
Input images: Image 1 is the previous production illustration and edit target. Image 2 is the approved brand-system reference. Image 3 is the approved workspace-application target.
Primary request: correct Image 1 so its subjects and scene content match the approved design: a young male chess learner on the left and a young female chess learner on the right, calmly studying a chess position together as equals at a low wooden table.
Subject correction: change only the right-hand adult man into the young woman shown by the approved design direction, with tied-back dark hair, an attentive contemplative expression, and muted warm stone-and-terracotta clothing. Refine the left learner toward the approved young male figure without changing his pose or placement.
Scene invariants: keep the existing wide composition, chessboard, table, mountain mist, bamboo, generous blank space on the left, camera angle, and quiet scale unchanged.
Style/medium: faithfully preserve the original Chinese ink wash and transparent watercolor on absorbent warm-ivory rice paper, dry-brush navy contours, soft mist-blue and stone washes, restrained muted bamboo and terracotta accents. Match the approved boards' elegant hand-painted irregularity; no glossy digital rendering.
Composition/framing: very wide landscape, both figures and the board grouped entirely in the right half, broad quiet negative space on the left for UI, no cropping of heads, hands, board, or table.
Lighting/mood: contemplative, encouraging, quiet coaching; diffuse paper-white light.
Constraints: preserve the chess position and all scene geometry except the specified subject corrections. Original artwork informed by the supplied project references. No text, letters, Chinese characters, surname seal, logo, UI, watermark, extra people, extra limbs, or extra chessboards.
Avoid: two male subjects, adult-and-child coaching hierarchy, anime, chibi, photorealism, glossy painting, saturated color, ornate fantasy detail, decorative border.
```

## `mountain-mist.webp`

```text
Use case: stylized-concept
Asset type: subtle responsive application background illustration
Input images: Image 1 is a visual-language reference only; do not edit or reproduce its layout, logo, text, people, chess pieces, or exact brush shapes.
Primary request: create an original panoramic Chinese ink-wash landscape designed as a quiet background layer for a chess coaching workspace.
Scene/backdrop: warm ivory absorbent rice paper, three layers of mist-softened mountains receding into the distance, a small restrained bamboo cluster at the far right, two tiny abstract birds high in the distance.
Subject: landscape atmosphere only; no people, chessboard, chess pieces, buildings, banners, weapons, or narrative characters.
Style/medium: original monochrome ink wash and transparent watercolor, dry-brush ridge edges, soft pigment blooms, generous untouched paper, elegant hand-painted irregularity.
Composition/framing: very wide landscape; central and left regions extremely quiet and low contrast for UI overlay; visual weight confined mostly to the bottom edge and far right; seamless-feeling open edges suitable for responsive cropping.
Lighting/mood: calm, contemplative, encouraging, dawn mist.
Color palette: very pale mist blue, slate blue, diluted ink navy, stone gray, muted bamboo sage, warm ivory; no vermilion.
Constraints: original work; no text, no letters, no Chinese characters, no seal, no logo, no watermark, no UI, no decorative frame, no copied third-party map, faction imagery, or trade dress.
Avoid: battle scene, historical generals, ornate fantasy, high contrast, photorealism, anime, crayon, colored pencil, saturated color, crisp vector geometry.
```
