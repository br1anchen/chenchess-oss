import * as stylex from "@stylexjs/stylex"

const reduceMotion = "@media (prefers-reduced-motion: reduce)"
const narrow = "@media (max-width: 520px)"

/* The picker paints its own frame stroke. The keyframes are declared here
   rather than imported from watercolor.styles: StyleX resolves cross-file
   style imports through module resolution the Coach App's six artifact builds
   do not configure, and a keyframe compiles to its own rule anyway. */
const paintFrame = stylex.keyframes({
  "0%": {
    opacity: 0,
    maskSize:
      "0 var(--watercolor-brush-weight), var(--watercolor-brush-weight) 0, 0 var(--watercolor-brush-weight), var(--watercolor-brush-weight) 0",
  },
  "10%": { opacity: 0.82 },
  "34%": {
    maskSize:
      "100% var(--watercolor-brush-weight), var(--watercolor-brush-weight) 0, 0 var(--watercolor-brush-weight), var(--watercolor-brush-weight) 0",
  },
  "56%": {
    maskSize:
      "100% var(--watercolor-brush-weight), var(--watercolor-brush-weight) 100%, 0 var(--watercolor-brush-weight), var(--watercolor-brush-weight) 0",
  },
  "80%": {
    maskSize:
      "100% var(--watercolor-brush-weight), var(--watercolor-brush-weight) 100%, 100% var(--watercolor-brush-weight), var(--watercolor-brush-weight) 0",
  },
  "100%": { maskSize: "var(--watercolor-brush-sizes)" },
})

/**
 * The review navigator family: the moment carousel, the compact ply
 * navigator, and the evaluation graph. The graph's watercolor skin is a
 * variant here rather than a descendant selector reaching in from the card.
 */
