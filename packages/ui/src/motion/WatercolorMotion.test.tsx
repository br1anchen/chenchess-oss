// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import {
  DiffusionExit,
  DryBrushCircle,
  WatercolorWashPanel,
  PigmentBloom,
} from "./WatercolorMotion"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

test("reduced motion removes masks, blur, and decorative delay", () => {
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockImplementation(() => ({
      matches: true,
      addEventListener: vi.fn<() => void>(),
      removeEventListener: vi.fn<() => void>(),
    })),
  )

  render(
    <>
      <WatercolorWashPanel motionKey="reduced">
        <p>Immediate coaching content</p>
      </WatercolorWashPanel>
      <PigmentBloom active />
      <DryBrushCircle />
      <DiffusionExit visible>
        <p>Immediate exit content</p>
      </DiffusionExit>
    </>,
  )

  expect(screen.getByText("Immediate coaching content")).toBeTruthy()
  expect(screen.getByText("Immediate exit content")).toBeTruthy()
  expect(document.querySelector(".chen-dry-brush-circle")).toBeNull()
  for (const element of document.querySelectorAll("[data-reduced-motion]")) {
    expect(element.getAttribute("data-reduced-motion")).toBe("true")
    expect(element.getAttribute("style") ?? "").not.toContain("mask-image")
    expect(element.getAttribute("style") ?? "").not.toContain("blur")
  }
})
