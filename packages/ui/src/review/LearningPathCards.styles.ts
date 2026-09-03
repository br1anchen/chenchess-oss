import * as stylex from "@stylexjs/stylex"

const narrow = "@media (max-width: 520px)"

/** The Learning Plan's craft: the plan list, one card per idea, and the two
 * resource stages beneath it. */
export const learningStyles = stylex.create({
  plan: {
    display: "grid",
    gap: "0.5rem",
  },
  card: {
    display: "grid",
    gap: "0.5rem",
    padding: "0.75rem",
  },
  /** The widget reading, where the plan shares the card with a board. */
  cardCompact: {
    gap: "0.375rem",
    padding: "0.5rem",
  },
  header: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "0.5rem",
  },
  idea: {
    minWidth: 0,
    flexBasis: "auto",
    flexGrow: 1,
    flexShrink: 1,
    margin: 0,
    fontSize: "0.88rem",
    lineHeight: 1.25,
  },
  eyebrow: {
    color: "var(--color-text-disabled)",
    // Inline inside the idea heading: family and metrics are the heading's.
    fontFamily: "inherit",
    fontSize: "inherit",
    fontWeight: 680,
    lineHeight: "inherit",
  },
  stages: {
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(2, minmax(0, 1fr))",
      [narrow]: "minmax(0, 1fr)",
    },
    gap: "0.375rem",
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  stageItem: {
    display: "grid",
    minWidth: 0,
    padding: 0,
    borderWidth: 0,
    borderStyle: "none",
    backgroundColor: "transparent",
  },
  stage: {
    minWidth: 0,
    gap: "0.25rem",
  },
  /** A grid item of the stage link, wearing the link's own type. */
  stageTitle: {
    color: "inherit",
    fontFamily: "inherit",
    fontSize: "inherit",
    fontWeight: "inherit",
    lineHeight: "inherit",
  },
  stageLink: {
    display: "grid",
    minWidth: 0,
    gap: "0.125rem",
    borderRadius: "0.25rem",
    color: "var(--color-success)",
    fontSize: "0.69rem",
    fontWeight: 700,
    lineHeight: 1.25,
    textDecoration: { default: "none", ":hover": "underline" },
    textUnderlineOffset: "0.15rem",
    outline: {
      default: null,
      ":focus-visible":
        "3px solid color-mix(in srgb, var(--color-success) 62%, transparent)",
    },
    outlineOffset: "0.18rem",
  },
  stageLabel: {
    color: "var(--color-text-primary)",
    fontSize: "0.68rem",
  },
  feedback: {
    display: "flex",
    flexBasis: { default: "auto", [narrow]: "100%" },
    flexGrow: { default: 0, [narrow]: 1 },
    flexShrink: { default: 0, [narrow]: 1 },
    alignItems: "center",
    justifyContent: "space-between",
    gap: { default: "0.5rem", [narrow]: "0.25rem" },
  },
  feedbackPrompt: {
    margin: 0,
    color: "var(--color-text-disabled)",
    fontSize: { default: "0.64rem", [narrow]: "0.58rem" },
  },
  feedbackGroup: {
    display: "flex",
    gap: "0.125rem",
  },
  feedbackButton: {
    width: "1.75rem",
    height: "1.75rem",
    color: "var(--color-text-disabled)",
  },
  feedbackButtonPressed: {
    color: "var(--color-text-primary)",
  },
  feedbackAlert: {
    flexBasis: "100%",
    flexGrow: 1,
    flexShrink: 1,
    margin: 0,
    color: "var(--color-error)",
    fontSize: { default: "0.64rem", [narrow]: "0.58rem" },
  },
  icon: {
    width: "0.72rem",
    height: "0.72rem",
  },
})
