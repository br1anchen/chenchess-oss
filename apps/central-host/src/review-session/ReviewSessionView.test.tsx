// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import {
  fromLearningPathRef,
  fromLearningResourceId,
} from "@chenchess/coach-engine-sdk"
import { ChenTheme } from "@chenchess/ui/theme"
import { Text } from "@chenchess/ui"

import { ReviewSessionView } from "./ReviewSessionView"
import { hostTurnStepLabels } from "./thread-state"
import type { MomentLearningPath } from "./reviewMoments"

afterEach(cleanup)

const forkPath: MomentLearningPath = {
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
  learningPathRef: fromLearningPathRef("learning-path:view-fork"),
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
}

const moments = [
  {
    glyph: "!",
    label: "Early queen exposure",
    moveLabel: "3… Qxd5",
    ply: 6,
    tone: "improvement" as const,
  },
]

function renderView(
  props: Partial<Parameters<typeof ReviewSessionView>[0]> = {},
) {
  return render(
    <ChenTheme>
      <ReviewSessionView
        board={<Text>Board column</Text>}
        onAccountSettings={() => undefined}
        signOut={async () => undefined}
        conversation={{
          composer: { kind: "idle", draft: "" },
          failure: null,
          learningPaths: [],
          messages: [],
          onMessage: () => undefined,
          openingText: "Pinned note.",
          ...props.conversation,
        }}
        momentMarkers={moments}
        onSelectMoment={() => undefined}
        sessionPly={6}
        viewedPly={6}
        {...props}
      />
    </ChenTheme>,
  )
}

test("places the board column before the session thread", () => {
  renderView()
  const board = screen.getByText("Board column")
  const thread = screen.getByLabelText("Coaching conversation")
  expect(
    board.compareDocumentPosition(thread) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(screen.getByRole("main").dataset.hasConversation).toBe("true")
  expect(screen.getByRole("heading", { name: "Game review" })).toBeTruthy()
  expect(document.querySelector(".chen-watercolor-eyebrow")).toBeNull()
  expect(
    document.querySelector(
      ".chen-watercolor-session-title [data-watercolor-control='plaque']",
    )?.textContent,
  ).toBe("Game review")
  expect(screen.queryByText("Beta Access confirmed")).toBeNull()
  expect(screen.getByRole("button", { name: "Account settings" })).toBeTruthy()
  expect(screen.getByRole("button", { name: "Log out" })).toBeTruthy()
  expect(
    document.querySelectorAll(".chen-session-header-label").length,
  ).toBeGreaterThan(0)
})

test("places compact game details in the session column before the thread", () => {
  renderView({
    gameInfo: <Text>Opening · A00 Van Geet Opening</Text>,
  })
  const details = screen.getByText("Opening · A00 Van Geet Opening")
  const thread = screen.getByLabelText("Coaching conversation")
  expect(details.closest(".chen-review-session-thread")).toBeTruthy()
  expect(details.closest(".chen-review-session-board")).toBeNull()
  expect(
    details.compareDocumentPosition(thread) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
})

test("places the evaluation graph in the session column, not on the board", () => {
  renderView({
    evaluationGraph: <Text>Real-game evaluation</Text>,
  })
  const graph = screen.getByText("Real-game evaluation")
  const thread = screen.getByLabelText("Coaching conversation")
  expect(graph.closest(".chen-review-session-thread")).toBeTruthy()
  expect(graph.closest(".chen-review-session-board")).toBeNull()
  expect(
    graph.compareDocumentPosition(thread) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(screen.getByRole("button", { name: "Previous moment" })).toBeTruthy()
  expect(screen.getByRole("button", { name: "Next moment" })).toBeTruthy()
})

test("places Learning Paths in the Review Moment thread", () => {
  renderView({
    conversation: {
      composer: { kind: "idle", draft: "" },
      failure: null,
      learningPaths: [forkPath],
      messages: [],
      onMessage: () => undefined,
      openingText: "Pinned note.",
    },
  })
  const thread = screen.getByLabelText("Coaching conversation")
  const paths = screen.getByRole("region", {
    name: "Learning plan for this moment",
  })
  expect(thread.contains(paths)).toBe(true)
  expect(
    screen.getByRole("heading", { name: /Missing idea.*Fork/ }),
  ).toBeTruthy()
  expect(
    screen.getByRole("link", { name: /Concept lesson: The Fork/ }),
  ).toBeTruthy()
  expect(
    screen.getByRole("link", { name: /Pattern drilling: Fork/ }),
  ).toBeTruthy()
})

test("keeps the message composer after the thread so it can sit at the bottom", () => {
  renderView()
  const thread = screen.getByLabelText("Coaching conversation")
  const messages = thread.querySelector("[aria-live='polite']")
  const composer = screen.getByLabelText("Message the coach")
  expect(messages).toBeTruthy()
  expect(
    messages!.compareDocumentPosition(composer) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
})

test("orders move controls then thread in the session column", () => {
  renderView({
    moveControls: <Text>Move sequence controls</Text>,
  })
  const controls = screen.getByText("Move sequence controls")
  const thread = screen.getByLabelText("Coaching conversation")
  expect(controls.closest(".chen-review-session-thread")).toBeTruthy()
  expect(
    controls.compareDocumentPosition(thread) & Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy()
  expect(
    screen.queryByRole("heading", { name: "Early queen exposure" }),
  ).toBeNull()
})

test("shows D9 HostTurn labels and never a capability name", () => {
  renderView({
    conversation: {
      composer: {
        kind: "hostTurn",
        draft: "",
        progress: { label: hostTurnStepLabels.checkingThatLine },
      },
      failure: null,
      learningPaths: [],
      messages: [],
      onCancel: () => undefined,
      onMessage: () => undefined,
      openingText: "Pinned note.",
    },
  })
  expect(screen.getByText(hostTurnStepLabels.checkingThatLine)).toBeTruthy()
  expect(screen.queryByText(/evaluate_line|read_moment|capability/i)).toBeNull()
})

test("default moment open shows the grounded comment without HostTurn chrome", () => {
  renderView()
  expect(screen.getByText("Pinned note.")).toBeTruthy()
  expect(screen.queryByRole("button", { name: "Ask a follow-up" })).toBeNull()
  expect(screen.getByLabelText("Message the coach")).toBeTruthy()
  expect(document.activeElement).not.toBe(
    screen.getByLabelText("Message the coach"),
  )
  expect(document.querySelector(".chen-review-companion")).toBeNull()
  expect(
    screen.queryByRole("link", {
      name: "Open this Review Moment on its own page",
    }),
  ).toBeNull()
})

test("renders #433 thread kinds without a second model", () => {
  renderView({
    conversation: {
      composer: { kind: "idle", draft: "" },
      failure: null,
      learningPaths: [],
      messages: [
        { id: "p", kind: "playerMessage", text: "Why this move?" },
        {
          answer: "Because the knight hangs.",
          effects: {},
          id: "c",
          kind: "coachAnswer",
        },
        {
          id: "u",
          kind: "unavailable",
          reason: { kind: "rateLimited", retryAfterSeconds: 30 },
        },
        { id: "r", kind: "refusal", reason: "notAboutChess" },
        { id: "j", kind: "rejected", recovery: { kind: "selectReviewSide" } },
      ],
      onMessage: () => undefined,
      openingText: "Pinned note.",
    },
  })
  expect(screen.getByText("Why this move?")).toBeTruthy()
  expect(screen.getByText("Because the knight hangs.")).toBeTruthy()
})
