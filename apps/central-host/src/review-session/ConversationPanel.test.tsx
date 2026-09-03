// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ReactElement } from "react"
import { afterEach, expect, test, vi } from "vitest"

import {
  fromLearningPathRef,
  fromLearningResourceId,
} from "@chenchess/coach-engine-sdk"
import { brandAssets } from "@chenchess/ui/assets"

import { ChenTheme } from "@chenchess/ui/theme"

import { ConversationPanel } from "./ConversationPanel"
import { hostTurnRefusalText, hostTurnStepLabels } from "./thread-state"
import { INTERACTIVE_COACHING_UNAVAILABLE } from "./useReviewSessionCommands"

function renderPanel(ui: ReactElement) {
  return render(ui, { wrapper: ChenTheme })
}

afterEach(cleanup)

test("uses the shared watercolor card and app icon for Coach messages", () => {
  const { container } = renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled
      learningPaths={[]}
      messages={[
        {
          kind: "coachAnswer",
          id: "coach-follow-up",
          answer: "Look again.",
          effects: {},
        },
      ]}
      onMessage={vi.fn()}
      openingText="Start with the forcing move."
    />,
  )

  expect(screen.getAllByText("Coach")).toHaveLength(2)
  expect(container.querySelector("img")?.getAttribute("src")).toBe(
    brandAssets.appIcons.primary,
  )
})

test("keeps plan discussion conversational without lifecycle controls", async () => {
  const user = userEvent.setup()
  const onMessage = vi.fn()
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      openingText="My best guess is that e4 may have aimed to occupy the center."
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={onMessage}
      failure={null}
    />,
  )

  expect(
    screen.queryByRole("button", { name: "Yes, that was my plan" }),
  ).toBeNull()
  expect(screen.queryByRole("button", { name: "No" })).toBeNull()
  expect(screen.queryByRole("button", { name: "Skip" })).toBeNull()
  expect(
    screen.queryByRole("button", { name: "Answer clarification" }),
  ).toBeNull()

  await user.type(
    screen.getByLabelText("Message the coach"),
    "I wanted to occupy the center.{Enter}",
  )
  expect(onMessage).toHaveBeenCalledWith("I wanted to occupy the center.")
})

test("HostTurn composer stays unscoped and exposes cancellation while writing", async () => {
  const user = userEvent.setup()
  const onMessage = vi.fn()
  const onCancel = vi.fn()
  renderPanel(
    <ConversationPanel
      busyLabel="Coach is reviewing this branch…"
      openingText="Consider the branch."
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onCancel={onCancel}
      onMessage={onMessage}
      failure={null}
    />,
  )

  await user.type(
    screen.getByLabelText("Message the coach"),
    "Compare this with the played move.{Enter}",
  )
  expect(screen.queryByText(/Coach target/)).toBeNull()
  expect(screen.queryByText(/d2d4/)).toBeNull()
  expect(onMessage).toHaveBeenCalledWith("Compare this with the played move.")
  await user.click(screen.getByRole("button", { name: "Cancel" }))
  expect(onCancel).toHaveBeenCalledOnce()
})

test("disables the composer only for an active operation", () => {
  renderPanel(
    <ConversationPanel
      busyLabel="Refreshing grounded evidence…"
      openingText="Reviewing e4."
      inputDisabled
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      failure={null}
    />,
  )

  expect(screen.getByLabelText("Message the coach")).toHaveProperty(
    "disabled",
    true,
  )
})

test("refuses Send while the composer is disabled", async () => {
  const onMessage = vi.fn()
  renderPanel(
    <ConversationPanel
      busyLabel="Evaluating e4…"
      openingText="Reviewing e4."
      inputDisabled
      learningPaths={[]}
      messages={[]}
      onMessage={onMessage}
      failure={null}
    />,
  )

  const composer = screen.getByLabelText("Message the coach")
  expect(composer).toHaveProperty("disabled", true)
  expect(screen.getByRole("button", { name: "Send" })).toHaveProperty(
    "disabled",
    true,
  )
  fireEvent.keyDown(composer, { key: "Enter" })
  fireEvent.click(screen.getByRole("button", { name: "Send" }))

  expect(onMessage).not.toHaveBeenCalled()
})

