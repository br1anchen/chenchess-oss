import * as stylex from "@stylexjs/stylex"

/**
 * The watercolor craft, authored as StyleX.
 *
 * Every rule that used to live in `watercolor.css` as an unlayered
 * `chen-watercolor-*` class is a style object here, applied by the wrapper in
 * `watercolor.tsx` via `stylex.props` (native elements) or `xstyle` (Astryx
 * components). The `chen-watercolor-*` class names survive as structural
 * hooks for per-surface layout CSS and tests — they carry no visuals.
 *
 * The mask tokens (`--watercolor-brush-frame`, `--watercolor-brush-sizes`,
 * `--watercolor-control-frame*`, `--watercolor-dot`, `--watercolor-splash`,
 * `--watercolor-brush-circle`) are defined in `theme/chenTokens.css`, because
 * their `url()` values resolve relative to a stylesheet. They compose through
 * `--watercolor-brush-weight`, which the styles below set per primitive.
 *
 * Parent-state pseudo-element craft (a hover that repaints the `::before`
 * stroke) rides on custom properties: the parent flips the property under its
 * own pseudo-class, the pseudo-element reads it.
 */

const reduceMotion = "@media (prefers-reduced-motion: reduce)"
const compactNav = "@media (max-width: 520px)"
const phone = "@media (max-width: 620px)"

/**
 * The torn-silhouette gate. Where `clip-path: shape()` exists, a surface's
 * paper is a generated torn slab (`theme/generated/watercolorShapes.css`)
 * instead of a rectangle — the Three-Kingdoms message-panel read. Everywhere
 * else the current rectangular craft stays, untouched.
 */
const supportsTornSilhouette =
  "@supports (clip-path: shape(from 0% 0%, line to 100% 100%))"

export const paintFrame = stylex.keyframes({
  "0%": {
    opacity: 0,
    maskSize:
      "0 var(--watercolor-brush-weight), var(--watercolor-brush-weight) 0, 0 var(--watercolor-brush-weight), var(--watercolor-brush-weight) 0",
  },
  "10%": {
    opacity: "var(--watercolor-card-frame-opacity, 0.82)",
  },
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
  "100%": {
    maskSize: "var(--watercolor-brush-sizes)",
  },
})

export const spin = stylex.keyframes({
  to: { transform: "rotate(1turn)" },
})

/**
 * A left-to-right brush wipe: the artwork mask stays put while a soft-edged
 * gradient layer grows across it, and `mask-composite: intersect` reveals the
 * ink the way a loaded brush lays it down (the objectBoundingBox clip-reveal
 * trick, done with CSS mask layers).
 */
export const paintSweep = stylex.keyframes({
  from: {
    maskSize: "100% 100%, 0% 100%",
  },
  to: {
    maskSize: "100% 100%, 220% 100%",
  },
})

/**
 * The calligraphy draw-on: a guide stroke with `pathLength=1` sweeps its
 * dashoffset to zero inside an alpha mask of the brush artwork, so the ink
 * appears along the stroke's own direction rather than behind a straight
 * wipe.
 */
export const drawInk = stylex.keyframes({
  from: { strokeDashoffset: 1 },
  to: { strokeDashoffset: 0 },
})

export const buttonStyles = stylex.create({
  base: {
    "--watercolor-button-fill": "linear-gradient(transparent, transparent)",
    "--watercolor-button-inner": "transparent",
    "--watercolor-button-inner-inset": "0.08rem 0.12rem",
    "--watercolor-button-shadow": "none",
    "--watercolor-button-stroke-opacity": "1",
    /* The hover switch, read by the wash element (see `hoverWashStyles`):
       `1` while the control is hovered or focused, `0` otherwise. Each wash
       kind multiplies it by its own strength. The sweep below is a registered
       custom property (`chenTokens.css`), so it interpolates and the
       background that reads it repaints each frame — that travel is the brush
       stroke. Reduced motion drops the whole transition, leaving the
       highlight without the journey. */
    "--watercolor-hover-on": {
      default: "0",
      ":hover": "1",
      ":focus-visible": "1",
      ":disabled": "0",
    },
    "--watercolor-hover-sweep": {
      default: "0%",
      /* Past 100%: the gradient's solid ink runs to 80% of its own width and
         the wet tip past that, so the stroke only covers the far edge once it
         has travelled beyond it — and the tip runs off rather than parking. */
      ":hover": "165%",
      ":focus-visible": "165%",
      ":disabled": "0%",
    },
    /* The card-sized alternative to the sweep: how far the ink drop has
       spread through the paper. Read as a mask size by `hoverWashStyles.bloom`
       and registered beside the sweep, for the same reason — an unregistered
       property has no type to interpolate, so the blot would snap open. It
       starts wide enough to be a drop rather than a dot, and stops just short
       of the card: grown past it the blot's ragged rim leaves the box
       entirely and the wash goes back to being the flat scrim a hover on a
       card is not supposed to be. */
    "--watercolor-hover-bloom": {
      default: "12%",
      ":hover": "95%",
      ":focus-visible": "95%",
      ":disabled": "12%",
    },
    position: "relative",
    isolation: "isolate",
    display: "inline-flex",
    minWidth: 0,
    alignItems: "center",
    justifyContent: "center",
    gap: "0.5rem",
    overflow: "visible",
    borderWidth: 0,
    borderStyle: "none",
    borderRadius: 0,
    backgroundColor: "transparent",
    fontFamily: "var(--font-family-heading)",
    fontWeight: 600,
    letterSpacing: "0.012em",
    lineHeight: 1,
    whiteSpace: "nowrap",
    textDecoration: "none",
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    filter: { default: null, ":disabled": "saturate(0.42)" },
    opacity: { default: null, ":disabled": 0.48 },
    outline: {
      default: "none",
      ":focus-visible":
        "3px solid color-mix(in srgb, var(--focus-outline-color) 68%, transparent)",
    },
    outlineOffset: "3px",
    transform: {
      default: null,
      ":hover": "translateY(-1px)",
      ":active": "translateY(1px) scale(0.98)",
      ":disabled": "none",
    },
    /* The control's own answer — frame, lift, paper — lands in 160ms
       whatever the brush is doing. The label crosses with the ink: it
       turns to paper once the slab has reached the first glyph (about a
       quarter of the way in, ~80ms on the sweep's curve) and is paper by
       the time the slab has crossed it. The exit is the fast phase: the
       wash fades out in 140ms, so the sweep and the label come back with
       it rather than trailing a retreat nobody can see. The stroke's
       curve carries speed through the middle and eases out at the wet
       tip. The bloom is a spread rather than a travelling front, so it
       takes a plain ease-out and a little longer than the first frame
       the eye lands on. */
    /* A transition reads the timing of the state it is heading into, so the
       two segments that differ between arriving and leaving ride custom
       properties flipped under the same pseudo-classes. */
    "--watercolor-label-transition": {
      default: "100ms ease 20ms",
      ":hover": "60ms ease 80ms",
      ":focus-visible": "60ms ease 80ms",
    },
    "--watercolor-sweep-transition": {
      default: "160ms ease",
      ":hover": "420ms cubic-bezier(0.32, 0, 0.24, 1)",
      ":focus-visible": "420ms cubic-bezier(0.32, 0, 0.24, 1)",
    },
    transition: {
      default:
        "color var(--watercolor-label-transition), background-color 160ms ease, box-shadow 160ms ease, transform 160ms ease, --watercolor-hover-sweep var(--watercolor-sweep-transition), --watercolor-hover-bloom 320ms cubic-bezier(0.25, 0.46, 0.45, 0.94)",
      [reduceMotion]: "none",
    },
    WebkitTapHighlightColor: "transparent",
    /* Three layers under the label, painted back to front: the brushed fill
       (-3), the inner paper (-2), and the hover wash (-1, an element — see
       `hoverWashStyles`). Inline content paints above every negative layer, so
       the label stays legible while the stroke sweeps in beneath it. */
    "::before": {
      position: "absolute",
      zIndex: -3,
      inset: "-0.16rem -0.3rem",
      backgroundImage: "var(--watercolor-button-fill)",
      content: '""',
      filter: "var(--watercolor-button-shadow)",
      opacity: "var(--watercolor-button-stroke-opacity)",
      pointerEvents: "none",
      transition:
        "filter 160ms ease, opacity 120ms ease, background-color 180ms ease",
      mask: "var(--watercolor-control-frame) center / 100% 100% no-repeat",
    },
    "::after": {
      position: "absolute",
      zIndex: -2,
      inset: "var(--watercolor-button-inner-inset)",
      borderRadius: "0.8rem 0.86rem 0.82rem 0.76rem",
      backgroundColor: "var(--watercolor-button-inner)",
      content: '""',
      pointerEvents: "none",
      transition: "background-color 180ms ease",
    },
  },
  /* Pale stroke controls only (`buttonCraft` in `watercolor.tsx`). Where the
     slab has landed the resting frame is cut away, so even while it is
     fading out it is never underneath brushed ink. A filled control wears
     both passes at once, and a bloom control's frame is its identity with
     the drop landing over it — neither takes this. */
  strokeClip: {
    "::before": {
      clipPath: "inset(0 0 0 min(100%, var(--watercolor-hover-sweep, 0%)))",
    },
  },
  primary: {
    /* Hover does not recolour: it is the same ink, pulled once more. The
       brush artwork carries partial alpha, so a stroke that *replaced* the
       fill would read lighter than the resting slab — the control would look
       like it had been rubbed out. It lays over instead (see the `::before`
       clip below), and the second pull is a shade deeper than the first. */
    "--watercolor-hover-ink":
      "color-mix(in srgb, var(--color-text-primary) 74%, var(--color-ink-deep))",
    "--watercolor-hover-strength": "1",
    "--watercolor-button-fill":
      "linear-gradient(180deg, transparent 55%, rgb(0 0 0 / 0.16)), radial-gradient(ellipse at 22% 8%, rgb(255 255 255 / 0.1), transparent 46%), linear-gradient(96deg, color-mix(in srgb, var(--color-text-primary) 96%, black), var(--color-text-primary) 60%, color-mix(in srgb, var(--color-text-secondary) 30%, var(--color-text-primary)))",
    "--watercolor-button-shadow": {
      default:
        "drop-shadow(0 0.5rem 0.7rem color-mix(in srgb, var(--color-ink) 20%, transparent))",
      ":hover":
        "drop-shadow(0 0.68rem 0.9rem color-mix(in srgb, var(--color-ink) 26%, transparent))",
    },
    color: "var(--color-background-surface)",
  },
  secondary: {
    /* Paper button: its identity is the frame. On hover the brush is
       loaded over it and the frame leaves as the slab lands — one gesture,
       so the control is never wearing the box and the brush together. The
       label crosses to paper as the ink reaches it (timing in `base`), and
       the frame comes back only where the retreating front has uncovered
       it. */
    "--watercolor-hover-ink":
      "color-mix(in srgb, var(--color-text-primary) 74%, var(--color-ink-deep))",
    "--watercolor-hover-strength": "1",
    "--watercolor-brush-weight": "0.22rem",
    "--watercolor-button-inner-inset": "0.08rem 0.12rem",
    "--watercolor-button-fill":
      "linear-gradient(color-mix(in srgb, var(--color-text-primary) 84%, black), color-mix(in srgb, var(--color-text-primary) 84%, black))",
    "--watercolor-button-stroke-opacity": {
      default: "1",
      ":hover": "0",
      ":focus-visible": "0",
    },
    /* The paper clears as the stroke lands: a tint left under full-strength
       ink is what turned the control grey rather than inked. */
    "--watercolor-button-inner": {
      default: "color-mix(in srgb, var(--color-paper-raised) 62%, transparent)",
      ":hover": "transparent",
      ":focus-visible": "transparent",
    },
    color: {
      default: "var(--color-text-primary)",
      ":hover": "var(--color-background-surface)",
      ":focus-visible": "var(--color-background-surface)",
    },
    "::before": {
      inset: "-0.04rem -0.12rem",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
    },
    "::after": {
      borderRadius: "0.24rem 0.3rem 0.22rem 0.28rem",
    },
  },
  quiet: {
    /* Quiet owns no edge, at rest or on hover: the wash travelling under
       the label at a fifth of the ink is the whole gesture, and the label
       stays the darkest thing on it the whole way across. The frame mask
       and fill stay declared for the controls that compose a resting edge
       on top of quiet (`moveNavStyles.jump`). */
    "--watercolor-hover-ink":
      "color-mix(in srgb, var(--color-text-primary) 74%, var(--color-ink-deep))",
    "--watercolor-hover-strength": "0.2",
    "--watercolor-button-inner": "transparent",
    "--watercolor-brush-weight": "0.22rem",
    "--watercolor-button-inner-inset": "0.08rem 0.12rem",
    "--watercolor-button-fill":
      "linear-gradient(color-mix(in srgb, var(--color-text-primary) 62%, black), color-mix(in srgb, var(--color-text-primary) 62%, black))",
    "--watercolor-button-stroke-opacity": "0",
    color: "var(--color-text-primary)",
    "::before": {
      inset: "-0.04rem -0.12rem",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
    },
    "::after": {
      borderRadius: "0.24rem 0.3rem 0.22rem 0.28rem",
    },
  },
  danger: {
    /* See primary: the second pull deepens the vermilion rather than
       replacing it, so destruction never looks rubbed out at the moment of
       commitment. */
    "--watercolor-hover-ink":
      "color-mix(in srgb, var(--color-error) 82%, var(--color-vermilion-shadow))",
    "--watercolor-hover-strength": "1",
    "--watercolor-button-fill":
      "radial-gradient(ellipse at 22% 8%, rgb(255 255 255 / 0.09), transparent 44%), linear-gradient(96deg, color-mix(in srgb, var(--color-error) 86%, var(--color-vermilion-shadow)), var(--color-error) 70%, color-mix(in srgb, var(--color-error) 88%, var(--color-vermilion-highlight)))",
    "--watercolor-button-shadow":
      "drop-shadow(0 0.5rem 0.72rem color-mix(in srgb, var(--color-vermilion) 24%, transparent))",
    color: "var(--color-background-surface)",
  },
  /** Merged after the variant so a disabled button drops its hover craft. */
  disabledCraft: {
    transform: "none",
  },
  sm: {
    minHeight: "2rem",
    padding: "0.5rem 1rem",
    fontSize: "0.75rem",
  },
  md: {
    minHeight: "2.65rem",
    padding: "0.75rem 1.5rem",
    fontSize: "0.875rem",
  },
  lg: {
    minHeight: "3.1rem",
    padding: "0.75rem 1.75rem",
    fontSize: "0.95rem",
  },
  icon: {
    width: "2.65rem",
    height: "2.65rem",
    padding: 0,
  },
  block: {
    width: "100%",
  },
  /* The filled block variants stretch onto the real wide brush stroke; the
     compact blob would smear into a lens at that aspect ratio. */
  blockWideMask: {
    "::before": {
      /* Oversize the artwork vertically and sit low in it, so the dense body
         of the stroke carries the control and the thin top spatter crops
         away. */
      mask: "var(--watercolor-brush-stroke-wide) center 68% / 100% 155% no-repeat",
    },
  },
})

