import { mkdir, readFile, writeFile } from "node:fs/promises"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

/**
 * The torn silhouettes: `clip-path: shape()` values for the watercolor
 * surfaces, generated the way Temani Afif builds blobs (control points
 * jittered by a depth, midpoints of consecutive controls become the on-curve
 * points, one quadratic `curve` per control — adjacent curves share a tangent,
 * so the outline stays continuous).
 *
 * Cards are rectangles, not circles, so the container families walk the
 * control points along a rectangle's perimeter. The torn family jitters
 * inward only (a ragged slab); the splash family sits on an inset base line
 * and wanders both ways, so the outline bulges into soft lobes like pigment
 * spreading from a drop. Neither ever reaches past the pseudo-element's box,
 * which is what keeps the craft out of `scrollWidth` (the pass-3 layout-gate
 * trap).
 *
 * Seeded PRNG, fixed seeds: the output is deterministic and committed, with a
 * `--check` mode like generate-piece-sprite. Shapes that must morph into each
 * other (`-a`/`-b` pairs) share a point structure, because `shape()` only
 * interpolates between identical command lists.
 */

const packageRoot = fileURLToPath(new URL("..", import.meta.url))
const outputPath = resolve(
  packageRoot,
  "src/theme/generated/watercolorShapes.css",
)

type Point = { x: number; y: number }