export const pickerStyles = stylex.create({
  picker: {
    "--review-moment-frame":
      "color-mix(in srgb, var(--color-text-primary) 56%, var(--color-text-disabled))",
    position: "relative",
    isolation: "isolate",
    display: "grid",
    minWidth: 0,
    overflow: "hidden",
    gap: "0.5rem",
    borderWidth: 0,
    borderStyle: "none",
    borderRadius: "0.72rem 0.82rem 0.74rem 0.86rem",
    padding: "1rem 0.75rem 0.75rem",
    backgroundColor:
      "color-mix(in srgb, var(--color-paper-raised) 94%, transparent)",
    backgroundImage:
      "radial-gradient(ellipse at 8% 0%, color-mix(in srgb, var(--color-paper-raised) 98%, transparent), transparent 42%)",
    boxShadow:
      "0 0.7rem 1.7rem color-mix(in srgb, var(--color-ink) 7%, transparent)",
    color: "var(--color-text-primary)",
    colorScheme: "light",
    /* The picker shares the watercolor surface frame so it sits beside the
       board as one painted family. Title and moment share this frame. */
    "::before": {
      "--watercolor-brush-weight": "0.42rem",
      position: "absolute",
      zIndex: 3,
      inset: "0.08rem",
      backgroundColor: "var(--review-moment-frame)",
      content: '""',
      opacity: 0.82,
      pointerEvents: "none",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
      animationName: paintFrame,
      animationDuration: { default: "640ms", [reduceMotion]: "0s" },
      animationTimingFunction: "cubic-bezier(0.23, 1, 0.32, 1)",
      animationFillMode: "both",
    },
  },
  /* Inside the framed card. With stamps: title left, stamps right.
     Without stamps: Critical moments N/M is one centered line. */
  header: {
    display: "flex",
    alignItems: "baseline",
    justifyContent: "space-between",
    flexWrap: "wrap",
    gap: "0.25rem 0.75rem",
    minWidth: 0,
    width: "100%",
  },
  headerCentered: {
    justifyContent: "center",
    textAlign: "center",
  },
  title: {
    display: "flex",
    minWidth: "12ch",
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "12ch",
    margin: 0,
    alignItems: "baseline",
    justifyContent: "center",
    gap: "0.375rem",
    overflowWrap: "anywhere",
    fontFamily: "var(--font-family-heading)",
    fontSize: "1rem",
    fontWeight: 620,
    lineHeight: 1,
    whiteSpace: "normal",
    textAlign: "center",
  },
  titleCentered: {
    flexGrow: 0,
    flexShrink: 0,
    flexBasis: "auto",
    width: "auto",
    justifyContent: "center",
    textAlign: "center",
  },
  count: {
    color: "var(--color-text-disabled)",
    fontFamily: "var(--font-family-heading)",
    fontSize: "0.72rem",
    fontWeight: 650,
    letterSpacing: "0.02em",
    fontVariantNumeric: "tabular-nums",
  },
  options: {
    display: "flex",
    overflowX: "auto",
    gap: 0,
    padding: 0,
    overscrollBehaviorInline: "contain",
    scrollPaddingInline: 0,
    scrollSnapType: "inline mandatory",
    scrollbarWidth: "none",
    touchAction: "pan-x",
    "::-webkit-scrollbar": { display: "none" },
  },
  slide: {
    display: "grid",
    minWidth: "100%",
    alignContent: "start",
    flexBasis: "100%",
    flexGrow: 0,
    flexShrink: 0,
    gap: "0.5rem",
    padding: "0.125rem 0.25rem",
    scrollSnapAlign: "start",
    scrollSnapStop: "always",
  },
  /* In compound mode the slide carries the board and the call to action, so
     the whole review card travels with the swipe. */
  body: {
    display: "grid",
    minWidth: 0,
    gap: "0.75rem",
  },
  row: {
    display: "grid",
    width: "100%",
    alignItems: "center",
    gridTemplateColumns: "2.35rem minmax(0, 1fr) 2.35rem",
    gap: "0.375rem",
  },
  /* The selector widget's density: the picker shares a host viewport with a
     board, so every row loses a few tenths. */
  pickerCompact: {
    gap: "0.5rem",
    minWidth: 0,
    overflow: "hidden",
    padding: "0.75rem 0.5rem 0.75rem",
  },
  rowCompact: {
    gridTemplateColumns: "1.85rem minmax(0, 1fr) 1.85rem",
    gap: "0.25rem",
  },
  slideCompact: {
    gap: "0.5rem",
    padding: "0.125rem",
  },
  bodyCompact: {
    gap: "0.5rem",
  },
  momentCardCompact: {
    minHeight: "3.4rem",
    gap: "0.5rem",
    padding: "0.5rem 0.75rem",
  },
})

/**
 * The navigator's round blot buttons. One authored circle at a stable square
 * aspect ratio — a rectangular outline mask collapses into a stray mark at
 * icon-button sizes. The hover craft rides a custom property because a
 * pseudo-element cannot read its own parent's `:hover`.
 */
export const navButtonStyles = stylex.create({
  base: {
    "--review-nav-blot": "rotate(-2deg)",
    /* The circle is the control: the carousel passes `hoverWash="none"`, so
       no stroke sweeps and nothing clips the blot. On an icon blot that wash
       was a 4px line, and a tap left Next looking like a stray arrow until
       hover cleared. */
    width: "2.35rem",
    height: "2.35rem",
    color: "var(--color-text-primary)",
    "::before": {
      inset: "-0.1rem",
      backgroundColor:
        "color-mix(in srgb, var(--color-text-primary) 86%, var(--color-text-disabled))",
      backgroundImage: "none",
      opacity: 0.88,
      maskImage: "var(--watercolor-brush-circle)",
      maskPosition: "center",
      maskSize: "100% 100%",
      transform: "var(--review-nav-blot)",
    },
    "::after": {
      inset: "0.24rem",
      borderRadius: "50%",
      backgroundColor:
        "color-mix(in srgb, var(--color-background-surface) 82%, transparent)",
    },
  },
  previous: {
    "--review-nav-blot": {
      default: "rotate(-2deg)",
      ":hover:not(:disabled)": "rotate(1deg) scale(1.06)",
    },
  },
  next: {
    "--review-nav-blot": {
      default: "rotate(3deg) scaleX(-1)",
      ":hover:not(:disabled)": "rotate(-1deg) scale(-1.06, 1.06)",
    },
  },
  compact: {
    width: "1.85rem",
    height: "1.85rem",
  },
  icon: {
    width: "1rem",
    height: "1rem",
    strokeWidth: 2.35,
  },
})