/**
 * The hover highlight: a dry-brush stroke that paints across a control from
 * left to right, the way a loaded brush lays pigment down.
 *
 * A filled button has its identity in its fill, so the wash lays a deeper
 * pull of that same ink over it — a second pass of the brush, not a
 * different colour. A pale button (secondary, outline) has its identity in
 * its frame, so the frame leaves as the slab lands (`buttonStyles.secondary`)
 * and the control is never wearing both; `buttonStyles.strokeClip` cuts the
 * frame away under the ink while it fades. Quiet has no edge at all and
 * takes the travelling stroke at a fifth of the ink. One gesture per control.
 *
 * Two pieces do the work, and neither is `mask-composite`. The **mask** is the
 * artwork alone, so the four frame strips union the way they normally do —
 * intersecting them against a sweep layer would intersect them with *each
 * other* and leave nothing. The **travel** is `background-size`, driven by the
 * registered `--watercolor-hover-sweep`: the background paints only the swept
 * width, and its gradient carries a vermilion wet tip near the leading edge
 * that fades out at the very end. Because the tip is positioned in the
 * gradient rather than in the element, it advances with the paint and then
 * runs off, leaving deepened ink behind it.
 *
 * Reduced motion keeps the highlight and drops the travel.
 */
export const hoverWashStyles = stylex.create({
  wash: {
    position: "absolute",
    zIndex: -1,
    /* Vertical bleed only. A horizontal bleed widens the scrollable overflow
       of whatever lays the control out, which pushes narrow surfaces (the move
       nav at 375px) into a horizontal scrollbar; the artwork's own ragged ends
       give the stroke its brush edge without the extra width. */
    inset: "-0.14rem 0",
    content: '""',
    pointerEvents: "none",
    /* Both values are flipped by the control itself under its own `:hover` /
       `:focus-visible` — StyleX cannot express an ancestor selector, so
       parent-state craft rides on custom properties (see the file header). */
    opacity:
      "calc(var(--watercolor-hover-on, 0) * var(--watercolor-hover-strength, 1))",
    backgroundImage:
      "linear-gradient(90deg, var(--watercolor-hover-ink, currentColor) 0, var(--watercolor-hover-ink, currentColor) 80%, var(--watercolor-hover-tip, var(--watercolor-hover-ink, currentColor)) 93%, transparent 100%)",
    backgroundSize: "var(--watercolor-hover-sweep, 0%) 100%",
    backgroundRepeat: "no-repeat",
    backgroundPosition: "left center",
    mask: "var(--watercolor-brush-stroke-wide) center 68% / 100% 155% no-repeat",
    transition: "opacity 140ms ease",
  },
  /** Card-sized controls take a splash instead of a stroke. A brush slab as
   * wide as a card reads as a fill rather than as a hover — so the ink lands
   * as a drop and ripples out through the paper, the gesture an arriving
   * Coaching Digest makes (`digestTransition.styles.ts`). Sharper than that
   * one: the digest's wash is a drop drying away, so it fades as it grows,
   * while this one answers a pointer and has to be legible the instant it
   * settles. */
  bloom: {
    inset: "-0.3rem",
    /* The ink-blot artwork is the ripple, and it keeps its own shape: the
       bloom sets the width and the height follows the artwork, so a wide
       row shows the drop's middle band with true ragged ends where a size
       stretched to the box was a smear with no edge at all. */
    mask: "var(--watercolor-ink-blot) center / var(--watercolor-hover-bloom) auto no-repeat",
    /* The pigment under the blot fills the box and carries its own falloff,
       so the wash has depth at the centre and thins at the rim — a wet mark
       rather than a flat scrim. It is held near-solid well past halfway,
       which is the difference between this and the digest's soft drop. */
    backgroundImage:
      "radial-gradient(ellipse at center, var(--watercolor-hover-ink, currentColor) 0 58%, color-mix(in srgb, var(--watercolor-hover-ink, currentColor) 62%, transparent) 82%, color-mix(in srgb, var(--watercolor-hover-ink, currentColor) 18%, transparent) 100%)",
    backgroundPosition: "center",
    backgroundSize: "100% 100%",
    /* A wash under a whole card settles far lighter than a stroke along one
       control's edge — the copy on top has to stay the darkest thing in the
       row — and it carries that strength itself rather than inheriting the
       stroke's: a quiet card's fifth-of-the-ink, times the stroke's
       card factor, was 0.044 and read as nothing. The switch is the shared
       hover state; a disabled control renders no wash element at all. */
    opacity:
      "calc(var(--watercolor-hover-on, 0) * var(--watercolor-bloom-strength, 0.12))",
    /* The ripple's own travel is the button's transition on
       `--watercolor-hover-bloom`, which reduced motion drops there; the
       settled tint stays either way. */
    transition: "opacity 150ms ease",
  },
  /** The compact square controls (icon buttons, ply jumps) take the blot: the
   * wide slab would smear into a lens at that aspect ratio. */
  compact: {
    inset: "-0.1rem 0",
    mask: "var(--watercolor-ink-blot) center / 100% 100% no-repeat",
  },
})

export const spinnerStyles = stylex.create({
  spinner: {
    width: "0.9rem",
    height: "0.9rem",
    flexGrow: 0,
    flexShrink: 0,
    borderWidth: "2px",
    borderStyle: "solid",
    borderColor: "currentColor",
    borderRightColor: "transparent",
    borderRadius: "50%",
    animationName: spin,
    animationDuration: { default: "720ms", [reduceMotion]: "1.5s" },
    animationTimingFunction: "linear",
    animationIterationCount: "infinite",
  },
})