test("hides the compact learning component for neutral local material", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="Neutral: e4. Verified observation: White played e4 at ply 1."
    />,
  )

  expect(
    screen.queryByRole("region", {
      name: "Learning plan for this moment",
    }),
  ).toBeNull()
})

test("renders a missing idea as concept lesson and pattern drilling", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[
        {
          cluster: "Lichess Curriculum",
          conceptLessons: [
            {
              resourceId: fromLearningResourceId("lichess:practice:Qj281y1p"),
              role: "learn",
              kind: "practiceModule",
              title: "The Fork",
              canonicalUrl:
                "https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p",
            },
          ],
          idea: "Fork",
          id: "curriculum:fork",
          learningPathRef: fromLearningPathRef("learning-path:fixture-fork"),
          patternDrills: [
            {
              resourceId: fromLearningResourceId("lichess:puzzles:fork"),
              role: "drill",
              kind: "puzzleStream",
              title: "Fork",
              canonicalUrl: "https://lichess.org/training/fork",
            },
          ],
          purpose: "missing",
        },
      ]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="Review the grounded Fork in this position."
    />,
  )

  expect(
    screen
      .getByRole("link", { name: /Concept lesson: The Fork/ })
      .getAttribute("href"),
  ).toBe("https://lichess.org/practice/fundamental-tactics/the-fork/Qj281y1p")
  expect(
    screen
      .getByRole("link", { name: /Pattern drilling: Fork/ })
      .getAttribute("href"),
  ).toBe("https://lichess.org/training/fork")
  expect(screen.getByText(/Missing idea/)).toBeTruthy()
  expect(screen.queryByText("Real-game application")).toBeNull()
  expect(screen.queryByText("Rank 1")).toBeNull()
})

test("keeps pattern drilling when no exact Practice module exists", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[
        {
          cluster: "Lichess Curriculum",
          conceptLessons: [],
          idea: "Hanging piece",
          id: "curriculum:hangingPiece",
          learningPathRef: fromLearningPathRef("learning-path:fixture-hanging"),
          patternDrills: [
            {
              resourceId: fromLearningResourceId(
                "lichess:puzzles:hangingPiece",
              ),
              role: "drill",
              kind: "puzzleStream",
              title: "Hanging piece",
              canonicalUrl: "https://lichess.org/training/hangingPiece",
            },
          ],
          purpose: "missing",
        },
      ]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="After Bxb5, cxb5 wins the bishop."
    />,
  )

  expect(
    screen
      .getByRole("link", { name: /Pattern drilling: Hanging piece/ })
      .getAttribute("href"),
  ).toBe("https://lichess.org/training/hangingPiece")
  expect(screen.queryByText("Real-game application")).toBeNull()
})

test("records review feedback from the Coach message header", async () => {
  const user = userEvent.setup()
  const onReviewFeedback = vi.fn()
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="After Bxb5, cxb5 wins the bishop."
      reviewFeedback={{
        failure: null,
        onVote: onReviewFeedback,
        pending: false,
        vote: null,
      }}
    />,
  )

  expect(screen.getByText("Coach")).toBeTruthy()
  expect(screen.getByRole("group", { name: "Review feedback" })).toBeTruthy()
  await user.click(screen.getByRole("button", { name: "Not helpful" }))
  expect(onReviewFeedback).toHaveBeenCalledWith("thumbsDown")
  await user.click(screen.getByRole("button", { name: "Helpful" }))
  expect(onReviewFeedback).toHaveBeenLastCalledWith("thumbsUp")
})

test("does not explain Review Snapshot retention on the vote control", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="After Bxb5, cxb5 wins the bishop."
      reviewFeedback={{
        failure: null,
        onVote: vi.fn(),
        pending: false,
        vote: null,
      }}
    />,
  )

  expect(screen.getByText("Helpful?")).toBeTruthy()
  expect(screen.getByRole("group", { name: "Review feedback" })).toBeTruthy()
  expect(screen.queryByText(/Sending feedback/)).toBeNull()
  expect(screen.queryByText(/pseudonymized Review Snapshot/)).toBeNull()
})

