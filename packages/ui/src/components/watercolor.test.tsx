// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, expect, test, vi } from "vitest"
import type { ReactNode } from "react"

import { workspaceFixture } from "../fixtures"
import { ChenTheme } from "../theme/ChenTheme"
import {
  SessionHeaderLabel,
  WatercolorBadge,
  WatercolorButton,
  WatercolorChatComposer,
  WatercolorChessboard,
  WatercolorEvaluationBar,
  WatercolorEvaluationGraph,
  WatercolorField,
  WatercolorInkStroke,
  WatercolorInput,
  WatercolorMoveNav,
  WatercolorTextarea,
} from "./watercolor"
import { WatercolorSessionHeader } from "./WatercolorSessionHeader"

afterEach(cleanup)

function renderUi(ui: ReactNode) {
  return render(<ChenTheme>{ui}</ChenTheme>)
}

test("session header title cannot collapse beside a nowrap badge", () => {
  renderUi(
    <WatercolorSessionHeader
      actions={<WatercolorBadge>Resumable · 107 plies</WatercolorBadge>}
      eyebrow="Review Session"
      title="A long Scandinavian Defense Review Session title"
    />,
  )
  const title = document.querySelector(".chen-watercolor-session-title")
  expect(title?.textContent).toContain("Scandinavian")
  expect(title).toBeTruthy()
})

test("session header without title or eyebrow omits the title stack", () => {
  renderUi(
    <WatercolorSessionHeader
      actions={<WatercolorBadge>Saved</WatercolorBadge>}
    />,
  )
  expect(document.querySelector(".chen-watercolor-session-title")).toBeNull()
  expect(document.querySelector(".chen-watercolor-eyebrow")).toBeNull()
})

test("session header without a title uses the dry-brush plaque as the H1", () => {
  renderUi(<WatercolorSessionHeader eyebrow="Review Session" />)
  expect(document.querySelector(".chen-watercolor-eyebrow")).toBeNull()
  const title = document.querySelector(".chen-watercolor-session-title")
  expect(title?.tagName).toBe("H1")
  expect(
    title?.querySelector(
      ".chen-watercolor-session-subtitle[data-watercolor-control='plaque']",
    )?.textContent,
  ).toBe("Review Session")
})

test("header action labels sit beside the icon for assistive text", () => {
  renderUi(
    <WatercolorButton
      aria-label="Share"
      size="sm"
      type="button"
      variant="quiet"
    >
      <SessionHeaderLabel>Share</SessionHeaderLabel>
    </WatercolorButton>,
  )
  const label = document.querySelector(".chen-session-header-label")
  expect(label?.textContent).toBe("Share")
  expect(screen.getByRole("button", { name: "Share" })).toBeTruthy()
})

test("encodes the brand move-nav pair as one primitive", async () => {
  const onNavigate = vi.fn()
  const user = userEvent.setup()
  renderUi(
    <WatercolorMoveNav
      aria-label="Move sequence"
      maxPly={24}
      onNavigate={onNavigate}
      ply={6}
    />,
  )

  const nav = screen.getByRole("group", { name: "Move sequence" })
  expect(nav.getAttribute("data-watercolor-control")).toBe("move-sequence")
  expect(screen.getByRole("button", { name: "Previous move" })).toBeTruthy()
  expect(screen.getByRole("button", { name: "Next move" })).toBeTruthy()
  expect(nav.textContent).toMatch(/6\s*\/\s*24/)

  await user.click(screen.getByRole("button", { name: "Next move" }))
  expect(onNavigate).toHaveBeenCalledWith(7)
  await user.click(screen.getByRole("button", { name: "Previous move" }))
  expect(onNavigate).toHaveBeenCalledWith(5)
  await user.click(screen.getByRole("button", { name: "Last move" }))
  expect(onNavigate).toHaveBeenCalledWith(24)
})

