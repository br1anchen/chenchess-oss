import * as stylex from "@stylexjs/stylex"

/**
 * The chrome the static public pages wear: the skip link, the header, the
 * footer, and the column that holds their copy.
 */

const narrow = "@media (max-width: 860px)"
const phone = "@media (max-width: 620px)"
const wideWidth = "min(120rem, calc(100% - 2rem))"

export const chromeStyles = stylex.create({
  skipLink: {
    position: "fixed",
    zIndex: 20,
    top: "0.75rem",
    left: "0.75rem",
    padding: "0.75rem 1rem",
    borderRadius: "var(--radius-element)",
    backgroundColor: "var(--color-text-primary)",
    color: "var(--color-background-surface)",
    textDecoration: "none",
    transform: { default: "translateY(-180%)", ":focus": "translateY(0)" },
  },
  header: {
    position: "relative",
    zIndex: 10,
    display: "flex",
    alignItems: { default: "center", [phone]: "flex-start" },
    justifyContent: "space-between",
    gap: "2rem",
    width: wideWidth,
    marginInline: "auto",
    padding: "1.5rem 0",
    backgroundImage:
      "linear-gradient(180deg, color-mix(in srgb, var(--color-paper) 80%, transparent), color-mix(in srgb, var(--color-paper) 32%, transparent))",
    backdropFilter: "blur(8px)",
  },
  nav: {
    display: { default: "flex", [phone]: "grid" },
    flexWrap: "wrap",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: { default: "clamp(1rem, 3vw, 2rem)", [phone]: "0.75rem" },
    textAlign: { default: null, [phone]: "right" },
  },
  navLink: {
    color: {
      default: "var(--color-text-disabled)",
      ":hover": "var(--color-text-primary)",
    },
    fontSize: "0.88rem",
    fontWeight: 700,
    textDecoration: { default: "none", ":hover": "underline" },
    textDecorationColor: "var(--color-error)",
    textDecorationThickness: "2px",
    textUnderlineOffset: "0.35rem",
  },
  footer: {
    display: "grid",
    gridTemplateColumns: { default: "1fr auto 1fr", [narrow]: "1fr" },
    alignItems: "center",
    gap: "2rem",
    width: wideWidth,
    margin: "clamp(5rem, 10vw, 9rem) auto 0",
    padding: "2rem 0 3rem",
    color: "var(--color-text-disabled)",
    fontSize: "0.78rem",
    textAlign: { default: null, [narrow]: "center" },
  },
  footerBrand: {
    display: "grid",
    gap: "0.25rem",
    textAlign: { default: null, [narrow]: "center" },
  },
  footerNav: {
    display: "flex",
    flexWrap: "wrap",
    justifyContent: "center",
    gap: "1.5rem",
  },
  footerLink: {
    color: "inherit",
    fontWeight: 750,
    textUnderlineOffset: "0.25rem",
  },
  footerNote: {
    margin: 0,
    textAlign: { default: "right", [narrow]: "center" },
  },
  footerTagline: {
    fontSize: "inherit",
    lineHeight: "inherit",
  },
})
