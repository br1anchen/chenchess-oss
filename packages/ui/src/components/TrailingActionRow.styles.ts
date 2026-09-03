import * as stylex from "@stylexjs/stylex"

/** How much of the row the drawer's action occupies once it is open. */
export const SWIPE_REVEAL_WIDTH_REM = 6

export const trailingActionRowStyles = stylex.create({
  /** Shared by both rows: the box the action is positioned against. */
  row: {
    position: "relative",
    width: "100%",
    minWidth: 0,
    /* No clip here on purpose: a watercolor card's dry-brush frame paints
       outside its box, and clipping the row would shave it. The page this
       lists into already clips its own horizontal overflow. */
  },
  /** Shared by both rows: what the Player's content sits on. */
  surface: {
    position: "relative",
    width: "100%",
    minWidth: 0,
  },

  /* — The mouse row (`HoverRevealedRow`) — */

  /** What the on-row action reads for its own appearance. StyleX cannot
   * express an ancestor selector, so the row flips the pair and the action
   * reads them — the same parent-state craft the watercolor controls use.
   * Focus counts as well as hover: a keyboard reaching the delete has to see
   * the control it is about to press. */
  hoverRow: {
    "--row-action-opacity": {
      default: "0",
      ":hover": "1",
      ":focus-within": "1",
    },
    "--row-action-events": {
      default: "none",
      ":hover": "auto",
      ":focus-within": "auto",
    },
  },
  actionOnRow: {
    position: "absolute",
    insetBlock: 0,
    insetInlineEnd: "0.75rem",
    display: "flex",
    alignItems: "center",
    opacity: "var(--row-action-opacity, 0)",
    pointerEvents: "var(--row-action-events, none)",
    transition: {
      default: "opacity 140ms ease",
      "@media (prefers-reduced-motion: reduce)": "none",
    },
  },

  /* — The touch row (`SwipeRevealedRow`) — */

  /** The lane the row is dragged off to uncover. */
  actionDrawer: {
    position: "absolute",
    insetBlock: 0,
    insetInlineEnd: 0,
    width: `${SWIPE_REVEAL_WIDTH_REM}rem`,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    paddingInline: "0.5rem",
  },
  /** Only the dragged surface claims the horizontal axis: vertical panning
   * still belongs to the page, and a row that never moves claims nothing. */
  draggable: {
    touchAction: "pan-y",
  },
  grabbing: {
    cursor: "grabbing",
    userSelect: "none",
    willChange: "transform",
  },
})
