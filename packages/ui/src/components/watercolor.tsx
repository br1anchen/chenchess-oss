import { Icon } from "../icons"
import { ChatMessageBubble } from "@astryxdesign/core/Chat"
import { Dialog } from "@astryxdesign/core/Dialog"
import { Tooltip } from "@astryxdesign/core/Tooltip"
import { HStack } from "@astryxdesign/core/HStack"
import { Text } from "@astryxdesign/core/Text"
import { VStack } from "@astryxdesign/core/VStack"
import * as stylex from "@stylexjs/stylex"
import { createContext, useContext, useId } from "react"
import type {
  ComponentProps,
  CSSProperties,
  HTMLAttributes,
  KeyboardEvent,
  ReactNode,
  Ref,
} from "react"

import {
  PresentationalChessboard,
  type BoardTransition,
} from "../board/PresentationalChessboard"
/**
 * The one asset this module names directly. Importing the `brandAssets`
 * manifest instead would pull every URL in it into each single-file Coach App
 * artifact, duplicating the payloads the stylesheet already inlines.
 */
import brushSwoosh from "../assets/brand/brush/brush-swoosh.svg?url"
import { brandWorkspaceAssets } from "../workspace/brandWorkspaceAssets"
import type { BoardArrow, BoardPresentation } from "../contracts"
import { EvaluationGraph } from "../review/EvaluationGraph"
import {
  evaluationAt,
  type ReviewMomentMarkerPresentation,
} from "../review/reviewNavigationPresentation"
import {
  backdropStyles,
  badgeStyles,
  buttonStyles,
  chatStyles,
  dialogStyles,
  cardPartStyles,
  cardStyles,
  chatComposerStyles,
  checkboxStyles,
  chessboardStyles,
  chipStyles,
  evaluationBarStyles,
  eyebrowStyles,
  fieldStyles,
  headerActionStyles,
  hoverWashStyles,
  inkStrokeStyles,
  momentCardStyles,
  momentToneStyles,
  moveNavStyles,
  noticeStyles,
  plaqueStyles,
  progressStyles,
  spinnerStyles,
  studioStyles,
  symbolStyles,
} from "./watercolor.styles"

/**
 * A compiled style (or array of them) from `watercolor.styles.ts`. Typed as an
 * opaque object: StyleX's published types reject valid authored styles
 * (conditions on `animationName` inside a pseudo-element, custom properties),
 * while the compiler validates the real constraint at build time.
 */
type StyleXStyles = object

/**
 * Compose compiled StyleX craft with the structural `chen-watercolor-*` class
 * hooks. The classes carry no visuals any more — they exist for per-surface
 * layout CSS and for tests to address the primitives.
 */
function craft(
  classNames: ReadonlyArray<string | false | undefined | null>,
  ...styles: ReadonlyArray<StyleXStyles | false | undefined | null>
) {
  // SAFETY: every argument comes from `stylex.create` in watercolor.styles.ts;
  // the compiler validated the declarations, the published parameter type just
  // cannot express them (conditions inside pseudo-element blocks).
  const applyStyles = stylex.props as (
    ...applied: readonly unknown[]
  ) => ReturnType<typeof stylex.props>
  const sx = applyStyles(...styles.filter(Boolean))
  return {
    ...sx,
    className: [sx.className, ...classNames].filter(Boolean).join(" "),
  }
}

type WatercolorButtonVariant =
  | "primary"
  | "secondary"
  | "outline"
  | "quiet"
  | "danger"
type WatercolorButtonSize = "sm" | "md" | "lg" | "icon"

type WatercolorButtonProps = Omit<ComponentProps<"button">, "children"> & {
  block?: boolean
  children?: ReactNode
  /** Which hover repaint the control wears. `bloom` is for controls the size
   * of a card, where a stroke as wide as the surface reads as a fill. */
  hoverWash?: WatercolorHoverWashKind
  loading?: boolean
  size?: WatercolorButtonSize
  variant?: WatercolorButtonVariant
  xstyle?: StyleXStyles
}

const buttonVariantStyle = {
  danger: buttonStyles.danger,
  outline: buttonStyles.secondary,
  primary: buttonStyles.primary,
  quiet: buttonStyles.quiet,
  secondary: buttonStyles.secondary,
} as const

type WatercolorHoverWashKind = "stroke" | "bloom" | "none"

function SessionHeaderLabel({ children }: { children: ReactNode }) {
  return (
    <span {...craft(["chen-session-header-label"], headerActionStyles.label)}>
      {children}
    </span>
  )
}

/**
 * The dry-brush highlight that paints across a control on hover. An element
 * rather than a pseudo-element: the button already spends both of its own on
 * the brushed fill and the inner paper.
 */
function WatercolorHoverWash({
  kind,
  size,
}: {
  kind: WatercolorHoverWashKind
  size: WatercolorButtonSize
}) {
  if (kind === "none") return null
  return (
    <span
      aria-hidden="true"
      {...craft(
        ["chen-watercolor-hover-wash"],
        hoverWashStyles.wash,
        kind === "bloom" && hoverWashStyles.bloom,
        kind === "stroke" && size === "icon" && hoverWashStyles.compact,
      )}
    />
  )
}

/**
 * The button craft, shared by the native button and the anchor. The sweep
 * clip belongs to pale stroke controls alone: a filled control wears both
 * passes at once, and a bloom control's frame is its identity.
 */
function buttonCraft({
  block,
  className,
  hoverWash,
  inert = false,
  size,
  variant,
  xstyle,
}: {
  block: boolean
  className?: string
  hoverWash: WatercolorHoverWashKind
  inert?: boolean
  size: WatercolorButtonSize
  variant: WatercolorButtonVariant
  xstyle?: StyleXStyles
}) {
  const filled = variant === "primary" || variant === "danger"
  return craft(
    [
      "chen-watercolor-button",
      variant === "outline"
        ? "chen-watercolor-button-secondary"
        : `chen-watercolor-button-${variant}`,
      `chen-watercolor-button-${size}`,
      block && "chen-watercolor-button-block",
      className,
    ],
    buttonStyles.base,
    hoverWash === "stroke" && !filled && buttonStyles.strokeClip,
    buttonVariantStyle[variant],
    buttonStyles[size],
    block && buttonStyles.block,
    block && filled && buttonStyles.blockWideMask,
    xstyle,
    inert && buttonStyles.disabledCraft,
  )
}