export const moveNavStyles = stylex.create({
  nav: {
    display: "flex",
    flexWrap: "nowrap",
    alignItems: "center",
    justifyContent: "stretch",
    width: "100%",
    minWidth: 0,
    gap: { default: "0.5rem 0.5rem", [compactNav]: "0.25rem 0.375rem" },
  },
  ply: {
    minWidth: { default: "4.75rem", [compactNav]: "2.55rem" },
    color: "var(--color-text-disabled)",
    textAlign: "center",
    fontFamily: "var(--font-family-heading)",
    fontSize: { default: "0.95rem", [compactNav]: "0.78rem" },
    fontVariantNumeric: "tabular-nums",
  },
  label: {
    display: { default: "inline", [compactNav]: "none" },
    color: "inherit",
    fontFamily: "inherit",
    fontSize: "inherit",
    fontWeight: "inherit",
    lineHeight: "inherit",
    letterSpacing: "inherit",
  },
  /* Previous/Next fill the line as the touch pair; the ply and jump buttons
     keep their natural width between and beside them. */
  step: {
    "--watercolor-button-shadow": "none",
    flexBasis: 0,
    flexGrow: 1,
    flexShrink: 1,
    minWidth: 0,
    minHeight: { default: "2.75rem", [compactNav]: "2.65rem" },
    padding: { default: "0.75rem 1rem", [compactNav]: "0.5rem 0.375rem" },
    fontSize: "0.92rem",
  },
  /* Previous is the way back, so it stays an outlined control the eye can
     skip. */
  stepFrame: {
    "::before": {
      inset: "-0.06rem -0.22rem",
    },
  },
  /* Next carries the Player forward through the Game, so at widget density it
     is the filled control on the real wide brush stroke (`blockWideMask`,
     the same artwork a block primary takes) while Previous stays outlined.
     Keyed to density alone: the earlier version branched on viewport width
     while the style applied on density, so the pair mismatched on anything
     wider than a phone and the stroke was dropped rather than unkeyed. */
  stepStroke: {
    "::before": {
      /* The stroke carries its ink weight above the baseline; keep the
         overhang symmetric so the label sits inside the body. */
      inset: "-0.32rem -0.62rem -0.32rem -0.55rem",
    },
  },
  /** The nav embedded under a widget board, sharing the line with notation. */
  compactStep: {
    minWidth: 0,
    minHeight: "2.15rem",
    padding: "0.375rem 0.5rem",
    fontSize: "0.78rem",
  },
  jump: {
    width: "2.1rem",
    height: "2.1rem",
    "--watercolor-button-stroke-opacity": { default: "0.46", ":hover": "0.82" },
  },
})

export const cardStyles = stylex.create({
  base: {
    "--watercolor-brush-weight": "0.24rem",
    "--watercolor-brush-sizes":
      "101.6% 0.27rem, 0.21rem 102.4%, 100.7% 0.24rem, 0.28rem 101.4%",
    "--watercolor-card-accent":
      "color-mix(in srgb, var(--color-text-primary) 88%, var(--color-text-secondary))",
    /* The border is filled black ink on every tone — a bold solid frame with
       dry-brush edges, per the brand reference card. The tone tints the wash
       and the stamps, never the frame. */
    "--watercolor-card-frame-ink":
      "color-mix(in srgb, var(--color-text-primary) 62%, black)",
    "--watercolor-card-frame-opacity": "1",
    "--watercolor-card-ink": "var(--color-text-primary)",
    "--watercolor-card-detail-ink": "var(--color-text-disabled)",
    /* The paper and its washes live in custom properties so the torn-sheet
       `::after` and the rectangular fallback read the same values. The tone
       sets the paper; the wash is the accent bloom painted on it. */
    "--watercolor-card-paper":
      "color-mix(in srgb, var(--color-paper-raised) 97%, transparent)",
    "--watercolor-card-paper-image":
      "linear-gradient(transparent, transparent)",
    "--watercolor-card-wash":
      "radial-gradient(ellipse at 96% 4%, color-mix(in srgb, var(--watercolor-card-accent) 10%, transparent), transparent 28%)",
    /* The pigment pooling at the splash's edge: real watercolor dries darker
       where the water stopped, and the ring is what makes an ivory splash
       readable on ivory paper. */
    "--watercolor-card-rim":
      "radial-gradient(ellipse 130% 118% at 50% 46%, transparent 62%, color-mix(in srgb, var(--watercolor-card-accent) 11%, transparent) 92%)",
    "--watercolor-card-shape": "var(--watercolor-shape-splash-a)",
    "--watercolor-card-sheet-bleed": "-0.65rem",
    "--watercolor-card-host-image": "linear-gradient(transparent, transparent)",
    /* What the splash sheet is filled with: the tone's own pigment, mixed wet
       into the paper. Splash is only for coloured tones — on white paper a
       splash of paper is invisible, so the card ignores the flag there. */
    "--watercolor-card-splash-fill":
      "color-mix(in srgb, var(--watercolor-card-accent) 26%, var(--color-paper))",
    /* A glaze over the painting: the full-strength asset would swallow the
       type, so the sheet lays a translucent wash of its own ground back over
       it — paper for the pale tones, navy for the ink card. */
    "--watercolor-card-glaze":
      "linear-gradient(rgb(255 249 237 / 0.45), rgb(255 249 237 / 0.45))",
    position: "relative",
    isolation: "isolate",
    display: "flex",
    minWidth: 0,
    flexDirection: "column",
    gap: "1.25rem",
    overflow: "visible",
    /* Padding composes from the density vars plus the splash allowance: an
       uneven edge wanders into the box, so the splash buys the text extra
       distance from it. */
    padding:
      "calc(var(--watercolor-card-pad-y) + var(--watercolor-card-splash-pad, 0rem)) calc(var(--watercolor-card-pad-x) + var(--watercolor-card-splash-pad, 0rem))",
    /* A real filled border carries the weight; the brushed ::before rides on
       top of it for the dry edge. This framed reading is the DEFAULT card —
       the splash silhouette is the `splash` variant, reserved for surfaces
       that matter. */
    borderWidth: "0.19rem",
    borderStyle: "solid",
    borderColor: "var(--watercolor-card-frame-ink)",
    borderRadius: "0.08rem 0.16rem 0.05rem 0.13rem",
    backgroundColor: "var(--watercolor-card-paper)",
    backgroundImage: "var(--watercolor-card-host-image)",
    color: "var(--color-text-primary)",
    boxShadow: "none",
    "::before": {
      position: "absolute",
      zIndex: 3,
      inset: "-0.3rem -0.24rem -0.34rem -0.28rem",
      backgroundColor: "var(--watercolor-card-frame-ink)",
      content: '""',
      opacity: "var(--watercolor-card-frame-opacity)",
      pointerEvents: "none",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
      animationName: paintFrame,
      animationDuration: { default: "640ms", [reduceMotion]: "0s" },
      animationTimingFunction: "cubic-bezier(0.23, 1, 0.32, 1)",
      animationFillMode: "both",
    },
    "::after": {
      position: "absolute",
      zIndex: -2,
      inset: 0,
      borderRadius: "inherit",
      backgroundImage: "var(--watercolor-card-wash)",
      content: '""',
      pointerEvents: "none",
    },
  },
  /**
   * The splash reading, for the surfaces that matter — a featured prompt, a
   * marked moment, a standing message. Where shape() exists the host hands
   * its paper and border to the `::after` sheet: a lobed silhouette with
   * pigment pooling at the rim and a blurred wet edge, bleeding past the box
   * top and bottom only (a horizontal bleed widens scrollWidth on narrow
   * hosts). The dry-brush frame retires here — it belongs to the small
   * controls. Without shape() the framed card stays, untouched.
   */
  splash: {
    "--watercolor-card-splash-pad": {
      default: "0rem",
      [supportsTornSilhouette]: "0.6rem",
    },
    borderColor: {
      default: "var(--watercolor-card-frame-ink)",
      [supportsTornSilhouette]: "transparent",
    },
    backgroundColor: {
      default: "var(--watercolor-card-paper)",
      [supportsTornSilhouette]: "transparent",
    },
    backgroundImage: {
      default: "var(--watercolor-card-host-image)",
      [supportsTornSilhouette]: "none",
    },
    "::before": {
      display: { default: null, [supportsTornSilhouette]: "none" },
    },
    "::after": {
      inset: {
        default: 0,
        [supportsTornSilhouette]: "var(--watercolor-card-sheet-bleed) 0",
      },
      borderRadius: { default: "inherit", [supportsTornSilhouette]: 0 },
      backgroundColor: {
        default: "transparent",
        [supportsTornSilhouette]: "var(--watercolor-card-splash-fill)",
      },
      /* The real watercolor painting fills the WHOLE sheet where the app
         loads `surfaces.css` (--watercolor-splash-texture); widgets fall back
         to the plain pigment. Gradients ignore the cover sizing — they always
         stretch to the box — so one size serves every layer. */
      backgroundImage: {
        default: "var(--watercolor-card-wash)",
        [supportsTornSilhouette]:
          "var(--watercolor-card-rim), var(--watercolor-card-glaze), var(--watercolor-splash-texture, linear-gradient(transparent, transparent)), var(--watercolor-card-wash), var(--watercolor-card-paper-image)",
      },
      backgroundPosition: {
        default: null,
        [supportsTornSilhouette]: "center",
      },
      backgroundRepeat: {
        default: null,
        [supportsTornSilhouette]: "no-repeat",
      },
      backgroundSize: { default: null, [supportsTornSilhouette]: "cover" },
      clipPath: {
        default: null,
        [supportsTornSilhouette]: "var(--watercolor-card-shape)",
      },
      /* The wet edge: a whisper of blur so the clipped outline dries soft,
         the way pigment feathers into the paper. */
      filter: { default: null, [supportsTornSilhouette]: "blur(1px)" },
    },
  },
  /** Small containers compress the full lobe count into a busy edge, so the
   * compact splash card wears the calm silhouette. Applied after `content`,
   * which would otherwise claim the heavy one. */
  splashCalm: {
    "--watercolor-card-shape": "var(--watercolor-shape-splash-calm-a)",
  },
  compact: {
    "--watercolor-card-plaque-pull": "0.1rem",
    "--watercolor-card-pad-y": "1.15rem",
    "--watercolor-card-pad-x": "1.25rem",
  },
  comfortable: {
    "--watercolor-card-plaque-pull": "0.15rem",
    "--watercolor-card-pad-y": "clamp(1.75rem, 3.6vw, 2.45rem)",
    "--watercolor-card-pad-x": "clamp(1.75rem, 3.6vw, 2.45rem)",
  },
  paper: {
    "--watercolor-card-paper":
      "color-mix(in srgb, var(--color-paper-raised) 97%, transparent)",
  },
  mist: {
    "--watercolor-card-accent": "var(--color-text-secondary)",
    "--watercolor-card-paper":
      "color-mix(in srgb, var(--color-paper-raised) 96%, transparent)",
  },
  bamboo: {
    "--watercolor-card-accent": "var(--color-success)",
    "--watercolor-card-paper":
      "color-mix(in srgb, var(--color-paper) 97%, transparent)",
  },
  vermilion: {
    "--watercolor-card-accent": "var(--color-error)",
    "--watercolor-card-paper":
      "color-mix(in srgb, var(--color-paper-raised) 97%, transparent)",
  },
  watercolor: {
    "--watercolor-brush-weight": "0.065rem",
    "--watercolor-brush-sizes":
      "100.6% 0.055rem, 0.045rem 101.2%, 100.3% 0.05rem, 0.07rem 100.5%",
    "--watercolor-card-accent": "var(--color-border)",
    "--watercolor-card-frame-opacity": "0.72",
    "--watercolor-card-ink": "var(--color-background-surface)",
    "--watercolor-card-detail-ink":
      "color-mix(in srgb, var(--color-paper-raised) 88%, transparent)",
    /* The ink slab: the whole navy wash rides the torn sheet, and its
       silhouette is the chunky panel shape — the notification panel of the
       Three-Kingdoms reference. */
    "--watercolor-card-paper": "var(--color-ink)",
    "--watercolor-card-paper-image":
      "linear-gradient(135deg, var(--color-ink), var(--color-ink-deep))",
    "--watercolor-card-wash":
      "radial-gradient(ellipse at 92% -10%, color-mix(in srgb, var(--color-mist) 20%, transparent), transparent 44%), radial-gradient(ellipse at 5% 15%, color-mix(in srgb, var(--color-mist) 16%, transparent), transparent 31%), radial-gradient(ellipse at 90% 115%, color-mix(in srgb, var(--color-vermilion) 18%, transparent), transparent 40%)",
    "--watercolor-card-rim":
      "radial-gradient(ellipse 130% 118% at 50% 46%, transparent 64%, rgb(6 15 26 / 0.5) 94%)",
    "--watercolor-card-shape": "var(--watercolor-shape-panel)",
    "--watercolor-card-host-image":
      "radial-gradient(ellipse at 92% -10%, color-mix(in srgb, var(--color-mist) 20%, transparent), transparent 44%), linear-gradient(135deg, var(--color-ink), var(--color-ink-deep))",
    "--watercolor-card-splash-fill": "var(--color-ink)",
    "--watercolor-card-glaze":
      "linear-gradient(rgb(16 31 50 / 0.62), rgb(16 31 50 / 0.62))",
    borderRadius: "0.38rem 0.58rem 0.32rem 0.48rem",
    color: "var(--color-background-surface)",
    "::before": {
      inset: "-0.05rem -0.04rem -0.03rem -0.06rem",
    },
  },
  /** The morning-digest / review content card: a heavier fine-pen frame. */
  content: {
    "--watercolor-brush-weight": "0.26rem",
    "--watercolor-brush-sizes":
      "102.2% 0.29rem, 0.22rem 103.2%, 100.6% 0.26rem, 0.3rem 102%",
    "--watercolor-card-frame-ink":
      "color-mix(in srgb, var(--color-text-primary) 58%, black)",
    "--watercolor-card-frame-opacity": "1",
    "--watercolor-card-shape": "var(--watercolor-shape-splash-heavy)",
    "--watercolor-card-sheet-bleed": "-0.75rem",
    borderRadius: "0.06rem 0.14rem 0.04rem 0.11rem",
    "::before": {
      inset: "-0.13rem -0.03rem -0.17rem -0.09rem",
    },
  },
  contentPaper: {
    "--watercolor-card-paper":
      "color-mix(in srgb, var(--color-paper-raised) 97%, transparent)",
  },
  /** A card nested inside another card keeps its paper and its spacing but
      drops the ink frame — two stacked borders read as a rendering fault. */
  flat: {
    borderWidth: 0,
    borderStyle: "none",
    /* A nested card is a section of its parent's splash, not a second sheet:
       no fill of its own, the parent's pigment shows through. */
    backgroundColor: "transparent",
    boxShadow: "none",
    "::before": {
      display: "none",
    },
    /* The accent wash reads as a smudge once the frame that framed it is
       gone — a negative z-index pseudo still paints over its own card's
       background. */
    "::after": {
      display: "none",
    },
  },
  hasSeal: {
    paddingBottom: "2.75rem",
  },
  inner: {
    minWidth: 0,
  },
})