/** The compact ply navigator that rides above a board. */
export const navigatorStyles = stylex.create({
  navigator: {
    display: "grid",
    minWidth: 0,
    alignItems: "center",
    gridTemplateColumns: "2rem minmax(0, 1fr) auto 2rem",
    gap: "0.375rem",
    borderBlockWidth: 0,
    borderBlockStyle: "none",
    paddingBlock: "0.375rem",
    color: "var(--color-text-primary)",
  },
  navigatorWithDiscuss: {
    gridTemplateColumns: "2rem minmax(0, 1fr) auto 2rem auto",
  },
  stepButton: {
    width: "2rem",
    height: "2rem",
  },
  stepIcon: {
    width: "0.9rem",
    height: "0.9rem",
  },
  identity: {
    display: "flex",
    minWidth: 0,
    alignItems: "baseline",
    gap: "0.375rem",
    overflow: "hidden",
    whiteSpace: "nowrap",
  },
  moveLabel: {
    flexBasis: "auto",
    flexGrow: 0,
    flexShrink: 0,
    fontSize: "0.82rem",
  },
  label: {
    minWidth: 0,
    flexBasis: "auto",
    flexGrow: 0,
    flexShrink: 1,
    overflow: "hidden",
    color: "var(--color-text-primary)",
    fontSize: "0.74rem",
    textOverflow: "ellipsis",
  },
  detail: {
    display: { default: null, [narrow]: "none" },
    minWidth: 0,
    flexBasis: "auto",
    flexGrow: 1,
    flexShrink: 1,
    overflow: "hidden",
    color: "var(--color-text-disabled)",
    fontSize: "0.68rem",
    textOverflow: "ellipsis",
  },
  title: {
    display: "flex",
    margin: 0,
    alignItems: "baseline",
    gap: "0.25rem",
    fontFamily: "inherit",
    fontSize: "0.7rem",
    fontWeight: 650,
    whiteSpace: "nowrap",
  },
  count: {
    color: "var(--color-text-disabled)",
    fontVariantNumeric: "tabular-nums",
  },
  discuss: {
    minWidth: "max-content",
    gap: "0.375rem",
    paddingInline: { default: null, [narrow]: "0.5rem" },
    fontSize: "0.7rem",
  },
})

const momentToneColor = {
  improvement: "var(--color-moment-improvement)",
  positive: "var(--color-moment-positive)",
  selected: "var(--color-ink-soft)",
} as const

export type ReviewMomentTone = keyof typeof momentToneColor

/** The measured evaluation plot, and the watercolor skin the review card
 * wraps it in. */
