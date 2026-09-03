// @vitest-environment jsdom

import { render, screen } from "@testing-library/react"
import { describe, expect, test } from "vitest"

import { languageLayerPrivacyNotice } from "@chenchess/ui"

import { PrivacyPage } from "./PrivacyPage"

describe("public privacy page Language Layer claim", () => {
  test("carries the same permitted claim as the shared notice", () => {
    const { container } = render(<PrivacyPage />)
    // The notice joins its paragraphs with a space; `textContent` would run
    // them together.
    const text = [...container.querySelectorAll("p")]
      .map((paragraph) => paragraph.textContent)
      .join(" ")

    expect(
      screen.getByRole("heading", { level: 2, name: "Hosted coaching notes" }),
    ).toBeTruthy()
    expect(text).toContain(languageLayerPrivacyNotice)
    expect(text).not.toMatch(/being prepared/i)
    expect(text).not.toMatch(/zero data retention/i)
    expect(text).not.toMatch(/openrouter|vertex|gemini|90 days/i)
  })
})