export const cardPartStyles = stylex.create({
  header: {
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(12ch, 1fr) auto",
      [phone]: "minmax(12ch, 1fr)",
    },
    alignItems: "start",
    gap: "0.75rem 1rem",
    minWidth: 0,
  },
  masthead: {
    display: "grid",
    minWidth: "12ch",
    gap: "0.5rem",
    justifyItems: "stretch",
  },
  title: {
    minWidth: "12ch",
    margin: 0,
    overflowWrap: "anywhere",
    color: "var(--watercolor-card-ink, var(--color-text-primary))",
    fontFamily: "var(--font-family-heading)",
    fontSize: "1.35rem",
    fontWeight: 640,
    lineHeight: 1.2,
    letterSpacing: "0.01em",
    whiteSpace: "normal",
  },
  /* The card title rides a centred ink-splash slab. It sits inside the ink
     frame with room on both sides: over the border it reads as a rendering
     fault, and against the body copy it crowds the card. */
  titleRow: {
    display: "flex",
    justifyContent: "center",
    marginTop: "var(--watercolor-card-plaque-pull, 0.15rem)",
    marginBottom: "0.5rem",
    minWidth: "12ch",
  },
  titlePlaque: {
    minWidth: "12ch",
    maxWidth: "100%",
    overflowWrap: "anywhere",
    whiteSpace: "normal",
    textAlign: "center",
    textTransform: "none",
    fontSize: "1.35rem",
    fontWeight: 640,
    letterSpacing: "0.02em",
  },
  contentTitle: {
    fontSize: "clamp(1.35rem, 2.8vw, 1.85rem)",
    fontWeight: 560,
    letterSpacing: "-0.03em",
    lineHeight: 1.15,
  },
  meta: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: "0.375rem 0.75rem",
    color: "var(--watercolor-card-detail-ink, var(--color-text-disabled))",
    fontSize: "0.75rem",
    lineHeight: 1.5,
  },
  description: {
    gridColumn: "1 / -1",
    margin: 0,
    color: "var(--watercolor-card-detail-ink, var(--color-text-disabled))",
    fontSize: "0.78rem",
    lineHeight: 1.55,
  },
  content: {
    display: "grid",
    gap: "0.75rem",
    minWidth: 0,
    color: "var(--watercolor-card-ink, var(--color-text-primary))",
    fontSize: "0.9rem",
    lineHeight: 1.62,
  },
  footer: {
    display: "flex",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: "0.5rem",
    marginTop: "0.125rem",
    paddingTop: "0.375rem",
  },
  seal: {
    position: "absolute",
    zIndex: 2,
    right: "0.85rem",
    bottom: "0.7rem",
    left: "auto",
    width: "1.85rem",
    height: "1.85rem",
    fontFamily: "var(--font-family-seal)",
    fontSize: "1rem",
  },
})

export const badgeStyles = stylex.create({
  base: {
    "--watercolor-badge-color": "var(--color-text-secondary)",
    "--watercolor-brush-weight": "0.16rem",
    position: "relative",
    isolation: "isolate",
    display: "inline-flex",
    minWidth: 0,
    minHeight: "1.55rem",
    maxWidth: "100%",
    width: "fit-content",
    flexShrink: 0,
    alignItems: "center",
    gap: "0.375rem",
    overflow: "hidden",
    borderWidth: 0,
    borderStyle: "none",
    borderRadius: "0.18rem 0.42rem 0.22rem 0.34rem",
    padding: "0.25rem 0.5rem",
    backgroundColor:
      "color-mix(in srgb, var(--watercolor-badge-color) 16%, var(--color-background-surface))",
    color:
      "color-mix(in srgb, var(--watercolor-badge-color) 78%, var(--color-ink-deep))",
    fontSize: "0.68rem",
    fontWeight: 760,
    letterSpacing: "0.06em",
    lineHeight: 1,
    textTransform: "uppercase",
    whiteSpace: "nowrap",
    "::before": {
      position: "absolute",
      zIndex: 1,
      inset: 0,
      backgroundColor: "var(--watercolor-badge-color)",
      content: '""',
      opacity: 0.62,
      pointerEvents: "none",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
    },
  },
  neutral: { "--watercolor-badge-color": "var(--color-text-disabled)" },
  info: { "--watercolor-badge-color": "var(--color-text-secondary)" },
  success: { "--watercolor-badge-color": "var(--color-success)" },
  warning: { "--watercolor-badge-color": "var(--color-error)" },
  danger: { "--watercolor-badge-color": "var(--color-error)" },
  onWatercolorCard: {
    backgroundColor:
      "color-mix(in srgb, var(--color-paper-raised) 8%, transparent)",
    color:
      "color-mix(in srgb, var(--watercolor-badge-color) 48%, var(--color-background-surface))",
  },
})