function WatercolorButton({
  block = false,
  children,
  className,
  disabled,
  hoverWash = "stroke",
  loading = false,
  size = "md",
  style,
  type = "button",
  variant = "primary",
  xstyle,
  ...props
}: WatercolorButtonProps) {
  const inert = disabled || loading
  const sx = buttonCraft({
    block,
    className,
    hoverWash,
    inert,
    size,
    variant,
    xstyle,
  })
  return (
    <button
      aria-busy={loading || undefined}
      data-variant={variant}
      data-watercolor-control="button"
      disabled={inert}
      type={type}
      {...props}
      {...sx}
      style={{ ...sx.style, ...style }}
    >
      {/* No wash at all while disabled or loading: the hover switch is a
          custom property flipped under `:hover`, and StyleX orders `:hover`
          after `:disabled`, so a pointer resting on a disabled control would
          otherwise still light it. */}
      <WatercolorHoverWash kind={inert ? "none" : hoverWash} size={size} />
      {loading ? (
        <span
          aria-hidden="true"
          {...craft(["chen-watercolor-spinner"], spinnerStyles.spinner)}
        />
      ) : null}
      {children}
    </button>
  )
}

type WatercolorButtonLinkProps = Omit<ComponentProps<"a">, "children"> & {
  block?: boolean
  children?: ReactNode
  /** See `WatercolorButtonProps["hoverWash"]`. */
  hoverWash?: WatercolorHoverWashKind
  size?: WatercolorButtonSize
  variant?: WatercolorButtonVariant
  xstyle?: StyleXStyles
}

/** The button craft on a native anchor, for links that read as actions
 * (landing calls to action, sign-in handoffs). */
function WatercolorButtonLink({
  block = false,
  children,
  className,
  hoverWash = "stroke",
  size = "md",
  style,
  variant = "primary",
  xstyle,
  ...props
}: WatercolorButtonLinkProps) {
  const sx = buttonCraft({ block, className, hoverWash, size, variant, xstyle })
  return (
    <a
      data-variant={variant}
      data-watercolor-control="button"
      {...props}
      {...sx}
      style={{ ...sx.style, ...style }}
    >
      <WatercolorHoverWash kind={hoverWash} size={size} />
      {children}
    </a>
  )
}

type WatercolorCardTone =
  | "paper"
  | "mist"
  | "bamboo"
  | "vermilion"
  | "watercolor"
type WatercolorCardPadding = "compact" | "comfortable"
type WatercolorCardHeadingLevel = 1 | 2 | 3

type WatercolorCardProps = {
  children?: ReactNode
  className?: string
  eyebrow?: ReactNode
  /** Nested inside another card: keep the paper and the spacing, drop the ink
   * frame. Two stacked ink borders read as a rendering fault. */
  frame?: boolean
  headingLevel?: WatercolorCardHeadingLevel
  meta?: ReactNode
  padding?: WatercolorCardPadding
  ref?: Ref<HTMLElement>
  seal?: boolean
  /** The splash reading: the card becomes a lobed drop of the tone's own
   * pigment, filled edge to edge with the watercolor painting, and the ink
   * frame retires. For the surfaces that matter — a featured prompt, a marked
   * moment — never the routine inline card. Coloured tones only: on white
   * paper a splash of paper is invisible, so `tone="paper"` ignores it. */
  splash?: boolean
  title?: ReactNode
  /** Craft for the title plaque, for recipes with their own title type. */
  titleXstyle?: StyleXStyles
  tone?: WatercolorCardTone
  xstyle?: StyleXStyles
} & Omit<HTMLAttributes<HTMLElement>, "title">

const cardToneStyle = {
  bamboo: cardStyles.bamboo,
  mist: cardStyles.mist,
  paper: cardStyles.paper,
  vermilion: cardStyles.vermilion,
  watercolor: cardStyles.watercolor,
} as const

function watercolorCardSurface({
  className,
  composed,
  frame,
  padding,
  seal,
  splashApplied,
  tone,
  xstyle,
}: {
  className?: string
  composed: boolean
  frame: boolean
  padding: WatercolorCardPadding
  seal: boolean
  splashApplied: boolean
  tone: WatercolorCardTone
  xstyle?: StyleXStyles
}) {
  return craft(
    [
      "chen-watercolor-card",
      `chen-watercolor-card-${tone}`,
      `chen-watercolor-card-${padding}`,
      seal ? "chen-watercolor-card-has-seal" : undefined,
      frame ? undefined : "chen-watercolor-card-flat",
      splashApplied ? "chen-watercolor-card-splash" : undefined,
      className,
    ],
    cardStyles.base,
    cardToneStyle[tone],
    cardStyles[padding],
    composed && cardStyles.content,
    /* contentPaper re-papers the composed card in ivory — exactly the pigment
       the splash fill replaces, so it stands down when the splash is on. */
    composed &&
      tone !== "watercolor" &&
      !splashApplied &&
      cardStyles.contentPaper,
    seal && cardStyles.hasSeal,
    splashApplied && cardStyles.splash,
    splashApplied && padding === "compact" && cardStyles.splashCalm,
    !frame && cardStyles.flat,
    xstyle,
  )
}

function WatercolorCard({
  children,
  className,
  eyebrow,
  frame = true,
  headingLevel = 3,
  meta,
  padding = "comfortable",
  ref,
  seal = false,
  splash = false,
  style,
  title,
  titleXstyle,
  tone = "paper",
  xstyle,
  ...props
}: WatercolorCardProps) {
  const composed = Boolean(eyebrow || meta || title)
  /* Splash needs pigment: white paper has none to spread. */
  const splashApplied = splash && tone !== "paper"
  const sx = watercolorCardSurface({
    className,
    composed,
    frame,
    padding,
    seal,
    splashApplied,
    tone,
    xstyle,
  })
  return (
    <article
      data-watercolor-composition={composed ? "content" : undefined}
      data-watercolor-frame={frame ? undefined : "none"}
      data-watercolor-splash={splashApplied ? "" : undefined}
      data-watercolor-surface="card"
      ref={ref}
      {...props}
      {...sx}
      style={{ ...sx.style, ...style }}
    >
      {composed ? (
        <WatercolorCardMasthead
          eyebrow={eyebrow}
          headingLevel={headingLevel}
          meta={meta}
          title={title}
          titleXstyle={titleXstyle}
        />
      ) : null}
      {children}
    </article>
  )
}

/**
 * The masthead (and every text run in this module) renders native elements
 * styled by StyleX directly. Routing them through Astryx Text/Heading puts two
 * copies of the same atom hash on the element — Astryx ships the identical
 * class names behind `:not(#\#)` specificity hacks — and which copy wins is a
 * source-order coin flip. Native elements keep the craft deterministic.
 */
