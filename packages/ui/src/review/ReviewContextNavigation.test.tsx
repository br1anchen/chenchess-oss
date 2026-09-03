// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, test, vi } from "vitest"

import { EvaluationGraph, ReviewMomentPicker } from "./ReviewContextNavigation"

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

describe("EvaluationGraph", () => {
  test("uses large semantic moment controls with the measured evaluation", async () => {
    const user = userEvent.setup()
    const onSelect = vi.fn<(ply: number) => void>()
    render(
      <EvaluationGraph
        activePly={12}
        disabled={false}
        maxPly={24}
        moments={[
          {
            glyph: "?!",
            label: "Improvement Opportunity",
            moveLabel: "6. Ne4",
            ply: 12,
            tone: "improvement",
          },
          {
            glyph: "!",
            label: "Positive Highlight",
            moveLabel: "9… d5",
            ply: 18,
            tone: "positive",
          },
        ]}
        onSelect={onSelect}
        points={[
          { label: "+0.40", ply: 12, value: 40 },
          { label: "−1.25", ply: 18, value: -125 },
        ]}
      />,
    )

    const active = screen.getByRole("button", {
      name: /Improvement Opportunity at 6\. Ne4, \+0\.40/,
    })
    const next = screen.getByRole("button", {
      name: /Positive Highlight at 9… d5, −1\.25/,
    })
    expect(active.getAttribute("aria-current")).toBe("step")
    expect(next.getAttribute("aria-current")).toBeNull()
    expect(
      parseFloat(active.style.left) + parseFloat(active.style.width),
    ).toBeLessThanOrEqual(parseFloat(next.style.left))

    next.focus()
    await user.keyboard(" ")
    expect(onSelect).toHaveBeenCalledWith(18)
  })

  test("can omit the visual graph title while preserving the evaluation", () => {
    render(
      <EvaluationGraph
        activePly={12}
        disabled={false}
        maxPly={24}
        moments={[]}
        onSelect={vi.fn()}
        points={[{ label: "+0.40", ply: 12, value: 40 }]}
        title={null}
      />,
    )

    expect(screen.queryByText("Real-game evaluation")).toBeNull()
    expect(
      screen.getByRole("status", {
        name: "Evaluation at the selected moment",
      }),
    ).toBeTruthy()
  })
})