export const chipStyles = stylex.create({
  base: {
    "--watercolor-chip-ink": "var(--color-text-primary)",
    "--watercolor-brush-weight": "0.14rem",
    position: "relative",
    isolation: "isolate",
    display: "inline-flex",
    width: "fit-content",
    minHeight: "1.35rem",
    alignItems: "center",
    gap: "0.25rem",
    padding: "0.125rem 0.375rem",
    borderWidth: 0,
    borderStyle: "none",
    borderRadius: "0.08rem 0.22rem 0.1rem 0.18rem",
    backgroundColor:
      "color-mix(in srgb, var(--watercolor-chip-ink) 12%, var(--color-background-surface))",
    color: "var(--watercolor-chip-ink)",
    fontSize: "0.62rem",
    fontWeight: 780,
    letterSpacing: "0.08em",
    lineHeight: 1,
    textTransform: "uppercase",
    whiteSpace: "nowrap",
    "::before": {
      position: "absolute",
      zIndex: 1,
      inset: 0,
      backgroundColor: "var(--watercolor-chip-ink)",
      content: '""',
      opacity: 0.55,
      pointerEvents: "none",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
    },
  },
  neutral: { "--watercolor-chip-ink": "var(--color-text-disabled)" },
  draw: { "--watercolor-chip-ink": "var(--color-text-disabled)" },
  win: { "--watercolor-chip-ink": "var(--color-text-primary)" },
  reinforced: { "--watercolor-chip-ink": "var(--color-success)" },
  missing: { "--watercolor-chip-ink": "var(--color-text-secondary)" },
  loss: { "--watercolor-chip-ink": "var(--color-error)" },
})

export const symbolStyles = stylex.create({
  base: {
    "--symbol-ink": "var(--color-text-primary)",
    position: "relative",
    isolation: "isolate",
    display: "inline-grid",
    width: "2.75rem",
    height: "2.75rem",
    flexGrow: 0,
    flexShrink: 0,
    borderWidth: 0,
    borderStyle: "none",
    placeItems: "center",
    color: "var(--symbol-ink)",
    "::before": {
      position: "absolute",
      zIndex: -1,
      inset: 0,
      backgroundColor:
        "color-mix(in srgb, var(--symbol-ink) 13%, var(--color-background-surface))",
      content: '""',
      opacity: 0.95,
    },
  },
  circle: {
    "::before": {
      borderWidth: "1px",
      borderStyle: "solid",
      borderColor: "color-mix(in srgb, var(--symbol-ink) 36%, transparent)",
      borderRadius: "50%",
    },
  },
  seal: {
    color: "var(--color-background-surface)",
    "::before": {
      borderRadius: "0.16rem 0.28rem 0.12rem 0.22rem",
      backgroundColor: "var(--symbol-ink)",
      boxShadow:
        "inset 0 0 0 1px color-mix(in srgb, var(--color-paper-raised) 20%, transparent)",
      transform: "rotate(-1.6deg)",
    },
  },
  soft: {
    /* A real ink-blot alpha mask: the raster carries the dry-brush feathering
       the old border-radius blob only gestured at. */
    "::before": {
      mask: "var(--watercolor-ink-blot) center / 100% 100% no-repeat",
      transform: "rotate(-3deg)",
    },
  },
  watercolor: { "--symbol-ink": "var(--color-text-primary)" },
  slate: { "--symbol-ink": "var(--color-text-secondary)" },
  bamboo: { "--symbol-ink": "var(--color-success)" },
  vermilion: { "--symbol-ink": "var(--color-error)" },
})

export const eyebrowStyles = stylex.create({
  eyebrow: {
    margin: 0,
    color: "var(--color-error)",
    fontSize: "0.66rem",
    fontWeight: 750,
    letterSpacing: "0.11em",
    textTransform: "uppercase",
  },
})

/**
 * The ink-splash plaque: a black brush splash carrying a short title, the way
 * the brand art carries "Critical Moment" on an ink stroke. One authored
 * splash mask, ivory text, an optional vermilion seal accent at the edge.
 */
export const plaqueStyles = stylex.create({
  base: {
    position: "relative",
    isolation: "isolate",
    display: "inline-flex",
    minWidth: 0,
    maxWidth: "100%",
    alignItems: "center",
    justifyContent: "center",
    gap: "0.5rem",
    padding: "0.5rem 2rem 0.75rem",
    color: "var(--color-background-surface)",
    fontFamily: "var(--font-family-heading)",
    fontWeight: 620,
    letterSpacing: "0.05em",
    lineHeight: 1.1,
    textTransform: "uppercase",
    whiteSpace: "nowrap",
    "::before": {
      position: "absolute",
      zIndex: -1,
      inset: "-0.3rem -1rem",
      backgroundImage:
        "linear-gradient(180deg, transparent 55%, rgb(0 0 0 / 0.22)), linear-gradient(97deg, color-mix(in srgb, var(--color-text-primary) 88%, black), var(--color-text-primary) 58%, color-mix(in srgb, var(--color-text-primary) 92%, black))",
      content: '""',
      pointerEvents: "none",
      /* Real dry-brush slab artwork, intersected with a soft gradient layer
         the paintSweep animation grows across it. At rest the gradient covers
         the whole slab, so the intersection is the slab itself. */
      mask: "var(--watercolor-brush-slab) center / 100% 100% no-repeat, linear-gradient(100deg, #000 72%, transparent 94%) left center / 220% 100% no-repeat",
      maskComposite: "intersect",
      animationName: paintSweep,
      animationDuration: { default: "420ms", [reduceMotion]: "0s" },
      animationTimingFunction: "cubic-bezier(0.33, 0, 0.2, 1)",
      animationFillMode: "both",
    },
  },
  sm: {
    fontSize: "0.72rem",
    padding: "0.375rem 1.5rem 0.5rem",
  },
  md: {
    fontSize: "0.9rem",
  },
  lg: {
    fontSize: "1.1rem",
    padding: "0.75rem 2.5rem 0.75rem",
  },
})

/**
 * The self-drawing swoosh (`WatercolorInkStroke`). The SVG stretches to its
 * host box; the guide stroke inside carries the draw-on animation.
 */
export const inkStrokeStyles = stylex.create({
  root: {
    display: "block",
    width: "100%",
    height: "100%",
    overflow: "visible",
  },
  guide: {
    strokeDasharray: 1,
    animationName: drawInk,
    animationDuration: { default: "560ms", [reduceMotion]: "0s" },
    animationTimingFunction: "cubic-bezier(0.3, 0, 0.25, 1)",
    animationFillMode: "both",
  },
})

export const noticeStyles = stylex.create({
  body: {
    width: "100%",
  },
  featuredBody: {
    minHeight: "26rem",
    justifyContent: "center",
  },
  copy: {
    display: "grid",
    minWidth: 0,
    gap: "0.25rem",
  },
  featuredCopy: {
    gap: "0.75rem",
  },
  heading: {
    margin: 0,
    color: "var(--watercolor-card-ink, var(--color-text-primary))",
  },
  featuredHeading: {
    maxWidth: "21ch",
    fontFamily: "var(--font-family-heading)",
    fontSize: "clamp(2rem, 5vw, 3.75rem)",
    fontWeight: 620,
    lineHeight: 1.02,
    letterSpacing: "-0.04em",
  },
  detail: {
    margin: 0,
    color: "var(--watercolor-card-detail-ink, var(--color-text-disabled))",
    fontSize: "0.75rem",
    lineHeight: 1.55,
  },
  featuredDetail: {
    maxWidth: "62ch",
    fontSize: "1rem",
    lineHeight: 1.65,
  },
})

/** Header action copy: icon+text on desktop, icon-only on a phone. */
export const headerActionStyles = stylex.create({
  label: {
    display: {
      default: null,
      [phone]: "none",
    },
  },
})

export const sessionHeaderStyles = stylex.create({
  row: {
    display: "flex",
    minWidth: 0,
    flexWrap: "wrap",
    alignItems: "center",
    justifyContent: "flex-start",
    gap: "0.75rem 1rem",
    marginBottom: {
      default: "1.25rem",
      [phone]: "0.75rem",
    },
  },
  /* Desktop line: brand · title · meta … actions. Phone rows via flex order:
     brand + actions first, then the plaque and the meta each on a full row. */
  title: {
    minWidth: "12ch",
    order: { default: 0, [phone]: 2 },
    flexGrow: 0,
    flexShrink: 1,
    flexBasis: { default: "auto", [phone]: "100%" },
    marginLeft: { default: "1rem", [phone]: 0 },
    paddingBottom: {
      default: "0.375rem",
      [phone]: 0,
    },
  },
  /* Full-row plaque on a phone. The splash art normally bleeds 1rem past the
     text box; at full width that bleed would poke past the page edge, so it
     pulls in under the page padding. */
  plaqueStretch: {
    width: { default: null, [phone]: "100%" },
    "::before": {
      /* Restates the plaque base bleed: a null default here would unset it
         and leave the desktop banner with no slab at all. */
      inset: { default: "-0.3rem -1rem", [phone]: "-0.3rem -0.4rem" },
    },
  },
  meta: {
    minWidth: 0,
    order: { default: 0, [phone]: 3 },
    flexGrow: 0,
    flexShrink: 1,
    flexBasis: { default: "auto", [phone]: "100%" },
    alignSelf: "center",
    paddingLeft: { default: "1.25rem", [phone]: 0 },
    borderLeftWidth: { default: "1px", [phone]: 0 },
    borderLeftStyle: { default: "solid", [phone]: "none" },
    borderLeftColor:
      "color-mix(in srgb, var(--color-ink-soft) 32%, transparent)",
  },
  actions: {
    order: { default: 0, [phone]: 1 },
    marginLeft: "auto",
  },
  heading: {
    minWidth: "12ch",
    margin: 0,
    overflowWrap: "anywhere",
    color: "var(--color-text-primary)",
    fontFamily: "var(--font-family-heading)",
    fontSize: "clamp(1.35rem, 2.4vw, 1.85rem)",
    fontWeight: 560,
    letterSpacing: "-0.03em",
    lineHeight: 1.15,
    whiteSpace: "normal",
  },
  plaqueHeading: {
    minWidth: "12ch",
    width: { default: null, [phone]: "100%" },
    margin: 0,
    lineHeight: 1,
  },
})

const studioPhone = "@media (max-width: 620px)"

