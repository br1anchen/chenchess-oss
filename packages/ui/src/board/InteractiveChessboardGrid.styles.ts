import * as stylex from "@stylexjs/stylex"

const compact = "@media (max-width: 620px)"
const reduceMotion = "@media (prefers-reduced-motion: reduce)"

/**
 * The interactive board's own geometry: the eval-bar column, the 8×8 grid and
 * its square states. The square skins, piece sprite offsets and the board
 * frame stay in WatercolorBoard.css — they address the
 * `.chen-workspace-square-light` / `-dark` and `.chen-workspace-piece` hooks
 * this component builds dynamically, shared with the coach-app board.
 */
export const boardStyles = stylex.create({
  row: {
    display: "grid",
    gridTemplateColumns: {
      default: "0.72rem minmax(0, 1fr)",
      [compact]: "0.3rem minmax(0, 1fr)",
    },
    gap: { default: "0.75rem", [compact]: "0.125rem" },
    alignItems: "stretch",
  },
  evalBar: {
    display: "flex",
    overflow: "hidden",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "color-mix(in srgb, var(--color-mist) 52%, transparent)",
    borderRadius: "999px",
    backgroundColor: "var(--color-ink)",
    alignItems: "flex-end",
    boxShadow: "0 0.2rem 0.7rem rgb(0 0 0 / 0.18)",
  },
  evalFill: {
    display: "block",
    width: "100%",
    minHeight: "4%",
    backgroundColor: "var(--color-paper)",
    backgroundImage:
      "linear-gradient(color-mix(in srgb, var(--color-paper-raised) 64%, transparent), color-mix(in srgb, var(--color-paper) 18%, transparent))",
    transitionProperty: { default: "height", [reduceMotion]: "none" },
    transitionDuration: "220ms",
    transitionTimingFunction: "ease-out",
  },
  boardCell: {
    position: "relative",
    isolation: "isolate",
    minWidth: 0,
    width: "min(100%, 820px)",
    justifySelf: "center",
  },
  board: {
    position: "relative",
    isolation: "isolate",
    display: "grid",
    width: "min(100%, 820px)",
    aspectRatio: "1",
    justifySelf: "center",
    gridTemplateColumns: "repeat(8, 1fr)",
  },
  /* Fill mode: the host wrapper is already capped to the leftover box, so the
     row and board just span it; the square keeps itself square. No max-height
     chains — percentage max-heights against auto-height flex parents resolve
     to none and let the board overflow its column. */
  fillRow: {
    width: "100%",
    height: "auto",
    maxWidth: "100%",
    justifySelf: "stretch",
    alignItems: "stretch",
  },
  fillBoard: {
    width: "100%",
    height: "auto",
    maxWidth: "100%",
    aspectRatio: "1",
    justifySelf: "stretch",
  },
  fillCell: {
    width: "100%",
    height: "auto",
    maxWidth: "100%",
    justifySelf: "stretch",
  },
  square: {
    position: "relative",
    display: "grid",
    minWidth: 0,
    borderWidth: 0,
    borderStyle: "none",
    padding: 0,
    placeItems: "center",
  },
  /* A square can carry several states; declaration order keeps the CSS
     cascade's winners: destination over selected over last-move. */
  squareLast: {
    boxShadow: "inset 0 0 0 999px rgb(127 146 116 / 0.32)",
  },
  squareSelected: {
    boxShadow: "inset 0 0 0 4px var(--color-vermilion)",
    zIndex: 1,
  },
  squareDestination: {
    boxShadow:
      "inset 0 0 0 999px color-mix(in srgb, var(--color-bamboo) 24%, transparent)",
  },
  piece: {
    position: "relative",
    zIndex: 2,
    userSelect: "none",
  },
  destination: {
    position: "absolute",
    zIndex: 3,
    width: "22%",
    aspectRatio: "1",
    borderWidth: "2px",
    borderStyle: "solid",
    borderColor: "rgb(33 55 44 / 0.48)",
    borderRadius: "50%",
    backgroundColor: "rgb(127 146 116 / 0.72)",
    pointerEvents: "none",
  },
  coordinate: {
    position: "absolute",
    zIndex: 4,
    color: "color-mix(in srgb, var(--color-ink) 68%, transparent)",
    fontSize: "clamp(0.5rem, 1vw, 0.72rem)",
    fontWeight: 800,
    pointerEvents: "none",
  },
  rankCoordinate: {
    top: "0.2rem",
    left: "0.25rem",
  },
  fileCoordinate: {
    right: "0.25rem",
    bottom: "0.15rem",
  },
})