test("keeps first and last ply jumps and outlines Previous and Next", () => {
  renderUi(
    <WatercolorMoveNav
      aria-label="Move sequence"
      maxPly={24}
      onNavigate={() => undefined}
      ply={6}
    />,
  )

  expect(screen.getByRole("button", { name: "First move" })).toBeTruthy()
  expect(screen.getByRole("button", { name: "Last move" })).toBeTruthy()
  expect(
    screen.getByRole("button", { name: "Previous move" }).className,
  ).toContain("chen-watercolor-button-secondary")
  expect(screen.getByRole("button", { name: "Next move" }).className).toContain(
    "chen-watercolor-button-secondary",
  )
})

test("keeps a non-empty accessible name on every move-nav button when labels drop", () => {
  renderUi(
    <WatercolorMoveNav
      aria-label="Move sequence"
      maxPly={24}
      onNavigate={() => undefined}
      ply={6}
    />,
  )

  const names = ["First move", "Previous move", "Next move", "Last move"]
  for (const name of names) {
    expect(screen.getByRole("button", { name })).toBeTruthy()
  }
})

test("every button carries a decorative hover wash that never joins its name", () => {
  const { container } = renderUi(
    <>
      <WatercolorButton>Explore line</WatercolorButton>
      <WatercolorButton aria-label="First move" size="icon" variant="quiet">
        <span aria-hidden="true">«</span>
      </WatercolorButton>
    </>,
  )

  const washes = container.querySelectorAll(".chen-watercolor-hover-wash")
  expect(washes).toHaveLength(2)
  for (const wash of washes) {
    expect(wash.getAttribute("aria-hidden")).toBe("true")
  }
  // The wash is a sibling of the label, so it must stay out of the name.
  expect(screen.getByRole("button", { name: "Explore line" })).toBeTruthy()
  expect(screen.getByRole("button", { name: "First move" })).toBeTruthy()
})

test("a disabled or loading button renders no hover wash at all", () => {
  const { container } = renderUi(
    <>
      <WatercolorButton disabled>Explore line</WatercolorButton>
      <WatercolorButton loading>Reviewing</WatercolorButton>
    </>,
  )

  // The hover switch is a custom property flipped under `:hover`, which
  // StyleX orders after `:disabled`; the only way a pointer resting on a
  // disabled control cannot light it is for there to be nothing to light.
  expect(
    container.querySelectorAll(".chen-watercolor-hover-wash"),
  ).toHaveLength(0)
  expect(container.querySelectorAll("button[disabled]")).toHaveLength(2)
})

test("the ink stroke is decorative unless labelled, and reveals through its own artwork mask", () => {
  const { container, rerender } = renderUi(<WatercolorInkStroke />)

  const decorative = container.querySelector(".chen-watercolor-ink-stroke")
  expect(decorative?.getAttribute("aria-hidden")).toBe("true")
  expect(decorative?.getAttribute("role")).toBeNull()

  const guide = decorative?.querySelector("path")
  const mask = decorative?.querySelector("mask")
  expect(guide?.getAttribute("pathLength")).toBe("1")
  expect(guide?.getAttribute("stroke")).toBe("currentColor")
  expect(mask?.querySelector("image")?.getAttribute("href")).toMatch(
    /brush-swoosh/,
  )
  expect(guide?.getAttribute("mask")).toBe(`url(#${mask?.id ?? ""})`)

  rerender(
    <ChenTheme>
      <WatercolorInkStroke label="Brush divider" />
    </ChenTheme>,
  )
  expect(screen.getByRole("img", { name: "Brush divider" })).toBeTruthy()
})