export const studioStyles = stylex.create({
  studio: {
    position: "relative",
    isolation: "isolate",
    boxSizing: "border-box",
    minHeight: "100vh",
    color: "var(--color-text-primary)",
    colorScheme: "light",
    backgroundColor: "var(--color-background-body)",
    /* The brand frames bleed: a plaque's brush `::before` is ~16px wider than
       the plaque, by design. Against a phone edge that bleed was reaching the
       viewport and giving the Coaching Board 4px of real sideways scroll. The
       shell owns the viewport, so the shell clips it — and it clips with
       `clip`, not `hidden`, because `hidden` would make this a scroll
       container and break the `position: sticky` chrome inside it. */
    overflowX: "clip",
  },
  /* Same fixed cover wash the landing page uses: the ridge fills the opening
     viewport, and session chrome sits on top of it as the page scrolls. */
  mistRoot: {
    position: "fixed",
    zIndex: -1,
    inset: 0,
    overflow: "hidden",
    pointerEvents: "none",
  },
  mist: {
    position: "absolute",
    inset: 0,
    width: "100%",
    height: "100%",
    objectFit: "cover",
    objectPosition: {
      default: "center bottom",
      [studioPhone]: "86% 100%",
    },
    transform: {
      default: null,
      [studioPhone]: "scale(1.55)",
    },
    transformOrigin: {
      default: null,
      [studioPhone]: "86% 92%",
    },
  },
  mistWash: {
    position: "absolute",
    inset: 0,
    backgroundImage: {
      default:
        "radial-gradient(ellipse at 8% 4%, color-mix(in srgb, var(--color-mist) 16%, transparent), transparent 28rem), linear-gradient(180deg, color-mix(in srgb, var(--color-paper) 32%, transparent), transparent 38%), linear-gradient(90deg, color-mix(in srgb, var(--color-paper) 48%, transparent), color-mix(in srgb, var(--color-paper) 22%, transparent) 48%, color-mix(in srgb, var(--color-paper) 36%, transparent))",
      [studioPhone]:
        "radial-gradient(ellipse at 8% 4%, color-mix(in srgb, var(--color-mist) 16%, transparent), transparent 18rem), linear-gradient(180deg, color-mix(in srgb, var(--color-paper) 28%, transparent), transparent 42%), linear-gradient(90deg, color-mix(in srgb, var(--color-paper) 36%, transparent), color-mix(in srgb, var(--color-paper) 14%, transparent) 46%, color-mix(in srgb, var(--color-paper) 22%, transparent))",
    },
  },
})

export const fieldStyles = stylex.create({
  field: {
    display: "grid",
    gap: "0.375rem",
    color: "var(--color-text-primary)",
  },
  label: {
    fontSize: "0.78rem",
    fontWeight: 760,
    letterSpacing: "0.015em",
  },
  hint: {
    color: "var(--color-text-disabled)",
    fontSize: "0.7rem",
    lineHeight: 1.45,
  },
  error: {
    color: "var(--color-error)",
    fontSize: "0.7rem",
    lineHeight: 1.45,
  },
  frame: {
    "--watercolor-brush-weight": "0.2rem",
    "--watercolor-input-ink":
      "color-mix(in srgb, var(--color-text-secondary) 72%, var(--color-text-primary))",
    position: "relative",
    isolation: "isolate",
    display: "block",
    width: "100%",
    "::before": {
      position: "absolute",
      zIndex: 1,
      inset: 0,
      backgroundColor: "var(--watercolor-input-ink)",
      content: '""',
      opacity: 0.72,
      pointerEvents: "none",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
    },
  },
  frameInvalid: {
    "--watercolor-input-ink": "var(--color-error)",
  },
  input: {
    width: "100%",
    minHeight: "2.75rem",
    borderWidth: 0,
    borderStyle: "none",
    borderRadius: "0.18rem 0.36rem 0.22rem 0.3rem",
    padding: "0.75rem",
    backgroundColor: "var(--color-background-surface)",
    backgroundImage:
      "radial-gradient(ellipse at 96% 110%, color-mix(in srgb, var(--color-mist) 18%, transparent), transparent 42%)",
    boxShadow: {
      default:
        "inset 0 0.12rem 0.45rem color-mix(in srgb, var(--color-ink) 4%, transparent)",
      ":hover":
        "inset 0 0.12rem 0.45rem color-mix(in srgb, var(--color-ink) 7%, transparent)",
    },
    color: "var(--color-text-primary)",
    colorScheme: "light",
    fontFamily: "inherit",
    fontSize: "inherit",
    fontWeight: "inherit",
    lineHeight: 1.45,
    outline: {
      default: "none",
      ":focus-visible":
        "3px solid color-mix(in srgb, var(--focus-outline-color) 62%, transparent)",
    },
    outlineOffset: "2px",
    transition: {
      default: "background-color 160ms ease, box-shadow 160ms ease",
      [reduceMotion]: "none",
    },
    "::placeholder": {
      color: "color-mix(in srgb, var(--color-text-disabled) 72%, transparent)",
    },
  },
  dateInput: {
    minHeight: "2.75rem",
    appearance: "none",
  },
  textarea: {
    minHeight: "7.5rem",
    resize: "vertical",
  },
  select: {
    minHeight: "2.75rem",
    appearance: "none",
    paddingRight: "2.75rem",
    backgroundImage:
      "linear-gradient(45deg, transparent 50%, var(--color-text-secondary) 50%), linear-gradient(135deg, var(--color-text-secondary) 50%, transparent 50%), linear-gradient(90deg, transparent, color-mix(in srgb, var(--color-text-secondary) 16%, transparent))",
    backgroundPosition:
      "calc(100% - 1.18rem) 50%, calc(100% - 0.9rem) 50%, calc(100% - 2.15rem) 0",
    backgroundRepeat: "no-repeat",
    backgroundSize: "0.3rem 0.3rem, 0.3rem 0.3rem, 1px 100%",
  },
})

/**
 * ChatComposer structure — one outlined box, send seated inside it —
 * without Astryx's raised pill, circular ↑, or 1px gray Flat ring.
 */
export const chatComposerStyles = stylex.create({
  box: {
    "--watercolor-brush-weight": "0.38rem",
    display: "grid",
    gap: "0.375rem",
    padding: "0.5rem 0.5rem 0.375rem",
    backgroundColor:
      "color-mix(in srgb, var(--color-paper-raised) 86%, transparent)",
  },
  input: {
    width: "100%",
    minHeight: "4.5rem",
    borderWidth: 0,
    borderStyle: "none",
    borderRadius: 0,
    padding: "0.125rem 0.125rem 0",
    backgroundColor: "transparent",
    backgroundImage: "none",
    boxShadow: "none",
    color: "var(--color-text-primary)",
    colorScheme: "light",
    fontFamily: "inherit",
    fontSize: "inherit",
    lineHeight: 1.45,
    resize: "none",
    outline: {
      default: "none",
      ":focus-visible":
        "3px solid color-mix(in srgb, var(--focus-outline-color) 62%, transparent)",
    },
    outlineOffset: "2px",
    "::placeholder": {
      color: "color-mix(in srgb, var(--color-text-disabled) 72%, transparent)",
    },
  },
  sendRow: {
    display: "flex",
    justifyContent: "flex-end",
    alignItems: "center",
  },
  sendLabel: {
    display: {
      default: "inline",
      [phone]: "none",
    },
  },
  sendButton: {
    [phone]: {
      width: "2.35rem",
      height: "2.35rem",
      minHeight: "2.35rem",
      padding: 0,
      gap: 0,
    },
  },
})

export const checkboxStyles = stylex.create({
  root: {
    "--watercolor-mark-ink": {
      default:
        "color-mix(in srgb, var(--color-text-secondary) 78%, var(--color-text-primary))",
      ":has(input:checked)": "var(--color-text-primary)",
    },
    "--watercolor-mark-mask": {
      default: "var(--watercolor-brush-frame)",
      ":has(input:checked)":
        "var(--watercolor-control-frame) center / 100% 100% no-repeat",
    },
    "--watercolor-mark-mask-size": {
      default: "100% 0.18rem, 0.18rem 100%, 100% 0.18rem, 0.18rem 100%",
      ":has(input:checked)": "100% 100%",
    },
    "--watercolor-mark-opacity": {
      default: "0.78",
      ":has(input:checked)": "1",
    },
    "--watercolor-mark-glyph": {
      default: "transparent",
      ":has(input:checked)": "var(--color-background-surface)",
    },
    "--watercolor-mark-outline": {
      default: "none",
      ":has(input:focus-visible)":
        "3px solid color-mix(in srgb, var(--focus-outline-color) 55%, transparent)",
    },
    display: "inline-flex",
    width: "fit-content",
    alignItems: "flex-start",
    gap: "0.5rem",
    color: "var(--color-text-primary)",
    cursor: { default: "pointer", ":has(input:disabled)": "not-allowed" },
    filter: { default: null, ":has(input:disabled)": "saturate(0.42)" },
    opacity: { default: null, ":has(input:disabled)": 0.48 },
    fontSize: "0.82rem",
    fontWeight: 650,
    lineHeight: 1.45,
  },
  input: {
    position: "absolute",
    width: "1px",
    height: "1px",
    overflow: "hidden",
    opacity: 0,
    pointerEvents: "none",
  },
  mark: {
    position: "relative",
    isolation: "isolate",
    display: "grid",
    width: "1.25rem",
    height: "1.25rem",
    flexGrow: 0,
    flexShrink: 0,
    placeItems: "center",
    color: "var(--watercolor-mark-glyph)",
    fontFamily: "var(--font-family-seal)",
    fontSize: "0.88rem",
    lineHeight: 1,
    outline: "var(--watercolor-mark-outline)",
    outlineOffset: "3px",
    "::before": {
      position: "absolute",
      zIndex: -1,
      inset: "-0.12rem",
      backgroundColor: "var(--watercolor-mark-ink)",
      content: '""',
      opacity: "var(--watercolor-mark-opacity)",
      mask: "var(--watercolor-mark-mask)",
      maskSize: "var(--watercolor-mark-mask-size)",
    },
  },
})

export const progressStyles = stylex.create({
  track: {
    position: "relative",
    height: "0.52rem",
    overflow: "hidden",
    backgroundColor:
      "color-mix(in srgb, var(--color-text-secondary) 16%, transparent)",
    mask: "var(--watercolor-brush-h) center / 100% 100% no-repeat",
  },
  fill: {
    display: "block",
    width: "var(--watercolor-progress)",
    height: "100%",
    backgroundImage:
      "linear-gradient(90deg, var(--color-text-secondary), var(--color-text-primary))",
    transition: { default: "width 220ms ease", [reduceMotion]: "none" },
  },
})

