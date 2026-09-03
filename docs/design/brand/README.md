# ChenChess brand system

This directory records the approved visual direction for issue #154. The
production-ready files live in `packages/ui/src/assets/brand/`; these raster
boards are reference material and must not be bundled into the application.

Product chrome rules — no invented subtitles, watercolor notices instead of
Astryx banners — live in `docs/design/product-chrome.md`.

## Identity

- Brand name: **ChenChess**
- Product line: **COACH**
- Promise: **Understand. Improve. Enjoy Chess.**
- Surname seal: traditional Chinese `陳` (`U+9673`) only
- Voice: calm, clear, encouraging, observant

The primary mark combines an open watercolor-brush circle, a dignified watercolor
knight, and a restrained vermilion surname seal. The knight is recognizable and
warm without becoming a mascot or an expressive cartoon character.

## Palette

| Token      | Hex       | Purpose                            |
| ---------- | --------- | ---------------------------------- |
| Rice paper | `#F7F2E8` | primary background                 |
| Warm ivory | `#FFF9ED` | light pieces and elevated surfaces |
| Ink navy   | `#142B46` | primary foreground and dark pieces |
| Slate blue | `#4D6F99` | secondary ink and interaction      |
| Mist blue  | `#A8BED0` | quiet washes and board squares     |
| Stone      | `#C9C5BC` | neutral texture and borders        |
| Bamboo     | `#7F9274` | understanding and positive support |
| Vermilion  | `#B85C45` | surname seal and rare emphasis     |

Vermilion is not a general-purpose error color. Text and controls must use
semantic foreground/background pairs with verified contrast rather than
sampling pale watercolor areas.

## Illustration and motion

The art language uses absorbent rice paper, transparent watercolor layers,
dry-brush contours, mist, and generous untouched space. Illustration is
atmospheric support, never the only carrier of information.

_Total War: Three Kingdoms_ is a high-level reference only for historical
gravitas, dry-brush energy, painted silhouettes, layered mountain mist, and
ink-plume transitions. ChenChess remains an original, quieter coaching system.
Do not copy or derive that game's characters, factions, symbols, campaign maps,
UI frames, fonts, logos, textures, audio, cinematics, exact transitions,
animation curves, or trade dress.

Motion for React should orchestrate local SVG masks and CSS:

- interaction feedback: approximately 140–220 ms
- non-blocking pigment/wash transition: approximately 240–420 ms
- optional brush-circle drawing: up to 600 ms
- reduced motion: immediate state, outline/color, or opacity-only transitions

Do not use a real-time fluid simulation, particle engine, or per-frame
turbulence. Commands, focus, announcements, cancellation, and recovery never
wait for decorative motion.

## Production asset rules

- Consume files through a shared `@chenchess/ui` asset interface.
- Treat logos and meaningful illustrations as named images; mark purely
  decorative texture and motion masks as decorative.
- Keep chess rules, state, and piece identity outside SVG path data.
- Preserve the piece view box, baseline, and padding when refining artwork.
- Do not recolor the sides so closely that side identity depends on board color.
- Do not ship this directory's raster reference boards at runtime.

## Provenance

- `chenchess-brand-system-reference.jpg` is the Player-approved art-direction
  board supplied in the product-design conversation.
- `chenchess-workspace-application-target.jpg` is an AI-generated application
  target retained for implementation guidance, not for runtime use.
- The production logo, app-icon, and value-icon SVGs wrap local high-fidelity
  WebP crops derived from the Player-approved brand board. Chess pieces start
  as those same lossless WebP crops, then `@chenchess/ui` traces them once with
  VTracer into scalable vector SVGs with the controlled `0 0 100 120` view
  box, then SVGO minifies the path commands. The one-shot trace keeps
  native resolution and stacked watercolor washes (`color_precision` 6,
  `gradient_step` 16). Run
  `node docs/design/brand/regenerate-assets.mjs` and then
  `bun run --cwd packages/ui vectorize:chess-pieces` to reproduce them.
  `verify:piece-sprite` checks `chess-pieces/vectorize.lock.json` against those
  source WebPs and the committed VTracer/SVGO parameters. The layered
  `logos/knight-mark.svg` is a hybrid: `data-layer="knight"` and
  `data-layer="circle"` are painted WebP crops from `logos/mark.svg` so dry-brush
  hair survives. The PR 452 white-knight VTracer stays on the chessboard. Run
  `bun run --cwd packages/ui compose:knight-mark`.
- `review-companions.webp` is an original AI-generated illustration corrected
  with the built-in image generation tool from the approved brand and
  application boards. `mountain-mist.webp` is an original AI-generated
  illustration created from the approved brand board. The constrained prompts
  are recorded in `GENERATION.md`.