test("hides review feedback while the Coach comment is still authoring", () => {
  renderPanel(
    <ConversationPanel
      busyLabel="Opening the selected Review Moment…"
      comment={null}
      commentPublished={false}
      failure={null}
      firstOpenStartedAt={Date.now()}
      inputDisabled
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText=""
      reviewFeedback={{
        failure: null,
        onVote: vi.fn(),
        pending: false,
        vote: null,
      }}
      safeRendering="Good: c3 advanced the passed pawn to c3."
    />,
  )

  expect(screen.queryByRole("button", { name: "Helpful" })).toBeNull()
  expect(screen.queryByRole("button", { name: "Not helpful" })).toBeNull()
})

test("keeps recorded review feedback pressed on the Coach header", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="After Bxb5, cxb5 wins the bishop."
      reviewFeedback={{
        failure: null,
        onVote: vi.fn(),
        pending: false,
        vote: "thumbsDown",
      }}
    />,
  )

  expect(
    screen
      .getByRole("button", { name: "Not helpful" })
      .getAttribute("aria-pressed"),
  ).toBe("true")
  expect(screen.getByText("Recorded")).toBeTruthy()
})

test("holds the review feedback buttons closed while the vote is in flight", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="After Bxb5, cxb5 wins the bishop."
      reviewFeedback={{
        failure: null,
        onVote: vi.fn(),
        pending: true,
        vote: null,
      }}
    />,
  )

  expect(
    screen.getByRole("button", { name: "Helpful" }).hasAttribute("disabled"),
  ).toBe(true)
  expect(
    screen
      .getByRole("button", { name: "Not helpful" })
      .hasAttribute("disabled"),
  ).toBe(true)
  expect(screen.getByText("Saving…")).toBeTruthy()
})

test("surfaces a failed review feedback write beside the Coach comment", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="After Bxb5, cxb5 wins the bishop."
      reviewFeedback={{
        failure: "Review feedback is unavailable.",
        onVote: vi.fn(),
        pending: false,
        vote: null,
      }}
    />,
  )

  expect(screen.getByRole("alert").textContent).toBe(
    "Review feedback is unavailable.",
  )
  expect(
    screen.getByRole("button", { name: "Helpful" }).hasAttribute("disabled"),
  ).toBe(false)
})

test("omits review feedback when no writer is wired to the conversation", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="After Bxb5, cxb5 wins the bishop."
    />,
  )

  expect(screen.queryByRole("group", { name: "Review feedback" })).toBeNull()
  expect(screen.queryByText(/Sending feedback/)).toBeNull()
})

test("records, replaces, and removes structured learning path relevance", async () => {
  const user = userEvent.setup()
  const onLearningPathVote = vi.fn()
  const learningPathRef = fromLearningPathRef("learning-path:feedback")
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPathFeedback={{
        [learningPathRef]: {
          currentVote: "thumbsUp",
          exposedSurfaces: ["web"],
          learningPathRef,
        },
      }}
      learningPaths={[
        {
          cluster: "Lichess Curriculum",
          conceptLessons: [],
          idea: "Defensive move",
          id: "curriculum:defensiveMove",
          learningPathRef,
          patternDrills: [],
          purpose: "missing",
        },
      ]}
      messages={[]}
      onLearningPathVote={onLearningPathVote}
      onMessage={vi.fn()}
      openingText="Review the defensive resource."
    />,
  )

  expect(screen.getByText("Recorded")).toBeTruthy()
  expect(
    screen
      .getByRole("button", { name: "Relevant" })
      .getAttribute("aria-pressed"),
  ).toBe("true")
  await user.click(screen.getByRole("button", { name: "Relevant" }))
  expect(onLearningPathVote).toHaveBeenCalledWith(learningPathRef, null)
  await user.click(screen.getByRole("button", { name: "Not relevant" }))
  expect(onLearningPathVote).toHaveBeenLastCalledWith(
    learningPathRef,
    "thumbsDown",
  )
})