export const evaluationBarStyles = stylex.create({
  bar: {
    "--evaluation-white-share": "50%",
    position: "relative",
    isolation: "isolate",
    display: "flex",
    width: "2.15rem",
    minHeight: "16rem",
    flexBasis: "2.15rem",
    flexGrow: 0,
    flexShrink: 0,
    flexDirection: "column",
    gap: "0.375rem",
    color: "var(--color-text-primary)",
  },
  track: {
    position: "relative",
    display: "block",
    overflow: "hidden",
    width: "100%",
    height: "100%",
    minHeight: 0,
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: "auto",
    borderRadius: "0.42rem 0.5rem 0.44rem 0.48rem",
    backgroundColor: "var(--color-text-primary)",
    backgroundImage:
      "repeating-linear-gradient(92deg, rgb(255 255 255 / 0.05) 0, rgb(255 255 255 / 0.05) 1px, transparent 1px, transparent 4px), radial-gradient(ellipse at 68% 6%, color-mix(in srgb, var(--color-mist) 20%, transparent), transparent 26%)",
    boxShadow:
      "inset 0 0 0 1px color-mix(in srgb, var(--color-ink) 55%, transparent)",
  },
  white: {
    position: "absolute",
    right: 0,
    bottom: 0,
    left: 0,
    height: "var(--evaluation-white-share)",
    backgroundColor: "var(--color-background-surface)",
    backgroundImage:
      "repeating-linear-gradient(92deg, color-mix(in srgb, var(--color-ink) 5%, transparent) 0, color-mix(in srgb, var(--color-ink) 5%, transparent) 1px, transparent 1px, transparent 4px)",
    boxShadow: "0 -1px 0 color-mix(in srgb, var(--color-ink) 50%, transparent)",
    transition: {
      default: "height 200ms cubic-bezier(0.23, 1, 0.32, 1)",
      [reduceMotion]: "none",
    },
  },
  value: {
    flexGrow: 0,
    flexShrink: 0,
    padding: "0.25rem 0.125rem",
    borderRadius: "0.28rem 0.34rem 0.3rem 0.32rem",
    backgroundColor:
      "color-mix(in srgb, var(--color-paper-raised) 72%, transparent)",
    boxShadow:
      "inset 0 0 0 1px color-mix(in srgb, var(--color-ink) 28%, transparent)",
    color: "var(--color-text-primary)",
    fontFamily: "var(--font-family-heading)",
    fontSize: "0.68rem",
    fontWeight: 640,
    fontVariantNumeric: "tabular-nums",
    letterSpacing: "-0.01em",
    lineHeight: 1,
    textAlign: "center",
  },
})

export const chessboardStyles = stylex.create({
  frame: {
    "--board-data-frame-color":
      "color-mix(in srgb, var(--color-text-primary) 68%, black)",
    position: "relative",
    isolation: "isolate",
    minWidth: 0,
    width: "min(38rem, 100%)",
    padding: "clamp(1.1rem, 2.4vw, 1.5rem)",
    "::before": {
      "--watercolor-brush-weight": "0.72rem",
      position: "absolute",
      zIndex: 4,
      inset: 0,
      backgroundColor: "var(--board-data-frame-color)",
      content: '""',
      opacity: 0.92,
      pointerEvents: "none",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
      animationName: paintFrame,
      animationDuration: { default: "700ms", [reduceMotion]: "0s" },
      animationTimingFunction: "cubic-bezier(0.23, 1, 0.32, 1)",
      animationFillMode: "both",
    },
    "::after": {
      position: "absolute",
      zIndex: -1,
      inset: "0.24rem",
      backgroundColor:
        "color-mix(in srgb, var(--color-paper-raised) 78%, transparent)",
      backgroundImage:
        "radial-gradient(ellipse at 7% 4%, color-mix(in srgb, var(--color-mist) 28%, transparent), transparent 35%)",
      content: '""',
    },
  },
  /** Recent-game tiles and other positional thumbs: the square fills its
   * host and the brush frame thins so a 5–8rem board still reads. */
  preview: {
    width: "100%",
    minWidth: 0,
    padding: "0.25rem",
    "::before": {
      "--watercolor-brush-weight": "0.34rem",
    },
    "::after": {
      inset: "0.1rem",
    },
  },
})

/** The review tone palette. The moment card and the evaluation graph both
 * paint from `--review-moment-color`; the tone class alone carries no
 * visuals since the craft moved to StyleX. */
export const momentToneStyles = stylex.create({
  improvement: { "--review-moment-color": "var(--color-moment-improvement)" },
  positive: { "--review-moment-color": "var(--color-moment-positive)" },
  selected: { "--review-moment-color": "var(--color-ink-soft)" },
})

export const momentCardStyles = stylex.create({
  card: {
    "--review-moment-color": "var(--color-icon-secondary)",
    "--watercolor-button-fill":
      "linear-gradient(var(--review-moment-color), var(--review-moment-color))",
    "--watercolor-button-inner":
      "var(--color-background-body, rgb(255 255 255 / 0.56))",
    "--watercolor-button-inner-inset": "0.02rem 0.16rem",
    /* A moment's frame is its selected state (`current`), never a hover. */
    "--watercolor-button-stroke-opacity": "0",
    display: "flex",
    width: "100%",
    minHeight: "4.4rem",
    alignItems: "center",
    justifyContent: "flex-start",
    gap: "0.5rem",
    padding: "0.5rem 0.75rem",
    color: "inherit",
    fontFamily: "inherit",
    fontSize: "0.82rem",
    lineHeight: 1.3,
    textAlign: "left",
    whiteSpace: "normal",
    transform: {
      default: null,
      ":hover": "none",
      ":active": "scale(0.995)",
      ":disabled": "none",
    },
    opacity: { default: null, ":disabled": 0.48 },
  },
  current: {
    "--watercolor-button-stroke-opacity": "1",
    "--watercolor-button-inner":
      "color-mix(in srgb, var(--review-moment-color) 14%, var(--color-background-body, rgb(255 255 255 / 0.56)))",
  },
  glyph: {
    "--symbol-ink": "var(--review-moment-color)",
    width: "2rem",
    height: "2rem",
    fontSize: "0.82rem",
    fontWeight: 800,
  },
  copy: {
    display: "grid",
    minWidth: 0,
    gap: "0.125rem",
  },
  move: {
    color: "inherit",
    fontFamily: "inherit",
    fontSize: "inherit",
    lineHeight: "inherit",
    fontWeight: 700,
  },
  detail: {
    color: "var(--color-text-disabled)",
    fontSize: "0.72rem",
    fontWeight: 400,
    letterSpacing: "normal",
  },
  /* The widget density: the stamp and its copy share a host viewport with a
     board, so both come down a step and the detail stays on one line. */
  glyphCompact: {
    width: "1.65rem",
    height: "1.65rem",
  },
  copyCompact: {
    gap: "0.125rem",
  },
  detailCompact: {
    whiteSpace: "nowrap",
  },
})

/**
 * The chat wash: ChenChess skins Astryx's ChatMessageBubble with uneven ink
 * corners and paper/tone washes instead of the stock sender colors. The coach
 * speaks on ivory paper with a fine ink edge; the Player on a bamboo-tinted
 * wash; system notes sit on the muted paper.
 */
/**
 * Backdrops for the two container surfaces: the chat bubble and the dialog.
 *
 * `painted` masks a flat tone with the square ink blot, so the tone comes from
 * the surface and the artwork only decides where paint landed. `cloud` places
 * the one full-color asset behind the copy, faded so type keeps its contrast —
 * paint, so it never repeats within a view.
 */
export const backdropStyles = stylex.create({
  /**
   * The splash the copy sits on. The blot reaches well past the text box on
   * every side, so what the reader sees is a torn-edged patch of pigment with
   * words on it — not a rectangle with a texture behind it. The host drops its
   * own fill and border for this reason; the paint is the surface.
   */
  painted: {
    "::after": {
      position: "absolute",
      zIndex: -1,
      inset: "-0.85rem -1.25rem -1rem -1.1rem",
      backgroundColor: "var(--watercolor-backdrop-ink, var(--color-border))",
      /* The pooling ring, and the real watercolor painting filling the whole
         drop where the app loads `surfaces.css`. The pseudo's own opacity
         keeps the painting as faint as the pigment it rides on. */
      backgroundImage: {
        default: null,
        [supportsTornSilhouette]:
          "radial-gradient(ellipse at 50% 48%, transparent 54%, color-mix(in srgb, var(--watercolor-backdrop-ink, var(--color-border)) 55%, transparent) 97%), var(--watercolor-splash-texture, linear-gradient(transparent, transparent))",
      },
      backgroundPosition: {
        default: null,
        [supportsTornSilhouette]: "center",
      },
      backgroundRepeat: {
        default: null,
        [supportsTornSilhouette]: "no-repeat",
      },
      backgroundSize: { default: null, [supportsTornSilhouette]: "cover" },
      content: '""',
      opacity: "var(--watercolor-backdrop-opacity, 0.22)",
      pointerEvents: "none",
      /* Without shape() the splash is the ink-square raster; with it the
         generated splash silhouette takes over — a soft-edged drop of
         pigment, not a dry-brushed stamp. */
      mask: {
        default: "var(--watercolor-ink-square) center / 100% 100% no-repeat",
        [supportsTornSilhouette]: "none",
      },
      clipPath: {
        default: null,
        [supportsTornSilhouette]: "var(--watercolor-shape-splash-calm-a)",
      },
      filter: { default: null, [supportsTornSilhouette]: "blur(1px)" },
    },
    /* A second, smaller pull, mirrored and rotated: one drop reads as a
       printed shape, two overlapping read as a wet edge where the water went
       back over itself. */
    "::before": {
      position: "absolute",
      zIndex: -1,
      inset: "-0.3rem -0.5rem -0.75rem -0.35rem",
      backgroundColor: "var(--watercolor-backdrop-ink, var(--color-border))",
      content: '""',
      opacity:
        "calc(var(--watercolor-backdrop-opacity, 0.22) * var(--watercolor-backdrop-second-pull, 0.7))",
      pointerEvents: "none",
      transform: "scaleX(-1) rotate(1.4deg)",
      mask: {
        default: "var(--watercolor-ink-square) center / 100% 100% no-repeat",
        [supportsTornSilhouette]: "none",
      },
      clipPath: {
        default: null,
        [supportsTornSilhouette]: "var(--watercolor-shape-splash-calm-b)",
      },
      filter: { default: null, [supportsTornSilhouette]: "blur(1.5px)" },
    },
  },
  /** The host's own box, dropped so the splash is the only surface. */
  unboxed: {
    borderRadius: 0,
    backgroundColor: "transparent",
    backgroundImage: "none",
    boxShadow: "none",
  },
  cloud: {
    "::after": {
      position: "absolute",
      zIndex: -1,
      inset: 0,
      backgroundImage: "var(--watercolor-cloud-wash)",
      backgroundPosition:
        "var(--watercolor-cloud-position, right -18% top -22%)",
      backgroundRepeat: "no-repeat",
      backgroundSize: "var(--watercolor-cloud-size, 62% auto)",
      borderRadius: "inherit",
      /* On a torn surface the host names its silhouette through
         `--watercolor-surface-clip`, and the cloud's pigment stays on the
         paper; anywhere else the var is unset and nothing is clipped. */
      clipPath: {
        default: null,
        [supportsTornSilhouette]: "var(--watercolor-surface-clip, none)",
      },
      content: '""',
      opacity: "var(--watercolor-cloud-opacity, 0.5)",
      pointerEvents: "none",
    },
  },
  /** The cloud as a quiet tint rather than a picture: small, cornered, and
   * faint enough to read as pigment in the paper. */
  cloudTint: {
    "--watercolor-cloud-opacity": "0.3",
    "--watercolor-cloud-position": "right -12% bottom -30%",
    "--watercolor-cloud-size": "48% auto",
  },
})

