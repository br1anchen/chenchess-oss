import * as stylex from "@stylexjs/stylex"

const serif = '"Iowan Old Style", "Palatino Linotype", Palatino, Georgia, serif'
const focusRing =
  "3px solid color-mix(in srgb, var(--focus-outline-color) 62%, transparent)"

/** The morning-digest recipe of the watercolor card: serif coverage title,
 * clickable Learning Path priorities, and the host-owned game children. */
export const digestStyles = stylex.create({
  selected: {
    "--watercolor-card-accent": "var(--color-error)",
    "--watercolor-card-frame-opacity": "0.96",
  },
  title: {
    maxWidth: "100%",
    overflow: "hidden",
    fontFamily: serif,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  titleLarge: {
    fontSize: "clamp(1.35rem, 2.6vw, 1.85rem)",
  },
  titleSmall: {
    fontSize: "1.05rem",
  },
  /** The whole-card hit for a list row; the resource links sit above it. */
  hit: {
    position: "absolute",
    zIndex: 2,
    inset: 0,
    margin: 0,
    padding: 0,
    borderWidth: 0,
    borderStyle: "none",
    backgroundColor: "transparent",
    color: "inherit",
    font: "inherit",
    textDecoration: "none",
    cursor: "pointer",
    outline: { default: null, ":focus-visible": focusRing },
    outlineOffset: "-0.55rem",
  },
  source: {
    color: "var(--color-text-disabled)",
  },
  priorities: {
    display: "grid",
    minWidth: 0,
    gap: "0.75rem",
  },
  prioritiesTitle: {
    margin: 0,
    color: "var(--color-text-primary)",
    fontSize: "0.92rem",
    fontWeight: 720,
    letterSpacing: "0.01em",
  },
  /* The coach's read of the day, above the priorities; the homework line
     closes them. Both are body copy, not chrome. */
  summary: {
    margin: 0,
    color: "var(--color-text-secondary)",
    fontSize: "0.95rem",
    lineHeight: 1.6,
  },
  homework: {
    margin: 0,
    color: "var(--color-text-primary)",
    fontSize: "0.9rem",
    fontWeight: 620,
    lineHeight: 1.55,
  },
  games: {
    display: "grid",
    minWidth: 0,
    overflowX: "clip",
    gap: "0.75rem",
  },
})
