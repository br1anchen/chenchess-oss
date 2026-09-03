import * as stylex from "@stylexjs/stylex"

const compact = "@media (max-width: 860px)"

export const publicUtilityStyles = stylex.create({
  main: {
    display: "grid",
    gap: "1.5rem",
    width: "min(42rem, calc(100% - 2rem))",
    marginInline: "auto",
    paddingBlock: "2rem 4rem",
  },
  articleCopy: {
    display: "grid",
    gap: "0.75rem",
  },
})

export const notFoundStyles = stylex.create({
  actions: {
    paddingTop: "0.5rem",
  },
  note: {
    margin: 0,
    maxWidth: "36rem",
    [compact]: {
      textAlign: "center",
    },
  },
})
