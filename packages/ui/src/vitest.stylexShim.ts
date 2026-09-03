/**
 * The StyleX surface for Vitest graphs.
 *
 * The StyleX compiler runs in every Vite build and dev server via
 * `chenStylexVitePlugin`, but Vitest transforms modules without it, so an
 * authored `stylex.create` would throw at import time. jsdom resolves no
 * stylesheets either way — visual correctness belongs to the browser gates
 * (the foundation check and the preview surfaces), never to unit tests.
 *
 * This shim keeps imports working in both directions:
 * - Astryx ships precompiled styles (`$$css` maps whose values are class
 *   names); `props` collects those strings so structural classes survive.
 * - Authored craft objects pass through untransformed; their simple top-level
 *   declarations surface as inline style so a test could still observe them.
 *
 * Wire it as the `@stylexjs/stylex` alias in a `vitest.config`.
 */

/** A style declaration as StyleX authors it: values stay opaque to the shim. */
type StyleValue = string | number | boolean | null | StyleObject | undefined
type StyleObject = { [property: string]: StyleValue }

export function create<T extends Record<string, StyleObject>>(styles: T): T {
  return styles
}

export function keyframes(_frames: StyleObject): string {
  return "chen-watercolor-animation"
}

export function defineVars<T extends StyleObject>(vars: T): T {
  return vars
}

export function createTheme(): StyleObject {
  return {}
}

export function firstThatWorks(...values: readonly string[]): string {
  return values[0] ?? ""
}

function parseCompiledStyle(
  input: unknown,
  classNames: string[],
  style: Record<string, string | number>,
): void {
  if (input == null || input === false) return
  if (Array.isArray(input)) {
    for (const entry of input) parseCompiledStyle(entry, classNames, style)
    return
  }
  if (typeof input !== "object") return
  // SAFETY: anything object-shaped here is either a precompiled StyleX entry
  // ($$css map of class-name strings) or an authored declaration object; both
  // are read generically below, never mutated.
  const entry = input as StyleObject
  if (entry.$$css) {
    for (const [key, value] of Object.entries(entry)) {
      if (key !== "$$css" && typeof value === "string") classNames.push(value)
    }
    return
  }
  for (const [key, value] of Object.entries(entry)) {
    if (typeof value === "string" || typeof value === "number") {
      if (!key.startsWith(":") && !key.startsWith("@")) style[key] = value
    }
  }
}

export function props(...styles: readonly unknown[]) {
  const classNames: string[] = []
  const style: Record<string, string | number> = {}
  parseCompiledStyle(styles, classNames, style)
  return { className: classNames.join(" "), style }
}

export default {
  create,
  createTheme,
  defineVars,
  firstThatWorks,
  keyframes,
  props,
}