function WatercolorCardMasthead({
  eyebrow,
  headingLevel,
  meta,
  title,
  titleXstyle,
}: Pick<
  WatercolorCardProps,
  "eyebrow" | "headingLevel" | "meta" | "title" | "titleXstyle"
>) {
  const TitleTag = `h${headingLevel ?? 3}` as const
  return (
    <div {...craft(["chen-watercolor-card-masthead"], cardPartStyles.masthead)}>
      {title ? (
        <TitleTag
          {...craft(["chen-watercolor-card-title"], cardPartStyles.titleRow)}
        >
          <WatercolorPlaque
            size="lg"
            xstyle={[cardPartStyles.titlePlaque, titleXstyle]}
          >
            {title}
          </WatercolorPlaque>
        </TitleTag>
      ) : null}
      {eyebrow ? <WatercolorEyebrow>{eyebrow}</WatercolorEyebrow> : null}
      {meta ? (
        <div {...craft(["chen-watercolor-card-meta"], cardPartStyles.meta)}>
          {meta}
        </div>
      ) : null}
    </div>
  )
}

type WatercolorCardSectionProps = Omit<HTMLAttributes<HTMLElement>, "color">

function WatercolorCardHeader({
  children,
  className,
  ...props
}: WatercolorCardSectionProps) {
  return (
    <header
      {...props}
      {...craft(
        ["chen-watercolor-card-header", className],
        cardPartStyles.header,
      )}
    >
      {children}
    </header>
  )
}

function WatercolorCardTitle({
  children,
  className,
  ...props
}: WatercolorCardSectionProps) {
  return (
    <h3
      {...props}
      {...craft(
        ["chen-watercolor-card-title", className],
        cardPartStyles.title,
      )}
    >
      {children}
    </h3>
  )
}

function WatercolorCardDescription({
  children,
  className,
  ...props
}: WatercolorCardSectionProps) {
  return (
    <p
      {...props}
      {...craft(
        ["chen-watercolor-card-description", className],
        cardPartStyles.description,
      )}
    >
      {children}
    </p>
  )
}

function WatercolorCardContent({
  children,
  className,
  ...props
}: WatercolorCardSectionProps) {
  return (
    <div
      {...props}
      {...craft(
        ["chen-watercolor-card-content", className],
        cardPartStyles.content,
      )}
    >
      {children}
    </div>
  )
}

function WatercolorCardFooter({
  children,
  className,
  ...props
}: WatercolorCardSectionProps) {
  return (
    <footer
      {...props}
      {...craft(
        ["chen-watercolor-card-footer", className],
        cardPartStyles.footer,
      )}
    >
      {children}
    </footer>
  )
}

type WatercolorEyebrowProps = {
  children?: ReactNode
  className?: string
}

function WatercolorEyebrow({ children, className }: WatercolorEyebrowProps) {
  return (
    <p
      {...craft(["chen-watercolor-eyebrow", className], eyebrowStyles.eyebrow)}
    >
      {children}
    </p>
  )
}

type WatercolorPlaqueSize = "sm" | "md" | "lg"

type WatercolorPlaqueProps = Omit<HTMLAttributes<HTMLSpanElement>, "color"> & {
  size?: WatercolorPlaqueSize
  xstyle?: StyleXStyles
}

/**
 * An ink-splash plaque for a short title, the way the brand art floats
 * "Critical Moment" on a black brush stroke. Place it inside the heading
 * element that owns the copy: `<Heading level={2}><WatercolorPlaque>…`.
 */
function WatercolorPlaque({
  children,
  className,
  size = "md",
  style,
  xstyle,
  ...props
}: WatercolorPlaqueProps) {
  const sx = craft(
    ["chen-watercolor-plaque", className],
    plaqueStyles.base,
    plaqueStyles[size],
    xstyle,
  )
  return (
    <span
      data-watercolor-control="plaque"
      {...props}
      {...sx}
      style={{ ...sx.style, ...style }}
    >
      {children}
    </span>
  )
}

type WatercolorInkStrokeProps = Omit<ComponentProps<"svg">, "children"> & {
  /** Decorative by default; a label announces the stroke as an image. */
  label?: string
  xstyle?: StyleXStyles
}

/**
 * The brush swoosh that paints itself in: real dry-brush artwork inside an
 * alpha `<mask>`, revealed by a guide stroke whose dashoffset sweeps to zero
 * along the swoosh's own spine (the irregular-stroke handwriting trick, with
 * artwork and guide swapped so the guide carries `currentColor`). The right
 * end of the stroke is flat by design — let it bleed off its container, as a
 * divider or an underline behind a hero heading. Size it from the host; ink
 * follows `currentColor`.
 */
function WatercolorInkStroke({
  className,
  label,
  style,
  xstyle,
  ...props
}: WatercolorInkStrokeProps) {
  const maskId = useId()
  const sx = craft(
    ["chen-watercolor-ink-stroke", className],
    inkStrokeStyles.root,
    xstyle,
  )
  return (
    <svg
      aria-hidden={label ? undefined : true}
      aria-label={label}
      data-watercolor-control="ink-stroke"
      preserveAspectRatio="none"
      role={label ? "img" : undefined}
      viewBox="50 250 1000 371"
      {...props}
      {...sx}
      style={{ ...sx.style, ...style }}
    >
      <mask
        id={maskId}
        maskUnits="userSpaceOnUse"
        style={{ maskType: "alpha" }}
        x="50"
        y="250"
        width="1000"
        height="371"
      >
        <image
          href={brushSwoosh}
          preserveAspectRatio="none"
          x="50"
          y="250"
          width="1000"
          height="371"
        />
      </mask>
      {/* The guide follows the swoosh's measured spine; its width only has to
          cover the artwork, the mask supplies every ragged edge. */}
      <path
        d="M58 412 C 240 398, 430 458, 600 493 C 770 468, 900 412, 1050 385"
        fill="none"
        mask={`url(#${maskId})`}
        pathLength={1}
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth={340}
        {...stylex.props(inkStrokeStyles.guide)}
      />
    </svg>
  )
}

type WatercolorSymbolSilhouette = "circle" | "seal" | "soft"
type WatercolorSymbolTone = "watercolor" | "slate" | "bamboo" | "vermilion"

type WatercolorSymbolProps = Omit<HTMLAttributes<HTMLSpanElement>, "color"> & {
  label?: string
  silhouette?: WatercolorSymbolSilhouette
  tone?: WatercolorSymbolTone
  xstyle?: StyleXStyles
}

const symbolSilhouetteStyle = {
  circle: symbolStyles.circle,
  seal: symbolStyles.seal,
  soft: symbolStyles.soft,
} as const

const symbolToneStyle = {
  bamboo: symbolStyles.bamboo,
  slate: symbolStyles.slate,
  vermilion: symbolStyles.vermilion,
  watercolor: symbolStyles.watercolor,
} as const