/** Deterministic 32-bit PRNG (mulberry32). */
export function mulberry32(seed: number) {
  let state = seed >>> 0
  return () => {
    state = (state + 0x6d2b79f5) >>> 0
    let t = state
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

type TornRectOptions = {
  /** Control points per horizontal edge. */
  horizontal: number
  /** Control points per vertical edge. */
  vertical: number
  /** How deep a bite can reach into the box, as a percentage of its width. */
  depthX: number
  /** Bite depth as a percentage of the box height. */
  depthY: number
}

/** Interior edge points stay this far (%) from the corners, so the corner
 * controls keep the rounding tight — without them the midpoint construction
 * bridges half an edge-step and a wide card grows enormous round corners. */
const cornerPad = 5

/**
 * Control points around a rectangle's perimeter, clockwise from the top-left,
 * each pulled inward by a random fraction of the depth. A dedicated, lightly
 * jittered control pins each corner; the interior points carry the bites.
 */
export function tornRectControls(
  seed: number,
  { horizontal, vertical, depthX, depthY }: TornRectOptions,
): Point[] {
  const random = mulberry32(seed)
  const points: Point[] = []
  const spread = (index: number, count: number) =>
    cornerPad + (index / (count - 1)) * (100 - 2 * cornerPad)
  const corner = (right: boolean, bottom: boolean) => ({
    x: right ? 100 - random() * depthX * 0.6 : random() * depthX * 0.6,
    y: bottom ? 100 - random() * depthY * 0.6 : random() * depthY * 0.6,
  })
  points.push(corner(false, false))
  for (let i = 0; i < horizontal; i += 1) {
    points.push({ x: spread(i, horizontal), y: random() * depthY })
  }
  points.push(corner(true, false))
  for (let i = 0; i < vertical; i += 1) {
    points.push({ x: 100 - random() * depthX, y: spread(i, vertical) })
  }
  points.push(corner(true, true))
  for (let i = 0; i < horizontal; i += 1) {
    points.push({
      x: spread(horizontal - 1 - i, horizontal),
      y: 100 - random() * depthY,
    })
  }
  points.push(corner(false, true))
  for (let i = 0; i < vertical; i += 1) {
    points.push({ x: random() * depthX, y: spread(vertical - 1 - i, vertical) })
  }
  return points
}

type SplashRectOptions = {
  /** Control points per horizontal edge. */
  horizontal: number
  /** Control points per vertical edge. */
  vertical: number
  /** How far (%) the base line sits inside the left/right box edges. */
  insetX: number
  /** Base-line inset from the top/bottom edges, as a % of the box height. */
  insetY: number
  /** How far a lobe may wander to either side of the base line, % of width. */
  depthX: number
  /** Lobe amplitude on the horizontal edges, % of the box height. */
  depthY: number
}

/**
 * Control points for a splash: the perimeter of a rectangle inset from the
 * box, with every point wandering around that base line at two scales. Most
 * points tremble close to the line; some throw fingers OUTWARD toward the box
 * edge and others pinch inward — water spreading from a drop, puddling where
 * the paper lets it. Point spacing itself is jittered (under half a step, so
 * the outline never crosses itself), because evenly-spaced lobes read as a
 * sine wave, not a splash.
 */
export function splashRectControls(
  seed: number,
  { horizontal, vertical, insetX, insetY, depthX, depthY }: SplashRectOptions,
): Point[] {
  const random = mulberry32(seed)
  const points: Point[] = []
  /* The jitter is over half a step on a sparse edge (vertical: 3 steps 45%),
     so an end point can wander past the box — clamp it back inside. Outside
     0–100% a clip-path percentage paints past the pseudo box, which is the
     scrollWidth trap the sheet's vertical-only bleed exists to avoid. */
  const spread = (index: number, count: number) => {
    const step = (100 - 2 * cornerPad) / (count - 1)
    const jitter = (random() - 0.5) * step * 0.55
    return Math.min(99, Math.max(1, cornerPad + index * step + jitter))
  }
  /* Positive = inward (deeper into the box), negative = outward. Fingers run
     out to nearly the box edge; the inset is what bounds them. */
  const wander = (depth: number, inset: number) => {
    const roll = random()
    if (roll < 0.3) return -inset * (0.55 + 0.43 * random())
    if (roll < 0.62) return depth * (0.4 + 0.6 * random())
    return (random() * 2 - 1) * depth * 0.35
  }
  const corner = (right: boolean, bottom: boolean) => {
    const dx = (random() * 2 - 1) * depthX * 0.7
    const dy = (random() * 2 - 1) * depthY * 0.7
    return {
      x: right ? 100 - insetX - dx : insetX + dx,
      y: bottom ? 100 - insetY - dy : insetY + dy,
    }
  }
  points.push(corner(false, false))
  for (let i = 0; i < horizontal; i += 1) {
    points.push({
      x: spread(i, horizontal),
      y: insetY + wander(depthY, insetY),
    })
  }
  points.push(corner(true, false))
  for (let i = 0; i < vertical; i += 1) {
    points.push({
      x: 100 - insetX - wander(depthX, insetX),
      y: spread(i, vertical),
    })
  }
  points.push(corner(true, true))
  for (let i = 0; i < horizontal; i += 1) {
    points.push({
      x: spread(horizontal - 1 - i, horizontal),
      y: 100 - insetY - wander(depthY, insetY),
    })
  }
  points.push(corner(false, true))
  for (let i = 0; i < vertical; i += 1) {
    points.push({
      x: insetX + wander(depthX, insetX),
      y: spread(vertical - 1 - i, vertical),
    })
  }
  return points
}

type BlobOptions = {
  /** Number of control points around the circle (the article's granularity). */
  granularity: number
  /** How far inward a point may wander, as a percentage of the box. */
  depth: number
}

/** Control points around a circle, each pulled toward the centre — the
 * article's blob, for round stamps. */
export function blobControls(
  seed: number,
  { granularity, depth }: BlobOptions,
): Point[] {
  const random = mulberry32(seed)
  const points: Point[] = []
  for (let i = 0; i < granularity; i += 1) {
    const radius = 50 - random() * depth
    const angle = (2 * Math.PI * i) / granularity
    points.push({
      x: 50 + radius * Math.cos(angle),
      y: 50 + radius * Math.sin(angle),
    })
  }
  return points
}

const coordinate = (value: number) =>
  `${(Math.round(value * 100) / 100).toFixed(2)}%`

/**
 * The controls become a `shape()`: on-curve points are the midpoints of
 * consecutive controls, and each segment curves with the control it wraps.
 */
export function silhouetteFromControls(controls: Point[]): string {
  const midpoints = controls.map((point, index) => {
    // SAFETY: the modulo keeps the neighbour index inside the array.
    const next = controls[(index + 1) % controls.length] as Point
    return { x: (point.x + next.x) / 2, y: (point.y + next.y) / 2 }
  })
  const commands = controls.map((_, index) => {
    const wrapped = (index + 1) % controls.length
    // SAFETY: midpoints mirrors controls one-to-one and the index is wrapped.
    const to = midpoints[wrapped] as Point
    // SAFETY: the wrapped index stays inside the array.
    const control = controls[wrapped] as Point
    return `curve to ${coordinate(to.x)} ${coordinate(to.y)} with ${coordinate(control.x)} ${coordinate(control.y)}`
  })
  const [first] = midpoints
  if (!first) throw new Error("a silhouette needs control points")
  return `shape(from ${coordinate(first.x)} ${coordinate(first.y)}, ${commands.join(", ")}, close)`
}

/** Every silhouette the theme ships, by token name. The `-a`/`-b` pairs share
 * a structure so `shape()` can interpolate between them. */
export function watercolorSilhouettes() {
  const splash: SplashRectOptions = {
    horizontal: 12,
    vertical: 5,
    insetX: 2,
    insetY: 5.5,
    depthX: 1.6,
    depthY: 4.5,
  }
  return {
    "--watercolor-shape-splash-a": silhouetteFromControls(
      splashRectControls(11, splash),
    ),
    "--watercolor-shape-splash-b": silhouetteFromControls(
      splashRectControls(47, splash),
    ),
    /* The calm pair: fewer, gentler waves for SMALL containers — a chat
       bubble or a compact card compresses the full lobe count into a busy
       edge, so the small surfaces get fewer points and shallower wander. */
    "--watercolor-shape-splash-calm-a": silhouetteFromControls(
      splashRectControls(61, {
        horizontal: 6,
        vertical: 3,
        insetX: 1.5,
        insetY: 3.8,
        depthX: 1.1,
        depthY: 2.6,
      }),
    ),
    "--watercolor-shape-splash-calm-b": silhouetteFromControls(
      splashRectControls(83, {
        horizontal: 6,
        vertical: 3,
        insetX: 1.5,
        insetY: 3.8,
        depthX: 1.1,
        depthY: 2.6,
      }),
    ),
    "--watercolor-shape-splash-heavy": silhouetteFromControls(
      splashRectControls(23, {
        horizontal: 13,
        vertical: 5,
        insetX: 2.3,
        insetY: 6,
        depthX: 1.9,
        depthY: 5,
      }),
    ),
    "--watercolor-shape-panel": silhouetteFromControls(
      splashRectControls(5, {
        horizontal: 14,
        vertical: 6,
        insetX: 2.6,
        insetY: 6.5,
        depthX: 2.2,
        depthY: 5.5,
      }),
    ),
    "--watercolor-shape-tooltip": silhouetteFromControls(
      tornRectControls(17, {
        horizontal: 4,
        vertical: 2,
        depthX: 4,
        depthY: 9,
      }),
    ),
    "--watercolor-shape-blot-a": silhouetteFromControls(
      blobControls(3, { granularity: 14, depth: 9 }),
    ),
    "--watercolor-shape-blot-b": silhouetteFromControls(
      blobControls(29, { granularity: 14, depth: 9 }),
    ),
  }
}

export function watercolorSilhouettesCss(): string {
  const lines = Object.entries(watercolorSilhouettes()).map(
    ([name, value]) => `    ${name}: ${value};`,
  )
  return `/**
 * Generated by scripts/generate-watercolor-shapes.ts — do not edit.
 * Regenerate with \`bun run generate:watercolor-shapes\` from packages/ui.
 */
@layer chen-tokens {
  :root {
${lines.join("\n")}
  }
}
`
}

if (import.meta.main) {
  const css = watercolorSilhouettesCss()
  if (process.argv.includes("--check")) {
    const current = await readFile(outputPath, "utf8").catch(() => "")
    if (current !== css) {
      throw new Error(
        "Watercolor shapes are stale; run bun run generate:watercolor-shapes from packages/ui",
      )
    }
    process.stdout.write("verified generated watercolor shapes\n")
  } else {
    await mkdir(dirname(outputPath), { recursive: true })
    await writeFile(outputPath, css)
    process.stdout.write(`generated ${outputPath}\n`)
  }
}
