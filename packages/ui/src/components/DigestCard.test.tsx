// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, test, vi } from "vitest"

import { DigestCard } from "./DigestCard"

afterEach(cleanup)

test("renders the morning-digest anatomy on a watercolor card", () => {
  render(
    <DigestCard
      appearance="featured"
      eyebrow="From your archive"
      gameCount={4}
      ideas={[
        {
          purpose: "improvement",
          resources: [
            {
              href: "https://lichess.org/practice/discovered-attacks",
              label: "Discovered Attacks",
              role: "learn",
            },
            {
              href: "https://lichess.org/training/discoveredAttack",
              label: "Discovered-attack puzzles",
              role: "drill",
            },
          ],
          title: "Discovered Attacks",
        },
        {
          purpose: "reinforcement",
          resources: [
            {
              href: "https://lichess.org/training/xRayAttack",
              label: "X-Ray puzzles",
              role: "drill",
            },
          ],
          title: "X-Ray",
        },
        {
          purpose: "improvement",
          title: "A third idea must not appear",
        },
      ]}
      source="Published Aug 16, 2026, 5:15 AM"
      title="Saturday, August 15, 2026"
    >
      <p>4 Games in this digest · 26 grounded learning paths</p>
    </DigestCard>,
  )

  const card = document.querySelector('[data-watercolor-surface="digest"]')
  if (!card) throw new Error("Expected a digest watercolor card")
  expect(card.getAttribute("data-watercolor-surface")).toBe("digest")
  expect(card.getAttribute("data-watercolor-composition")).toBe("content")
  expect(screen.getByText("From your archive")).toBeTruthy()
  expect(
    screen.getByRole("heading", {
      level: 1,
      name: "Saturday, August 15, 2026",
    }),
  ).toBeTruthy()
  expect(screen.getByText("4 games")).toBeTruthy()
  expect(
    screen.getByRole("heading", { name: "Today’s priorities" }),
  ).toBeTruthy()
  expect(
    screen.getByRole("heading", { name: /Missing idea.*Discovered Attacks/ }),
  ).toBeTruthy()
  expect(
    screen.getByRole("heading", { name: /Idea reinforced.*X-Ray/ }),
  ).toBeTruthy()
  expect(
    document.querySelectorAll(".chen-watercolor-card-bamboo").length,
  ).toBeGreaterThan(0)
  expect(
    document.querySelectorAll("[data-watercolor-splash]").length,
  ).toBeGreaterThan(0)
  expect(screen.queryByText("A third idea must not appear")).toBeNull()
  expect(
    screen
      .getByRole("link", { name: "Concept lesson: Discovered Attacks" })
      .getAttribute("href"),
  ).toBe("https://lichess.org/practice/discovered-attacks")
  expect(
    screen
      .getByRole("link", {
        name: "Pattern drilling: Discovered-attack puzzles",
      })
      .getAttribute("href"),
  ).toBe("https://lichess.org/training/discoveredAttack")
  expect(
    screen
      .getByRole("link", { name: "Pattern drilling: X-Ray puzzles" })
      .getAttribute("href"),
  ).toBe("https://lichess.org/training/xRayAttack")
  expect(screen.getByText("Published Aug 16, 2026, 5:15 AM")).toBeTruthy()
  expect(
    screen.getByText("4 Games in this digest · 26 grounded learning paths"),
  ).toBeTruthy()
  expect(screen.queryByRole("button")).toBeNull()
})

test("omits the eyebrow when the coverage date is already the title", () => {
  render(
    <DigestCard
      appearance="featured"
      gameCount={1}
      title="Sunday, 09/08/2026"
    />,
  )

  expect(
    screen.getByRole("heading", { level: 1, name: "Sunday, 09/08/2026" }),
  ).toBeTruthy()
  expect(document.querySelector(".chen-watercolor-eyebrow")).toBeNull()
})

test("keeps an archive card as the #digest= target and writes the hash", async () => {
  const onSelect = vi.fn()
  const user = userEvent.setup()
  render(
    <DigestCard
      appearance="list"
      eyebrow="Sunday, August 9, 2026"
      href="#digest=daily-2026-08-09"
      onSelect={onSelect}
      selected
      title="1 Game · 1 path"
    />,
  )

  expect(screen.getByRole("article").getAttribute("aria-current")).toBe("true")
  const hit = screen.getByRole("link", { name: /Sunday, August 9, 2026/ })
  expect(hit.getAttribute("href")).toBe("#digest=daily-2026-08-09")
  await user.click(hit)
  expect(onSelect).toHaveBeenCalledOnce()
  expect(window.location.hash).toBe("#digest=daily-2026-08-09")
})