function WatercolorSymbol({
  children,
  className,
  label,
  silhouette = "soft",
  style,
  tone = "watercolor",
  xstyle,
  ...props
}: WatercolorSymbolProps) {
  const sx = craft(
    [
      "chen-watercolor-symbol",
      `chen-watercolor-symbol-${silhouette}`,
      `chen-watercolor-symbol-${tone}`,
      className,
    ],
    symbolStyles.base,
    symbolSilhouetteStyle[silhouette],
    symbolToneStyle[tone],
    xstyle,
  )
  return (
    <span
      aria-hidden={label ? undefined : true}
      aria-label={label}
      data-watercolor-control="symbol"
      data-watercolor-silhouette={silhouette}
      data-watercolor-tone={tone}
      role={label ? "img" : undefined}
      {...props}
      {...sx}
      style={{ ...sx.style, ...style }}
    >
      {children}
    </span>
  )
}

type WatercolorBadgeTone = "neutral" | "info" | "success" | "warning" | "danger"

type WatercolorBadgeProps = Omit<HTMLAttributes<HTMLSpanElement>, "color"> & {
  tone?: WatercolorBadgeTone
}

const badgeToneStyle = {
  danger: badgeStyles.danger,
  info: badgeStyles.info,
  neutral: badgeStyles.neutral,
  success: badgeStyles.success,
  warning: badgeStyles.warning,
} as const

/** A dry-brush stamp, not a pill. */
function WatercolorBadge({
  children,
  className,
  style,
  tone = "neutral",
  ...props
}: WatercolorBadgeProps) {
  const sx = craft(
    ["chen-watercolor-badge", `chen-watercolor-badge-${tone}`, className],
    badgeStyles.base,
    badgeToneStyle[tone],
  )
  return (
    <span
      data-watercolor-control="badge"
      {...props}
      {...sx}
      style={{ ...sx.style, ...style }}
    >
      {children}
    </span>
  )
}

type WatercolorChipTone =
  | "draw"
  | "loss"
  | "missing"
  | "neutral"
  | "reinforced"
  | "win"

type WatercolorChipProps = Omit<HTMLAttributes<HTMLSpanElement>, "color"> & {
  tone?: WatercolorChipTone
}

const chipToneStyle = {
  draw: chipStyles.draw,
  loss: chipStyles.loss,
  missing: chipStyles.missing,
  neutral: chipStyles.neutral,
  reinforced: chipStyles.reinforced,
  win: chipStyles.win,
} as const

/** A result or idea chip on an ink-tinted wash. */
function WatercolorChip({
  children,
  className,
  style,
  tone = "neutral",
  ...props
}: WatercolorChipProps) {
  const sx = craft(
    ["chen-watercolor-chip", `chen-watercolor-chip-${tone}`, className],
    chipStyles.base,
    chipToneStyle[tone],
  )
  return (
    <span
      data-watercolor-control="chip"
      {...props}
      {...sx}
      style={{ ...sx.style, ...style }}
    >
      {children}
    </span>
  )
}

type WatercolorNoticeAppearance = "compact" | "featured"

type WatercolorNoticeProps = Omit<
  WatercolorCardProps,
  "children" | "seal" | "title"
> & {
  appearance?: WatercolorNoticeAppearance
  children?: ReactNode
  detail?: ReactNode
  glyph: ReactNode
  heading: ReactNode
}

