import * as stylex from "@stylexjs/stylex"

const stack = "@media (max-width: 64rem)"

/**
 * The branded review shell's own surface. Rules that reach into markup other
 * components own (headings, tone vars, the board wash) stay in
 * review-session.css. The mist asset URL arrives per-instance as the
 * `--review-mist` inline custom property.
 */
export const shellStyles = stylex.create({
  root: {
    /* Navy-on-paper board tokens for every board this shell seats, so a
       later sheet cannot leak a second card family onto cream. */
    "--color-icon-secondary": "#c9c5bc",
    "--color-board-light": "#f2ebdd",
    "--color-board-dark": "#b3c1cc",
    "--color-board-last-move": "rgb(127 146 116 / 0.32)",
    "--color-board-check": "var(--color-vermilion)",
    position: "relative",
    display: "grid",
    width: "min(112rem, 100%)",
    minHeight: "100vh",
    gap: "clamp(0.85rem, 1.6vw, 1.35rem)",
    marginInline: "auto",
    padding: {
      default: "clamp(0.85rem, 2vw, 1.8rem)",
      [stack]: "0.75rem",
    },
    backgroundColor: "var(--color-background-body)",
    backgroundImage:
      "linear-gradient(180deg, color-mix(in srgb, var(--color-paper) 78%, transparent) 0%, color-mix(in srgb, var(--color-paper) 42%, transparent) 38%, color-mix(in srgb, var(--color-paper) 70%, transparent) 100%), var(--review-mist)",
    backgroundPosition: "0 0, center bottom",
    backgroundSize: "auto, min(96rem, 160%) auto",
    backgroundRepeat: "repeat, no-repeat",
    color: "var(--color-text-primary)",
    colorScheme: "light",
    fontFamily:
      'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  },
})