describe("ReviewMomentPicker", () => {
  test("centers the active card without scrolling the containing chat page", () => {
    const scrollTo = vi.fn()
    const scrollIntoView = vi.fn()
    const originalScrollTo = HTMLElement.prototype.scrollTo
    const originalScrollIntoView = HTMLElement.prototype.scrollIntoView
    HTMLElement.prototype.scrollTo = scrollTo
    HTMLElement.prototype.scrollIntoView = scrollIntoView

    try {
      render(
        <ReviewMomentPicker
          activePly={12}
          disabled={false}
          moments={[
            {
              glyph: "?!",
              label: "Improvement Opportunity",
              moveLabel: "6. Ne4",
              ply: 12,
              tone: "improvement",
            },
          ]}
          onSelect={vi.fn()}
        />,
      )

      expect(scrollTo).toHaveBeenCalledWith({
        behavior: "auto",
        left: 0,
      })
      expect(scrollIntoView).not.toHaveBeenCalled()
    } finally {
      HTMLElement.prototype.scrollTo = originalScrollTo
      HTMLElement.prototype.scrollIntoView = originalScrollIntoView
    }
  })

  test("centers the active slide when the controlled selection changes", () => {
    const scrollTo = vi.fn()
    const originalScrollTo = HTMLElement.prototype.scrollTo
    HTMLElement.prototype.scrollTo = scrollTo

    try {
      const moments = [
        {
          glyph: "?!",
          label: "Improvement Opportunity",
          moveLabel: "6. Ne4",
          ply: 12,
          tone: "improvement" as const,
        },
        {
          glyph: "!",
          label: "Positive Highlight",
          moveLabel: "9… d5",
          ply: 18,
          tone: "positive" as const,
        },
      ]
      const { rerender } = render(
        <ReviewMomentPicker
          activePly={12}
          disabled={false}
          moments={moments}
          onSelect={vi.fn()}
        />,
      )
      const slides = screen.getAllByRole("group", { name: /of 2/ })
      Object.defineProperty(slides[1]!, "offsetLeft", {
        configurable: true,
        value: 320,
      })

      rerender(
        <ReviewMomentPicker
          activePly={18}
          disabled={false}
          moments={moments}
          onSelect={vi.fn()}
        />,
      )

      expect(scrollTo).toHaveBeenLastCalledWith({
        behavior: "auto",
        left: 320,
      })
    } finally {
      HTMLElement.prototype.scrollTo = originalScrollTo
    }
  })

  test("presents Critical Moments as an accessible card carousel", async () => {
    const user = userEvent.setup()
    const onSelect = vi.fn<(ply: number) => void>()
    render(
      <ReviewMomentPicker
        activePly={12}
        disabled={false}
        moments={[
          {
            glyph: "?!",
            label: "Improvement Opportunity",
            moveLabel: "6. Ne4",
            ply: 12,
            summary: "The center became vulnerable.",
            tone: "improvement",
          },
          {
            glyph: "!",
            label: "Positive Highlight",
            moveLabel: "9… d5",
            ply: 18,
            summary: "You found the active break.",
            tone: "positive",
          },
        ]}
        onSelect={onSelect}
      />,
    )

    const carousel = screen.getByRole("region", { name: "Review moments" })
    expect(carousel.getAttribute("aria-roledescription")).toBe("carousel")
    expect(
      screen.getByRole("group", { name: "Critical moment cards" }),
    ).toBeTruthy()
    expect(
      screen
        .getByRole("group", { name: "1 of 2" })
        .getAttribute("aria-roledescription"),
    ).toBe("slide")
    expect(
      screen
        .getByRole("button", { name: /6\. Ne4: Improvement Opportunity/ })
        .getAttribute("aria-current"),
    ).toBe("step")
    const heading = screen.getByRole("heading", {
      name: "Critical moments 1/2",
    })
    const picker = document.querySelector(".chen-review-moment-picker")
    expect(heading).toBeTruthy()
    expect(picker?.contains(heading)).toBe(true)
    const count = screen.getByText("1/2")
    expect(count.getAttribute("aria-live")).toBe("polite")
    expect(count.getAttribute("aria-label")).toBe("1/2: 6. Ne4")
    expect(picker?.contains(count)).toBe(true)
    const controls = screen.getByRole("group", {
      name: "Critical moment carousel controls",
    })
    expect(controls.contains(count)).toBe(false)
    expect(
      controls.contains(
        screen.getByRole("button", {
          name: /6\. Ne4: Improvement Opportunity/,
        }),
      ),
    ).toBe(true)
    expect(
      controls.contains(
        screen.getByRole("button", { name: "Next critical moment" }),
      ),
    ).toBe(true)
    expect(
      screen.getByRole("button", { name: "Previous critical moment" }),
    ).toHaveProperty("disabled", true)

    await user.click(
      screen.getByRole("button", { name: "Next critical moment" }),
    )
    expect(onSelect).toHaveBeenCalledWith(18)
  })

  test("a titled picker announces the count through its heading", () => {
    render(
      <ReviewMomentPicker
        activePly={12}
        disabled={false}
        headerExtra={<span>Saragossa Opening · A00</span>}
        moments={[
          {
            glyph: "?!",
            label: "Improvement Opportunity",
            moveLabel: "6. Ne4",
            ply: 12,
            summary: "The center became vulnerable.",
            tone: "improvement",
          },
          {
            glyph: "!",
            label: "Positive Highlight",
            moveLabel: "9… d5",
            ply: 18,
            summary: "You found the active break.",
            tone: "positive",
          },
        ]}
        onSelect={() => undefined}
        title="Critical moments"
      />,
    )

    const heading = screen.getByRole("heading", {
      name: "Critical moments 1/2",
    })
    expect(heading).toBeTruthy()
    expect(
      document.querySelector(".chen-review-moment-picker")?.contains(heading),
    ).toBe(true)
    const count = screen.getByText("1/2")
    expect(count.getAttribute("aria-live")).toBe("polite")
    expect(count.getAttribute("aria-label")).toBe("1/2: 6. Ne4")
    expect(screen.getByText("Saragossa Opening · A00")).toBeTruthy()
  })

  test("selects the card snapped into view after a horizontal swipe", async () => {
    vi.useFakeTimers()
    const onSelect = vi.fn<(ply: number) => void>()
    render(
      <ReviewMomentPicker
        activePly={12}
        disabled={false}
        moments={[
          {
            glyph: "?!",
            label: "Improvement Opportunity",
            moveLabel: "6. Ne4",
            ply: 12,
            tone: "improvement",
          },
          {
            glyph: "!",
            label: "Positive Highlight",
            moveLabel: "9… d5",
            ply: 18,
            tone: "positive",
          },
        ]}
        onSelect={onSelect}
      />,
    )
    const cards = screen.getByRole("group", {
      name: "Critical moment cards",
    })
    const slides = screen.getAllByRole("group", { name: /of 2/ })
    Object.defineProperties(cards, {
      clientWidth: { configurable: true, value: 300 },
      scrollLeft: { configurable: true, value: 300, writable: true },
    })
    Object.defineProperties(slides[0]!, {
      offsetLeft: { configurable: true, value: 0 },
      offsetWidth: { configurable: true, value: 280 },
    })
    Object.defineProperties(slides[1]!, {
      offsetLeft: { configurable: true, value: 300 },
      offsetWidth: { configurable: true, value: 280 },
    })

    fireEvent.scroll(cards)
    await vi.advanceTimersByTimeAsync(150)

    expect(onSelect).toHaveBeenCalledWith(18)
  })

  test("carries slide bodies for the reachable moments only", () => {
    const moments = [12, 18, 24, 30].map((ply, index) => ({
      glyph: "!",
      label: `Moment ${index}`,
      moveLabel: `${ply / 2}. Nf3`,
      ply,
      tone: "positive" as const,
    }))

    const { container } = render(
      <ReviewMomentPicker
        activePly={12}
        disabled={false}
        moments={moments}
        onSelect={vi.fn()}
        renderMoment={(moment, { active }) => (
          <p data-active={active} data-ply={moment.ply}>
            body {moment.ply}
          </p>
        )}
      />,
    )

    // The active moment and its single neighbour can be revealed by a swipe;
    // anything further away must not mount its body.
    const bodies = [...container.querySelectorAll("p[data-ply]")]
    expect(bodies.map((body) => body.getAttribute("data-ply"))).toEqual([
      "12",
      "18",
    ])
    expect(bodies.map((body) => body.getAttribute("data-active"))).toEqual([
      "true",
      "false",
    ])

    const wrappers = [...container.querySelectorAll("[data-slide-body]")]
    expect(wrappers).toHaveLength(4)
    expect(
      wrappers.map((wrapper) => wrapper.getAttribute("aria-hidden")),
    ).toEqual([null, "true", "true", "true"])
  })

  test("stays a bare picker when no slide body is supplied", () => {
    const { container } = render(
      <ReviewMomentPicker
        activePly={12}
        disabled={false}
        moments={[
          {
            glyph: "!",
            label: "Positive Highlight",
            moveLabel: "6. Ne4",
            ply: 12,
            tone: "positive",
          },
        ]}
        onSelect={vi.fn()}
      />,
    )

    expect(container.querySelector("[data-slide-body]")).toBeNull()
    expect(
      container
        .querySelector('[aria-roledescription="carousel"]')
        ?.getAttribute("data-compound"),
    ).toBeNull()
  })
})