export const graphStyles = stylex.create({
  figure: {
    position: "relative",
    display: "grid",
    minWidth: 0,
    gap: "0.5rem",
    margin: 0,
  },
  figureWatercolor: {
    gap: 0,
  },
  caption: {
    display: "flex",
    alignItems: "end",
    justifyContent: "space-between",
    gap: "1rem",
  },
  plot: {
    position: "relative",
    height: "7rem",
    minWidth: 0,
  },
  plotWatercolor: {
    height: "clamp(8rem, 18vw, 10.5rem)",
  },
  plotSparkline: {
    height: "3.5rem",
  },
  svg: {
    display: "block",
    width: "100%",
    height: "100%",
    borderWidth: 0,
    borderStyle: "none",
    backgroundColor: "var(--color-background-surface)",
    backgroundImage:
      "linear-gradient(180deg, color-mix(in srgb, var(--color-paper-raised) 35%, transparent), transparent 48%)",
  },
  svgWatercolor: {
    borderRadius: 0,
    backgroundColor: "transparent",
    backgroundImage: "none",
    boxShadow: "none",
  },
  line: {
    fill: "none",
    stroke: "var(--color-text-primary)",
    strokeWidth: 2.25,
    vectorEffect: "non-scaling-stroke",
  },
  point: {
    fill: "var(--color-text-primary)",
  },
  zero: {
    stroke: "color-mix(in srgb, var(--color-ink) 20%, transparent)",
    strokeWidth: 1,
  },
  marker: {
    stroke: "var(--color-error)",
    strokeWidth: 1.5,
    vectorEffect: "non-scaling-stroke",
  },
  markerWatercolor: {
    strokeWidth: 2.5,
    strokeLinecap: "round",
    strokeDasharray: "34 2 18 1.5 46",
    opacity: 0.85,
    filter:
      "drop-shadow(0 0 1px color-mix(in srgb, var(--color-vermilion) 40%, transparent))",
  },
  moment: {
    position: "absolute",
    top: 0,
    bottom: 0,
    zIndex: 2,
    display: "block",
    height: "100%",
    borderWidth: 0,
    borderStyle: "none",
    padding: 0,
    backgroundColor: "transparent",
    /* Ivory on the tone-coloured dot: a near-black glyph disappears on the
       navy brush ring. */
    color: "var(--color-background-surface)",
    fontSize: "0.64rem",
    fontWeight: 800,
    textShadow:
      "0 1px 1px color-mix(in srgb, var(--color-ink) 35%, transparent)",
    opacity: { default: null, ":disabled": 0.48 },
    outline: { default: null, ":focus-visible": "none" },
  },
  momentActive: {
    zIndex: 3,
  },
  dot: {
    position: "absolute",
    isolation: "isolate",
    display: "grid",
    width: "1.55rem",
    height: "1.55rem",
    placeItems: "center",
    transform: "translate(-50%, -50%)",
    borderWidth: 0,
    borderStyle: "none",
    borderRadius: "999px",
    backgroundColor: "var(--review-moment-color)",
    boxShadow: "0 3px 12px rgb(0 0 0 / 0.35)",
    pointerEvents: "none",
    transition: {
      default: "width 140ms ease, height 140ms ease, border-radius 140ms ease",
      [reduceMotion]: "none",
    },
    "::after": {
      position: "absolute",
      zIndex: -1,
      inset: "-0.24rem",
      backgroundColor: "var(--color-text-primary)",
      content: '""',
      opacity: 0.76,
      pointerEvents: "none",
      transform: "rotate(-6deg)",
      transition: "inset 140ms ease, opacity 140ms ease, transform 140ms ease",
      mask: "var(--watercolor-brush-circle) center / 100% 100% no-repeat",
    },
  },
  dotActive: {
    width: "2rem",
    height: "2rem",
    borderRadius: "50%",
    outline: "none",
    "::after": {
      inset: "-0.42rem",
      opacity: 1,
      transform: "rotate(-11deg)",
    },
  },
  dotFocus: {
    outline: {
      default: null,
      ":focus-visible":
        "3px solid color-mix(in srgb, var(--color-success) 68%, transparent)",
    },
    outlineOffset: "2px",
  },
  dotWatercolor: {
    width: "1.7rem",
    height: "1.7rem",
    borderWidth: 0,
    borderStyle: "none",
    borderRadius: 0,
    boxShadow: "none",
    filter:
      "drop-shadow(0 1px 3px color-mix(in srgb, var(--color-ink) 26%, transparent))",
    mask: "var(--watercolor-dot) center / 100% 100% no-repeat",
  },
  dotWatercolorActive: {
    width: "2.15rem",
    height: "2.15rem",
    borderRadius: 0,
    outline: 0,
    filter:
      "drop-shadow(0 0 3px color-mix(in srgb, var(--color-vermilion) 60%, transparent)) drop-shadow(0 1px 3px color-mix(in srgb, var(--color-ink) 26%, transparent))",
  },
})

export const momentToneStyles = stylex.create({
  improvement: { "--review-moment-color": momentToneColor.improvement },
  positive: { "--review-moment-color": momentToneColor.positive },
  selected: { "--review-moment-color": momentToneColor.selected },
})
