import * as stylex from "@stylexjs/stylex"
import {
  animate,
  motion,
  useMotionValue,
  useReducedMotion,
  useTransform,
} from "motion/react"
import { useCallback, useEffect, useRef, useState } from "react"
import type { PointerEvent as ReactPointerEvent, ReactNode } from "react"

import { projectedRestingPoint, resistedOffset } from "./swipeMomentum"
import type { RowAction } from "./TrailingActionRow"
import {
  SWIPE_REVEAL_WIDTH_REM,
  trailingActionRowStyles as styles,
} from "./TrailingActionRow.styles"
import { WatercolorButton } from "./watercolor"

/**
 * The one row currently showing its action, so opening another closes it.
 *
 * A list with two rows open has two destructive controls on screen and no
 * telling which one the next tap belongs to. Module scope is the right scope:
 * the rule is about the screen, not about any one list.
 */
let openRow: { close: () => void; row: symbol } | null = null

/** The reveal width before the row has been measured, at a 16px root. */
const DEFAULT_REVEAL_PIXELS = SWIPE_REVEAL_WIDTH_REM * 16
/** Movement, in CSS pixels, before a drag commits to the horizontal axis. */
const AXIS_HYSTERESIS = 10
/** How far past the reveal the row must be thrown before the action commits. */
const COMMIT_FRACTION = 0.55

/**
 * The touch row: the action lives in a drawer the row is dragged off.
 *
 * The row tracks the finger one-to-one, resists past its reveal width instead
 * of stopping dead, and lands where the throw was going rather than where the
 * finger left off. Throwing it past `COMMIT_FRACTION` of its own width runs
 * the action directly, which is safe because the action a caller passes opens
 * a confirmation rather than doing the destructive thing itself.
 *
 * Reached through `TrailingActionRow`, which is what decides that this is the
 * row a given pointer gets.
 */
export function SwipeRevealedRow({
  action,
  children,
}: {
  action: RowAction
  children: ReactNode
}) {
  const reduceMotion = useReducedMotion()
  const row = useRef<HTMLDivElement>(null)
  const x = useMotionValue(0)
  const [dragging, setDragging] = useState(false)
  const drag = useRef<DragState | null>(null)
  const reveal = useRef(DEFAULT_REVEAL_PIXELS)
  /** This row's identity in the one-open-row rule, stable for its lifetime. */
  const [identity] = useState(() => Symbol("swipe-revealed-row"))
  /* A watercolor card's paper is not opaque, so an action sitting behind a
     closed row would show through it. Its presence is the reveal itself: it
     arrives as the row uncovers it and is gone by the time the row is home. */
  const actionOpacity = useTransform(x, (offset) => {
    const travelled = -offset / reveal.current
    return Math.min(Math.max((travelled - 0.15) / 0.45, 0), 1)
  })

  useEffect(() => {
    const element = row.current
    if (element !== null) reveal.current = revealWidth(element)
  }, [])

  const settle = useCallback(
    (to: number, velocity: number) => {
      if (to !== 0) {
        if (openRow !== null && openRow.row !== identity) openRow.close()
        openRow = { close: () => settle(0, 0), row: identity }
      } else if (openRow?.row === identity) {
        openRow = null
      }
      if (reduceMotion) {
        x.set(to)
        return
      }
      animate(x, to, {
        type: "spring",
        // Bounce only where the gesture carried momentum into the landing.
        bounce: velocity === 0 ? 0 : 0.2,
        // The top of WATERCOLOR.md's interaction budget, not the wash budget:
        // this settle answers a finger, so it lands rather than drifts.
        duration: 0.22,
        velocity,
      })
    },
    [identity, reduceMotion, x],
  )
  const close = useCallback(() => settle(0, 0), [settle])

  useEffect(
    () => () => {
      if (openRow?.row === identity) openRow = null
    },
    [identity],
  )

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    event.currentTarget.setPointerCapture(event.pointerId)
    drag.current = {
      committed: false,
      lastAt: event.timeStamp,
      lastX: event.clientX,
      pointerId: event.pointerId,
      rowWidth: event.currentTarget.getBoundingClientRect().width,
      startOffset: x.get(),
      startX: event.clientX,
      startY: event.clientY,
      velocity: 0,
    }
  }

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const state = drag.current
    if (state === null || state.pointerId !== event.pointerId) return
    const travelled = event.clientX - state.startX
    if (!state.committed) {
      const vertical = Math.abs(event.clientY - state.startY)
      if (Math.abs(travelled) < AXIS_HYSTERESIS) return
      /* A gesture that is mostly vertical belongs to the page's scroll, so the
         row lets go of it rather than fighting for the same pixels. */
      if (vertical > Math.abs(travelled)) {
        drag.current = null
        return
      }
      state.committed = true
      setDragging(true)
    }
    const elapsed = Math.max(event.timeStamp - state.lastAt, 1)
    state.velocity = ((event.clientX - state.lastX) / elapsed) * 1000
    state.lastAt = event.timeStamp
    state.lastX = event.clientX
    x.set(
      resistedOffset(
        state.startOffset + travelled,
        reveal.current,
        state.rowWidth,
      ),
    )
  }

  const onPointerUp = () => {
    const state = drag.current
    drag.current = null
    setDragging(false)
    if (state === null || !state.committed) return
    const landing = x.get() + projectedRestingPoint(state.velocity)
    if (landing <= -state.rowWidth * COMMIT_FRACTION) {
      settle(0, state.velocity)
      action.onAction()
      return
    }
    settle(landing <= -reveal.current / 2 ? -reveal.current : 0, state.velocity)
  }

  return (
    <div ref={row} {...stylex.props(styles.row)}>
      <motion.span
        style={{ opacity: actionOpacity }}
        {...stylex.props(styles.actionDrawer)}
      >
        <WatercolorButton
          aria-label={action.accessibleLabel}
          disabled={action.busy === true}
          onBlur={close}
          onKeyDown={(event) => {
            if (event.key === "Escape") close()
          }}
          onClick={action.onAction}
          onFocus={() => settle(-reveal.current, 0)}
          type="button"
          variant="danger"
        >
          {action.label}
        </WatercolorButton>
      </motion.span>
      <motion.div
        onPointerCancel={onPointerUp}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        style={{ x }}
        {...stylex.props(
          styles.surface,
          styles.draggable,
          dragging && styles.grabbing,
        )}
      >
        {children}
      </motion.div>
    </div>
  )
}

type DragState = {
  committed: boolean
  lastAt: number
  lastX: number
  pointerId: number
  rowWidth: number
  startOffset: number
  startX: number
  startY: number
  velocity: number
}

/** The reveal width in pixels, at whatever root size the Player reads at. */
function revealWidth(row: HTMLDivElement): number {
  const { fontSize } = getComputedStyle(row.ownerDocument.documentElement)
  return SWIPE_REVEAL_WIDTH_REM * Number.parseFloat(fontSize)
}
