import * as stylex from "@stylexjs/stylex"
import { spacingVars } from "@astryxdesign/core/theme/tokens.stylex"

const compact = "@media (max-width: 1000px)"
const phone = "@media (max-width: 620px)"

/** Desktop columns are shared: ChatGPT | Claude | account, then games
 * across the page, then digest | calendar on the same tracks. */
export const dashboardWorkspaceStyles = stylex.create({
  shell: {
    display: "grid",
    width: "100%",
    minWidth: 0,
    gap: "0.75rem",
    alignItems: "stretch",
    gridTemplateColumns: {
      default: "minmax(0, 1fr) minmax(0, 1fr) minmax(15rem, 17.5rem)",
      [compact]: "minmax(0, 1fr) minmax(0, 1fr)",
    },
    gridTemplateAreas: {
      default: `
        "chatgpt claude account"
        "games games games"
        "tabs tabs tabs"
        "digest digest calendar"
      `,
      [compact]: `
        "chatgpt claude"
        "account account"
        "games games"
        "tabs tabs"
        "digest digest"
      `,
    },
  },
  /** Imported Games hides the archive column; the body takes the full row. */
  shellFullDigest: {
    gridTemplateAreas: {
      default: `
        "chatgpt claude account"
        "games games games"
        "tabs tabs tabs"
        "digest digest digest"
      `,
      [compact]: `
        "chatgpt claude"
        "account account"
        "games games"
        "tabs tabs"
        "digest digest"
      `,
    },
  },
  tabs: {
    gridArea: "tabs",
    minWidth: 0,
  },
  hostGrid: {
    display: "contents",
  },
  chatgpt: {
    gridArea: "chatgpt",
    minWidth: 0,
    height: "100%",
  },
  claude: {
    gridArea: "claude",
    minWidth: 0,
    height: "100%",
  },
  account: {
    display: "flex",
    flexDirection: "column",
    gridArea: "account",
    gap: "0.75rem",
    minWidth: 0,
    height: "100%",
  },
  games: {
    gridArea: "games",
    gridColumn: "1 / -1",
    minWidth: 0,
    width: "100%",
  },
  digest: {
    display: "grid",
    gridArea: "digest",
    gap: "0.75rem",
    minWidth: 0,
    overflowX: "clip",
  },
  digestNav: {
    display: { default: "none", [compact]: "block" },
    minWidth: 0,
  },
  calendar: {
    display: { default: "block", [compact]: "none" },
    gridArea: "calendar",
    minWidth: 0,
  },
  hostCard: {
    boxSizing: "border-box",
    height: "100%",
    minWidth: 0,
    /* hostCard is merged last onto WatercolorCard. StyleX keeps one class
       per property, so compact pad vars must restate comfortable defaults
       or desktop padding collapses to 0. */
    "--watercolor-card-pad-y": {
      default: "clamp(1.75rem, 3.6vw, 2.45rem)",
      [compact]: "0.7rem",
    },
    "--watercolor-card-pad-x": {
      default: "clamp(1.75rem, 3.6vw, 2.45rem)",
      [compact]: "0.7rem",
    },
  },
  hostBody: {
    flex: 1,
    minWidth: 0,
    justifyContent: "space-between",
    gap: {
      default: spacingVars["--spacing-3"],
      [compact]: "0.5rem",
    },
  },
  hostBrand: {
    flexDirection: { default: "column", [compact]: "row" },
    alignItems: { default: "flex-start", [compact]: "center" },
    gap: { default: spacingVars["--spacing-2"], [compact]: "0.375rem" },
  },
  hostMark: {
    display: "flex",
    color: "var(--color-text-primary)",
    fontSize: { default: "1.65rem", [compact]: "1.2rem" },
    lineHeight: 1,
  },
  hostCopy: {
    display: { default: "block", [compact]: "none" },
  },
  hostAction: {
    width: { default: "auto", [compact]: "100%" },
  },
  /** A connected Player reads their own profile as settings, not as the day's
   * coaching: on a phone the card pushed Recent games and the digest below the
   * fold, and everything it offers is in the header's Account settings. The
   * disconnected pointer in the same slot stays, because it is how that Player
   * gets back to connection setup. */
  connectedProfileCard: {
    display: { default: "block", [phone]: "none" },
  },
  accountCard: {
    flex: 1,
    minHeight: 0,
    height: "100%",
    paddingBottom: `calc(var(--watercolor-card-pad-y) + ${spacingVars["--spacing-5"]})`,
  },
})

export const importedGamesStyles = stylex.create({
  page: {
    minWidth: 0,
    width: "100%",
    overflowX: "clip",
  },
  card: {
    minWidth: 0,
    width: "100%",
    maxWidth: "100%",
  },
  fields: {
    display: "flex",
    minWidth: 0,
    width: "100%",
    alignItems: "end",
    flexWrap: "wrap",
    gap: "0.75rem",
  },
  pair: {
    minWidth: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: { default: "16rem", [phone]: "100%" },
    alignItems: "start",
  },
  field: {
    minWidth: 0,
    width: "100%",
  },
})

export const recentGamesStyles = stylex.create({
  card: {
    width: "100%",
  },
  scroller: {
    display: "flex",
    minWidth: 0,
    margin: 0,
    padding: "0 0 0.25rem",
    gap: "0.75rem",
    overflowX: "auto",
    listStyle: "none",
    scrollSnapType: "x mandatory",
  },
  item: {
    flex: "0 0 clamp(5.4rem, 28vw, 8.4rem)",
    width: "clamp(5.4rem, 28vw, 8.4rem)",
    minWidth: 0,
    listStyle: "none",
    scrollSnapAlign: "start",
  },
  tile: {
    position: "relative",
    display: "block",
    width: "100%",
    minWidth: 0,
    minHeight: 0,
    height: "auto",
    padding: "0.375rem",
    overflow: "visible",
    transform: { default: "none", ":hover": "none", ":active": "none" },
  },
  board: {
    position: "relative",
    zIndex: -2,
    width: "100%",
    minWidth: 0,
    aspectRatio: "1",
    pointerEvents: "none",
  },
  chessboard: {
    width: "100%",
    minWidth: 0,
  },
  caption: {
    display: "block",
    marginTop: "0.25rem",
    fontSize: "0.68rem",
    fontWeight: 650,
    lineHeight: 1.25,
  },
})
