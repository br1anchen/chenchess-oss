// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import { LearningPathCards } from "./LearningPathCards"
import { LEARNING_PLAN_FEEDBACK_UNCONFIRMED } from "./useAcknowledgedLearningPathFeedback"

afterEach(cleanup)

test("makes each learning stage link to its individual material", () => {
  const onVote = vi.fn()
  render(
    <LearningPathCards
      onVote={onVote}
      paths={[
        {
          cluster: "Lichess Curriculum",
          conceptLessons: [
            {
              canonicalUrl: "https://lichess.org/practice/fork",
              resourceId: "lesson:fork",
              role: "learn",
              title: "Fork",
            },
          ],
          idea: "Fork",
          id: "curriculum:fork",
          learningPathRef: "learning-path:fork",
          patternDrills: [],
          purpose: "missing",
        },
      ]}
    />,
  )

  const cards = screen.getAllByRole("article")
  const card = cards[0]
  expect(cards.length).toBeGreaterThan(1)
  expect(card?.getAttribute("data-watercolor-surface")).toBe("card")
  expect(card?.getAttribute("data-watercolor-frame")).toBeNull()
  expect(cards.at(-1)?.classList.contains("chen-learning-stage")).toBe(true)
  expect(screen.getByText("Concept lesson")).toBeTruthy()
  expect(screen.queryByText("Pattern drilling")).toBeNull()
  expect(screen.queryByText(/No exact themed puzzle/i)).toBeNull()
  expect(
    screen.getByRole("heading", { name: /Missing idea.*Fork/ }),
  ).toBeTruthy()
  expect(screen.queryByText("Lichess Curriculum")).toBeNull()
  const heading = screen.getByRole("heading", {
    name: /Missing idea.*Fork/,
  })
  const relevance = screen.getByRole("group", {
    name: "Learning path relevance",
  })
  expect(heading.closest("header")?.contains(relevance)).toBe(true)
  expect(screen.getByText("Relevant?")).toBeTruthy()
  expect(screen.getByRole("button", { name: "Relevant" })).toBeTruthy()
  const conceptLesson = screen.getByRole("link", {
    name: "Concept lesson: Fork",
  })
  expect(conceptLesson.getAttribute("href")).toBe(
    "https://lichess.org/practice/fork",
  )
  expect(card.getAttribute("role")).toBeNull()
})

test("resolves a Player-initiated vote into in-flight, recorded, and failed states", () => {
  const path = {
    cluster: "Lichess Curriculum" as const,
    conceptLessons: [],
    idea: "Fork",
    id: "curriculum:fork",
    learningPathRef: "learning-path:fork",
    patternDrills: [],
    purpose: "missing" as const,
  }

  const { rerender } = render(
    <LearningPathCards onVote={vi.fn()} paths={[path]} pending={() => true} />,
  )
  expect(screen.getByRole("status").textContent).toBe("Saving…")

  rerender(
    <LearningPathCards
      currentVote={() => "thumbsUp"}
      onVote={vi.fn()}
      paths={[path]}
    />,
  )
  expect(screen.getByRole("status").textContent).toBe("Recorded")
  expect(
    screen
      .getByRole("button", { name: "Relevant" })
      .getAttribute("aria-pressed"),
  ).toBe("true")

  rerender(
    <LearningPathCards
      failure={() => LEARNING_PLAN_FEEDBACK_UNCONFIRMED}
      onVote={vi.fn()}
      paths={[path]}
    />,
  )
  expect(screen.getByRole("alert").textContent).toBe(
    LEARNING_PLAN_FEEDBACK_UNCONFIRMED,
  )
  expect(screen.getByText("Relevant?")).toBeTruthy()
  expect(screen.queryByRole("status")).toBeNull()

  rerender(
    <LearningPathCards
      currentVote={() => "thumbsUp"}
      failure={() => LEARNING_PLAN_FEEDBACK_UNCONFIRMED}
      onVote={vi.fn()}
      paths={[path]}
    />,
  )
  expect(screen.getByRole("status").textContent).toBe("Recorded")
  expect(screen.getByRole("alert").textContent).toBe(
    LEARNING_PLAN_FEEDBACK_UNCONFIRMED,
  )
  expect(screen.queryByText("Relevant?")).toBeNull()
  expect(
    screen
      .getByRole("button", { name: "Relevant" })
      .getAttribute("aria-pressed"),
  ).toBe("true")
})

test("places an unconfirmed-vote alert on its own row so the title is not forced to one word per line", () => {
  render(
    <LearningPathCards
      failure={() => LEARNING_PLAN_FEEDBACK_UNCONFIRMED}
      onVote={vi.fn()}
      paths={[
        {
          cluster: "Opening Tactical Awareness",
          conceptLessons: [],
          idea: "Opening Tactical Awareness",
          id: "opening:awareness",
          learningPathRef: "learning-path:awareness",
          patternDrills: [],
          purpose: "missing",
        },
      ]}
    />,
  )

  const heading = screen.getByRole("heading", {
    name: /Missing idea.*Opening Tactical Awareness/,
  })
  const alert = screen.getByRole("alert")
  const feedback = heading.nextElementSibling
  expect(alert.textContent).toBe(LEARNING_PLAN_FEEDBACK_UNCONFIRMED)
  expect(feedback?.classList.contains("chen-learning-feedback")).toBe(true)
  expect(feedback?.contains(alert)).toBe(false)
  expect(feedback?.querySelector("p")?.textContent).toBe("Relevant?")
  expect(heading.closest("header")?.contains(alert)).toBe(true)
  expect(alert.classList.contains("chen-learning-feedback-alert")).toBe(true)
})

test("does not announce Saving… for an automatic exposure write", () => {
  render(
    <LearningPathCards
      disabled={() => true}
      onVote={vi.fn()}
      paths={[
        {
          cluster: "Lichess Curriculum",
          conceptLessons: [],
          idea: "Fork",
          id: "curriculum:fork",
          learningPathRef: "learning-path:fork",
          patternDrills: [],
          purpose: "missing",
        },
      ]}
    />,
  )

  expect(screen.getByText("Relevant?")).toBeTruthy()
  expect(screen.queryByRole("status")).toBeNull()
  expect(screen.getByRole("button", { name: "Relevant" })).toHaveProperty(
    "disabled",
    true,
  )
})

test("drops the ink frame only when a digest surface asks", () => {
  const path = {
    cluster: "Lichess Curriculum" as const,
    conceptLessons: [],
    idea: "Fork",
    id: "curriculum:fork",
    learningPathRef: "learning-path:fork",
    patternDrills: [],
    purpose: "missing" as const,
  }
  const { rerender } = render(<LearningPathCards paths={[path]} />)
  expect(
    screen.getAllByRole("article")[0]?.getAttribute("data-watercolor-frame"),
  ).toBeNull()

  rerender(<LearningPathCards frame={false} paths={[path]} />)
  expect(
    screen.getAllByRole("article")[0]?.getAttribute("data-watercolor-frame"),
  ).toBe("none")
})
