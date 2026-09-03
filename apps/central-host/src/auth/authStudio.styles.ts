import { spacingVars } from "@chenchess/ui/theme/tokens.stylex"
import * as stylex from "@stylexjs/stylex"

const compact = "@media (max-width: 860px)"

export const authStudioStyles = stylex.create({
  page: {
    paddingBlock: spacingVars["--spacing-10"],
    paddingInline: spacingVars["--spacing-10"],
    [compact]: {
      paddingBlock: spacingVars["--spacing-4"],
      paddingInline: spacingVars["--spacing-4"],
    },
  },
  /* The sign-in column sits above the mist rather than filling the page. */
  column: {
    marginInline: "auto",
    width: "100%",
  },
})
