import * as stylex from "@stylexjs/stylex"

const stack = "@media (max-width: 64rem)"

export const coachingBoardStyles = stylex.create({
  page: {
    boxSizing: "border-box",
    minWidth: 0,
    width: "min(112rem, 100%)",
    marginInline: "auto",
    padding: {
      default: "clamp(0.85rem, 2vw, 1.8rem)",
      [stack]: "0.75rem",
    },
  },
  target: {
    color: "color-mix(in srgb, var(--color-ink) 72%, transparent)",
  },
  pageTitle: {
    boxSizing: "border-box",
    display: "flex",
    width: "100%",
    maxWidth: "100%",
    justifyContent: "center",
    whiteSpace: "nowrap",
    marginBottom: "0.75rem",
    textTransform: "none",
    fontSize: "1.35rem",
    fontWeight: 640,
    letterSpacing: "0.02em",
  },
  dialogExits: {
    display: "grid",
    minWidth: 0,
    width: "100%",
    gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1fr)",
  },
  importMeta: {
    display: "grid",
    minWidth: 0,
    width: "100%",
    gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1fr)",
    alignItems: "start",
  },
  // The picker stays mounted while the board is shown, so it needs a display
  // that beats the Stack's own `flex` — the `hidden` attribute alone loses to
  // an author-level rule.
  hiddenPane: {
    display: "none",
  },
  momentPicker: {
    minWidth: 0,
    paddingBlock: "1rem",
    paddingInline: "0.75rem",
    backgroundColor:
      "color-mix(in srgb, var(--color-paper-raised) 94%, transparent)",
  },
})
