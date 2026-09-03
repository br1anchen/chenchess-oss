# Watercolor component set

Shared ChenChess surfaces. Tokens in `packages/ui/src/styles/globals.css` are
canonical. Do not restyle a screen with a parallel palette.

## Primitives

Astryx on StyleX stays the foundation underneath: the craft — dry-brush
strokes, blot masks, stamps — is authored StyleX in `watercolor.styles.ts`,
applied by the wrappers in `watercolor.tsx`. Text runs render native elements
(Astryx Text/Heading would put duplicate atom hashes on the element and turn
the cascade into a coin flip). The `chen-watercolor-*` classes survive as
structural hooks for per-surface layout CSS and tests; they carry no visuals.
Product chrome uses these wrappers, not raw Astryx, so the ChenChess look
survives on every surface (regression #465).

| Component                                                                                                                         | Use                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| --------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `WatercolorButton`                                                                                                                | Primary blot mask, secondary dry-brush edge, quiet, danger. Wraps Astryx Button/IconButton.                                                                                                                                                                                                                                                                                                                                                                                       |
| `WatercolorButtonLink`                                                                                                            | The same craft on a native anchor, for links that read as actions (landing calls to action, sign-in handoffs).                                                                                                                                                                                                                                                                                                                                                                    |
| `WatercolorCard` (+ `Header`, `Title`, `Description`, `Content`, `Footer`)                                                        | Shared **content card**: eyebrow, title, meta, body, optional 陳 corner stamp (`U+9673`). Quiet paper/ivory; craft is a thin, irregular four-stroke ink border, with the title on an ink-splash plaque inside the frame. Tones stay paper / mist / bamboo / vermilion / watercolor. Nested inside another card, pass `frame={false}` — two stacked ink borders read as a rendering fault. `titleXstyle` sizes the plaque for a recipe with its own title type.                    |
| `WatercolorField`, `WatercolorInput`, `WatercolorTextarea`, `WatercolorSelect`, `WatercolorCheckbox`                              | Form controls on paper (native elements with the dry-brush input frame). The field associates its label and its `hint`/`error` note explicitly, so the note describes the control instead of joining its name; the watercolor controls pick that up from context, a foreign child needs its own `id`. The checkbox hides its input behind the painted mark; a pointer reaches it through the label.                                                                               |
| `WatercolorBadge`, `WatercolorChip`, `WatercolorSymbol`, `WatercolorNotice`, `WatercolorEyebrow`                                  | Stamp (dry-brush, not a pill), result/idea chip, seal/glyph, empty states, vermilion kicker                                                                                                                                                                                                                                                                                                                                                                                       |
| `WatercolorNotice` `featured`                                                                                                     | Standing empty state on a watercolor card: eyebrow, serif title, optional meta, quieter detail                                                                                                                                                                                                                                                                                                                                                                                    |
| `WatercolorStudio`                                                                                                                | Rice-paper page with mountain mist. Login, dashboard, import, and other studio chrome.                                                                                                                                                                                                                                                                                                                                                                                            |
| `WatercolorSessionHeader`                                                                                                         | Review Session page header: lockup, title, optional eyebrow/meta/actions. The title keeps a 12ch floor so a nowrap badge cannot crush it to 0px.                                                                                                                                                                                                                                                                                                                                  |
| `WatercolorProgress`                                                                                                              | Ink-wash progress track                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `WatercolorPlaque`                                                                                                                | Ink-splash title plaque on the real dry-brush slab; paints itself in with a brush-wipe sweep (place it inside the heading element that owns the copy)                                                                                                                                                                                                                                                                                                                             |
| `WatercolorInkStroke`                                                                                                             | The brush swoosh that draws itself along its own spine (guide-stroke mask reveal). Flat right end bleeds off the container; hero underline / divider. Ink follows `currentColor`; host sets the box.                                                                                                                                                                                                                                                                              |
| `WatercolorChatBubble`                                                                                                            | Astryx ChatMessageBubble on the watercolor wash; compose inside ChatMessage / ChatMessageList. Tones: coach (paper), player (bamboo), system (muted). `backdrop` paints behind it — `patch` puts the copy **on** a torn-edged splash — the bubble drops its own fill and border, and two offset pulls of the blot give it a wet edge — while `wash` keeps the box and floats cloud pigment behind it. Default `none`: artwork on every bubble in a thread reads as texture noise. |
| `WatercolorChessboard`, `WatercolorEvaluationBar`, `WatercolorEvaluationGraph`, `WatercolorMomentCard`, `WatercolorMomentSummary` | Review session. The eval graph is a content card: title, eval as meta, plot on ivory.                                                                                                                                                                                                                                                                                                                                                                                             |
| `WatercolorMoveNav`                                                                                                               | Shared ply pair: labeled **Previous move** (dry-brush outline), ply/position as text, labeled **Next move**, which at widget density is the filled control on the real wide brush stroke. Below 520px the labels drop and the ply shrinks; accessible names stay on every button. Jump-to-start/end stay quiet/small.                                                                                                                                                             |

| `WatercolorDialog` | Astryx Dialog with the card's ink frame, painted on open, and an ink-wash `::backdrop`. `backdrop`: `paper` (routine confirmation), `cloud` (painting behind the copy), `ink` (navy wash, inverted controls). |
| `WatercolorTooltip` | Astryx Tooltip on the ink surface. Astryx paints the popover itself and exposes no style prop, so the craft is registered on the theme's `tooltip` target in `theme/inkWash.ts`; import this wrapper so the seam stays in one place. |

## Hover

Every `WatercolorButton` / `WatercolorButtonLink` repaints itself as a
dry-brush stroke on hover and on `:focus-visible`. Nothing is recoloured and
nothing is layered on top: the stroke is the button's **own** ink, and what
changes is the shape — a flat slab becomes brushwork, left to right.

On a **pale** control (secondary, outline) the frame is the control's
identity, and it leaves as the slab lands: it fades over 120 ms and is
clipped away under the ink where the stroke has already painted
(`buttonStyles.strokeClip`), so the control is never wearing the box and the
brush at once. The label crosses to paper only once the ink has reached it,
and back to navy only once the retreating front has uncovered it. A
**filled** control keeps both — the brush artwork carries partial alpha, so a
stroke that replaced the slab would read _lighter_ than the resting one and
the button would look rubbed out. It wears the second pull over the first, a
shade deeper.

A control the size of a **card** takes the other wash: `hoverWash="bloom"`
lands an ink drop at its centre and ripples it out through the paper by
growing the blot artwork's own mask. A brush stroke as wide as a card reads as
a fill rather than as a hover, and the splash keeps a ragged edge at any card
width where a scaled gradient has none. It is the digest's arriving wash
(`digestTransition.styles.ts`) held sharp and kept, rather than faded away.

The stroke repaints whatever carries that control's identity, and nothing
else on the control moves. Filled buttons (primary, danger) keep their exact
colour — `danger` stays unmistakably destructive at the moment of
commitment. Quiet owns no edge at rest or on hover: the same stroke travels
under its label at a fifth of the ink and the label stays the darkest thing
on it. Two quiet controls keep a _resting_ edge on purpose — the move-nav
jump buttons (0.46, deepening to 0.82 on hover) and a `current` Review Moment
(its selected state) — and neither is a hover decoration. Card-sized
controls (`hoverWash="bloom"`) never take the stroke's clip: their frame is
their identity and the drop lands over it.

Travel is `background-size`, driven by `--watercolor-hover-sweep`, which is
**registered** with `@property` in `chenTokens.css`: an unregistered custom
property has no type to interpolate, so the stroke would snap to full instead
of being painted on. `--watercolor-hover-tip` is an opt-in second colour at the
leading edge — off by default, since a second colour is the extra layer the
stroke exists to avoid. The wash bleeds vertically only; a horizontal bleed
widens scrollable overflow and pushes narrow surfaces into a horizontal
scrollbar. Reduced motion keeps the highlight and drops the travel.

Controls the size of a card — the Review Moment card, an Imported Game row —
pass `hoverWash="bloom"` instead. A brush stroke as wide as a card reads as a
fill rather than as a hover, so the ink lands as a drop and spreads through
the paper: a pigment gradient grown by `--watercolor-hover-bloom` (registered
alongside the sweep) and settling at `--watercolor-bloom-strength` (0.12 by
default — far lighter than a stroke, since the copy on top of it has to stay
the darkest thing in the row; a host may set the property on the control).
The blot keeps its own aspect: the bloom sets its width and the height
follows the artwork, so a wide row shows the drop's middle band rather than
a smear. It is the gesture an arriving Coaching Digest makes
(`daily-coaching/digestTransition.styles.ts`). `hoverWash="none"` opts out
entirely, and a disabled or loading button renders no wash element at all —
the switch is a custom property flipped under `:hover`, which StyleX orders
after `:disabled`, so a pointer resting on a disabled control would otherwise
still light it.

## Backdrops and asset weight

`patch` / `wash` / `cloud` / `ink` read two rasters that live in
`theme/surfaces.css`, not `chenTokens.css`: every `url()` in the global token
sheet is inlined into each single-file Coach App artifact, and no widget renders
a dialog or a painted bubble. An app that shows these surfaces imports
`@chenchess/ui/surfaces.css` next to the theme; without it the backdrop simply
does not paint. For the same reason `assets.ts` names no brush asset — one file
reachable from both the stylesheet and the JS manifest is inlined twice and
fails `assertUniqueImageDataUris`.

## Product artifacts

| Component     | Use                                                                                                                                                                                                                                                                                                                                |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `BrandLockup` | The product wordmark, plain text. `TRADEMARKS.md` reserves the name, so a fork puts its own here. Sizes: `header`, `footer`, `workspace`. This snapshot ships no mark artwork, so `mark` is accepted and ignored.                                                                                                                  |
| `DigestCard`  | Morning-digest **recipe** of `WatercolorCard`. Slots: vermilion eyebrow, serif coverage-date title, published time + Games pill as meta, numbered Today’s priorities and host-owned game children as body. `featured` / `detail` / `list` stay content recipes. Do not wrap featured/detail in a hit — games keep their own links. |

Content cards use the four-stroke `--watercolor-brush-frame` as a thin
fine-pen ink/grey line (weight varies a little per side; overflow stays visible
so the stroke can bleed). The fill is opaque ivory paper (~0.96–0.97). Ink
stays on the border. No washi, fiber wallpaper, calligraphy grid, torn-paper
fill, drop shadow, or charcoal nested panel. Type is navy on cream; vermilion
is the eyebrow and the rare 陳 corner stamp.

`BrandedReviewWorkspace` sets navy-on-paper `--color-text-primary-*` tokens.
Graph and moment-picker fallbacks are cream, not charcoal.

## Motion

Interaction 140–220 ms. Wash 240–420 ms. Brush circle ≤600 ms. Reduced motion
drops decorative wait and keeps outline or opacity only.
