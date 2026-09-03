import * as stylex from "@stylexjs/stylex"
import { useSyncExternalStore } from "react"
import type { ReactNode } from "react"

import { SwipeRevealedRow } from "./SwipeRevealedRow"
import { trailingActionRowStyles as styles } from "./TrailingActionRow.styles"
import { WatercolorButton } from "./watercolor"

/** The one action a row offers. A row that offers nothing is not a row. */
export type RowAction = {
  /** The word on the control. */
  label: string
  /** What a screen reader announces, which names the row rather than the verb. */
  accessibleLabel: string
  busy?: boolean
  onAction: () => void
}

/**
 * A list row that offers one trailing action, by the means the Player's
 * pointer has.
 *
 * A precise pointer has no swipe, and sliding a row out from under a mouse to
 * uncover a control it could simply be shown is a gesture borrowed from a
 * device that is not there. So on a mouse the action sits on the row and
 * appears when the row is hovered or something inside it takes focus, and the
 * row never moves. A touch pointer gets the drag (`SwipeRevealedRow`).
 *
 * Two components rather than two branches: a gesture row is most of a
 * component — a motion value, a drag state machine, a place in the one-row-open
 * registry — and none of it means anything to a mouse. Choosing between them
 * here also means a pointer that changes kind mid-session (a laptop reaching a
 * dock) gets a genuinely fresh row rather than one still carrying the offset
 * and the registry seat it held as the other kind.
 *
 * Either way the action is a real button, so a keyboard reaches it by tabbing
 * and nothing here is the only way to do anything.
 */
export function TrailingActionRow({
  action,
  children,
}: {
  action: RowAction
  children: ReactNode
}) {
  return usePrecisePointer() ? (
    <HoverRevealedRow action={action}>{children}</HoverRevealedRow>
  ) : (
    <SwipeRevealedRow action={action}>{children}</SwipeRevealedRow>
  )
}

/**
 * The mouse row: nothing moves, and the action fades in over the trailing end
 * of the row while the row is hovered or holds focus.
 *
 * The control sits *on* the row rather than behind it — a watercolor card's
 * paper is not opaque, so an action parked underneath would show through — and
 * over the card rather than beside it, because a column held clear for a
 * control that is invisible most of the time is a gutter down the whole list.
 */
function HoverRevealedRow({
  action,
  children,
}: {
  action: RowAction
  children: ReactNode
}) {
  return (
    <div {...stylex.props(styles.row, styles.hoverRow)}>
      <div {...stylex.props(styles.surface)}>{children}</div>
      <span {...stylex.props(styles.actionOnRow)}>
        <WatercolorButton
          aria-label={action.accessibleLabel}
          disabled={action.busy === true}
          onClick={action.onAction}
          /* Small: this one sits inside the card's own bounds, where the
             drawer's control has a 6rem lane to fill. */
          size="sm"
          type="button"
          variant="danger"
        >
          {action.label}
        </WatercolorButton>
      </span>
    </div>
  )
}

const precisePointerQuery = "(hover: hover) and (pointer: fine)"

/**
 * True where the Player points with a mouse or trackpad. False before
 * hydration, so a touch device — which cannot answer the query on the server
 * either — never renders a control it would then have to take away.
 */
function usePrecisePointer(): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const media = window.matchMedia(precisePointerQuery)
      media.addEventListener("change", onChange)
      return () => media.removeEventListener("change", onChange)
    },
    () => window.matchMedia(precisePointerQuery).matches,
    () => false,
  )
}
