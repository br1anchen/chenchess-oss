import * as stylex from "@stylexjs/stylex"

const stack = "@media (max-width: 64rem)"
/* A viewport no wider than it is tall by much — a chat host's side panel at
   full height, a tall window. The board is square and capped by its column's
   width, so here the column, not the leftover height, is what limits it. */
const squarish = "@media (min-width: 64.01rem) and (max-aspect-ratio: 4 / 3)"

export const reviewSessionShellStyles = stylex.create({
  page: {
    boxSizing: "border-box",
    minWidth: 0,
    width: "min(112rem, 100%)",
    marginInline: "auto",
    padding: {
      default: "clamp(0.85rem, 2vw, 1.8rem)",
      [stack]: "0.75rem",
    },
    height: {
      default: "100vh",
      [stack]: "auto",
    },
    overflow: {
      default: "hidden",
      [stack]: "visible",
    },
  },
  columns: {
    display: "grid",
    minWidth: 0,
    minHeight: 0,
    /* The board column takes a larger share where the board is width-bound,
       so the height it cannot use goes into the board instead of into the
       margin around it. On a wide viewport the board is capped by height
       already and the extra width would only pad its column, so the default
       stands there. */
    gridTemplateColumns: {
      default: "minmax(0, 1.15fr) minmax(0, 1fr)",
      [squarish]: "minmax(0, 1.5fr) minmax(0, 1fr)",
      [stack]: "minmax(0, 1fr)",
    },
    alignItems: "stretch",
    alignContent: "stretch",
    gap: {
      default: "clamp(0.85rem, 1.6vw, 1.35rem)",
      [stack]: "0",
    },
    flexGrow: 1,
    overflow: {
      default: "hidden",
      [stack]: "visible",
    },
  },
  board: {
    display: "flex",
    minWidth: 0,
    minHeight: 0,
    flexGrow: 1,
    flexDirection: "column",
    overflow: {
      default: "hidden",
      [stack]: "visible",
    },
  },
  boardColumn: {
    display: "flex",
    minWidth: 0,
    minHeight: 0,
    flexGrow: 1,
    flexDirection: "column",
    alignItems: "stretch",
    alignSelf: "stretch",
    width: "100%",
  },
  /* Desktop: the fill is a size container, so 100cqh names the exact leftover
     height and children cannot inflate it. On the 64rem stack the page flows,
     so containment would collapse the fill to 0px — the board sizes from the
     column width instead. */
  boardFill: {
    display: "flex",
    minWidth: 0,
    minHeight: 0,
    flexGrow: { default: 1, [stack]: 0 },
    flexBasis: { default: 0, [stack]: "auto" },
    flexDirection: "column",
    alignItems: "stretch",
    alignSelf: "stretch",
    width: "100%",
    containerType: { default: "size", [stack]: "normal" },
  },
  boardSquare: {
    display: "flex",
    flexDirection: "column",
    /* A square board is capped by the column's width, so a tall or squarish
       viewport leaves height it cannot use. Centred, that slack was split
       into a band above the board and another below it; anchored to the top
       the board sits under its own controls and the leftover collects in one
       place at the foot of the column. */
    justifyContent: "flex-start",
    alignItems: "stretch",
    alignSelf: "stretch",
    width: "100%",
    minWidth: 0,
    minHeight: 0,
    maxWidth: "100%",
    flexGrow: { default: 1, [stack]: 0 },
  },
  boardFillChild: {
    width: "100%",
    minWidth: 0,
    minHeight: 0,
    flexGrow: 0,
    maxWidth: "100%",
  },
  /* The assembly hugs the board. 100cqh caps its edge at the leftover height;
     the eval-bar column's width slack absorbs the frame padding, so the row
     inside always fits. Below the cut 100cqh falls back to the small viewport
     and the min() keeps the width-sized square. */
  boardAssemblyFill: {
    width: "min(100%, 100cqh)",
    marginInline: "auto",
  },
  boardMeta: {
    minWidth: 0,
    flexShrink: 0,
  },
  /* Desktop: the card flexes into the leftover column and only the messages
     scroll. Stack: the page is the scroller, so the card sizes from content —
     a 0 basis in the auto-height column collapses the thread entirely. */
  conversation: {
    display: "flex",
    minWidth: 0,
    minHeight: 0,
    flexGrow: 1,
    flexBasis: { default: 0, [stack]: "auto" },
    flexDirection: "column",
    /* The stack keeps the thread nearly full-bleed: the page padding is the
       only horizontal margin the bubbles get. */
    paddingInline: { default: null, [stack]: "0.125rem" },
    paddingBlock: { default: null, [stack]: "0.5rem" },
  },
  /* Stack chat: the sidebar avatar hides and a copy joins the name row as a
     flex sibling, so avatar, name and the feedback votes center on one line
     and the bubble spans the column. */
  chatMessage: {
    flexDirection: { default: "row", [stack]: "column" },
    alignItems: { default: null, [stack]: "stretch" },
    rowGap: { default: null, [stack]: 0 },
  },
  chatSlotAvatar: {
    display: { default: null, [stack]: "none" },
  },
  chatNameAvatar: {
    display: { default: "none", [stack]: "block" },
    width: "1.5rem",
    height: "1.5rem",
  },
  chatName: {
    alignItems: "center",
  },
  conversationBody: {
    display: "flex",
    minWidth: 0,
    minHeight: 0,
    flexGrow: 1,
    flexDirection: "column",
    /* Clearance for the floating composer button on the stack. */
    paddingBottom: { default: null, [stack]: "3.6rem" },
  },
  conversationThread: {
    minWidth: 0,
    minHeight: 0,
    flexGrow: 1,
    overflowY: { default: "auto", [stack]: "visible" },
    overscrollBehavior: "contain",
  },
  conversationComposer: {
    flexShrink: 0,
  },
  evaluationGraph: {
    display: {
      default: null,
      [stack]: "none",
    },
    minWidth: 0,
  },
  thread: {
    display: "flex",
    minWidth: 0,
    minHeight: 0,
    flexGrow: 1,
    flexDirection: "column",
    overflow: {
      default: "hidden",
      [stack]: "visible",
    },
    gap: {
      default: "0.25rem",
      [stack]: 0,
    },
    paddingTop: 0,
    marginTop: {
      default: null,
      [stack]: 0,
    },
  },
  /* Still worn by the coaching board's Critical moments carousel. */
  pickerFlush: {
    marginTop: {
      default: 0,
      [stack]: "0.375rem",
    },
  },
  graphFrame: {
    position: "relative",
    minWidth: 0,
  },
  momentStepper: {
    position: "absolute",
    zIndex: 2,
    top: "0.3rem",
    right: "0.4rem",
  },
  /* The stack composer: a floating brush button that expands into the input
     when touched, so the thread keeps the column. */
  composerFab: {
    position: "fixed",
    zIndex: 30,
    right: "0.9rem",
    bottom: "0.9rem",
    width: "3.4rem",
    height: "3.4rem",
  },
  composerSheet: {
    position: "fixed",
    zIndex: 31,
    insetInline: "0.7rem",
    bottom: "0.7rem",
    padding: "0.25rem",
    borderRadius: "0.45rem",
    backgroundColor: "var(--color-paper)",
    filter: "drop-shadow(0 0.4rem 1rem rgb(20 43 70 / 0.28))",
  },
  moveListLine: {
    flexWrap: "nowrap",
    overflowX: "auto",
    overflowY: "hidden",
    alignSelf: "stretch",
    minWidth: 0,
    width: "100%",
  },
  /** A branch row is a carousel: each chip is a snap stop, so a flick lands
   * on a whole move rather than half of one. */
  branchCarousel: {
    scrollSnapType: "x proximity",
    scrollPaddingInline: "0.25rem",
  },
  branchCarouselItem: {
    flexShrink: 0,
    scrollSnapAlign: "start",
  },
  boardFace: {
    width: "100%",
    maxWidth: "100%",
    maxHeight: "100%",
    aspectRatio: 1,
  },
  boardAssembly: {
    position: "relative",
    isolation: "isolate",
    width: "100%",
    minWidth: 0,
    padding: "0.25rem",
    "::before": {
      position: "absolute",
      zIndex: 4,
      inset: 0,
      backgroundColor:
        "color-mix(in srgb, var(--color-text-primary) 68%, black)",
      content: '""',
      opacity: 0.92,
      pointerEvents: "none",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
    },
  },
  /* A Critical Moment's chip in the move list wears the moment's tone, the
     same ink the carousel and graph markers used. */
  momentMoveChip: {
    backgroundColor:
      "color-mix(in srgb, var(--review-moment-color, var(--color-ink-soft)) 24%, transparent)",
    boxShadow:
      "inset 0 0 0 1px color-mix(in srgb, var(--review-moment-color, var(--color-ink-soft)) 55%, transparent)",
  },
  moveChip: {
    flexShrink: 0,
    paddingInline: {
      default: "1.25rem",
      [stack]: "1.75rem",
    },
    paddingBlock: "0.5rem",
  },
  /* Stack spacing: the column's own gap already separates the nav, board and
     Discuss bar; these pull the board tight against both neighbours. */
  boardLift: {
    marginTop: {
      default: null,
      [stack]: "-0.35rem",
    },
  },
  boardMetaSnug: {
    marginTop: {
      default: null,
      [stack]: "-0.35rem",
    },
  },
})