test("shows a bounded wait instead of a comment, then the full comment", () => {
  const { rerender } = renderPanel(
    <ConversationPanel
      busyLabel="Opening the selected Review Moment…"
      comment={null}
      commentPublished={false}
      failure={null}
      firstOpenStartedAt={Date.now()}
      inputDisabled
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText=""
      safeRendering="Good: c3 advanced the passed pawn to c3."
    />,
  )
  const authoring = document.querySelector("[data-comment-wait='bounded']")
  expect(authoring).toBeTruthy()
  expect(authoring?.querySelector("svg")).toBeTruthy()
  expect(screen.queryByText(/Good: e4/)).toBeNull()
  expect(screen.queryByText(/After e4, occupy/)).toBeNull()

  rerender(
    <ConversationPanel
      busyLabel={null}
      comment={{ text: "After e4, occupy the center." }}
      commentPublished
      failure={null}
      firstOpenStartedAt={Date.now() - 1_000}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText=""
      safeRendering="Good: c3 advanced the passed pawn to c3."
    />,
  )
  expect(screen.getByText("After e4, occupy the center.")).toBeTruthy()
  expect(document.querySelector("[data-comment-wait]")).toBeNull()
})

test("does not retract a mounted hosted note for later unpublished text", () => {
  const { rerender } = renderPanel(
    <ConversationPanel
      busyLabel="Opening the selected Review Moment…"
      comment={null}
      commentPublished={false}
      failure={null}
      firstOpenStartedAt={Date.now()}
      inputDisabled
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText=""
      safeRendering="Good: c3 advanced the passed pawn to c3."
    />,
  )
  rerender(
    <ConversationPanel
      busyLabel={null}
      comment={{ text: "After e4, occupy the center." }}
      commentPublished
      failure={null}
      firstOpenStartedAt={Date.now() - 1_000}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText=""
      safeRendering="Good: c3 advanced the passed pawn to c3."
    />,
  )
  expect(screen.getByText("After e4, occupy the center.")).toBeTruthy()
  rerender(
    <ConversationPanel
      busyLabel={null}
      comment={{ text: "Good: c3 advanced the passed pawn to c3." }}
      commentPublished={false}
      failure={null}
      firstOpenStartedAt={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="Good: c3 advanced the passed pawn to c3."
      safeRendering="Good: c3 advanced the passed pawn to c3."
    />,
  )
  expect(screen.getByText("After e4, occupy the center.")).toBeTruthy()
  expect(
    screen.queryByText("Good: c3 advanced the passed pawn to c3."),
  ).toBeNull()
})

test("deadline settles the unpublished safe text without a hosted note", async () => {
  vi.useFakeTimers()
  const onAuthoringDeadline = vi.fn()
  renderPanel(
    <ConversationPanel
      busyLabel="Opening the selected Review Moment…"
      comment={null}
      commentPublished={false}
      failure={null}
      firstOpenStartedAt={Date.now()}
      inputDisabled
      learningPaths={[]}
      messages={[]}
      onAuthoringDeadline={onAuthoringDeadline}
      onMessage={vi.fn()}
      openingText={null}
      safeRendering="Good: c3 advanced the passed pawn to c3."
    />,
  )
  expect(document.querySelector("[data-comment-wait='bounded']")).toBeTruthy()
  await act(async () => {
    vi.advanceTimersByTime(10_000)
  })
  expect(onAuthoringDeadline).toHaveBeenCalledOnce()
  expect(
    screen.getByText(/Good: c3 advanced the passed pawn to c3/),
  ).toBeTruthy()
  vi.useRealTimers()
})

test("empty deadline keeps waiting so a late published comment can still mount", async () => {
  vi.useFakeTimers()
  const onAuthoringDeadline = vi.fn()
  const { rerender } = renderPanel(
    <ConversationPanel
      busyLabel="Opening the selected Review Moment…"
      comment={null}
      commentPublished={false}
      failure={null}
      firstOpenStartedAt={Date.now()}
      inputDisabled
      learningPaths={[]}
      messages={[]}
      onAuthoringDeadline={onAuthoringDeadline}
      onMessage={vi.fn()}
      openingText={null}
      safeRendering=""
    />,
  )
  await act(async () => {
    vi.advanceTimersByTime(10_000)
  })
  expect(onAuthoringDeadline).not.toHaveBeenCalled()
  expect(document.querySelector("[data-comment-wait='bounded']")).toBeTruthy()
  rerender(
    <ConversationPanel
      busyLabel={null}
      comment={{ text: "After e4, occupy the center." }}
      commentPublished
      failure={null}
      firstOpenStartedAt={Date.now() - 11_000}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onAuthoringDeadline={onAuthoringDeadline}
      onMessage={vi.fn()}
      openingText=""
      safeRendering=""
    />,
  )
  expect(screen.getByText("After e4, occupy the center.")).toBeTruthy()
  expect(document.querySelector("[data-comment-wait]")).toBeNull()
  vi.useRealTimers()
})

test("renders a HostTurn answer as a Coach thread message", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[
        { kind: "playerMessage", id: "q1", text: "Why was this a mistake?" },
        {
          kind: "coachAnswer",
          id: "a1",
          answer: "The knight was hanging.",
          effects: {},
        },
      ]}
      onMessage={vi.fn()}
      openingText="Start here."
    />,
  )

  expect(screen.getByText("Why was this a mistake?")).toBeTruthy()
  expect(screen.getByText("The knight was hanging.")).toBeTruthy()
})

