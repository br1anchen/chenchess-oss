/**
 * The foundation viewport queries. Surface stylesheets and StyleX files may
 * only name these widths — never a one-off 42rem / 42.5rem pair.
 *
 * StyleX files copy these literals locally. Coach App artifact builds cannot
 * resolve a non-`.stylex.ts` import inside `stylex.create`.
 *
 * 64rem is the accepted Review Session column stack (#235). 1000px / 860px /
 * 620px are the landing and dashboard set. 520px is the mandated move-nav
 * compact cut (D1).
 */
export const foundationBreakpoints = {
  stack: "@media (max-width: 64rem)",
  compact: "@media (max-width: 1000px)",
  narrow: "@media (max-width: 860px)",
  phone: "@media (max-width: 620px)",
  moveNav: "@media (max-width: 520px)",
} as const