test("composes the board, bounded evaluation bar, and interactive graph", async () => {
  const onSelect = vi.fn()
  const user = userEvent.setup()
  const points = [
    { label: "+0.18", ply: 0, value: 18 },
    { label: "+0.86", ply: 6, value: 86 },
  ]
  const moments = [
    {
      glyph: "!",
      label: "Queen exposed",
      moveLabel: "3… Qxd5",
      ply: 6,
      tone: "improvement",
    },
  ] as const

  renderUi(
    <>
      <WatercolorChessboard board={workspaceFixture.board} />
      <WatercolorEvaluationBar valueLabel="+8.00" whiteShare={112} />
      <WatercolorEvaluationGraph
        activePly={6}
        disabled={false}
        maxPly={6}
        moments={moments}
        onSelect={onSelect}
        points={points}
      />
    </>,
  )

  expect(
    screen.getByRole("img", {
      name: /Chessboard\. Black queen moved from d8 to d5/,
    }),
  ).toBeTruthy()
  const evaluationBar = screen.getByRole("meter", {
    name: "Position evaluation",
  })
  expect(evaluationBar.getAttribute("aria-valuenow")).toBe("100")
  expect(evaluationBar.getAttribute("aria-valuetext")).toBe("+8.00")

  const graph = screen
    .getByRole("heading", { name: "Real-game evaluation" })
    .closest("[data-watercolor-surface='evaluation-graph']")
  expect(graph?.getAttribute("data-watercolor-composition")).toBe("content")
  expect(
    screen.getByRole("status", { name: "Evaluation at the selected moment" })
      .textContent,
  ).toBe("+0.86")

  await user.click(
    screen.getByRole("button", {
      name: /Evaluation graph: Queen exposed at 3… Qxd5/,
    }),
  )
  expect(onSelect).toHaveBeenCalledWith(6)
})

test("sparkline graph drops the evaluation blot and duplicate eval text", () => {
  renderUi(
    <WatercolorEvaluationGraph
      activePly={6}
      density="sparkline"
      disabled={false}
      maxPly={6}
      moments={[
        {
          glyph: "!",
          label: "Queen exposed",
          moveLabel: "3… Qxd5",
          ply: 6,
          tone: "improvement",
        },
      ]}
      onSelect={() => undefined}
      points={[
        { label: "+0.18", ply: 0, value: 18 },
        { label: "+0.86", ply: 6, value: 86 },
      ]}
    />,
  )

  expect(
    screen.queryByRole("heading", { name: "Real-game evaluation" }),
  ).toBeNull()
  expect(
    screen.queryByRole("status", { name: "Evaluation at the selected moment" }),
  ).toBeNull()
  expect(
    screen.getByRole("img", { name: "Measured real-game evaluation graph" }),
  ).toBeTruthy()
})

test("names a field from its label alone and describes it with the hint", () => {
  renderUi(
    <WatercolorField hint="0/280 characters" label="Message to Coach">
      <WatercolorTextarea />
    </WatercolorField>,
  )

  const control = screen.getByRole("textbox", { name: "Message to Coach" })
  const describedBy = control.getAttribute("aria-describedby")
  expect(describedBy).toBeTruthy()
  expect(document.getElementById(describedBy!)?.textContent).toBe(
    "0/280 characters",
  )
  expect(control.getAttribute("aria-invalid")).toBeNull()
})

test("marks the control invalid and paints the frame when the field errors", () => {
  renderUi(
    <WatercolorField
      error="Enter a complete Lichess game URL."
      label="Game source"
    >
      <WatercolorInput />
    </WatercolorField>,
  )

  const control = screen.getByRole("textbox", { name: "Game source" })
  expect(control.getAttribute("aria-invalid")).toBe("true")
  const describedBy = control.getAttribute("aria-describedby")
  expect(document.getElementById(describedBy!)?.textContent).toBe(
    "Enter a complete Lichess game URL.",
  )
  expect(screen.getByRole("alert").textContent).toBe(
    "Enter a complete Lichess game URL.",
  )
  // The frame carries the error ink; jsdom cannot see the compiled StyleX, so
  // the hook the craft rides on is what a unit test can hold.
  expect(
    control.closest("[data-invalid='true'].chen-watercolor-input-frame"),
  ).toBeTruthy()
})

test("seats Send inside the outlined chat composer box", () => {
  renderUi(
    <WatercolorChatComposer
      onChange={() => undefined}
      onSend={() => undefined}
      placeholder="Describe your plan or ask a follow-up…"
      value="Why this capture?"
    />,
  )
  const input = screen.getByLabelText("Message the coach")
  const send = screen.getByRole("button", { name: "Send" })
  const box = document.querySelector(
    "[data-watercolor-surface='chat-composer']",
  )
  expect(box).toBeTruthy()
  expect(box?.contains(input)).toBe(true)
  expect(box?.contains(send)).toBe(true)
  expect(send).toHaveProperty("disabled", false)
})