export const chatStyles = stylex.create({
  bubble: {
    display: "flex",
    flexDirection: "column",
    gap: "0.5rem",
    minWidth: 0,
    maxWidth: "100%",
    overflow: "hidden",
    borderRadius: "0.16rem 0.55rem 0.2rem 0.45rem",
    boxShadow:
      "inset 0 0 0 1px color-mix(in srgb, var(--color-text-primary) 22%, transparent), 0 0.35rem 1rem color-mix(in srgb, var(--color-ink) 5%, transparent)",
    color: "var(--color-text-primary)",
  },
  ghost: {
    width: "100%",
    backgroundColor: "transparent",
    backgroundImage: "none",
    boxShadow: "none",
    borderRadius: 0,
  },
  coach: {
    backgroundColor:
      "color-mix(in srgb, var(--color-paper-raised) 97%, transparent)",
    backgroundImage:
      "radial-gradient(ellipse at 96% 4%, color-mix(in srgb, var(--color-mist) 14%, transparent), transparent 32%)",
  },
  player: {
    backgroundColor:
      "color-mix(in srgb, var(--color-success) 14%, var(--color-background-surface))",
    boxShadow:
      "inset 0 0 0 1px color-mix(in srgb, var(--color-success) 42%, transparent), 0 0.35rem 1rem color-mix(in srgb, var(--color-ink) 5%, transparent)",
  },
  system: {
    backgroundColor: "var(--color-background-muted)",
    color: "var(--color-text-disabled)",
  },
  /** The bubble sits over its own painted patch, so a run of messages reads as
   * ink laid on paper instead of a stack of identical rectangles. */
  backdropHost: {
    position: "relative",
    isolation: "isolate",
  },
  /* Strong enough that the copy reads as sitting on pigment, light enough that
     navy type keeps its contrast against the paper showing through. */
  coachBackdrop: {
    "--watercolor-backdrop-ink": "var(--color-text-secondary)",
    "--watercolor-backdrop-opacity": "0.3",
  },
  playerBackdrop: {
    "--watercolor-backdrop-ink": "var(--color-success)",
    "--watercolor-backdrop-opacity": "0.36",
  },
  systemBackdrop: {
    "--watercolor-backdrop-ink": "var(--color-text-disabled)",
    "--watercolor-backdrop-opacity": "0.24",
  },
  /** Copy sitting on paint wants a little more room than copy in a box: the
   * torn edge of the splash should not crowd the first and last words. */
  splashPadding: {
    padding: "0.75rem 1rem",
  },
})

/**
 * The dialog and its tooltip sibling. The dialog is a paper card with the ink
 * frame the rest of the surfaces wear; the backdrop behind it is an ink wash
 * rather than the stock scrim.
 */
export const dialogStyles = stylex.create({
  surface: {
    "--watercolor-brush-weight": "0.26rem",
    "--watercolor-brush-sizes":
      "102.2% 0.29rem, 0.22rem 103.2%, 100.6% 0.26rem, 0.3rem 102%",
    "--watercolor-dialog-frame-ink":
      "color-mix(in srgb, var(--color-text-primary) 58%, black)",
    "--watercolor-dialog-paper": "rgb(255 249 237 / 0.98)",
    "--watercolor-dialog-paper-image":
      "linear-gradient(transparent, transparent)",
    "--watercolor-dialog-rim":
      "radial-gradient(ellipse 130% 118% at 50% 46%, transparent 64%, color-mix(in srgb, var(--color-mist) 26%, transparent) 93%)",
    "--watercolor-dialog-shape": "var(--watercolor-shape-splash-heavy)",
    position: "relative",
    isolation: "isolate",
    overflow: "visible",
    borderWidth: "0.19rem",
    borderStyle: "solid",
    borderColor: "var(--watercolor-dialog-frame-ink)",
    borderRadius: "0.06rem 0.14rem 0.04rem 0.11rem",
    backgroundColor: "var(--watercolor-dialog-paper)",
    color: "var(--color-text-primary)",
    boxShadow: "0 1.4rem 3.4rem rgb(20 43 70 / 0.26)",
    "::before": {
      position: "absolute",
      zIndex: 3,
      inset: "-0.13rem -0.03rem -0.17rem -0.09rem",
      backgroundColor: "var(--watercolor-dialog-frame-ink)",
      content: '""',
      pointerEvents: "none",
      mask: "var(--watercolor-brush-frame)",
      maskSize: "var(--watercolor-brush-sizes)",
      animationName: paintFrame,
      animationDuration: { default: "560ms", [reduceMotion]: "0s" },
      animationTimingFunction: "cubic-bezier(0.23, 1, 0.32, 1)",
      animationFillMode: "both",
    },
    "::backdrop": {
      backgroundColor: "rgb(20 43 70 / 0.34)",
      backdropFilter: "blur(2px)",
    },
  },
  /** The cloud painting behind the copy, top-right, where dialog content is
   * thinnest. */
  cloudSurface: {
    "--watercolor-cloud-opacity": "0.42",
  },
  /**
   * The splash reading of the dialog. Splash lives only on a coloured
   * surface, and the ink backdrop is the dialog's coloured surface — the
   * paper and cloud readings keep the framed rectangle. Where shape() exists
   * the host hands everything to the sheet element and the frame retires.
   */
  splashSurface: {
    /* The cloud ::after reads this so its paint stays on the torn paper. */
    "--watercolor-surface-clip": "var(--watercolor-dialog-shape)",
    borderColor: {
      default: "var(--watercolor-dialog-frame-ink)",
      [supportsTornSilhouette]: "transparent",
    },
    backgroundColor: {
      default: "var(--watercolor-dialog-paper)",
      [supportsTornSilhouette]: "transparent",
    },
    backgroundImage: {
      default: "var(--watercolor-dialog-host-image, none)",
      [supportsTornSilhouette]: "none",
    },
    boxShadow: {
      default: "0 1.4rem 3.4rem rgb(20 43 70 / 0.26)",
      [supportsTornSilhouette]: "none",
    },
    "::before": {
      display: { default: null, [supportsTornSilhouette]: "none" },
    },
  },
  /** The dark reading of the dialog: navy wash, ivory type, the watercolor
   * painting filling the whole panel. For a standing moment, not a
   * confirmation. */
  inkSurface: {
    "--watercolor-cloud-opacity": "0.4",
    "--watercolor-cloud-position": "center",
    "--watercolor-cloud-size": "cover",
    /* Computed from --color-ink-deep, NOT --color-text-primary: this rule
       remaps the text tokens to paper values, and custom properties resolve
       at computed-value time — the swapped read would paint the frame as
       paper mixed with black instead of a deeper navy. */
    "--watercolor-dialog-frame-ink":
      "color-mix(in srgb, var(--color-ink-deep) 82%, black)",
    "--watercolor-dialog-paper": "#142b46",
    "--watercolor-dialog-rim":
      "radial-gradient(ellipse 130% 118% at 50% 46%, transparent 64%, rgb(6 15 26 / 0.5) 94%)",
    "--watercolor-dialog-paper-image":
      "radial-gradient(ellipse at 92% -10%, rgb(168 190 208 / 0.2), transparent 44%), linear-gradient(135deg, #142b46, #101f32)",
    "--watercolor-dialog-shape": "var(--watercolor-shape-panel)",
    /* Astryx's Text reads the theme's own colour tokens, so a dark surface has
       to hand it paper values or the copy stays navy on navy. Swap the surface
       token with them: the pair is what every control reads for ink-on-paper,
       so inverting both together flips the buttons instead of erasing them. */
    "--color-text-primary": "#fff9ed",
    "--color-text-secondary": "#cddbe6",
    "--color-text-disabled": "rgb(255 249 237 / 0.72)",
    "--color-background-surface": "#142b46",
    "--watercolor-dialog-host-image":
      "radial-gradient(ellipse at 92% -10%, rgb(168 190 208 / 0.2), transparent 44%), linear-gradient(135deg, #142b46, #101f32)",
    backgroundImage: "var(--watercolor-dialog-host-image, none)",
    /* The remapped text token — paper on navy. Reading the (also remapped)
       surface token here would inherit navy-on-navy into anything that is
       not an Astryx Text. */
    color: "var(--color-text-primary)",
  },
  /* The dialog's torn paper: a real element, because both pseudos are spent
     (the frame and the cloud). It paints the paper and its gradient in every
     browser — behind the host's identical rectangle where shape() is missing,
     as the visible torn slab where it exists. */
  sheet: {
    position: "absolute",
    zIndex: -2,
    /* Vertical bleed pushes the wandering edge outside the content box, so
       the copy keeps its distance without touching the dialog's own padding
       (horizontal bleed would nudge viewport scroll on small screens). */
    inset: { default: 0, [supportsTornSilhouette]: "-0.6rem 0" },
    backgroundColor: "var(--watercolor-dialog-paper)",
    backgroundImage:
      "var(--watercolor-dialog-rim), var(--watercolor-dialog-paper-image)",
    clipPath: {
      default: null,
      [supportsTornSilhouette]: "var(--watercolor-dialog-shape)",
    },
    filter: {
      default: null,
      [supportsTornSilhouette]:
        "blur(1px) drop-shadow(0 1.2rem 2.6rem rgb(20 43 70 / 0.3))",
    },
    pointerEvents: "none",
  },
})
