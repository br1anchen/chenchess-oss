import * as stylex from "@stylexjs/stylex"

const belowWide = "@media (max-width: 1000px)"
const belowNarrow = "@media (max-width: 860px)"
const belowTiny = "@media (max-width: 520px)"

/**
 * The craft the retired primitives catalog carried, kept where the stories
 * that replaced it can use it. The catalog's page — its paper ground, its
 * landscape wash and its column — did not come with it: the specimens sit on
 * the plain Storybook canvas, spaced like every other story here.
 */
export const catalogStyles = stylex.create({
  /** The breathing room every story in this tree sits in. */
  stage: {
    padding: "2rem",
  },
  /** The same, for a form column that should not run the full canvas. */
  stageNarrow: {
    maxWidth: "26.25rem",
    padding: "2rem",
  },
  swatches: {
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(6, minmax(0, 1fr))",
      [belowNarrow]: "repeat(3, minmax(0, 1fr))",
    },
    gap: "0.75rem",
  },
  swatch: {
    display: "grid",
    minWidth: 0,
    gap: "0.125rem",
  },
  swatchChip: {
    display: "block",
    height: "clamp(5rem, 11vw, 8.5rem)",
    marginBottom: "0.375rem",
    borderRadius: "0.25rem 0.55rem 0.32rem 0.48rem",
    boxShadow: "inset 0 0 0 1px var(--color-border)",
  },
  swatchLabel: {
    overflow: "hidden",
    fontSize: "0.75rem",
    textOverflow: "ellipsis",
  },
  swatchHex: {
    color: "var(--color-text-secondary)",
    fontSize: "0.66rem",
    fontVariantNumeric: "tabular-nums",
    textTransform: "uppercase",
  },
  positionShowcase: {
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(22rem, 1.08fr) minmax(19rem, 0.92fr)",
      [belowWide]: "minmax(20rem, 1fr) minmax(16rem, 0.78fr)",
      [belowNarrow]: "1fr",
    },
    alignItems: { default: "center", [belowWide]: "start" },
    gap: "clamp(1.5rem, 4vw, 3.5rem)",
  },
  boardSample: {
    minWidth: 0,
    width: { default: null, [belowNarrow]: "min(37rem, 100%)" },
    justifySelf: { default: null, [belowNarrow]: "center" },
  },
  graphSample: {
    display: "grid",
    minWidth: 0,
    gap: "1rem",
  },
  graphCaption: {
    width: "min(31rem, 100%)",
    margin: 0,
    paddingInline: "0.25rem",
    color: "var(--color-text-secondary)",
    fontSize: "0.7rem",
    lineHeight: 1.5,
  },
  boardWithEvaluation: {
    display: "grid",
    minWidth: 0,
    gridTemplateColumns: "auto minmax(0, 1fr)",
    alignItems: "stretch",
    gap: { default: "0.75rem", [belowTiny]: "0.375rem" },
  },
  evaluationBar: {
    height: "100%",
    minHeight: 0,
    width: { default: null, [belowTiny]: "2.4rem" },
    flexBasis: { default: null, [belowTiny]: "2.4rem" },
    padding: { default: null, [belowTiny]: "0.375rem" },
  },
  moveNav: {
    width: "100%",
    padding: "0.25rem 0",
    backgroundColor: "transparent",
    boxShadow: "none",
  },
  cardGrid: {
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(2, minmax(0, 1fr))",
      [belowNarrow]: "1fr",
    },
    alignItems: "start",
    gap: "1rem",
  },
  values: {
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(4, minmax(0, 1fr))",
      [belowWide]: "repeat(2, minmax(0, 1fr))",
      [belowTiny]: "1fr",
    },
    gap: "0.75rem",
  },
  valuesTitle: {
    margin: 0,
    fontFamily: "var(--font-family-seal)",
    fontSize: "0.92rem",
  },
  valuesDetail: {
    margin: "0.125rem 0 0",
    color: "var(--color-text-secondary)",
    fontSize: "0.68rem",
    lineHeight: 1.4,
  },
  symbolRow: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: "0.75rem",
    padding: "1rem",
    borderBlockWidth: "1px",
    borderBlockStyle: "solid",
    borderBlockColor: "var(--color-border)",
  },
  momentCarouselSample: {
    width: "min(64rem, 100%)",
  },
  /* Brand icons are viewBox-only SVGs; an <img> in a Token's icon slot has no
     intrinsic size to fall back on and stretches to whatever the slot gives
     it. The catalog page had the same unsized markup — the seal came out as a
     smeared strip. */
  tokenIcon: {
    display: "block",
    width: "1.15rem",
    height: "1.15rem",
    objectFit: "contain",
  },
})

/** The palette chips, keyed the way the swatch data attribute used to be. */
/**
 * The Brushwork / TornSilhouettes staging: brand tokens and layout boxes so
 * the artwork stories compose from the same system they document, instead of
 * hand-rolled `<div style>` islands.
 */
export const brushworkStyles = stylex.create({
  heading: {
    margin: 0,
  },
  underline: {
    display: "block",
    height: "1.6rem",
    marginTop: "0.375rem",
    color: "var(--color-ink)",
  },
  strokeAccent: {
    display: "block",
    height: "0.9rem",
    color: "var(--color-vermilion)",
  },
})

export const silhouetteStyles = stylex.create({
  slab: {
    display: "block",
    padding: "1.5rem 2rem",
    color: "var(--color-paper)",
    backgroundImage:
      "linear-gradient(135deg, var(--color-ink), var(--color-ink-deep))",
    transition: "clip-path 480ms cubic-bezier(0.23, 1, 0.32, 1)",
  },
  slabA: { clipPath: "var(--watercolor-shape-splash-a)" },
  slabB: { clipPath: "var(--watercolor-shape-splash-b)" },
  panel: { clipPath: "var(--watercolor-shape-panel)" },
  blot: {
    display: "block",
    width: "7rem",
    height: "7rem",
    backgroundColor: "var(--color-vermilion)",
  },
  blotA: { clipPath: "var(--watercolor-shape-blot-a)" },
  blotB: { clipPath: "var(--watercolor-shape-blot-b)" },
})

export const swatchToneStyles = stylex.create({
  paper: {
    backgroundColor: "var(--color-paper)",
    backgroundImage:
      "radial-gradient(ellipse at 18% 12%, color-mix(in srgb, var(--color-mist) 16%, transparent), transparent 40%)",
  },
  watercolor: {
    backgroundColor: "var(--color-ink)",
    backgroundImage:
      "radial-gradient(ellipse at 80% 8%, color-mix(in srgb, var(--color-mist) 22%, transparent), transparent 38%)",
  },
  slate: { backgroundColor: "var(--color-ink-soft)" },
  mist: { backgroundColor: "var(--color-mist)" },
  bamboo: { backgroundColor: "var(--color-bamboo)" },
  vermilion: { backgroundColor: "var(--color-vermilion)" },
})
