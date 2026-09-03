import { readFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

import { describe, expect, it } from "vitest"

import {
  blobControls,
  mulberry32,
  silhouetteFromControls,
  splashRectControls,
  tornRectControls,
  watercolorSilhouettes,
  watercolorSilhouettesCss,
} from "./generate-watercolor-shapes"

const committedSilhouettesCss = readFileSync(
  resolve(
    dirname(fileURLToPath(import.meta.url)),
    "../src/theme/generated/watercolorShapes.css",
  ),
  "utf8",
)

const curveCount = (value: string) => value.split("curve to").length - 1

describe("generate-watercolor-shapes", () => {
  it("matches the committed generated CSS for the shipped seeds", () => {
    expect(watercolorSilhouettesCss()).toBe(committedSilhouettesCss)
    expect(mulberry32(7)()).toBe(0.011704753153026104)
  })

  it("keeps every torn-rect control inside the box", () => {
    const controls = tornRectControls(11, {
      horizontal: 6,
      vertical: 3,
      depthX: 2.2,
      depthY: 3.4,
    })
    expect(controls).toHaveLength(22)
    for (const { x, y } of controls) {
      expect(x).toBeGreaterThanOrEqual(0)
      expect(x).toBeLessThanOrEqual(100)
      expect(y).toBeGreaterThanOrEqual(0)
      expect(y).toBeLessThanOrEqual(100)
    }
  })

  it("keeps every splash control inside the box", () => {
    const controls = splashRectControls(11, {
      horizontal: 12,
      vertical: 5,
      insetX: 2,
      insetY: 5.5,
      depthX: 1.6,
      depthY: 4.5,
    })
    expect(controls).toHaveLength(38)
    for (const { x, y } of controls) {
      expect(x).toBeGreaterThanOrEqual(0)
      expect(x).toBeLessThanOrEqual(100)
      expect(y).toBeGreaterThanOrEqual(0)
      expect(y).toBeLessThanOrEqual(100)
    }
  })

  it("keeps every SHIPPED silhouette inside the box", () => {
    for (const [name, silhouette] of Object.entries(watercolorSilhouettes())) {
      const coordinates = silhouette.match(/-?\d+(?:\.\d+)?%/g) ?? []
      expect(coordinates.length).toBeGreaterThan(0)
      for (const coordinate of coordinates) {
        const value = Number.parseFloat(coordinate)
        expect(value, `${name} has ${coordinate}`).toBeGreaterThanOrEqual(0)
        expect(value, `${name} has ${coordinate}`).toBeLessThanOrEqual(100)
      }
    }
  })

  it("keeps blob controls within their circle", () => {
    for (const { x, y } of blobControls(3, { granularity: 14, depth: 9 })) {
      const distance = Math.hypot(x - 50, y - 50)
      expect(distance).toBeLessThanOrEqual(50)
      expect(distance).toBeGreaterThanOrEqual(41 - 1e-9)
    }
  })

  it("emits one curve per control plus the close", () => {
    const controls = tornRectControls(5, {
      horizontal: 4,
      vertical: 2,
      depthX: 3,
      depthY: 6,
    })
    const silhouette = silhouetteFromControls(controls)
    expect(silhouette.startsWith("shape(from ")).toBe(true)
    expect(silhouette.endsWith(", close)")).toBe(true)
    expect(curveCount(silhouette)).toBe(controls.length)
  })

  it("gives morph pairs an identical command structure", () => {
    const silhouettes = watercolorSilhouettes()
    const structure = (value: string) =>
      `${String(curveCount(value))}:${value.replace(/-?\d+\.\d+%/g, "_")}`
    expect(structure(silhouettes["--watercolor-shape-splash-a"])).toBe(
      structure(silhouettes["--watercolor-shape-splash-b"]),
    )
    expect(structure(silhouettes["--watercolor-shape-splash-calm-a"])).toBe(
      structure(silhouettes["--watercolor-shape-splash-calm-b"]),
    )
    expect(structure(silhouettes["--watercolor-shape-blot-a"])).toBe(
      structure(silhouettes["--watercolor-shape-blot-b"]),
    )
  })
})
