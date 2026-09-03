# `@chenchess/ui` brand assets

Production assets for the shared Coach App workspace in issue #154.

## Families

- `app-icons/`: light and dark application icons in SVG, 180 px, and 512 px
  forms. Plain geometry, carrying no seal and no wordmark: the marks
  `TRADEMARKS.md` reserves are not part of this snapshot.
- `board/`: source-faithful dry-brush frame and watercolor square textures
- `chess-pieces/`: twelve self-contained vector SVG pieces with a shared
  `0 0 100 120` view box, plus `source/` WebP crops and a generated
  `sprite.svg`. Re-trace and SVGO-minify with `bun run vectorize:chess-pieces`.
- `icons/`: See, Understand, Improve, and Enjoy coaching-value icons
- `illustrations/`: optional responsive watercolor scenes
- `motion/`: deterministic pigment, wash, fine watercolor-line, and rounded watercolor-control masks
- `manifest.json`: stable inventory and intrinsic metadata for implementation

SVGs contain no remote resources or runtime font dependency. Value-icon
wrappers embed approved-board-derived WebP data so their content stays aligned
with the design source. Chess pieces are committed vector SVGs traced from
those WebP crops; they stay static files at `/brand/chess-pieces/` so the web
app can cache them by stable name. No asset here carries the reserved surname
seal, and `assets.test.ts` holds that line. Consumers own accessible naming and
may mark assets decorative when adjacent text already conveys the same meaning.

The board surfaces are compact crops from the approved workspace application
target: frame `800x774+157+138`, light square `98x94+363+334`, and dark square
`98x94+461+334`. They preserve the target's actual pigment and paper rather than
approximating it with gradients.

`motion/watercolor-control-frame.svg` embeds a hand-painted ink-blot alpha frame
cropped from the CodyHouse "Ink Transition Effect" tutorial sprite
(`ink.png` frame 6, cleaned of neighboring blots), used as a color-agnostic
mask and re-roughened by a deterministic turbulence filter. Only the alpha
silhouette is consumed; all color comes from component CSS. The raster illustrations are optional
atmosphere. Reserve their intrinsic aspect ratio before loading and keep UI text
on tokenized opaque/translucent surfaces rather than directly over high-detail
painted regions.
