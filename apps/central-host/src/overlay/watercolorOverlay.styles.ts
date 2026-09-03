import * as stylex from "@stylexjs/stylex"
import { spacingVars } from "@astryxdesign/core/theme/tokens.stylex"

const compact = "@media (max-width: 860px)"
const phone = "@media (max-width: 620px)"

export const watercolorOverlayStyles = stylex.create({
  dialog: {
    inset: 0,
    /* clip, not hidden: visible+hidden computes to overflow-x auto and kills
       the horizontal brush bleed; visible+clip is a legal pair. */
    overflowX: "visible",
    overflowY: "clip",
    margin: "auto",
    paddingInline: { default: "1.5rem", [phone]: "0.25rem" },
    [compact]: {
      width: "min(36rem, calc(100vw - 0.5rem))",
      maxWidth: "calc(100vw - 0.5rem)",
    },
  },
  card: {
    minHeight: 0,
    maxHeight: "100%",
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "auto",
    overflowX: "visible",
    overflowY: "clip",
    /* The comfortable density's clamp() floors at 28px a side; a phone has no
       28px to spend, so the card's own inset drops to 8px there. */
    "--watercolor-card-pad-x": { default: null, [phone]: "0.5rem" },
  },
  body: {
    boxSizing: "border-box",
    minWidth: 0,
    minHeight: 0,
    width: "100%",
    maxWidth: "100%",
    maxHeight: { default: "min(28rem, 60vh)", [compact]: "min(24rem, 50dvh)" },
    overflowX: "visible",
    overflowY: "auto",
    overscrollBehavior: "contain",
    paddingInline: { default: "1.25rem", [phone]: "0.25rem" },
    paddingBottom: `calc(${spacingVars["--spacing-7"]} + env(safe-area-inset-bottom, 0px))`,
  },
})
