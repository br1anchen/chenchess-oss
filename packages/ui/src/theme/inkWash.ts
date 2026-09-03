import { defineTheme } from "@astryxdesign/core/theme"
import { stoneTheme } from "@astryxdesign/theme-stone"

/**
 * The ink-wash theme is the only place an Astryx token value is written.
 * `astryx theme build` compiles it to `generated/`, and `bun run theme:check`
 * fails when those outputs drift from this file.
 *
 * `color.accent` drives the accent family — `--color-accent-muted`,
 * `--color-on-accent`, `--color-icon-accent` and `--focus-outline-color` all
 * derive from it, so no accent-adjacent token is written by hand.
 *
 * A single-string token value applies to both color schemes. ChenChess renders
 * light only (`ChenTheme` passes `mode="light"`), so paper cannot invert to ink
 * under a host OS preference.
 */
export const inkWashTheme = defineTheme({
  name: "ink-wash",
  extends: stoneTheme,
  color: {
    accent: "#8f3026",
    neutralStyle: "warm",
  },
  // The three stacks the app already renders with, moved from element rules in
  // `globals.css` to the theme that now owns them. No family here is loaded as a
  // webfont, on this branch or before it: each stack falls through to a system
  // face, so `astryx theme build` warns about all three.
  typography: {
    scale: { base: 16, ratio: 1.2 },
    body: {
      family: "Inter",
      fallbacks:
        "ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
    },
    heading: {
      family: "Iowan Old Style",
      fallbacks:
        "'Palatino Linotype', 'Book Antiqua', Palatino, Georgia, serif",
    },
    code: {
      family: "ui-monospace",
      fallbacks: "SFMono-Regular, Menlo, monospace",
    },
  },
  radius: { base: 4, multiplier: 1.2 },
  motion: { fast: 180, medium: 410, ratio: 0.75 },
  tokens: {
    "--color-background-body": "#f7f2e8",
    "--color-background-surface": "#fff9ed",
    "--color-background-card": "#fff9ed",
    "--color-background-muted": "#ede4d3",
    "--color-text-primary": "#142b46",
    "--color-text-secondary": "#4d6f99",
    "--color-text-disabled": "#536274",
    "--color-border": "#a8bed0",
    "--color-border-emphasized": "#4d6f99",
    "--color-error": "#a74836",
    "--color-error-muted": "#e9c9bc",
    "--color-success": "#718267",
    // The HCT accent scale lifts #8f3026 to #a03f33 for contrast headroom.
    // Pin it back: the seal red is drawn into the watercolor artwork, the app
    // icons and the `theme-color` meta, so a component that lifts it reads as a
    // second red. `--color-on-accent` stays white and gains contrast, since the
    // pin is darker than the value the scale derived it against.
    "--color-accent": "#8f3026",
    // Panels keep the softer ink-wash corner and the wide, ink-tinted lift the
    // watercolor surfaces are drawn against; the concentric scale above still
    // owns `--radius-inner` and `--radius-element`.
    "--radius-container": "1.25rem",
    "--shadow-low": "0 18px 50px rgb(20 43 70 / 0.09)",
    "--shadow-med": "0 22px 64px rgb(20 43 70 / 0.18)",
  },
  components: {
    dialog: {
      base: {
        boxShadow: "var(--shadow-med)",
        overscrollBehavior: "contain",
      },
    },
    // Astryx paints the tooltip popover itself and exposes no style prop for
    // it, so the ink surface is registered here — the one seam that reaches
    // it. `WatercolorTooltip` is the import product code uses. Colours are
    // token reads, so this is the single source next to the shape() half of
    // the craft (an `@supports` block in `styles/globals.css`, which the
    // theme builder cannot author).
    tooltip: {
      base: {
        borderRadius: "0.16rem 0.4rem 0.18rem 0.34rem",
        backgroundColor: "var(--color-text-primary)",
        backgroundImage:
          "radial-gradient(ellipse at 88% 6%, rgb(168 190 208 / 0.22), transparent 44%)",
        color: "var(--color-background-surface)",
        fontFamily: "var(--font-family-heading)",
        letterSpacing: "0.012em",
        boxShadow: "0 0.5rem 1.1rem rgb(20 43 70 / 0.22)",
      },
    },
  },
})