test("renders HostTurn unavailability as a thread message", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[
        {
          kind: "unavailable",
          id: "u1",
          reason: { kind: "languageLayer" },
        },
      ]}
      onMessage={vi.fn()}
      openingText="Start here."
    />,
  )

  expect(screen.getByText(INTERACTIVE_COACHING_UNAVAILABLE)).toBeTruthy()
})

test("renders each HostTurn refusal as a thread message", () => {
  const { rerender } = renderPanel(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[
        {
          kind: "refusal",
          id: "r1",
          reason: "notAboutThisReview",
        },
      ]}
      onMessage={vi.fn()}
      openingText="Start here."
    />,
  )
  expect(screen.getByText(hostTurnRefusalText.notAboutThisReview)).toBeTruthy()

  rerender(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[{ kind: "refusal", id: "r2", reason: "notAboutChess" }]}
      onMessage={vi.fn()}
      openingText="Start here."
    />,
  )
  expect(screen.getByText(hostTurnRefusalText.notAboutChess)).toBeTruthy()

  rerender(
    <ConversationPanel
      busyLabel={null}
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[{ kind: "refusal", id: "r3", reason: "unsafeRequest" }]}
      onMessage={vi.fn()}
      openingText="Start here."
    />,
  )
  expect(screen.getByText(hostTurnRefusalText.unsafeRequest)).toBeTruthy()
})

test("shows each HostTurn step label in the thinking row", () => {
  for (const label of Object.values(hostTurnStepLabels)) {
    const { unmount } = renderPanel(
      <ConversationPanel
        busyLabel={label}
        failure={null}
        inputDisabled={false}
        learningPaths={[]}
        messages={[]}
        onMessage={vi.fn()}
        openingText="Start here."
      />,
    )
    expect(screen.getByText(label)).toBeTruthy()
    expect(label).not.toMatch(
      /read_moment|list_moments|evaluate_line|learning_material|capability/i,
    )
    unmount()
  }
})

test("shows the composer without an Ask a follow-up control", () => {
  renderPanel(
    <ConversationPanel
      busyLabel={null}
      comment={{ text: "Start with the forcing move." }}
      commentPublished
      failure={null}
      inputDisabled={false}
      learningPaths={[]}
      messages={[]}
      onMessage={vi.fn()}
      openingText="Start with the forcing move."
    />,
  )
  expect(screen.getByText("Start with the forcing move.")).toBeTruthy()
  expect(screen.queryByRole("button", { name: "Ask a follow-up" })).toBeNull()
  expect(screen.getByLabelText("Message the coach")).toBeTruthy()
  expect(document.activeElement).not.toBe(
    screen.getByLabelText("Message the coach"),
  )
})