/** A standing message where a surface has no content to show yet. */
function WatercolorNotice({
  appearance = "compact",
  children,
  className,
  detail,
  eyebrow,
  glyph,
  heading,
  meta,
  padding,
  tone = "paper",
  ...props
}: WatercolorNoticeProps) {
  const featured = appearance === "featured"
  const copy = (
    <div
      {...craft(
        ["chen-watercolor-notice-copy"],
        noticeStyles.copy,
        featured && noticeStyles.featuredCopy,
      )}
    >
      {eyebrow ? <WatercolorEyebrow>{eyebrow}</WatercolorEyebrow> : null}
      {featured ? (
        <h2
          {...craft(
            ["chen-watercolor-notice-heading"],
            noticeStyles.heading,
            noticeStyles.featuredHeading,
          )}
        >
          {heading}
        </h2>
      ) : (
        <strong
          {...craft(["chen-watercolor-notice-heading"], noticeStyles.heading)}
        >
          {heading}
        </strong>
      )}
      {meta}
      {detail ? (
        <p
          {...craft(
            ["chen-watercolor-notice-detail"],
            noticeStyles.detail,
            featured && noticeStyles.featuredDetail,
          )}
        >
          {detail}
        </p>
      ) : null}
      {children}
    </div>
  )
  return (
    <WatercolorCard
      className={[
        "chen-watercolor-notice",
        featured ? "chen-watercolor-notice-featured" : undefined,
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      data-watercolor-surface="notice"
      padding={padding ?? (featured ? "comfortable" : "compact")}
      tone={tone}
      {...props}
    >
      {featured ? (
        <VStack
          className="chen-watercolor-notice-body"
          gap={3}
          hAlign="start"
          xstyle={[noticeStyles.body, noticeStyles.featuredBody]}
        >
          <WatercolorSymbol
            silhouette="soft"
            tone={noticeSymbolTone(tone, featured)}
          >
            {glyph}
          </WatercolorSymbol>
          {copy}
        </VStack>
      ) : (
        <HStack
          className="chen-watercolor-notice-body"
          gap={2}
          vAlign="center"
          xstyle={noticeStyles.body}
        >
          <WatercolorSymbol
            silhouette="soft"
            tone={noticeSymbolTone(tone, featured)}
          >
            {glyph}
          </WatercolorSymbol>
          {copy}
        </HStack>
      )}
    </WatercolorCard>
  )
}

function noticeSymbolTone(
  tone: WatercolorCardTone,
  featured: boolean,
): WatercolorSymbolTone {
  switch (tone) {
    case "vermilion":
      return "vermilion"
    case "bamboo":
      return "bamboo"
    case "watercolor":
      return "watercolor"
    case "mist":
      return "slate"
    case "paper":
      return featured ? "bamboo" : "slate"
    default: {
      const _exhaustive: never = tone
      return _exhaustive
    }
  }
}

type WatercolorStudioStyle = CSSProperties & {
  "--chen-review-mist": string
}

type WatercolorStudioProps = Omit<HTMLAttributes<HTMLElement>, "color"> & {
  as?: "div" | "main"
  /** Page padding and column craft the host supplies. */
  xstyle?: StyleXStyles
}

/** The rice-paper page with mountain mist behind studio chrome. */
function WatercolorStudio({
  as = "div",
  children,
  className,
  style,
  xstyle,
  ...props
}: WatercolorStudioProps) {
  const studioStyle: WatercolorStudioStyle = {
    ...style,
    "--chen-review-mist": `url("${brandWorkspaceAssets.mountainMist}")`,
  }
  return (
    <VStack
      as={as}
      className={["chen-watercolor-studio", className]
        .filter(Boolean)
        .join(" ")}
      data-watercolor-surface="studio"
      gap={0}
      hAlign="stretch"
      style={studioStyle}
      xstyle={[studioStyles.studio, xstyle]}
      {...props}
    >
      <span
        aria-hidden="true"
        data-watercolor-surface="studio-mist"
        {...stylex.props(studioStyles.mistRoot)}
      >
        <img
          alt=""
          src={brandWorkspaceAssets.mountainMist}
          {...stylex.props(studioStyles.mist)}
        />
        <span {...stylex.props(studioStyles.mistWash)} />
      </span>
      {children}
    </VStack>
  )
}

type WatercolorFieldProps = ComponentProps<"div"> & {
  error?: string
  hint?: string
  label: ReactNode
}

type WatercolorFieldBinding = {
  controlId: string
  describedBy: string | undefined
  invalid: boolean
}

const WatercolorFieldContext = createContext<WatercolorFieldBinding | null>(
  null,
)

/**
 * What the field tells the control it wraps: which id the label points at,
 * and which note describes it. A control rendered outside a field keeps
 * whatever the caller passed.
 */
function useFieldBinding(
  id: string | undefined,
  describedBy: string | undefined,
) {
  const field = useContext(WatercolorFieldContext)
  const invalid = field?.invalid ?? false
  return {
    control: {
      "aria-describedby": describedBy ?? field?.describedBy,
      "aria-invalid": invalid || undefined,
      id: id ?? field?.controlId,
    },
    frame: {
      "data-invalid": invalid ? ("true" as const) : undefined,
      style: invalid ? fieldStyles.frameInvalid : undefined,
    },
  }
}

/**
 * Label, control and its note as siblings, associated explicitly. The label
 * cannot wrap the control here: a wrapping label folds the hint and the error
 * into the control's accessible name, so a screen reader would announce the
 * character counter as part of the field's name.
 */
function WatercolorField({
  children,
  className,
  error,
  hint,
  label,
  style,
  ...props
}: WatercolorFieldProps) {
  const fieldId = useId()
  const note = error ?? hint
  const noteId = note ? `${fieldId}-note` : undefined
  const sx = craft(["chen-watercolor-field", className], fieldStyles.field)
  return (
    <div
      data-invalid={error ? "true" : undefined}
      {...props}
      {...sx}
      style={{ ...sx.style, ...style }}
    >
      <label
        htmlFor={fieldId}
        {...craft(["chen-watercolor-field-label"], fieldStyles.label)}
      >
        {label}
      </label>
      <WatercolorFieldContext.Provider
        value={{
          controlId: fieldId,
          describedBy: noteId,
          invalid: Boolean(error),
        }}
      >
        {children}
      </WatercolorFieldContext.Provider>
      {note ? (
        <span
          id={noteId}
          role={error ? "alert" : undefined}
          {...craft(
            [
              error
                ? "chen-watercolor-field-error"
                : "chen-watercolor-field-hint",
            ],
            error ? fieldStyles.error : fieldStyles.hint,
          )}
        >
          {note}
        </span>
      ) : null}
    </div>
  )
}

function WatercolorInput({
  "aria-describedby": describedBy,
  className,
  id,
  ...props
}: ComponentProps<"input">) {
  const binding = useFieldBinding(id, describedBy)
  return (
    <span
      data-invalid={binding.frame["data-invalid"]}
      {...craft(
        ["chen-watercolor-input-frame"],
        fieldStyles.frame,
        binding.frame.style,
      )}
    >
      <input
        data-watercolor-control="input"
        {...props}
        {...binding.control}
        {...craft(
          ["chen-watercolor-input", className],
          fieldStyles.input,
          props.type === "date" && fieldStyles.dateInput,
        )}
      />
    </span>
  )
}

function WatercolorTextarea({
  "aria-describedby": describedBy,
  className,
  id,
  ...props
}: ComponentProps<"textarea">) {
  const binding = useFieldBinding(id, describedBy)
  return (
    <span
      data-invalid={binding.frame["data-invalid"]}
      {...craft(
        ["chen-watercolor-input-frame"],
        fieldStyles.frame,
        binding.frame.style,
      )}
    >
      <textarea
        data-watercolor-control="textarea"
        {...props}
        {...binding.control}
        {...craft(
          ["chen-watercolor-input", "chen-watercolor-textarea", className],
          fieldStyles.input,
          fieldStyles.textarea,
        )}
      />
    </span>
  )
}

type WatercolorChatComposerProps = {
  disabled?: boolean
  onChange: (value: string) => void
  onKeyDown?: (event: KeyboardEvent<HTMLTextAreaElement>) => void
  onSend: () => void
  placeholder: string
  value: string
}

/**
 * ChatComposer structure on the watercolor field: one dry-brush box,
 * send seated inside it at the bottom-right. Not Astryx's raised pill,
 * circular ↑, or 1px gray Flat ring.
 */
function WatercolorChatComposer({
  disabled = false,
  onChange,
  onKeyDown,
  onSend,
  placeholder,
  value,
}: WatercolorChatComposerProps) {
  return (
    <span
      data-watercolor-surface="chat-composer"
      {...craft(
        ["chen-watercolor-chat-composer"],
        fieldStyles.frame,
        chatComposerStyles.box,
      )}
    >
      <textarea
        aria-label="Message the coach"
        data-watercolor-control="textarea"
        disabled={disabled}
        maxLength={4096}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        rows={3}
        value={value}
        {...craft(
          ["chen-watercolor-input", "chen-watercolor-textarea"],
          chatComposerStyles.input,
        )}
      />
      <span
        {...craft(
          ["chen-watercolor-chat-composer-send"],
          chatComposerStyles.sendRow,
        )}
      >
        <WatercolorButton
          aria-label="Send"
          disabled={disabled || !value.trim()}
          onClick={onSend}
          size="sm"
          type="button"
          variant="secondary"
          xstyle={chatComposerStyles.sendButton}
        >
          <Icon icon="send" size="sm" />
          <span {...stylex.props(chatComposerStyles.sendLabel)}>Send</span>
        </WatercolorButton>
      </span>
    </span>
  )
}

function WatercolorSelect({
  "aria-describedby": describedBy,
  className,
  id,
  ...props
}: ComponentProps<"select">) {
  const binding = useFieldBinding(id, describedBy)
  return (
    <span
      data-invalid={binding.frame["data-invalid"]}
      {...craft(
        ["chen-watercolor-input-frame"],
        fieldStyles.frame,
        binding.frame.style,
      )}
    >
      <select
        data-watercolor-control="select"
        {...props}
        {...binding.control}
        {...craft(
          ["chen-watercolor-input", "chen-watercolor-select", className],
          fieldStyles.input,
          fieldStyles.select,
        )}
      />
    </span>
  )
}

type WatercolorCheckboxProps = Omit<ComponentProps<"input">, "type"> & {
  label: ReactNode
}

function WatercolorCheckbox({
  className,
  label,
  ...props
}: WatercolorCheckboxProps) {
  return (
    <label
      {...craft(["chen-watercolor-checkbox", className], checkboxStyles.root)}
    >
      <input
        type="checkbox"
        {...props}
        {...stylex.props(checkboxStyles.input)}
      />
      <span
        aria-hidden="true"
        {...craft(["chen-watercolor-checkbox-mark"], checkboxStyles.mark)}
      >
        ✓
      </span>
      <span>{label}</span>
    </label>
  )
}

type WatercolorProgressStyle = CSSProperties & {
  "--watercolor-progress": string
}

type WatercolorProgressProps = Omit<HTMLAttributes<HTMLElement>, "children"> & {
  value: number
}

function WatercolorProgress({
  "aria-label": ariaLabel = "Progress",
  className,
  style,
  value,
  ...props
}: WatercolorProgressProps) {
  const normalizedValue = Math.min(100, Math.max(0, value))
  const progressStyle: WatercolorProgressStyle = {
    ...style,
    "--watercolor-progress": `${normalizedValue}%`,
  }
  return (
    <VStack
      aria-label={ariaLabel}
      aria-valuemax={100}
      aria-valuemin={0}
      aria-valuenow={normalizedValue}
      className={["chen-watercolor-progress", className]
        .filter(Boolean)
        .join(" ")}
      data-watercolor-control="progress"
      role="progressbar"
      style={progressStyle}
      xstyle={progressStyles.track}
      {...props}
    >
      <span aria-hidden="true" {...stylex.props(progressStyles.fill)} />
    </VStack>
  )
}

type WatercolorChatTone = "coach" | "player" | "system"

type WatercolorChatBackdrop = "none" | "patch" | "wash"

type WatercolorChatBubbleProps = ComponentProps<typeof ChatMessageBubble> & {
  /** What sits behind the bubble. `patch` is a dry-brush blot tinted to the
   * tone; `wash` is the cloud painting as a faint tint. Default `none`: in a
   * long thread, artwork on every bubble reads as texture noise, so paint the
   * openers and the moments worth marking. */
  backdrop?: WatercolorChatBackdrop
  tone?: WatercolorChatTone
}

const chatToneStyle = {
  coach: chatStyles.coach,
  player: chatStyles.player,
  system: chatStyles.system,
} as const

/**
 * Astryx's ChatMessageBubble on the watercolor wash: uneven ink corners,
 * paper for the coach, a bamboo tint for the Player. Compose it inside
 * Astryx's ChatMessage / ChatMessageList.
 */
const chatBackdropStyle = {
  coach: chatStyles.coachBackdrop,
  player: chatStyles.playerBackdrop,
  system: chatStyles.systemBackdrop,
} as const

function WatercolorChatBubble({
  backdrop = "none",
  className,
  tone = "coach",
  variant,
  xstyle,
  ...props
}: WatercolorChatBubbleProps) {
  const ghost = variant === "ghost"
  return (
    <ChatMessageBubble
      className={["chen-watercolor-chat-bubble", className]
        .filter(Boolean)
        .join(" ")}
      data-watercolor-backdrop={backdrop === "none" ? undefined : backdrop}
      variant={variant}
      // SAFETY: compiled StyleX from watercolor.styles.ts; the published prop
      // type cannot express the authored conditions (see `craft`).
      xstyle={
        [
          chatStyles.bubble,
          ghost ? chatStyles.ghost : chatToneStyle[tone],
          backdrop !== "none" && chatStyles.backdropHost,
          backdrop === "patch" && chatBackdropStyle[tone],
          backdrop === "patch" && backdropStyles.unboxed,
          backdrop === "patch" && backdropStyles.painted,
          backdrop === "patch" && chatStyles.splashPadding,
          backdrop === "wash" && backdropStyles.cloud,
          backdrop === "wash" && backdropStyles.cloudTint,
          xstyle,
        ].filter(Boolean) as never
      }
      {...props}
    />
  )
}

type WatercolorDialogBackdrop = "paper" | "cloud" | "ink"

type WatercolorDialogProps = ComponentProps<typeof Dialog> & {
  /** `paper` is the routine confirmation. `cloud` floats the painting behind
   * the copy and `ink` inverts to the navy wash — both for standing moments
   * (a first run, a finished review), not for asking a yes/no question. */
  backdrop?: WatercolorDialogBackdrop
}

/**
 * Astryx's Dialog wearing the card's ink frame: paper fill, four dry-brush
 * strokes painted on open, an ink-wash `::backdrop` instead of the stock
 * scrim. Compose the same Layout / DialogHeader children Astryx expects.
 */
function WatercolorDialog({
  backdrop = "paper",
  className,
  children,
  xstyle,
  ...props
}: WatercolorDialogProps) {
  const painted = backdrop !== "paper"
  return (
    <Dialog
      className={["chen-watercolor-dialog", className]
        .filter(Boolean)
        .join(" ")}
      data-watercolor-surface="dialog"
      data-watercolor-backdrop={backdrop}
      // SAFETY: compiled StyleX from watercolor.styles.ts; the published prop
      // type cannot express the authored conditions (see `craft`).
      xstyle={
        [
          dialogStyles.surface,
          backdrop === "cloud" && dialogStyles.cloudSurface,
          backdrop === "ink" && dialogStyles.inkSurface,
          backdrop === "ink" && dialogStyles.splashSurface,
          painted && backdropStyles.cloud,
          /* The caller's craft rides last, so it can extend the surface
             without silently replacing it (xstyle would otherwise arrive
             through {...props} after the authored array and win whole). */
          xstyle,
        ].filter(Boolean) as never
      }
      {...props}
    >
      {backdrop === "ink" ? (
        /* The splash sheet. A real element because the surface's pseudos are
           spent on the frame and the cloud; where shape() is missing it hides
           as a rectangle behind the host's identical paper. Only the ink
           backdrop splashes — splash lives on coloured surfaces, and paper
           and cloud keep the framed rectangle. */
        <span
          aria-hidden
          className="chen-watercolor-dialog-sheet"
          {...stylex.props(dialogStyles.sheet)}
        />
      ) : null}
      {children}
    </Dialog>
  )
}

type WatercolorTooltipProps = ComponentProps<typeof Tooltip>

/**
 * Astryx's Tooltip with the ink surface. Astryx paints the popover itself and
 * exposes no style prop for it, so the craft is registered once on the theme's
 * `tooltip` target (see `theme/inkWash.ts`); this wrapper is the seam product
 * code imports, so the day Astryx opens the surface up nothing else moves.
 */
function WatercolorTooltip(props: WatercolorTooltipProps) {
  return <Tooltip {...props} />
}

type WatercolorChessboardProps = HTMLAttributes<HTMLElement> & {
  arrows?: readonly BoardArrow[]
  board: BoardPresentation
  /** `preview` is a positional thumb: full host width, thin frame. */
  density?: "default" | "preview"
  transition?: BoardTransition
  /** Host sizing — the width cap a widget or column puts on the square. */
  xstyle?: StyleXStyles
}

// Astryx's `xstyle` uses StyleX's published prop types, which lag the runtime:
// a condition on an animation property inside a pseudo-element block compiles
// but does not type. Route the one affected style around the type only.
// SAFETY: the compiled style is valid at runtime; `undefined` is the widest
// value Astryx's `xstyle` prop type accepts without re-declaring its generics.
const chessboardFrameStyle = chessboardStyles.frame as unknown as undefined

function WatercolorChessboard({
  arrows = [],
  board,
  className,
  density = "default",
  transition,
  xstyle,
  ...props
}: WatercolorChessboardProps) {
  return (
    <VStack
      className={["chen-watercolor-chessboard", className]
        .filter(Boolean)
        .join(" ")}
      data-board-density={density === "preview" ? "preview" : undefined}
      data-watercolor-surface="chessboard"
      // SAFETY: compiled StyleX; see `chessboardFrameStyle` above.
      xstyle={
        [
          chessboardFrameStyle,
          density === "preview" && chessboardStyles.preview,
          xstyle,
        ].filter(Boolean) as never
      }
      {...props}
    >
      <PresentationalChessboard
        arrows={arrows}
        board={board}
        transition={transition}
      />
    </VStack>
  )
}

type WatercolorEvaluationBarProps = Omit<
  HTMLAttributes<HTMLElement>,
  "children"
> & {
  valueLabel: string
  whiteShare: number
  /** Host sizing — the height a widget's board row gives the bar. */
  xstyle?: StyleXStyles
}

type WatercolorEvaluationBarStyle = CSSProperties & {
  "--evaluation-white-share": string
}

function WatercolorEvaluationBar({
  "aria-label": ariaLabel = "Position evaluation",
  className,
  style,
  valueLabel,
  whiteShare,
  xstyle,
  ...props
}: WatercolorEvaluationBarProps) {
  const normalizedWhiteShare = Math.min(100, Math.max(0, whiteShare))
  const evaluationStyle: WatercolorEvaluationBarStyle = {
    ...style,
    "--evaluation-white-share": `${normalizedWhiteShare}%`,
  }

  return (
    <VStack
      aria-label={ariaLabel}
      aria-valuemax={100}
      aria-valuemin={0}
      aria-valuenow={Math.round(normalizedWhiteShare)}
      aria-valuetext={valueLabel}
      className={["chen-watercolor-evaluation-bar", className]
        .filter(Boolean)
        .join(" ")}
      data-watercolor-control="evaluation-bar"
      role="meter"
      style={evaluationStyle}
      // SAFETY: compiled StyleX from watercolor.styles.ts; see `craft`.
      xstyle={[evaluationBarStyles.bar, xstyle] as never}
      {...props}
    >
      <span
        aria-hidden="true"
        {...craft(
          ["chen-watercolor-evaluation-bar-track"],
          evaluationBarStyles.track,
        )}
      >
        <span
          {...craft(
            ["chen-watercolor-evaluation-bar-white"],
            evaluationBarStyles.white,
          )}
        />
      </span>
      <output aria-hidden="true" {...stylex.props(evaluationBarStyles.value)}>
        {valueLabel}
      </output>
    </VStack>
  )
}

const momentSymbolTone = {
  improvement: "vermilion",
  positive: "bamboo",
  selected: "slate",
} as const satisfies Record<
  ReviewMomentMarkerPresentation["tone"],
  WatercolorSymbolTone
>

type WatercolorMomentCardProps = Omit<
  WatercolorButtonProps,
  "children" | "variant"
> & {
  /** Host density — the card's own box, set by the surface that lists it. */
  cardXstyle?: StyleXStyles
  current?: boolean
  /** `compact` is the widget reading, where the stamp and its copy share a
   * host viewport with the board they describe. */
  density?: "default" | "compact"
  detail?: string
  glyph: string
  label: string
  moveLabel: string
  tone: ReviewMomentMarkerPresentation["tone"]
}

type WatercolorMomentSummaryProps = Pick<
  WatercolorMomentCardProps,
  "density" | "detail" | "glyph" | "label" | "moveLabel" | "tone"
>

function WatercolorMomentSummary({
  density = "default",
  detail,
  glyph,
  label,
  moveLabel,
  tone,
}: WatercolorMomentSummaryProps) {
  const compact = density === "compact"
  return (
    <>
      <WatercolorSymbol
        className="chen-watercolor-moment-card-glyph"
        silhouette="seal"
        tone={momentSymbolTone[tone]}
        xstyle={[
          momentToneStyles[tone],
          momentCardStyles.glyph,
          compact && momentCardStyles.glyphCompact,
        ]}
      >
        {glyph}
      </WatercolorSymbol>
      <span
        {...craft(
          ["chen-watercolor-moment-card-copy"],
          momentCardStyles.copy,
          compact && momentCardStyles.copyCompact,
        )}
      >
        <strong {...stylex.props(momentCardStyles.move)}>{moveLabel}</strong>
        <span {...stylex.props(momentCardStyles.detail)}>{label}</span>
        {detail ? (
          <small
            {...stylex.props(
              momentCardStyles.detail,
              compact && momentCardStyles.detailCompact,
            )}
          >
            {detail}
          </small>
        ) : null}
      </span>
    </>
  )
}

function WatercolorMomentCard({
  cardXstyle,
  className,
  current = false,
  density = "default",
  detail,
  glyph,
  label,
  moveLabel,
  tone,
  ...props
}: WatercolorMomentCardProps) {
  return (
    <WatercolorButton
      aria-current={current ? "step" : undefined}
      aria-label={`${moveLabel}: ${label}${detail ? `. ${detail}` : ""}`}
      className={[
        "chen-watercolor-moment-card",
        `chen-review-moment-${tone}`,
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      hoverWash="bloom"
      variant="quiet"
      xstyle={[
        momentCardStyles.card,
        momentToneStyles[tone],
        current ? momentCardStyles.current : undefined,
        cardXstyle,
      ]}
      {...props}
    >
      <WatercolorMomentSummary
        density={density}
        detail={detail}
        glyph={glyph}
        label={label}
        moveLabel={moveLabel}
        tone={tone}
      />
    </WatercolorButton>
  )
}

type WatercolorEvaluationGraphProps = Omit<
  ComponentProps<typeof EvaluationGraph>,
  "caption"
> & {
  className?: string
}

type WatercolorMoveNavProps = HTMLAttributes<HTMLElement> & {
  /** `compact` is the nav embedded under a widget board, where the line has
   * to share it with notation. */
  density?: "default" | "compact"
  disabled?: boolean
  firstAriaLabel?: string
  jumps?: boolean
  lastAriaLabel?: string
  maxPly: number
  minPly?: number
  onNavigate: (ply: number) => void
  ply: number
  plyLabel?: ReactNode
}

function WatercolorMoveNav({
  children,
  className,
  density = "default",
  disabled = false,
  firstAriaLabel = "First move",
  jumps = true,
  lastAriaLabel = "Last move",
  maxPly,
  minPly = 1,
  onNavigate,
  ply,
  plyLabel,
  ...props
}: WatercolorMoveNavProps) {
  const atStart = disabled || ply <= minPly
  const atEnd = disabled || ply >= maxPly
  return (
    <HStack
      className={["chen-watercolor-move-nav", className]
        .filter(Boolean)
        .join(" ")}
      data-layout-single-row=""
      data-watercolor-control="move-sequence"
      gap={2}
      role="group"
      vAlign="center"
      wrap="nowrap"
      xstyle={moveNavStyles.nav}
      {...props}
    >
      {jumps ? (
        <WatercolorButton
          aria-label={firstAriaLabel}
          disabled={atStart}
          onClick={() => onNavigate(minPly)}
          size="icon"
          type="button"
          variant="quiet"
          xstyle={moveNavStyles.jump}
        >
          <Icon icon="chevronFirst" size="sm" />
        </WatercolorButton>
      ) : null}
      <WatercolorButton
        aria-label="Previous move"
        className="chen-watercolor-move-previous"
        disabled={atStart}
        onClick={() => onNavigate(ply - 1)}
        type="button"
        variant="outline"
        xstyle={[
          moveNavStyles.step,
          density === "compact" && moveNavStyles.stepFrame,
          density === "compact" && moveNavStyles.compactStep,
        ]}
      >
        <Icon icon="chevronLeft" size="sm" />
        <span
          {...craft(["chen-watercolor-move-nav-label"], moveNavStyles.label)}
        >
          Previous move
        </span>
      </WatercolorButton>
      <span {...craft(["chen-watercolor-move-nav-ply"], moveNavStyles.ply)}>
        {plyLabel ?? `${ply} / ${maxPly}`}
      </span>
      <WatercolorButton
        aria-label="Next move"
        className="chen-watercolor-move-next"
        disabled={atEnd}
        onClick={() => onNavigate(ply + 1)}
        type="button"
        variant={density === "compact" ? "primary" : "outline"}
        xstyle={[
          moveNavStyles.step,
          density === "compact" && buttonStyles.blockWideMask,
          density === "compact" && moveNavStyles.stepStroke,
          density === "compact" && moveNavStyles.compactStep,
        ]}
      >
        <span
          {...craft(["chen-watercolor-move-nav-label"], moveNavStyles.label)}
        >
          Next move
        </span>
        <Icon icon="chevronRight" size="sm" />
      </WatercolorButton>
      {jumps ? (
        <WatercolorButton
          aria-label={lastAriaLabel}
          disabled={atEnd}
          onClick={() => onNavigate(maxPly)}
          size="icon"
          type="button"
          variant="quiet"
          xstyle={moveNavStyles.jump}
        >
          <Icon icon="chevronLast" size="sm" />
        </WatercolorButton>
      ) : null}
      {children}
    </HStack>
  )
}

function WatercolorEvaluationGraph({
  className,
  density = "default",
  title = "Real-game evaluation",
  ...props
}: WatercolorEvaluationGraphProps) {
  const sparkline = density === "sparkline"
  const activeEvaluation = evaluationAt(
    [...props.points].sort((left, right) => left.ply - right.ply),
    props.activePly,
  )
  return (
    <WatercolorCard
      className={["chen-watercolor-evaluation-graph", className]
        .filter(Boolean)
        .join(" ")}
      data-watercolor-surface="evaluation-graph"
      meta={
        sparkline ? undefined : (
          <Text aria-label="Evaluation at the selected moment" role="status">
            {activeEvaluation?.label ?? "—"}
          </Text>
        )
      }
      padding={sparkline ? "compact" : "comfortable"}
      title={sparkline ? undefined : (title ?? undefined)}
      tone="paper"
    >
      <EvaluationGraph
        caption={false}
        density={density}
        skin="watercolor"
        title={null}
        {...props}
      />
    </WatercolorCard>
  )
}

export {
  SessionHeaderLabel,
  WatercolorBadge,
  WatercolorButton,
  WatercolorButtonLink,
  WatercolorCard,
  WatercolorChatBubble,
  WatercolorChatComposer,
  WatercolorCardContent,
  WatercolorCardDescription,
  WatercolorCardFooter,
  WatercolorCardHeader,
  WatercolorCardTitle,
  WatercolorCheckbox,
  WatercolorChessboard,
  WatercolorChip,
  WatercolorDialog,
  WatercolorEvaluationBar,
  WatercolorEvaluationGraph,
  WatercolorEyebrow,
  WatercolorField,
  WatercolorInkStroke,
  WatercolorInput,
  WatercolorMomentCard,
  WatercolorMomentSummary,
  WatercolorMoveNav,
  WatercolorNotice,
  WatercolorPlaque,
  WatercolorProgress,
  WatercolorSelect,
  WatercolorStudio,
  WatercolorSymbol,
  WatercolorTextarea,
  WatercolorTooltip,
}
export type {
  WatercolorBadgeProps,
  WatercolorButtonProps,
  WatercolorButtonLinkProps,
  WatercolorCardHeadingLevel,
  WatercolorChatBackdrop,
  WatercolorChatBubbleProps,
  WatercolorChatComposerProps,
  WatercolorCardProps,
  WatercolorCheckboxProps,
  WatercolorChessboardProps,
  WatercolorChipProps,
  WatercolorDialogBackdrop,
  WatercolorDialogProps,
  WatercolorEvaluationBarProps,
  WatercolorEvaluationGraphProps,
  WatercolorEyebrowProps,
  WatercolorFieldProps,
  WatercolorInkStrokeProps,
  WatercolorMomentCardProps,
  WatercolorMomentSummaryProps,
  WatercolorMoveNavProps,
  WatercolorNoticeAppearance,
  WatercolorNoticeProps,
  WatercolorPlaqueProps,
  WatercolorProgressProps,
  WatercolorStudioProps,
  WatercolorSymbolProps,
  WatercolorTooltipProps,
}
